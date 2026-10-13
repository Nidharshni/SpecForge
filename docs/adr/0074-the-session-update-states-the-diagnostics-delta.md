# A session update states the diagnostics it added and removed

**Status:** accepted (2026-10-13, architecture round 9, plan 04). Amends ADR 0024 D6 (the diagnostics delta is the
update's; MCP adds only its I017), ADR 0058 D3 and D7 (what a diagnostics delta compares; watch prints it) and ADR 0047
D1 (the compiled project memoizes its report).

MCP decided for itself what changed in the diagnostics. It copied everything the server reports before and after
each update a client heard, and compared the copies by code, message and file. A diagnostic that moved, whose span
end, severity, suggestion or data changed, or that was one of two identical ones, was "unchanged": no
`resources/updated`, no `specforge/diagnosticsChanged`, and the subscriber kept a stale location, against the spec's
"so a client can locate it". `specforge watch` had a second rule: `changed_diagnostic_files`, the files whose
graph-build diagnostics changed. It never named a file whose import or check diagnostics changed. The project
session, which both hold, said only what it reported now.

## Decisions

- **D1. A diagnostic is compared whole.** Two diagnostics are the same in a delta exactly when every part a surface
  presents is: code, severity, message, span (file, start, end), suggestion, data, origin (`Diagnostic`'s
  equality; it derives `Eq` and `Hash`). One that moved or changed in any part is removed where it was and added
  where it is. Identical diagnostics count one by one. Order is not part of it.
- **D2. The update states it.** `specforge_project::Update` holds what the session reported before it
  (`previous_diagnostics`) and reports after (`diagnostics`), both shared with the compiled project's memo of its
  report, and `Update::diagnostics_delta()` compares them (`DiagnosticsDelta::between`). The comparison runs only
  when a holder asks: an update nobody asks about costs no copy and no comparison (ADR 0024 D6 kept).
- **D3. The compiled project memoizes its report.** `CompiledProject` keeps what it reports, made on first use after
  each change and reset where its imports', graph build's or checks' diagnostics change. `diagnostics()` is unchanged.
- **D4. MCP forwards the update's delta.** `McpState::applied` sends the update's delta, then the I017 its rebuilt
  extension surface table added and removed (the table is MCP's, ADR 0017), and only when a client hears the
  diagnostics. MCP keeps no snapshot of its own.
- **D5. Watch prints it.** Every `rebuilt`, `rechecked` and `extensions_reloaded` line of `specforge watch --format
  json` carries `diagnostics_delta`; `changed_diagnostic_files` is gone from watch, the session update and the
  graph build.
- **D6. The LSP does not read it.** An editor's list per file is not a function of the diagnostics as JSON presents
  them. A spanless diagnostic is placed at its entity's name (ADR 0016) and moves with the entity while the delta is
  empty. Formatter findings, version labels and the anchor of a diagnostic about no entity change outside it. The
  LSP keeps publishing every file on every publish.

## Consequences

- User-visible over MCP: a moved diagnostic, a changed span, severity, suggestion or data, and the removal of one of
  two identical diagnostics notify; inserting lines above N diagnostics sends N removed and N added.
- User-visible on the CLI: watch's JSON lines carry `diagnostics_delta`; `changed_diagnostic_files` is gone.
- `McpState` keeps no last update; `ensure_fresh` returns nothing.

## Rejected

- **Code, message, file and start**: a changed suggestion or quick-fix payload would leave a client stale.
- **A `moved` list**: needs a pairing heuristic for identical diagnostics and a new spec type.
- **An eager delta field on `Update`**: the LSP and other-project sessions would compare on every update for no reader.
- **The session keeping its own copy of the report**: a mirror of the compiled project (ADR 0047 D2).
- **A sequence comparison that also notifies a reorder**: no reader; it would split `resources/updated` from
  `diagnosticsChanged`.
- **The LSP narrowing its publishes by the delta**: wrong for placed spanless diagnostics (D6).

## What would reopen it

An LSP pull-diagnostics client whose result ids want per-file change tracking; a client that needs moves paired; a
second report a server adds besides I017.
