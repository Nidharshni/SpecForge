# An operation's outcome is one document both surfaces print

**Status:** accepted (2026-10-11). Amends ADR 0015 D6 and ADR 0004 D1-c; extends the list in
ADR 0048 D2. D5 extended by ADR 0058 to watch's lines. D1 and D5 amended in round 16 (plan 11); D1 and D2 in round 17 (plan 05): rename's document. D1 amended in round 18 (plan 10): the extensions, providers and schema listings answer their documents.
D8's "nothing goes to stderr" is a case of [ADR 0120](0120-what-a-cli-command-says-beside-its-document-is-said-once.md).
Amended by [ADR 0162](0162-a-mutations-preview-is-a-fact-of-its-outcome.md) (round 16, plan 05): every previewable document says `dry_run` (D4, D8's `check_only`).
ADR 0102: every file a document names follows one rule. D1's documents include the anchors read view's ([ADR 0140](0140-source-anchors-are-a-read-view.md)).

ADR 0048 typed every MCP reply, and its round typed the documents both surfaces already shared
(analyze, collect, providers, doctor, progress, gaps, trace's chain, schema). Six other outcomes
were still mapped to JSON by hand on each surface: add, remove, init, stats, the extensions
listing and migrate. Across the six, 53 keys appeared on one surface only: the extension was `name` on the CLI and
`extension` over MCP; an unsigned package's `key_id` was `null` on one and absent on the other;
stats had 14 keys on the CLI (with `proof_pct: null`, which the spec says is absent) and 6 over
MCP; init printed absolute paths under one set of names and relative ones under another. Each fix
to those mappings landed two or three times. `update` repeated add's publisher fields; the CLI
spliced doctor's credentials and infer-status's sections into documents by string key. ADR 0015
D6 had kept stats' two shapes on purpose, citing ADR 0004 D1-c's two diagnostic presenters, which
had since converged into one additive shape; and `ProjectStatistics`, the CLI's published stats
type, matched neither surface.

## Decision

- **D1. One document per outcome, in ops beside its operation.** It derives `Serialize` and
  `Shape` and carries no `null`. It is the outcome itself when the outcome is plain data
  (`stats::Stats`), else the outcome's `document()`: `AddDocument` (a union tagged by `action`:
  enabled, installed, already_present, planned), `RemoveDocument`, `UpdateDocument`,
  `InitDocument`, `ExtensionsDocument`, `MigrateDocument` (one record, with `pending`), and the
  read views' `ExplorationDocument`, `ReviewDocument` and guide documents. The CLI prints it under
  `--format json`; MCP's reply is it (`pub use … as Reply`).
- **D2. A surface adds only its own extras, typed, flattened beside the document.** MCP: the
  add's `note`, a mutation's `files_written` (ADR 0022 D7). CLI: `files_written` (ADR 0022 D6,
  `specforge_cli::outcome::Written`), doctor's credential health (ADR 0029 D3a; ops never names
  the credential store, ADR 0010), infer-status's `--gaps` and `--gaps-detail` sections. No
  surface renames, drops or re-derives a key.
- **D3. One vocabulary.** A single extension's document names it `extension`, list rows `name`;
  what an extension operation did is `action`; counts are `<thing>_count`; per-kind counts are
  `entity_counts: [{kind, count}]`; diagnostic tallies are `diagnostic_summary {errors, warnings,
  infos}`; a publisher's acceptance is `unsigned | pinned | pinned_now | repinned`, with `key_id`
  exactly when signed.
- **D4. A converged document keeps every fact either surface printed**, and drops only what a tag
  already states (`installed`, `already_present`, `dry_run`, a branch `message`).
- **D5. The rule covers every printed outcome**, update's included, though only the CLI prints it.
- **D6. The spec type of a document is its tool's `Mcp*` reply type** (ADR 0048 D8 holds it);
  `ProjectStatistics`, `EntityKindCount` and `InitOutput` are deleted.
- **D7. The CLI parity harness compares documents** (`Document` aspect): none differs (D8).

## Consequences

- `specforge stats --format json`: `entity_count`, `entity_counts` (array), `edge_count`,
  `diagnostic_summary`, no `proof_pct` without results. `specforge.stats` gains `entity_count`,
  `testable_count`, `declared_count`, `verified_count`.
- `specforge add --format json` and `specforge.add_extension`: `action` (`enabled`, `installed`,
  `already_present`, `planned`), `extension`, `source` always (`registry` for a registry
  package), `key_id` only when signed; MCP loses `installed`, `already_present`, `dry_run` and the
  already-present `message`, and its `note` is on every reply but a preview.
- `remove`: `action`, `extension`, `source` and `version` whenever known, on both surfaces.
- `init`: `project_root` (absolute), `name`, `version`, `config_file`, `starter_file` (relative),
  `extensions_installed`, on both surfaces.
- `extensions`: `extensions`, `count`, `lock_file_entries`, `entity_kinds_in_graph`.
- `migrate`: one record (`pending`, `files_migrated`, …, `backups`) on both surfaces; MCP's
  `Current`/`Ran` union and the CLI's `MigrationSummary` output are gone.
- `update --format json`: `{ok, extensions: [rows by status], extension_upgrade_completed: [one
  per extension an applied update moved], batch_update_completed}`; a failed update prints it (no
  upgrade completed) and exits 1, instead of the first failure's error document.
- `specforge infer-guide --format json` and the infer prompt leave out a kind's missing
  `description` instead of writing `null`.
