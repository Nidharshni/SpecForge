# A document names a project file from the project root

**Status:** accepted (2026-10-15, round 10, plan 11). Amends ADR 0022 D7 (what `files_written` holds) and ADR 0052
(every file a document names follows one rule). Names ADR 0055 D1 (graph locations) and ADR 0092 D1 and D6 (input
spellings) as its boundaries.

The JSON documents the CLI and MCP print named a project's files in whatever spelling reached the field.
`specforge format --format json` named a changed file as the user typed it (cwd-dependent: five working directories
gave five spellings), absolute when no path was named or over MCP, and root-relative in `files_written`, all in one
object. `collect`'s `report` was absolute in the root's spelling, so the CLI and MCP printed different bytes for a
root reached through a symlink. Render and migrate named files absolute beside a root-relative `files_written`.
`Writes::names_under` printed the OS separator and kept an outside path as recorded, against ADR 0022 D7 and S4. The
parity harnesses replaced both root spellings with `[ROOT]` before comparing, so no test saw any of it.

## Decision

**The rule.** A field of a surface document that names a file or directory an operation opened by path or wrote holds
a `specforge_common::ProjectPath`: relative to the root of the project the call acted on, `/` between components; a
path outside that root (only the CLI can name one, ADR 0092 D6) is absolute and canonical. A document's anchor
directory (`init`'s `project_root`, `watch`'s `spec_root`) is absolute. A location the graph declares (a span, a
navigation result, an export node, rename's plan) is named by its source key, relative to the spec root (ADR 0055
D1). The CLI's human output names files as the user typed them.

- **U1. The base is the project root.** Fifteen fields change: format (`changed_files`, `failed_files`,
  `diffs[].file_path`, `failures[].file`, W142's `span.file` and `file`), `files_written` at its two sites, collect's
  `report`, render's `output_files`, migrate's `results[].file_path`, `backups[].original_path`,
  `backups[].backup_path`, `diffs[].file_path` and the rollback results. It is the base `files_written`, the inference
  documents (ADR 0022 S4) and init's members already use, the only one both surfaces share (MCP has no cwd), and it
  does not move with the cwd or the root's spelling.
- **U2. Graph locations keep the spec root.** One file has two spellings across commands by rule (`a.spec` in
  `check`, `spec/a.spec` in `format` for an `init`ed project). Moving source keys to the project root touches the
  session's keys, the LSP's `key_of`, the anchors manifest and every span in every snapshot: a round of its own.
- **U3. W142's `span.file` in the format document is the project path,** like every other file field of that
  document: a format target need not be a source of the root's compile. The LSP publishes by URI and is unaffected.
- **U4. `rename`'s `affected_files` keeps the source key.** It is the set of `edits[].file`, graph positions.
  `files_written` is the write record, `affected_files` the plan's files.
- **D1. Separators.** A relative project path joins its components with `/` on every OS. An absolute one keeps the
  OS's spelling.
- **D2. Outside the root.** Absolute and canonical as far as it exists, never "as recorded". Over MCP every named path
  resolves inside (ADR 0092 D2).
- **D3. Anchors stay absolute.** They are what the relative names are read against. No document gains a
  `project_root`.
- **D4. The CLI's human output keeps the typed path.** Prose inside a document (format's `message`,
  `failures[].message`, migrate's failure message) names a file as the document's fields do. Other prose is text.
- **D5. The type lives in `specforge_common`,** beside `ProjectRoot`: every layer above uses it, including
  `specforge-migrate`, and it is not a blob input. It serializes as a string, so every outputSchema is unchanged.
- **D6. MCP's input spelling is ADR 0092's and stays.** `Containment::spelled` is what the operation opens; the
  document no longer echoes it.
- **D7. A consumer that reopens a named file joins it onto the root** (`ProjectPath::under`): migrate's record,
  writes bookkeeping, hook input. The hook input keeps today's absolute bytes.

## Rejected

- Absolute and canonical everywhere: reverses ADR 0022 D7 and puts machine paths in every document.
- As the caller spelled it: MCP has no cwd, so the surfaces cannot agree, and one file keeps several spellings.
- The spec root for these fields: `specforge.json`, `specforge-report.json`, `.spec.bak` and `out/` are not under it.
- A `&ProjectRoot` parameter instead of a type: the MCP pipeline holds `Option<&Path>`; and `serialize_with` on
  `String` fields, where the type would not say what the string is.

## What would reopen it

A surface that must name a file for a reader with no project (a document with no root to be relative to). Moving
source keys to the project root, if a second base for graph locations proves a cost.
