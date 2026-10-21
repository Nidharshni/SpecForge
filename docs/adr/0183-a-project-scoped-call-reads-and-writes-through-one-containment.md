# A project-scoped call reads and writes through one containment

**Status:** accepted (2026-10-21, architecture round 19, plan 06). Amends [ADR 0139](0139-a-compiled-project-reads-its-recorded-report-through-the-disk-it-is-handed.md) D2 (`ProjectView::contained` is `bounded_by`; the schema cache is read through the view's disk), [ADR 0169](0169-the-format-configuration-is-read-through-the-callers-disk.md) D3 (the typed-edit disk is the view's containment, over MCP the call's one), [ADR 0124](0124-what-the-caller-lends-an-operation-is-one-value.md) D4 (MCP builds the containment once per call and lends it from the view) and [ADR 0146](0146-a-file-with-more-than-one-name-is-not-read-through-a-bound.md) D4 (the schema cache joins the reads through the bound).

## Context

- An MCP call stated "what may this call read on disk" twice: the view got a `contained: bool` and rebuilt
  `Containment::within(root)` on every read, while the caller got its own `Containment::within(root.dir())` or the
  request's bound. Seven statements in two files had to agree; only a source scan held them.
- The flag bounded only what round 14 wired to it (the recorded report, then a typed edit's configuration). One file the
  view reads at its root was never bounded: the schema cache was read with `std::fs` below every containment. Over MCP
  a `.specforge/schema-cache.json` that was a symlink or a hard link to an outside file was read, its content quoted
  in I016 by `specforge.schema`, `export`, `model` and `trace`, and an outside record steered the schema version
  (reproduced, 06-D1). No ADR stated a rule for that read; ADR 0146 D4 did not list it.
- No production pair of view and caller differed (06-D2, not reproduced).

## Decision

- **D1.** The view carries the containment it reads every project file through (`containment: &'a Containment`;
  `ProjectView::of` is unbounded; `bounded_by` replaces it; `containment()` is public). The recorded report, the schema
  cache and a typed edit's format configuration are read through `containment().disk()`. `contained()` is gone.
- **D2.** MCP builds one containment per call in `Call::new` (the named project's root, else the request's bound). Its
  views are bounded by it and every caller it lends (`ProjectRef`, `ViewToWrite`, `NewProjectDir`) is it.
- **D3.** The schema cache is read through the view's disk. Bounded, a record that lands outside or has more than one
  name is I016 naming the disk's reason, never what it holds, and the version restarts at 1.0.0. The CLI and the LSP
  follow the user's link (ADR 0092 D6). A dangling link that points outside is now an I016 over MCP.
- **D4.** Ops opens file content only through `containment/` (a guard test), but for the collector machine's approvals
  store, a user file and not a project file.
- **D5.** Operations keep their signatures; ADR 0124 D2 stands; ADR 0169 D3's rejection of a `Caller` on `edit::plan`
  stands, since the plan does not need one.

## Rejected

- **Operations over a view read and write only through the view's containment** (`edit::run`, `rename::run` lose their
  `caller`): about 58 test sites and 7 operation signatures, and it amends ADR 0124 D1 and D2.
- **The view built from the caller** (`ProjectView::of(project, caller)`): 116 call sites, and the LSP has no caller.
- **The project's own containment on every surface for the schema cache**: a CLI behaviour change for no CLI threat.
- **The containment in `ProjectRef` with the view borrowing it**: a self-referential struct. **In `McpState`**: wrong
  for a project a path names, which is per call.
- **A refusal for a refused schema cache**: it would fail `schema`, `export`, `model`, `trace` and `render` for a stray link.
- **A guard over metadata probes** (`exists`, `is_dir`, `read_dir`): they disclose existence, not content.

## What would reopen it

- An operation that must read a project file through a bound other than its view's within one call.
- A second surface that bounds: its pairing would need the same construction as MCP's.
- `Disk` gaining a metadata method, so the guard could cover probes.
