# The MCP stdio transport is the MCP crate's; the CLI proves only the binary

**Status:** accepted (2026-10-12, architecture round 7, plan 13). Applies ADR 0029 D4b and D9 and ADR 0024 D6;
amends none.

`specforge mcp`'s stdio loop (`specforge_cli::mcp`) held the MCP stdio transport's rules: a message per
line, blank lines skipped, a response before the notifications it queued, a flush per message, stop when
stdout's reader leaves, disconnect at the end of input. A process was the only way to test them, so
`specforge-cli/tests/e2e_mcp.rs` (3,082 lines, 81 tests) proved MCP's replies again through the binary: 45 of
its 48 linked obligations were also linked in `specforge-mcp`, and 41 of the 45 commits that touched it also
touched `specforge-mcp`. Six of those in-crate twins were weaker than the e2e test (a limit asserted `<= 1`, a
kind filter shown only to exclude), so neither copy was the proof of record. The loop also read its input
through `Terminal::read_line`, which took a line that is not UTF-8 for the end of input: the session ended in
silence, exit 0, every later request lost.

## Decision

- **D1. The transport is `specforge_mcp::stdio`.** `McpServer::serve_stdio(input: impl BufRead, output: impl
  Write) -> Ended` serves one client until the connection ends and then disconnects it (ADR 0024 D6's
  `disconnect`). `Ended` is `InputEnded`, `InputFailed(io::Error)` or `OutputFailed(io::Error)`.
- **D2. It is synchronous.** The server answers one request at a time, and a notification is queued only by the
  message being answered (ADR 0014), so a blocking read per line is the whole loop.
- **D3. Its rules.** Each line (ended by `\n`, `\r\n` or the end of input) is one message. A line of only JSON
  whitespace is skipped. A line that is not UTF-8 is a -32700 Parse error and serving goes on (JSON text is
  UTF-8, RFC 8259 section 8.1; MCP's stdio transport requires it). A message's response, then each notification
  it queued, is written one line each, then the output is flushed. The end of the input, a failed read, or a
  failed write or flush ends the connection.
- **D4. The CLI hands it the terminal.** `Terminal::input` is the held input, byte for byte; `Terminal::output`
  reports a reader that has gone as `BrokenPipe` and panics on any other failure, as every command's output
  does (ADR 0029 D4b). `specforge mcp` exits 0 however the connection ends: a closed stdout is D4b's quiet end,
  and a broken input is the client's end, which no client can observe.
- **D5. Each surface's protocol is proven in its own crate; the CLI's process tests are the binary contract**
  (ADR 0029 D9): `specforge mcp`'s PATH argument, its streams and exit, real pipes delivering each reply before
  the client's next request, and the bytes reaching the transport undecoded. Four tests. A reply's shape, a
  tool's behaviour or a protocol error is a `specforge-mcp` test.

## Consequences

- User-visible over MCP stdio: a line that is not UTF-8 is answered with -32700 instead of ending the session;
  a line wrapped in non-ASCII whitespace (U+00A0) is now -32700 too, as JSON says.
- A reply change touches `specforge-mcp`'s tests only; `e2e_mcp.rs` is gone (`specforge-cli/tests/mcp.rs`, 4
  tests).
- `Terminal::read_line` is deleted; the transport's rules are tested in memory (`specforge-mcp/tests/stdio.rs`).

## Rejected

- **The loop kept in the CLI, tested over a captured terminal**: MCP's protocol would still be proven in the
  CLI's tests, which is the duplication this removes.
- **An async transport (tokio)**: a runtime dependency for a select with one arm.
- **Decoding lossily** (`from_utf8_lossy`): `caf\xe9` would be served as `caf\u{FFFD}`, a different request than
  the client sent, answered as if it were its own.
- **A failed read or write as a refusal (exit 1)**: a transport failure would need a failure kind ops cannot know,
  ADR 0029's own reopening trigger, and no client reads that exit.
- **Keeping `e2e_mcp.rs` beside the in-crate tests**: two copies of each reply and neither the proof of record.

## Amendment (architecture round 9, plan 08)

- **D5a.** Besides `tests/mcp.rs`, a CLI test spawns `specforge mcp` only for what a process alone shows: the
  `[ROOT]` argument resolved from the working directory, and the process environment (a proxy). Every other CLI
  test that needs an MCP reply asks `specforge_mcp::testing::Served::rooted`, the server `specforge mcp <root>`
  builds, in process.
