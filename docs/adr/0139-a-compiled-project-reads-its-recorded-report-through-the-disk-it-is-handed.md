# A compiled project reads its recorded report through the disk it is handed

**Status:** accepted (2026-10-17, architecture round 14, plan 07). Answers [ADR 0112](0112-every-project-file-an-operation-opens-goes-through-the-requests-containment.md)'s reopen condition ("a crate below app that must open a project file (the session's recorded report)") without its rejected kernel move. Extends [ADR 0108](0108-a-project-view-comes-only-from-a-compiled-project.md) D2 (only a compiled project reads its report, at its own root) and ADR 0107 D1's port to the recorded report. Keeps ADR 0092 D6. Amended by [ADR 0144](0144-a-project-file-is-written-by-replacing-its-name.md) (a project file is written by replacing its name). **Amended by [ADR 0169](0169-the-format-configuration-is-read-through-the-callers-disk.md) (2026-10-19):** D2: the view's disk rule (`ProjectView::containment`) also bounds a typed edit's format configuration. **Amended by [ADR 0183](0183-a-project-scoped-call-reads-and-writes-through-one-containment.md) (2026-10-21):** D2: `ProjectView::contained` is `bounded_by`, and the schema cache is read through the view's disk.

D2 is amended by [ADR 0145](0145-the-session-reads-sources-through-the-disk-the-surface-hands-it.md): the surface also decides the disk sources are read through; the hard-link read is stated there, and closed over MCP by [ADR 0146](0146-a-file-with-more-than-one-name-is-not-read-through-a-bound.md).

Every view that scores coverage passed the project's own root back into the project's memo
(`self.project.recorded().report(self.root())`), the shape ADR 0108 removed from `ProjectView::new`. The memo then
opened `<root>/specforge-report.json` with a raw `std::fs::read`, below every containment, so over MCP a report that
is a symlink to a file outside the served project was read, scored, and its parse errors sent to the client by
`specforge.coverage`, `stats`, `inspect`, `analyze`, and the trace and review prompts, although
`spec/invariants/mcp.spec` says the test report is read only from inside the project, and `specforge.collect`
refuses to write through the same link. One file format had three readers (`read_report`, which had no caller,
`read_report_file`, and the memo's inline read) over one private parser.

## Decision

- **D1.** `CompiledProject::test_report(&dyn Disk)` and `recorded(&dyn Disk)` are the one way to the recorded report:
  the project names its own file (`root()/specforge-report.json`) and a detached one asks nothing. The memo is private
  to the project; `RecordedCoverage::report` and `at` take no root.
- **D2.** The surface decides the disk. MCP's call target bounds every view it builds (`ProjectView::contained`), so
  ops reads through `ContainedDisk(Containment::within(root))`; the CLI and the LSP read through `PlainDisk` and
  follow the user's link (ADR 0092 D6). `Containment` stays in ops.
- **D3.** A report that lands outside a bounded view is E045 of kind `invalid_input` on every view; the target is
  never opened.
- **D4.** `TestReport::parse` is the one parser; `read_report` and `read_report_file` are gone; collect and analyze's
  named report read the `Held` they resolved.
- **D5.** The memo stays keyed on the report's content hash: a stamp would save at most 0.4 ms per call on a 1.1 MB
  report, needs a `Disk` method for metadata, and still hashes a report written within the timestamp granularity.

## Rejected

- `Containment` in a kernel crate (ADR 0112's rejected move).
- The project's own containment on every surface: the CLI's `stats` would refuse the report its own `collect` wrote
  through the user's link.
- A `Caller` on every read view (ADR 0124's rejected "parameter beside the request").
- A `dyn Disk` stored in the compiled project: it would carry a `Send + Sync` box and a setter, and the surface
  decides per view anyway.
- A typed refusal on `Disk::read`: it changes the trait for the checks, the environment load and the sources for one
  caller.
- A stamp in place of the hash.

## Consequences

MCP's read views no longer follow a report link out of the project. The CLI and the LSP are unchanged. A hard link
stays readable on the CLI and the LSP (amended by ADR 0146: MCP refuses a report with more than one name): a name rule cannot see it (a write never goes through one, ADR 0144).

## What would reopen it

A read view that must bound a file differently from its surface; `Disk` gaining a metadata method (then the stamp is
cheap); an `openat`-style handle (ADR 0112's other condition).
