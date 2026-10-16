# An export is written into the project by the export operation

**Status:** accepted (2026-10-16, architecture round 13, plan 09). Amends [ADR 0104](0104-containment-is-applied-where-the-operation-opens-the-file.md)
D4 (render's file), [ADR 0112](0112-every-project-file-an-operation-opens-goes-through-the-requests-containment.md) D4 (render's bound)
and [ADR 0102](0102-a-document-names-a-project-file-from-the-project-root.md) D2 (a name through a missing directory and
back). Notes [ADR 0092](0092-a-path-an-mcp-request-names-resolves-inside-the-served-project.md) D6.

## Context

`specforge.render` was the one surface module that wrote a project file itself. Its handler held the format to file
name table, built a bound with `Containment::within`, resolved the directory and the file, and wrote in place with
`Held::write`, failing as `internal_error`. Four containment fixes (`e2100971`, `ceff78dc`, `a760cb30`, `1776aba1`)
each edited that handler as well as ops. ADR 0104 D4 and ADR 0112 D4 named render as an accepted exception.

Four defects, each reproduced against the shared debug binary:

- a hard link at the output name was written through, so a file outside the project was overwritten;
- a directory that cannot be written, and an `out_dir` that names a file, were `internal_error`;
- `out_dir: "nope/../out"` was named absolute with the `..` in it, because `ProjectPath`'s canonical form stopped at a
  trailing `..` (kernel; every document that names a written file).

## Decision

- **D1. `export::write_into` writes an export into a directory of the view's project**, as
  `<dir>/<Format::file_name>`, through the caller's containment (`Directory { name, caller }`, ADR 0124). The directory
  and the file are both held before the export runs and before anything is created; the file is replaced with
  `Held::replace` (ADR 0112 D1), so a hard link's other name keeps its content and a failed write leaves the file as it
  was. It answers the file named from the project root beside the export's notices (`Noticed<ProjectPath>`, ADR 0130).
- **D2. Render is an adapter.** It parses the format and calls `export` or `write_into`. Its bound comes from the call:
  the `OutputView` input hands the handler the view and the call's `Caller` (`ViewToWrite::caller`), as `NewProject`
  does for init. No MCP tool writes a project file; outline's spec-root bound is the one bound a tool still builds.
- **D3. Render stays an output-artifact writer** (`Effect::WritesOutput`, `output_files`, no `files_written`; ADR 0022).
- **D4. The refusals**, all about `ExportField::Directory` (render's `out_dir`):

  | When | Kind | Code |
  |---|---|---|
  | the view has no root | `InvalidInput` | `directory_without_project` |
  | the directory or the file lands outside | `InvalidInput` | `outside_root` |
  | the name is, or goes through, a file | `InvalidInput` | `not_a_directory` |
  | any other failure | the I/O error's kind | `export_write_failed` |

  An unwritable directory is therefore `permission_denied` (about the environment, so it names no argument). The
  directory is checked before the export, so a call with a bad `scope` and a bad `out_dir` now gets the `out_dir`
  refusal.
- **D5. `ProjectPath` applies `..` where it stands**, so a name through a missing directory and back is named from the
  root.
- **D6. The write follows `Held::replace`'s rule** for a file the process may not write; render keeps no rule of its own.

## Rejected

- Bounding by the project's own root inside `write_into` (`Containment::within(root)`): it bends ADR 0104 D3 for one
  operation and would refuse a future CLI `--out-dir` outside the project.
- Making render a mutation (`Effect::Mutates`, `files_written`): it would rebuild a project whose sources did not
  change.
- `Writes` on the outcome: nothing reads it; collect, the precedent, returns the report's `ProjectPath`.
- Naming the file from `real`: an inside link would be renamed to its target, against ADR 0104 D2.
- A `WrittenExport` type: `Noticed<ProjectPath>` carries the file and the notices.

## Consequences

The hard-link escape is closed and the directory refusals have the right kinds. The containment refusal's wording is
ops'. ADR 0092 D6's `render --out-dir` names no CLI command: `specforge export` prints to stdout.

## What would reopen it

A second view tool that writes a file, or a CLI `--out-dir`.
