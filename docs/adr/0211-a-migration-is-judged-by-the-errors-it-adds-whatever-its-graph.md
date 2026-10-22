# A migration is judged by the errors it adds, whatever its graph

**Status:** accepted (2026-10-22, architecture round 23, plan 01). Amends [ADR 0186](0186-a-migration-is-judged-before-it-is-written.md) D6 and closes its "Left open (04-D3)"; amends [ADR 0151](0151-a-spec-file-is-changed-by-a-typed-edit.md) D3 (a moved diagnostic is the same diagnostic).

## Context

- `Verdict::refusal` returned `None` for a migration whose graph was unchanged before it read the errors the migration
  adds, so `migrate` could write a project that `check` then fails (04-D3).
- Round 22 declined to reorder the two tests: `edit::validate::added` keyed a diagnostic by its rendered message, and
  E002 and E078 name a source position in theirs, so a migration that only moves lines in a file reporting a duplicate ID
  would be refused for an error it did not add. The same key was live in typed edits: `specforge set` above a duplicate ID
  was refused with E080 for an error the project already had.

## Decisions

- **D1. The key collapses digits.** `added` pairs a diagnostic of the edited compile with one of the checked report by
  code, severity, file and the message with each run of ASCII digits collapsed. One private function in ops; no
  kernel type, no re-vendor. Typed edits and migration share it.
- **D2. `refusal` reads errors before the delta.** A hook that failed; else errors the planned project has that the
  project has not (`migration_does_not_compile`); else a changed graph (W054s); else kept. A kept migration may still
  carry errors the project had.

## Rejected

- A typed subject on `Diagnostic` (`DiagnosticData::DuplicateId { id }` and kin): exact for core codes, but it touches
  the kernel (a blob input, a re-vendor) and extension messages still compare by text.
- Counting by code and file with no message: E080 could not name each new error by pairing, and any swap of two errors
  of a code in a file would be invisible.
- The key as it was; a second comparison inside the migration judge.

## Consequences

- Breaking: a migration that adds an error and keeps its graph is refused (CLI exit 1, MCP `isError`, nothing written); a
  typed edit, MCP `edit` or LSP quick fix refused for a moved E002/E078 is written.
- A one-for-one swap of two errors with the same digit-collapsed message in one file is invisible; an added second one is
  still counted.

## What would reopen it

- A swap that slips past a judge is reported; or an extension whose messages differ only in digits for different subjects.
