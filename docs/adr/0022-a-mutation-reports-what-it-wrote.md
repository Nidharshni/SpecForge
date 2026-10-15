# A mutation reports what it wrote, typed

**Status:** accepted (2026-10-06); amended 2026-10-11 (architecture round 6, plan 09, last section).
D7's paths amended 2026-10-15 by ADR 0102 (canonical outside the root, `/` separators).

What an MCP mutation did crossed the dispatcher seam as JSON. Each operation returned a typed
outcome; the handler built a `json!` reply from it; a per-tool `effect` closure in the tool table
read that reply back by key (`o["installed"]`, `o["success"]`, `o["migrated"]`), or ignored it and
returned 3. Enabling a builtin edited one file and reported three; init with a local extension
wrote five and reported three; a format that failed on one file reported none of those it wrote; a
migration reported the file it migrated and not its backup. Domain events came from three
mechanisms (`push_event` in a handler, `ToolOutcome::with_event`, the dispatcher), with payloads
that were not the spec's, and the refresh of the call target after a write was decided in three
places (`Call::wrote` in two handlers, a dispatcher fallback, `init`'s own `serve`). And since
`mcp_mutation_completed` never leaves the process, a stdio client had no way to learn which files
`add_extension`, `remove_extension`, `init` or `infer_session` wrote.

## Decisions

- **D1. Operations record their writes.** Every writing operation (format, rename, init, add,
  remove, migrate, and the inference session steps since the amendment below) returns the files it created, rewrote or removed and left so
  (`specforge_ops::Writes`), recorded at the write call. A write that changed nothing is not one; a
  file restored by a rollback is forgotten.
- **D2. A mutation handler returns what it wrote.** `Handler::Mutation` returns `Mutated`: its reply
  and a `Written { files, entities, events }` built from the operation's outcome, or nothing for a
  preview (`dry_run`, `check`, `diff`), decided from its typed arguments. No JSON is read back.
- **D3. One module acts on it.** `specforge_mcp::mutation::refresh` brings the call target up to date
  when files were written, whether or not the call then succeeded, by the call-target rules of
  ADR 0014; `report` records the domain event (on success) and then `mcp_mutation_completed`. No
  handler refreshes, pushes an event or reads `Call` state about writes.
- **D4. `files_changed` counts every file the call wrote**, backups and a failed call's partial
  writes included; `entities_affected` the entities it changed. A removal names each file it
  deleted, not the directory; an operation that fails after writing returns its writes on its
  `OpError` (`OpError::writes`).
- **D5. Domain events carry the spec's payloads** (`extension_added { extensionSpecifier,
  totalExtensions, wasDuplicate }`, emitted for a duplicate add too, and once per specifier of an
  add of several, in order, a specifier repeating an earlier one a duplicate (amended 2026-10-11:
  `Written` carries `events`, a list); `project_initialized
  { projectName, extensionCount, specFilePath }`).
- **D6. The CLI reads `Writes`, not `Written`.** It has no session to refresh and no event sink;
  `specforge init` lists the files it wrote from its outcome, and `init`, `add`, `remove` and
  `migrate` put them in their JSON output as `files_written` (a failed run's JSON error too, when
  it left files written).
- **D7. A mutation tells its caller what it wrote.** Events never leave the process, so every MCP
  mutation reply that is not a preview carries `files_written`: root-relative paths (absolute
  outside the root), sorted, in `structuredContent` or in a refusal's `data`; `[]` when it wrote
  nothing. `mutation::report` is its one writer, and `files_changed` is its length.

## Consequences

- `MutationSpec`, `Effect`, `ToolOutcome::with_event`/`take_events` and `Call::wrote` are gone; a
  new mutation tool cannot report a count it did not compute.
- An argument refusal of a mutation is reported as a failed mutation even with `dry_run: true`:
  arguments that do not parse cannot say they asked for a preview.
- A migration that fails after writing is brought up to date in the same call.
- `collect` and `render` stay management tools: their files are output artifacts, not mutations.
- The seven mutation tools' output schemas declare `files_written` (not required: a preview has
  none; the schemas stay open, so existing clients keep validating).
- Amends ADR 0014 ("The MCP call target"): handlers no longer call `Call::wrote`.
- (ADR 0024, round-5 amendment) A mutation is a tool spec whose effect is `Mutates`; its outputSchema's `files_written` derives from that, not from its schema text.

## Inference sessions (amendment, architecture round 4, plan 06)

`specforge.infer_session` was the one mutation whose writes no operation made. Its handler read
`specforge-infer.json` twice: once through the shared reader, whose manifest type had no sessions,
and once by hand for the `sessions` key. It merged that key back into the JSON it wrote, and built its
`Writes` itself. A session the second read could not parse was dropped from the file on the next write
(and the one-active-session rule did not see it). Every write dropped the keys the type did not define,
and paths were recorded as the agent spelled them, a file outside the project included.

- **S1. Session steps are an operation.** `specforge_ops::infer::session(&view, SessionStep)`
  (start, mark analyzed, end) reads the inference manifest once, applies the step, writes it once and
  returns what it recorded with its `Writes` (D1). A refusal writes nothing. The MCP handler parses
  `action` and `status` through the option tables `infer::SESSION_ACTION` and `infer::END_STATUS`
  (ADR 0027), calls the operation and renders today's replies; `Written` carries the operation's
  writes and, for a mark, the entities produced (D2).
- **S2. One manifest type, one reader, one writer.** `InferenceManifest` holds the sessions, each
  with a typed `SessionStatus` (active, paused, completed). Keys it does not define are kept at every
  level. The writer sorts keys, syncs a temporary file and renames it.
- **S3. An unusable manifest is E071 for every reader.** That covers the session steps, progress,
  gaps, the infer prompt's plan and `check --lint inferred`; the anchors manifest is refused the same
  way by navigation. No reader treats such a file as empty, and no write follows such a read.
- **S4. Recorded paths are root-relative.** A marked file and each source root are recorded
  root-relative with `/` separators; a path outside the root is refused.

Consequences: a manifest whose sessions do not parse is refused where it used to lose them; unknown
keys survive a write; `specforge.infer_session`'s unknown `action`/`status` refusals use ADR 0027's
wording; `infer_progress` and `specforge infer-status` list the sessions.

## One mutation outcome, sent once (amendment, architecture round 6, plan 09)

A handler stated three inputs (did it preview, its answer, what it wrote) through seven `Mutated`
constructors, `Replied::refused` and `replied`; migrate and format rebuilt the preview × failure square by
hand. A failure had two spellings (`Err` and `Ok(Mutated::failed(..))`), and converting an `OpError` any
way but `refused_after` dropped its `writes`. The reply was serialized before the pipeline acted on it:
`refresh` inserted rename's fresh diagnostics by key into a placeholder field the handler set to `None`,
and `report` inserted `files_written` by key, so `WrittenReply`, the type declaring it, was never built.
A second tool asking for fresh diagnostics would have failed only at run time, and format, whose reply has
its own `diagnostics` (W141/W142), would have lost them silently. And each handler decided "preview" from
its operation's answer, not its arguments: a dry run of an extension already installed from a registry was
reported as a duplicate add (`files_written: []`, `extension_added`, `mcp_mutation_completed`), against
"a dry run emits none".

- **M1. A run or a failure.** A handler returns `Mutation<R> = Result<Mutated<R>, Failed>`.
  `Mutated::new(answer, written)` is a run's one constructor; `Failed` is a failure's McpError and the files
  it left written, from an `OpError` (its `writes`), an `McpError` (nothing written) or
  `Failed::after(error, files)`; `?` keeps what an operation left written. A failure changes no entity and
  produces no domain event. (Refines D2.)
- **M2. The preview is the arguments'.** A mutation's arguments say whether the call only previews
  (`MutationArgs::previews`: `dry_run`; format's `check` or `diff` without `write`); the pipeline reads it
  before the handler runs. A preview is sent as it is and records nothing, whatever its operation answered.
  (D2's "decided from its typed arguments", now by construction.)
- **M3. The reply is sent once, typed.** It is a `WrittenReply<R, A>`: the tool's reply, `files_written`
  and its after-write report `A = <R as MutationReply>::After`, either `NoDiagnostics` or
  `FreshDiagnostics` (`diagnostics`: what `specforge check` reports for the target once a successful call
  brought it up to date; rename's contract). Its derived schema is the outputSchema; nothing is inserted
  into a reply by key; a tool reply may not declare a key the pipeline adds. (Amends D7: `mutation::report`
  is no longer the writer; the `WrittenReply` field is.)
- **M4. One function completes a mutation.** `mutation::complete` brings the call target up to date when
  the call wrote a file or its report reads a successful call's diagnostics, sends the reply checked
  against the tool's outputSchema (a reply it refuses still lists the files in `data`), then records the
  domain event (when the reply sent succeeded) and `mcp_mutation_completed`, whose `success` is what the
  client got. `mutation::refused` is a mutation refused before its handler ran. (Amends D3: `refresh` and
  `report` are its steps, no longer separate functions.)

Consequences:

- User-visible over MCP: `specforge.add_extension` with `dry_run: true` on an extension already installed
  from a registry is a preview: no `files_written`, no `extension_added`, no `mcp_mutation_completed`.
  Every other reply, outputSchema and event is byte-identical.
- `Mutated::{wrote, failed, preview, failed_preview, refused, refused_unless_preview, refused_after,
  with_diagnostics}`, `Replied`, `replied`, `refresh`, `report`, `Written::fresh_diagnostics` and
  `ToolOutcome::{with_field, set_field}` are gone. A new mutation tool states two impls beside its types
  (`MutationArgs`, `MutationReply`) and returns `Mutated::new` or a failure.
- The schema check now covers `files_written` and the after-write report too.

Rejected:

- **`Mutated::new(preview, answer, writes)`**: every handler restates a preview its arguments state; the
  dry-run bug was a handler deciding it from the operation's answer.
- **A `diagnostics` field on every mutation's `WrittenReply`**: format's reply has its own, and six
  schemas would declare a key never sent; under another name, rename's key would change for nothing.
- **Fresh diagnostics after every mutation that wrote**: the project's whole report after each format or
  inference step; the key still collides with format's.
- **A slot method on the reply** (`checked(&mut self)`): the handler still initializes the field, and a
  reply can declare it without the slot.
- **An erased deferred serializer** keeping `refresh`/`report` in the dispatcher: a `dyn` to carry a type
  the caller already knows, and the step order left in a comment.

What would reopen it: a mutation whose preview its arguments cannot decide; a second after-write report
that is not one more `AfterWrite` adapter; an MCP revision that carries such reports outside
`structuredContent`.
