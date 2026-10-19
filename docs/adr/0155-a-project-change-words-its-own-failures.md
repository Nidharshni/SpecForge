# A project change words its own failures

**Status:** accepted (2026-10-19, architecture round 15, plan 03). Amends [ADR 0128](0128-one-project-change-over-held-files.md)
D2. Builds on [ADR 0021](0021-formatting-is-one-operation.md) D7 (its two file codes become every project change's) and
[ADR 0126](0126-a-lock-plan-writes-the-specforge-json-entries-it-judged.md) D3 (its `file_changed` code; the wording of
its refusal).

A failed project change handed each caller a `Cause`, and rename, the typed edits and the lock plan each wrote their
own four-arm match from cause to `OpError`. The codes drifted, all reproduced with a read-only file or directory:

- "a project file could not be written" was `file_unreadable` for rename and every typed edit, `config_write_failed` or
  `init_write_failed` for the lock plan (two codes inside one `init`), and `file_unwritable` for format, the only one
  ADR 0021 D7 documents.
- A *read* failure inside `init` was worded "cannot write" with a write code.
- "A file changed" was worded four ways, and the file was named by absolute path in some messages and from the spec
  root in others.

`CONTEXT.md` states the rule this broke: an operation's failure kind is stated where it is raised, never by a table of
codes.

## Decision

- **D1. One code and kind per cause, raised and worded in `ops::change`.** `Changed` is Conflict, `file_changed`;
  `Unreadable` and `Unwritable` are the OS error's kind with `file_unreadable` and `file_unwritable`. The three codes
  are `FILE_CHANGED`, `FILE_UNREADABLE` and `FILE_UNWRITABLE`, declared in the module and re-exported at the crate root.
- **D2. `ProjectChange::new(root, doing)`; `commit` and `commit_then` return the `OpError`.** The message is
  `"{doing}: {what}; {tail}"`, the file named from `root` (as spelled when it is not under it), the tail
  `nothing was written` or `{files} could not be put back`; the files left are the error's writes. A last step's own
  error is returned as it is, with the files left. `Failed` and `Failed::error` are deleted.
- **D3. The checks that run before a change** (`Opened::unchanged`, init's stage reads) word through
  `change::changed` and `change::unreadable`, so their messages read the same.
- **D4. `config_write_failed` and `init_write_failed` are deleted.** An unwritable `specforge.json`, `.gitignore` or
  starter is `file_unwritable`, like every other project file; the message names the file.
- **D5. Format shares the codes only.** It stays per file (ADR 0128 D5): one locked file must not stop the run.

## Rejected

- **A describer beside `Failed::error`**: two ways out, and a caller could still word it its own way.
- **A source-change entry point** (`write_sources`): after D2 the shared part is four lines; it would be about as long
  as the code it removes.
- **Per-file codes** (`stage_coded`): no spec or doc names the two codes, MCP never shows them, and the message names
  the file.
- **Format on the change**: one locked file would stop the whole run.

## Consequences

- CLI: `set`, `new`, `unset`, `link`, `unlink`, `delete` answer `error[file_unwritable]` (was `file_unreadable`) for a
  file they cannot write; `add`, `remove`, `update`, `init` answer `file_unwritable` (was `config_write_failed` or
  `init_write_failed`); `init` answers `file_unreadable` for a file it cannot read. Every project-change message is
  `cannot X: ...; nothing was written`, files named from the spec or project root. Exit codes are unchanged.
- MCP: the `code` does not move (the kinds are unchanged); the messages gain the operation and lose the absolute path.
- Rust API: `rename::UNREADABLE`, `format::{UNREADABLE, UNWRITABLE}`, `Failed`, `Cause<E>` and `Also::code` are gone.

## Reopen when

An operation needs to tell two unwritable project files apart by code rather than by name, or a change failure needs a
cause the three do not cover.
