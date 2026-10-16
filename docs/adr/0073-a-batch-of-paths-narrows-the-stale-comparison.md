# A batch of changed paths is the session's stale comparison, narrowed

**Status:** accepted (2026-10-13, architecture round 9, plan 03). Amends [ADR 0030](0030-a-sessions-inputs-are-one-value.md)
D1 and D4 and [ADR 0014](0014-the-session-keeps-itself-current.md) D9. D5 extended by ADR 0113 (inference sources: no
symlinked directory is entered).

Watch and the LSP told the project session which paths changed, and the session classified each path by its spelling: a
`.spec` file under the spec root was a source, a few named files were environment or check inputs, anything else was
nothing. A file watcher reports more than files. It reports a directory that moved, was created or was removed. It
reports a symlink by its own name, which the classifier resolved to its target's key. It reports an edit to a symlinked
source at its target, in a directory nobody watched. So a directory moved inside the spec root left watch reporting E002
for an entity `specforge check` no longer saw twice. A directory moved in, moved out, or holding an extension module
changed nothing in watch. An in-tree alias was rebuilt as its target. A symlinked source's edits were never seen. MCP had
none of these bugs: it never classified, and compared the stamps of everything the session read with disk (`stale`). The
classifier also ignored stamps, so a late or duplicate event for a file the session had already read rebuilt it. A
detached session read any `.spec` file a disk event named into its project.

## Decisions

- **D1. One comparison.** `ProjectSession::changes(paths)` is the comparison `stale()` makes, narrowed to what the paths
  reach. `SessionInputs::classify`, `SessionInputs::changes`, `InputRole` and `Changes::from_roles` are gone;
  `SessionWatch::classify` with them.
- **D2. What a path reaches.**
  - A path under the spec root reaches the sources at or under it: for a directory, every source discovery finds there
    now (`specforge_common::SourceRoot::files_at`, ADR 0080) and every source the session read from there; for a file, itself.
  - The spec root, or a directory above it, reaches every source.
  - An environment or check input is reached by its own path or a directory above it, and a missing check input's
    directory also by any file in it.
  - A symlinked source's target, or a directory above it, reaches that source.
- **D3. A reached entry changes only when it differs from its stamp.** A directory created on the way to a missing
  input's directory outside the root is that input's change whatever the stamps say (ADR 0030 D6).
- **D4. `Changes` names what changed.** Source keys; for environment and check inputs, the batch paths that reached one
  that changed (the recorded inputs, for `stale`). Watch's events are labelled from it.
- **D5. Symlinked sources.**
  - A symlink is a source when it resolves to a file. It is read through the link, wherever it points, and keyed by its
    own path: a source path is compared by its directory's canonical form and its own name, never resolved to its
    target (`source_key` too).
  - A symlinked directory is never entered.
  - Each symlinked source's target is a session input: watch watches its directory, the LSP asks its client to report
    it, and an update that re-links a source sets `inputs_changed`.
- **D6. The LSP asks for directories.** `Watched::Directories(root)` (and the spec root outside it) is registered as
  `**/*` for created and deleted entries: a client sends nothing for a `**/*.spec` glob when a parent folder moves.
- **D7. A detached session takes no change from disk** (ADR 0030 D5 made true).

## Consequences

- Watch reports what check reports after directory moves, symlink edits and module directory moves (user-visible). It
  prints nothing for an event whose file is as the session read it (user-visible: no late `rebuilt` after `ready`).
- A symlink named `*.spec` that points at a directory or at nothing is no source: no E025 (user-visible).
- The LSP registers one more watcher and receives create/delete events for every entry under the root (user-visible to
  clients).
- Watch, the LSP and MCP compare the same stamps. A directory event costs a walk of that directory; an event for a path
  that reaches nothing costs a parent canonicalization.

## Rejected

- **Directory and symlink cases in the classifier:** two rules for "what changed" was the bug, and stamps still ignored.
- **A full `stale()` for every batch:** a stat per source per batch, including batches of build noise under the root.
- **No symlink is a source:** check, MCP and format already compiled symlinked files; it would take a working feature
  away to save the target tracking.
- **The LSP catching up on any watched batch:** the client sends no batch for a folder move under a file glob.

## What would reopen it

A file system or client that reports moves as pairs with their contents, or a request for symlinked directories as
sources.
