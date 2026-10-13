# One MCP request pipeline; resources read through the view

**Status:** accepted (2026-10-06); D6 amended and D8 added (2026-10-08, architecture plan 12); a tool spec declares what it does once (2026-10-09, architecture plan 10, last section); A1's tool spec is one module implementing `CoreTool` (ADR 0049). D1's "record its events" step is a handler recording through the call's event sink; `Ran` is gone (ADR 0057). D6 amended by ADR 0058 (the deltas are sent whole); D6's diagnostics delta is the session update's (ADR 0074), MCP adds its I017 and keeps no snapshot. Amends
ADR 0004 "MCP tool table" (D4-e) and ADR 0017 D6 and D14's resource code.

`tools/call`, `resources/read` and `prompts/get` ran the same steps in three functions: read the
request, look the name up, record the invocation, resolve the call target, run the handler, drain
its events, build the reply. `prompts/get` called itself "the prompt-side twin" of `tools/call`. The
copies drifted:

- One failure had four shapes. A missing entity was an `isError` McpError from a tool, a JSON-RPC
  error with the McpError in `data` from a prompt, and a bare `-32602` string from a resource
  ("E003: …" as text from `graph?root=`, "Entity not found" with no E003 from `graph/{id}`), against
  `mcp_structured_error_responses`. A prompt's `file_not_found` on `path` was `-32603`.
- `tools/call` looked an extension tool up before bringing the served project up to date, and an
  unknown name never refreshed it: a tool enabled in `specforge.json` stayed "Unknown tool" until
  another request reloaded the environment. `resources/read` did it in the other order.
- Five of eight core resources read the server state instead of the call target and called the
  emitter themselves, restating the export schema policy. `specforge://graph/{id}` was a Graph
  Protocol 1.0 document without the `schema_ref` a scoped export carries; `?scope=`, the spec's
  parameter, was ignored, and so was every other part of a query the read did not know.
- `path` and `use_cached` were declared by nine schema properties, eleven handler fields kept only
  for a drift test, and that test.
- The two subscription eras wrote "the diagnostics URI changes with the diagnostics, every other with
  the graph" twice, computed the diagnostics delta twice, and `resources/subscribe` accepted URIs the
  server does not serve.
- Operations failed with code strings that MCP mapped with an 11-entry table; 8 slugs and 11
  diagnostic codes fell to `internal_error` (a token budget too small among them), and rename,
  validate and remove special-cased their own.

## Decision

**D1. One pipeline, three adapters.** `specforge_mcp::surface_call::serve::<S: Surface>` runs every
step once; `Tools`, `Resources` and `Prompts` supply lookup, target, handler, events and envelope. The
router guards initialization once for every method that needs a session. The no-project rule
(ADR 0025) is applied in the pipeline, to whichever refusal an outcome holds.

**D2. Extension entries are looked up in an up-to-date project.** A name no core table has brings the
served project up to date before the extension surface table is asked (ADR 0014 D12 for extension
names). This is one decision with one owner: `surface_call` (`find` for a lookup, `listed` for a
listing, both through one private `bring_surface_up_to_date`). `resources/subscribe` and
`subscriptions/listen` ask `find::<Resources>` before they refuse or honour a URI, and the four
listings run inside `listed::<S>`, so no other code calls `ensure_fresh` to decide whether an
extension entry exists. (A call's *target* still brings its project up to date, D5: that is a
project's freshness, not the table's.) `prompts/list` no longer reloads: no extension declares a
prompt.

**D3. A resource read refuses like a prompt.** Every refusal is an McpError, sent as the JSON-RPC
error's `data`; McpError gains `uri`, the URI read. One code rule for both
(`McpError::into_rpc_error`): `-32602` when the error names an argument or its code is client input
(`ErrorCode::rpc_code`), else `-32603`. "Unknown tool" and "Unknown prompt" stay plain `-32602`. A
resource that does not exist ("Unknown resource URI", or an entity its URI names) is `-32002` in a
handshake session and `-32602` in a 2026-07-28 request, each revision's server/resources "Error
Handling", with `data.uri`. A no-project refusal names `path` only where the entry takes one: a
resource or a prompt, which refuse a `path`, answer `-32603` without asking for one. With nothing
served, a not-found refusal is the no-project refusal (ADR 0025); an extension resource read with no
project served would be `-32603` (ADR 0017 D14 said `-32602`), a case a real server no longer reaches.

**D4. Graph views are exports.** `graph`, `context`, `brief` (scoped by their template, or by `scope`,
alias `root`) and `graph/{entity_id}` (depth 1) are `ops::export` over `call.view()` under the export
schema policy, byte-equal to `specforge export`. The entity list is `specforge.list`'s function; the
diagnostics are what the view reports. A read answers under the URI it was asked for. A query key the
read does not know, a repeated key, a value that does not parse, `scope` with `root`, a scope on a
templated URI or `depth` without a scope is refused as `invalid_input` naming the key; keys and values
are percent-decoded, and a count is read by ADR 0033 D2's rule. *(Amended, round 5 plan 15:
`specforge export` and `specforge.export` take `depth` and `kinds` too, and `specforge.export` takes
`schema_version`, so the byte-equality holds for every key.)*

