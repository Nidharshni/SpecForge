# MCP events are one typed value, recorded through one sink; a server keeps none

**Status:** accepted (2026-10-12, architecture round 7, plan 06). Refines ADR 0003 (tier 3, "MCP event
payloads": the timestamp is the keeping sink's) and ADR 0024 D1 (a handler records its events through
the call; `Ran` is gone). Keeps ADR 0022 D7 (events never leave the process).

An MCP server recorded what it did as `McpEvent { name: String, params: Value }` pushed into
`McpState.events`, a public `Vec` nothing in production read or drained. Each event stayed for the
server's lifetime: about 0.9 KB per request, whatever the request (60 000 tool calls held 71 MB against
18 MB for 60 000 pings; 120 000 held 123 MB). Events entered through four channels (`push_event`, the
pipeline's hook pairs, `Ran.events`, a `&mut Vec` through `Subscriptions`) under 19 names written as string
literals in 11 modules, each payload a `json!` with its own key casing. A hand-written line parser checked
payloads against `spec/events/mcp.spec` only, so four surface events carried a `timestamp` their spec did
not declare, and five fields declared `@optional` were always sent.

## Decisions

- **E1. One typed event.** `specforge_mcp::events::McpEvent` has one variant per event the server records
  (19: `spec/events/mcp.spec`, `surface-contributions.spec`, and `extension_added`/`project_initialized`),
  deriving `Serialize` and `Shape`; the variant name is the event name, the fields its payload under the
  spec's names. A mutation's domain events stay a closed subset (`MutationEvent`).
- **E2. One sink.** Every producer records through `EventSink::record`: the state's sink, `Call::events()`
  in a handler, or the sink a function is given where no call exists (a refusal before the target
  resolves, a subscription change). `push_event`, `Ran`, the `(String, Value)` pairs and the `Vec`
  parameter are gone.
- **E3. A server keeps none.** Every `McpServer` records into `Discard` unless given another sink
  (`McpServer::with_events`); tests give an `EventLog` (`features = ["testing"]`). A test holds a server's
  heap flat over hundreds of request rounds.
- **E4. The keeping sink stamps.** A payload carries no time; a sink that keeps an event records when.
  Every spec payload of a recorded event declares `timestamp timestamp`, and its optional fields are
  exactly those the server sometimes omits.
- **E5. The spec is checked by reflection.** A test compares each variant's derived schema with its
  `spec/events` payload (fields, optionality, types; aliases as enums) and requires every event of MCP's own
  event files to be recorded.
- **E6. Not shared.** No other crate keeps events: the CLI has no sink (ADR 0022 D6), the LSP announces
  `lsp_initialized` as an editor log message, ops prints `batch_update_completed` and
  `extension_upgrade_completed` in its update document (ADR 0052).

## Consequences

- A long-running server's memory no longer grows with the requests it answered. Nothing on the wire
  changes.
- In tests every recorded event carries a `timestamp`, `mcp_initialized` included; payloads are otherwise
  unchanged.
- Adding an event: a variant, its spec payload (with `timestamp`), and the producer's `record` call; the
  reflection test checks the pair.
- `chrono` is a test-only dependency of `specforge-mcp`.

## Rejected

- **A bounded ring buffer in production**: memory no reader uses.
- **`tracing` or stderr**: neither the CLI nor MCP has a subscriber, and stderr from an MCP stdio server is
  shown to users as server noise; events would leave the process against ADR 0022 D7.
- **`McpServer<S: EventSink>`**: every handler generic for a choice made once per process.
- **Producers return events for the pipeline to record** (`Ran`, typed): a second channel beside the sink,
  and every refusal path must remember to carry them.
- **Events built lazily** (`record(|| McpEvent::…)`): a closure at 30 sites to save a string per request.
- **Trimming `timestamp` from the spec**: re-litigates ADR 0003; any future keeping sink needs the time.
- **Deleting events**: the spec's 25 event obligations are proven through them, and some facts (a
  subscription ended at disconnect) no reply shows.
- **A crate-neutral recorder**: one real user.

## What would reopen it

A client-visible record of what the server did (MCP's logging capability, `notifications/message`): it is
a third `EventSink` adapter, and E3's default may change; another crate that keeps events (then the sink
moves to a shared crate); an event whose payload the derive cannot state.
