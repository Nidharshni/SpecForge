# Every project file an operation opens goes through the request's containment **Amended by [ADR 0131](0131-an-export-is-written-into-the-project-by-the-export-operation.md):** D4: render builds no bound; outline's spec-root bound is the one exception left.

**Status:** accepted (2026-10-16, architecture round 12, plan 01). Amends ADR 0104 D2, ADR 0092 (the audit table and D2) Amended by [ADR 0144](0144-a-project-file-is-written-by-replacing-its-name.md) (a project file is written by replacing its name).
and ADR 0029 D8. D2 (collect's request and `SessionRequest`) and D4 (`ProjectRef::caller`) amended by [ADR 0124](0124-what-the-caller-lends-an-operation-is-one-value.md); its Rejected line stands.
D1 (`replace`) amended by [ADR 0128](0128-one-project-change-over-held-files.md): it keeps the file's permissions and refuses a file the process may not write. Its reopen condition (the session's recorded report) is answered by [ADR 0139](0139-a-compiled-project-reads-its-recorded-report-through-the-disk-it-is-handed.md): the session reads the report through the `Disk` port and ops supplies the containment adapter; the Rejected kernel move stands. The Consequences bullet on `coverage::read_report` no longer holds.

ADR 0104 applied containment at an enumerated list of sites, so every reader or writer of a project file left off the
list still opened the file by name, through whatever symlink was there. Eight commits in three days (`4a75cee8`,
`e096e7e5`, `ceff78dc`, `00ea1740`, `3da57fb3`, `c8671d2e`, `a760cb30`, `cd97d186`) each added one site. Six more
escaped the served project over MCP, each reproduced:

- R1: `collect` overwrote a file outside through a symlinked `specforge-report.json`.
- R2: `infer_session start` wrote through a symlinked `specforge-infer.json.tmp`, then installed that link as the manifest.
- R3: `analyze {"test_results": "../outside/r.json"}` read outside the project, and its E045 kinds told the client whether
  any path exists and how it parses.
- R4: `collect` read a runner's captured stdout from outside.
- R5: `infer_progress` (and the anchors manifest's readers) sent an outside manifest's contents back to the client.
- R6: `migrate` wrote its `.spec.bak` and `.spec.tmp` through symlinks and turned the source into a link pointing outside.

## Decision

- **D1. A held file is opened where it is held.** `Held`'s methods (`read`, `read_to_string`, `write`, `create_new`,
  `create`, `replace`, `remove`, `create_parent`) open `real` and name `spelled`. `replace` is the one atomic write: its
  temp is `<real>.tmp`, created new (O_EXCL) after whatever is at that name is removed (a link, never its target).
- **D2. ADR 0104 D2's list grows.** Collect's report, its collectors' report locations, captured output and report
  files, `--report` files; analyze's named report; the inference session's manifest and temp; migrate's backups, temps
  and record; and the lock plan's config and `Also` files are opened through the `Held` they were checked as. Collect's
  request, `ReportSource::Named` and `SessionRequest` carry the containment, required.
- **D3. A file the project keeps at its root and only a bounded operation writes** (the inference and anchors
  manifests) is read through the project's own containment on every surface (`Containment::within(root)`). A manifest
  that resolves outside is E071 (kind `InvalidInput`).
- **D4. The call's project hands its containment** (`ProjectRef::containment`, ADR 0092 D2's second bullet). A tool
  builds none, but outline (the spec root bounds a file of the outline) and render (a view tool that writes its own file).
  Rename is bounded by the project root, not the spec root.
- **D5. A declaration is not a landing.** A collector's report location is judged by `containment::declared` (relative,
  no `..`: the rule moved from collect into the containment module), and where a declared name lands is the request's
  containment. The source path key rule is the manifest's (plan 02).
- **D6. A test holds it.** No `std::fs` open in `specforge-ops`' `src/` outside the held-file module but the listed
  ones, each with its reason (`tests/operations/project_files.rs`); no MCP tool builds its own bound or opens a file
  (`project_file_containment.rs`).

## Rejected

- A kernel place for the module (`specforge_common`): it moves `Containment` below app for two callers (ADR 0104
  rejected this); revisit when a third crate below app must open a project file.
- A temp refused when it is a link: the user would clean up a file the operation owns, and an inside link would still be
  followed.
- `AnalyzeOptions.containment`: it removes `Default` and touches every construction for a value one variant reads.
- Opening migrate's sources as `Held`: `specforge_migrate` names every result from the path it opens; the gain is only
  in a race.

## Consequences

- Over MCP R1-R6 are refused or contained; the CLI is unchanged, except that `infer-status` and `check --lint inferred`
  answer E071 for a manifest linked outside the project.
- Check-then-open stays two system calls for every component but the temp's.
- `replace` rewrites the target of an inside link and keeps the link, where `rename` onto the name used to replace the link.
- The recorded report (`specforge_project::coverage::read_report`, a session read) and the installed modules and lock
  (`specforge-installed`) still open by name; the inference source reads and the navigator's file text are routed by
  plans 02 and 05 of this round.

## What would reopen it

A crate below app that must open a project file (the session's recorded report), or an `openat`-style handle.