**D5. The call target declares its arguments.** `TargetSpec` contributes `path` (by reach) and
`use_cached` (by freshness) to the listed schema and to the field set the drift test reads; handlers
no longer carry them; `init`'s missing `path` is refused by the target.

**D6. One subscription module, one rule.** *(Amended 2026-10-08.)* `specforge_mcp::subscriptions::Subscriptions`
holds who hears about what: the resources the client subscribed to (`resources/subscribe`), its
`subscriptions/listen` streams and the notifications queued for it. It is `McpState`'s one field for
it, and only the subscription requests, an update, cancellation, disconnect and shutdown change it.
A subscription is to one resource: unsubscribing one leaves the others. `Watched::of(uri)` says what
a resource's content changes with, for both eras: `specforge://diagnostics` with the diagnostics,
`specforge://schema` with the environment, every other resource (the graph views, an extension's)
with the graph or the environment. `McpState::applied`, the one place every update of the served
project passes (ADR 0035 D3), hands the update's changes (`Changes`, computed once; the diagnostics
read only when someone hears them) to `Subscriptions::updated`. In this order:

1. each listen stream hears `notifications/resources/updated`, tagged with its id, for each resource
   it names that changed;
2. the client hears `notifications/resources/updated` for each subscribed resource that changed;
3. it hears `specforge/graphChanged` when the graph delta is not empty and it subscribed to a
   resource that changes with the graph;
4. it hears `specforge/diagnosticsChanged` when the diagnostics changed and it subscribed to them.

A listen names each resource once. Subscriptions belong to the connection: no request parameter
names another client. Its end (`McpServer::disconnect`) and `shutdown` end every subscription and
stream the same way, each removal recorded. `resources/subscribe` refuses an unserved URI as
`resources/read` does (not found); `unsubscribe` never refuses.

**D7. Operations fail with a kind.** `specforge_ops::OpErrorKind` is set where a failure is raised;
MCP maps it totally to its McpErrorCode. A catalogued code is raised with its kind
(`OpError::coded`), and a lower crate's diagnostic crosses with the kind its boundary states (`OpError::of`);
no table maps a code to a kind (amended by ADR 0054). The code stays what the CLI prints.

**D8. A request carries its revision.** *(Added 2026-10-08.)* The revision a request is served under,
`lifecycle::Revision` (the one `initialize` negotiated, or the stateless one its `_meta` names), is
passed from `McpServer::handle_message` through the router to the pipeline's `unknown` and
`envelope`, the tool listing and `resources/subscribe`. `McpState` keeps only what `initialize`
negotiated, and `is_initialized` means the handshake happened. Nothing sets and resets a per-request
field around a request.

## Consequences

- User-visible over MCP: a just-enabled extension tool is callable at once; core resource errors
  carry an McpError with `uri`, and a missing resource is `-32002` to a handshake client; a prompt's
  bad `path` is `-32602`; a prompt or resource that needs a project says "start the server in a
  project" instead of asking for a `path`; `graph/{id}` is Graph Protocol 2.0 with `schema_ref`;
  `?scope=` works; a malformed or unknown resource query is refused, no longer ignored; queried reads
  answer under their own URI; `specforge://graph`'s key order is the export's; `use_cached` has one
  description; subscribing to an unserved URI is refused as not found; E062, `export_failed`, the
  R-RES/R-TRUST/R-OPS codes, E033 and failed writes get specific McpError codes (`permission_denied`
  when the OS refused).
- The CLI's output is unchanged.
- An extension call stats the project twice per request (lookup, then target): ~6.5 ms per 1 000
  files.
- *(2026-10-08)* User-visible over MCP:
  - a handshake client that subscribed to a resource hears `notifications/resources/updated {uri}`
    when it changes, before the SpecForge delta;
  - unsubscribing one graph view keeps the client's other subscriptions;
  - an environment reload (a changed `specforge.json`, lock or extension module) is heard by
    `specforge://schema`, the graph views and extension resources in both eras;
  - a listen that names a resource twice is acknowledged and notified once;
  - `client_id` is no longer read.

  In the event log, `mcp_subscription_created`/`_removed` carry `resourceUri` (and `subscriptionId`
  on a stream), and shutdown records and counts the end of a listen stream.

## Rejected

- **A `match` on the request kind at each step**: three outcome types at every step.
- **`precondition_failed` → `-32602` in the code table**: changes every prompt that needs a project
  to keep one resource case.
- **Refusing `resources/unsubscribe` of an unserved URI**: a client could not drop a subscription to a
  resource its extension stopped serving.
- **Listings folded into the pipeline's `serve`**: ADR 0017 already makes listing and dispatch one
  table; the listings differ in ways a shared method would have to special-case. Only their freshness
  decision is shared (`listed`).