- CLI JSON keys follow the document's field order rather than alphabetical order.

## Rejected

- **Sharing only the field derivation, two shapes kept**: the names were what drifted, and each
  outcome would keep two spec types.
- **Doctor's credentials in `DoctorDocument`**: ops would name the user's credential store, and MCP's
  schema would list a key it never sends.
- **A generic `With<D, X>` wrapper**: every surface's extras are one or two keys; a local struct
  says what they are.
- **Renaming the shared spec types off `Mcp`**: a rename of some forty types, left for a round in
  which nothing else edits `mcp.spec`.
- **A CLI JSON for `specforge format`** in this round: a new feature, not a convergence. It
  followed in the round 6 follow-ups (D8).

## Amendment (round 6 follow-ups)

- **D8. `specforge format` takes `--format human|json`** (ADR 0029 D5b). Its document is
  `specforge_ops::format::FormatDocument` (`Outcome::document`, spec type `McpFormatResult`), which
  `specforge.format` answers and a failed call carries in its `data`; the CLI adds only
  `files_written` (D2), absent when the run only reports (`--check`, `--diff`). Under JSON the W141
  and W142 diagnostics and the failed files are the document's and nothing goes to stderr; the exit
  stays the run's verdict (`--check` with a change exits 1). `--stdin`, whose output is the
  formatted text, refuses `--format json` (`invalid_input`, the error document).

## What would reopen it

A consumer pinned to one surface's shape that extras cannot serve; or an outcome whose printed form
must differ per surface for a reason the surface alone knows, beyond adding keys.

## Amendment (2026-10-11): an add of several specifiers is one document

`specforge add` takes one or more specifiers, and `specforge.add_extension` a `specifiers` list
(its single `specifier` is gone), added as one change (ADR 0041's lock plan, judged on the whole
set; all or nothing). D1's `AddDocument` is now `{added: [AddResult]}`: one entry per specifier,
in the order given, each the union tagged by `action` that D1 described (enabled, installed,
already_present, planned), plus `duplicate` (`duplicate_of`: the earlier specifier of the same add
that names the extension the same way), and every entry names its `specifier` as given. MCP's
`note` (D2) sits beside `added`, once per reply; the CLI's `files_written` lists the files of the
whole set. The spec type is `McpAddExtensionResult { added: McpAddedExtension[], note? }`.

## Amendment (round 7, plan 16): explain is one document

- **D9. `specforge explain` is an operation** (`specforge_ops::explain`) whose outcome,
  `Explanation` (spec type `McpExplainResult`), `specforge.explain` answers and
  `specforge explain --format json` prints. Both surfaces read the code the same way (any case,
  trimmed) and refuse an uncatalogued code with the operation's `unknown_code` (MCP:
  `invalid_input` on argument `code`). The human rendering ends with the docs link the document
  carries. The CLI's refusal document follows D1's rule too: `suggestion` is absent when the
  refusal has none, never `null`.

## Amendment (architecture round 16, plan 11): the CLI-only documents

- **D1's list** gains `ValidateDocument` (beside `extension::validate`, publish's first two steps), `SearchDocument`
  (`Searched::document`) and `PublishDocument` (`PublishOutcome::document`: action `published`, the extension named
  `extension`, no constant `signed`). They were `json!` literals in the CLI, with a `null` (search's `contributes`),
  `name` for the extension (D3) and a second and a third diagnostic shape.
- **D5's reach.** D5's rule holds for the outcome of an operation ops performs. A command with no operation in ops
  keeps its document in its command module, under D3's vocabulary and with no `null`. Those commands are
  `extension init` and `build` (a scaffold write, a cargo build) and `login` and `logout` (the credential store,
  which ops never names: ADR 0010, and Rejected "Doctor's credentials").
- **A document's diagnostics** are `DiagnosticList`, the one shape every surface prints.
- Terminal text is the CLI's: `PlanOutcome::to_human` and `TraceOutcome::to_human` left ops.

## Amendment (architecture round 17, plan 05): rename's document

- **D1's list** gains `RenameDocument` (`Renamed::document`, beside `rename::run`): `old_name`, `new_name`,
  `affected_files`, `dry_run`. MCP is the only surface that prints it (D5).
- **D2's MCP extras** gain rename's `edits`: the spec's `TextEdit`, which MCP spells (ADR 0164 D3: no `Serialize`
  on ops' `TextEdit`), flattened beside the document as the add's `note` is.

## Amendment (architecture round 18, plan 10): an outcome that is only its document is the document

- **D1's test.** An outcome whose every field reaches its document, and that no reader reads past its document, is
  that document: the operation answers it. A `document()` stays where the outcome holds what its document does not
  print (the files a mutation wrote, a typed fact a human rendering matches on, a verdict, a borrowed view) or
  where the document needs more than the outcome (a request's field, a clock).
- **D1's list.** `extension::list` answers `ExtensionsDocument`, `extension::providers` `ProvidersDocument` (a
  provider's settings stay the environment's and are not listed), `read::schema::schema`
  `Noticed<SchemaDocument>`; `ExtensionListing`, `ProviderListing` and `SchemaOutcome` are gone.
- **D5's update.** `UpdateOutcome::batch_update_completed(ended_at)` answers the one `batch_update_completed`
  payload (`BatchUpdateCompletedPayload`); `BatchUpdateEvent` is gone.
- No printed document changed.
