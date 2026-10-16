# The LSP's reaction talks to the editor through a port

**Status:** accepted (2026-10-08). D7 amended by [ADR 0109](0109-reopening-a-project-in-place-is-one-session-operation.md) (`ProjectSession::reopen` is the one way to open in two steps).

ADR 0035 gave the LSP one reaction to a change: apply it to the project session, publish, follow the
session's inputs and catch up, refresh the editor's highlighting. The reaction held tower-lsp's
`Client`, so none of it ran without a JSON-RPC client. Tests rebuilt it by hand
(`tests/served.rs::apply_change`, "as the backend does"). The copy opened projects another way and had
no follow, catch-up, divergence report, token refresh or handler-side publishing. A bug lived in the
gap: `did_close` published an empty diagnostic set itself, and the reaction then published nothing for a
project source whose disk text was the compiled text, so closing a clean file with errors hid its errors
until an unrelated change republished them. The open sequence (indexing progress, the announcements)
was in the `initialized` handler, not the reaction. Of the LSP's 141 async tests, 35 asserted only what
the reaction sends, through JSON-RPC, several of them waiting seconds for messages that never come.
`initialize`'s capabilities went through a 15-field mirror whose function ignored its argument.

## Decisions

- **D1. Reactions are synchronous** (amends ADR 0023 D8, "answers are synchronous"). `Reaction<E:
  Editor>` applies a change, publishes, follows and refreshes in plain blocking code on the blocking
  pool. It reads and writes the LSP state with the blocking accessors and never holds the lock while it
  talks to the editor; readers meanwhile see the stand-in (ADR 0023). Its tests are plain `#[test]`s.
- **D2. The editor is a port.** `specforge_lsp::editor::Editor`: `publish`, `watch` (exactly these
  watchers from now on; `Err` when refused), `log`, `progress` (`WorkDone::{Begin, End}`) and
  `refresh_tokens` (not awaited). Each call returns once the message is sent and, for `watch`, answered,
  so the catch-up after the watchers move still follows the editor's answer (ADR 0035 D2). Two
  adapters: `ClientEditor` drives tower-lsp's client with `Handle::block_on` from the reaction's
  blocking thread; the tests' `Recorder` records what is sent, refuses watchers and acts while they
  move, as watch's `Watchers` recorder does. A contract test sends the same open sequence through both.
  The client adapter gives up on a call once the server is gone (the `Backend` dropped): a request the
  editor never answers would otherwise hold the reaction's thread, and the runtime's shutdown, for good.
- **D3. The queue is the backend's.** The backend holds the reaction in an async mutex and hands each
  change to the blocking pool once it holds it; `initialized` takes it before returning, so every change
  the client reports next waits for the project to open.
- **D4. Everything after a change is the reaction's.** Opening the workspace (the static watchers, the
  indexing progress, the open and its follow-up, the counts announced), a reload's announcement, a
  divergence, an update's panic, closing a document, following and catching up, the token refresh. The
  handlers keep the document store (an open, edited or closed buffer is recorded at once) and translate
  the protocol. What the client declared that steers the reaction is part of `ClientSupport`.
- **D5. A closed document's file is published as the project reports it**, once, in place of its
  buffer's diagnostics: a project source keeps its file's errors, a file that leaves the project is
  cleared.
- **D6. Formatting's publish stays on the request path.** `textDocument/formatting` publishes the
  formatter's W142 beside the compile's diagnostics through the client directly; queueing a format
  behind a running update would stall it for no gained behaviour. It is the one handler-side publish.
  (ADR 0056: the formatter's findings are recorded as the document's formatter layer; the request sends what
  the store answers, which is the whole list only when the compile layer is at the same version.)
- **D7. The reaction takes its runtime source.** Production passes `RuntimeSource::project()`, tests the
  in-process runtime: the `WasmRuntime` seam, not a parameter of the backend.
  `ProjectSession::begin_open(root, source)` is the one way to open in two steps.
- **D8. `initialize` answers one static value** (`capabilities::initialize_result`). The semantic token
  legend is every standard type, sent before extensions load, so no capability depends on the project.

## Consequences

- Closing a project source that has errors keeps them in the editor (user-visible); closing a file
  outside a project sends one empty publication instead of two.
- The recompile-failed log ends with the panic's message, not tokio's task error text.
- `tests/served.rs` drives the real reaction; there is no test copy of it. Its `Recorder` asserts what
  was published, watched, logged and refreshed, in order, without a client, a debounce or a timeout.
- The LSP's slowest JSON-RPC waits are gone (35 tests moved; the harness waits for the end of indexing
  instead of for log messages a project with no extension never sends).
- `ServerCapabilities`, `ServerInfo`, `server_capabilities` and `server_info` are gone from the crate's
  public items.

## Rejected

- **An async port.** The reaction's work is blocking (file reads, whole-graph checks); an async port
  keeps two hops to the blocking pool per change and a runtime in every test.
- **A pure core returning editor commands.** Each follow round needs the editor's answer to the
  registration before the catch-up, and a refusal changes what is asked next: the core would be
  re-entered after every answer.
- **A fire-and-forget adapter (an outbox drained by a task).** The catch-up could run before the editor
  took the new watchers, the race ADR 0035 closed.
- **Sharing `specforge_watch::Watchers`.** It watches directories for watch's own loop; ADR 0035
  rejected a session-driven loop over a watcher trait. The LSP keeps its loop; only its client moved
  behind the port.
  *(ADR 0099: the loop is shared over `Follower`; `Watchers` still is not.)*
- **The recorder behind a `testing` feature.** Only this crate's tests use it.
- **The document store inside the reaction.** A request would see a closed or stale buffer while an
  update runs; what the session makes of buffers is ADR 0046's.

## What would reopen it

A second LSP transport (another adapter, nothing else changes); a reaction that must run concurrently
with another (several workspace roots each with a session: one reaction per root); a client protocol
for watchers that does not answer registrations (then `watch` cannot wait for the answer, and the
catch-up needs another trigger).