- **Ignoring what a resource query does not read** (partial queries still serving): an agent that
  sends `graph?scop=a` got the full graph and could not tell; the export tool refuses an undeclared
  argument, and the resource over the same function now does too.
- **A client id per subscription** (the `client_id` request parameter): every notification went to
  the one connection's queue whatever the id; the parameter only let a connection subscribe as
  another and outlive `disconnect`. A transport with several connections gives each its own
  `Subscriptions`.
- **Only `notifications/resources/updated` in the handshake era**: the deltas are what
  `notify_graph_delta_via_mcp`'s clients read instead of re-reading the graph.
- **Only the SpecForge deltas** (the state before 2026-10-08): a standard client that subscribed
  never heard of a change.
- **Comparing the schema before and after a reload**: a reload is rare, the notification means "may
  have changed, read again", and the comparison would export the schema twice and still miss an
  extension resource whose module changed.
- **The request's revision as a field of `McpState`**, set and reset around the request: a
  set/reset protocol with a panic-path reset, and `Surface::envelope`/`unknown` took the whole state
  to read one value.

## What would reopen it

A fourth request kind that invokes a named entry (completion, an MCP revision's new method) that the
`Surface` trait cannot express without a method most adapters leave empty; or an MCP revision that
changes the not-found code again, or a transport with several connections per server (one
`Subscriptions` per connection), or an MCP revision that changes how a resource change is notified.

## A tool spec declares what it does once (amendment, architecture round 5, plan 10)

D5 made the call target declare `path` and `use_cached`. Whether a tool writes was still stated five times on its
entry: its category, its access, its target's reach (`WritesAnyProject`, which behaved exactly as `AnyProject` once
in-memory serving was gone, ADR 0025), its handler kind, and the `files_written` property added by hand to seven output
schemas. Whether a call answered with nothing served was stated nowhere: it was whichever accessor the handler called
(`call.view()` answered over the empty session, `call.project()?` refused, and `infer_progress` and `infer_gaps`
answered documents of their own making). Init's "not inside the served project" was checked by the target and, out of
reach, by `ops::init` again. Four annotations were wrong, and extension tools had none, so MCP's defaults called 40
read-only product queries destructive and open-world.

- **A1. One effect.** A tool spec is a name, a description, an output schema and an `Effect`: `Reads { group }`,
  `WritesOutput { group, hints }` (collect, render: ADR 0022's output artifacts) or `Mutates { hints }`, each holding
  its handler. The category (`mutation` exactly for `Mutates`, else the group), the annotations (`readOnlyHint` for
  `Reads`, else the hints) and a mutation's `files_written` output property derive from it.
- **A2. The handler's input is the target.** A handler is given nothing (explain), the project view, the project
  (`ProjectRef`), or the directory init creates (`&Path`, with the runtime its extensions' declarations are read in),
  then its typed arguments, never the `Call`. The target derives from that variant: `TargetSpec::{Unscoped,
  Project { target, without }, NewProject}`, `Reach::{Served, AnyProject}`. `WritesAnyProject` is deleted. Core
  prompts and resources are given the view.
- **A3. With nothing served, the target decides.** An entry given the view reads the empty session (ADR 0014 D7, ADR
  0025 D1, which stay); an entry given the project is refused as no project by `resolve`, before its undeclared and
  typed arguments are checked, like the target's other refusals. The rule the table follows: an entry that reads only
  the project view answers; one that reads or writes at the project root beyond the view, runs the project's
  extensions, or takes a `path` is refused. `infer_progress` and `infer_gaps` now refuse.
- **A4. Init's inside-the-served-project rule is the target's** (ADR 0014 D6). `ops::init` has no `forbid_inside`;
  the CLI allows a nested project, as `scaffold_new_project` says.
- **A5. Annotations say what a tool does.** A repeat that is refused and writes nothing is idempotent (rename, as
  remove already was); a tool that may overwrite is destructive (add_extension replaces a module, infer_session a
  record). Init is additive: it refuses an existing starter file, and a failed init puts back what was there. Every
  extension tool is annotated `readOnlyHint: true, openWorldHint: false` (ADR 0037 D1), and is listed in the group it
  declares, never as a mutation.

Consequences (user-visible over MCP): with nothing served, `infer_progress` and `infer_gaps` are `precondition_failed`;
a call that needs a project is refused for it before an unknown or malformed argument is; `add_extension` and
`infer_session` are `destructiveHint: true`, `rename` `idempotentHint: true`; extension tools carry annotations, and
one declaring `mutation` is listed `core`. On both surfaces, init refuses a directory whose `spec/hello.spec` exists.
No handler can read the server state or refuse "no project" itself.

Rejected: a `without_project` field beside the handler (two statements that must agree); refusing after the arguments
are read (a project handler could then observe a missing project); letting extensions declare annotations (they hold
no capability, so only read-only is true); answering extensions, providers and doctor over the empty session (a
healthy report of no project misleads).

Reopen if: ADR 0037 grants an extension a capability (its tools' annotations then come from its declaration); a tool
both writes project files and must not report them as a mutation; a prompt or resource needs the project on disk.
