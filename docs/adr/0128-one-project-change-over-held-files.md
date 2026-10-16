# One project change over held files

**Status:** accepted (2026-10-16, architecture round 13, plan 06). Amends [ADR 0028](0028-installed-extensions-are-one-module.md)
D4, [ADR 0112](0112-every-project-file-an-operation-opens-goes-through-the-requests-containment.md) D1 (`replace`) and
[ADR 0059](0059-trust-pins-are-written-with-the-lock-plan.md) D2's `commit_with(config, last)` sentence. Keeps
[ADR 0022](0022-a-mutation-reports-what-it-wrote.md) D1 and D4, which the code did not.

"Write several project files as one change, put each back on failure, report what could not be put back"
(ADR 0022 D1/D4, ADR 0028 D4) was written twice, with two rules. Rename wrote in place, ignored every failed restore
(`let _ = written.write(old)`) and returned an error with no writes. The installed crate snapshotted and restored
`specforge.json`, `.gitignore` and the starter by name with `std::fs`, while the writes went through `Held`. Two live
defects, reproduced with a file-size limit (`ulimit -f` with `SIGXFSZ` ignored, so a write past the limit fails with
`EFBIG` after a prefix is written):

- R1: a rename whose write of `b.spec` was refused, and whose restore of `a.spec` then failed, left `a.spec` truncated
  mid-identifier (E001) and replied `files_written: []`.
  `cargo nextest run -p specforge-cli -E 'test(partial_writes)'` pins it end to end.
- R2: a `format` whose write failed part-way truncated its source the same way and reported it only as a failure.

## Decision

- **D1. One crate-private module, `ops::change`, with four steps.** `ProjectChange` stages `(Held, before, after)`;
  `commit_then` (1) checks that every file still holds what it was staged over, read through its `Held`, so a file
  that changed is `Changed` and nothing is written; (2) writes in stage order, making the missing directories and
  replacing a file that was there (`Held::replace`) or creating one that was not (`create_new`); (3) runs the
  caller's last step; and on any failure (4) puts every file back in reverse stage order and removes the directories
  it made when empty. A file staged with its own bytes is not staged, so it is never written nor reported.
- **D2. `Failed::error` is the only way from a failed change to an `OpError`.** It takes the caller's wording of the
  cause and attaches the files the change could not put back as the error's writes, so an operation cannot drop them
  (ADR 0022 D4). The rollback of a created file unlinks `real`, not the name, so a file created through a link leaves
  the link in place.
- **D3. `Held::replace` keeps the file's permissions and refuses a file the process may not write.** It opens an
  existing file for writing, without truncating, before it writes the temp, and copies its permissions onto the temp.
  Ownership and extended attributes are not carried over. A writable file in a read-only directory can no longer be
  rewritten (the temp cannot be created there).
- **D4. Installed keeps the modules and the lock and runs the project change as its last step.** `Change::files` and
  `Installed::change_entries` are deleted, the lock is always read, and the nesting keeps ADR 0059's order: modules,
  lock, `specforge.json`, the `Also` files, the pins. A plan that installs nothing commits its project change alone
  and opens no installed change (ADR 0041 B4). A nested failure merges the project change's left files into the
  installed failure's writes. `specforge.json`'s edits are made on the extensions array in memory and written once.
- **D5. Format rewrites each file with `replace`.** Format is not all or nothing (every other file is still formatted,
  `formatting.spec`), so it does not use the module, but a file is written whole or not at all.

## Rejected

- **D1. The module under `containment/`**: a change is not what a request may reach. **A kernel or runtime place**:
  it moves `Containment` below app for two callers (ADR 0104, ADR 0112 Rejected). **A port in installed** that ops
  implements: installed would still own the role of snapshotting project files, with a trait for one adapter.
- **D2/D4. A participant trait (prepare, commit, abort) with a coordinator**: a protocol for two parties in a fixed
  order. **The project change as the outer one with an "install first" hook**: it needs an undo token from installed.
- **D3. Plain POSIX rename semantics** (a 0444 file is replaced and comes back 0644): six tests flip and a user's
  read-only source is silently rewritten. **No atomic writes**, only reporting what is left: R1's truncation and R2
  stay.

## Consequences

- A failed rename names the file it could not put back in `files_written`, and a failed format leaves its source
  whole.
- Init's starter that appears between the check and the write reports `file_changed` instead of `starter_exists`
  (the pre-check still answers `starter_exists`). A non-UTF-8 `.gitignore` is appended to, never overwritten.
- `Held::write` has no caller left in ops; render's is plan 09's to move to `replace`, after which `write` goes.
- Render's read-only output file is refused with `permission_denied` once render replaces (one `replace`, one rule).

## Reopen when

A third writer below app must change several files, or a project file is opened by handle (`openat`) instead of by
name.
