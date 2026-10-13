# Spec promises the design can't keep are trimmed, not faked

**Status:** accepted (2026-09-30)

An audit of every linked test found obligations no test could honestly prove, because the design
rules them out. Linking a test that checks something else hid the gap; leaving them unproven
forever misleads agents that read the spec. For each one we either built the feature or changed
the spec to say what SpecForge does:

- **MCP cancellation.** The server handles one request at a time, so a cancel always names a
  finished request. The spec no longer promises stopping in-flight work or partial results; a
  cancel is an acknowledged no-op. `notifications/cancelled` (MCP's name) is now routed.
- **LSP add-import action.** References resolve across the whole project without `use`, so the
  E003 that would trigger it never happens. The behavior and its dead helper are gone.
- **LSP extension grammars for highlighting.** Nothing loaded them; `GrammarCache` was an unused
  map. Highlighting comes from semantic tokens. The behavior and the cache are gone.
- **W021 severity.** A manifest that contradicts itself is the extension author's mistake; it
  warns and does not fail the user's compile.
- **`use "x.spec"`.** The resolver treats the extension as implicit either way, so spelling it
  out is allowed rather than rejected.
- **Spec-root `format_version` field.** Never built; the header comment is the one form.

Built instead of trimmed, because the machinery already existed: `specforge migrate`'s schema
snapshot and W053 compatibility check, and `specforge watch --verify-incremental`.
*(ADR 0067 D6: the schema snapshot and its check are gone; a migration cannot change the schema.)*

A second pass (tier 3, strengthening weak tests) settled more:

- **MCP delta notification names.** The spec now says `specforge/graphChanged` and
  `specforge/diagnosticsChanged`, what the server sends: MCP reserves `notifications/` for
  protocol messages.
- **MCP event payloads** follow `spec/events/mcp.spec` field for field, with a timestamp (stamped by the sink that keeps the event, ADR 0057).
- **Built:** extension starter templates for `specforge init`, colour-coded diagnostics on a
  terminal (plain when piped or under `NO_COLOR`), unknown-kind reports from the MCP query and
  search tools, and auto-promotion of extension CLI commands to MCP tools.
- **Known gap, left unproven:** the CLI does not run extension-contributed commands. No builtin
  extension contributes one yet; the obligations stay unproven until one does. *Closed by
  [ADR 0008](0008-extension-commands-run-over-the-graph.md):* `@specforge/product` contributes the
  `specforge product` commands, the CLI routes to them, and the dispatch obligations (args to the
  export, a trap as an extension error, exit code and output back to the CLI, a disabled command not
  routed) are proven.
