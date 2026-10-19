# The format configuration is read through the caller's disk

**Status:** accepted (2026-10-19, architecture round 18, plan 01). Amends [ADR 0021](0021-formatting-is-one-operation.md) D2 (configuration discovery is ops' `Configs`, over the caller's disk), [ADR 0139](0139-a-compiled-project-reads-its-recorded-report-through-the-disk-it-is-handed.md) D2 (the view's disk rule also bounds a typed edit's configuration: `ProjectView::containment`), [ADR 0146](0146-a-file-with-more-than-one-name-is-not-read-through-a-bound.md) D4 (format's targets and the format configuration join the reads through the bound) and [ADR 0153](0153-a-value-is-spelled-by-one-module.md) (a typed edit's layout is its plan's, not a fresh lookup).

## Context

`.specforgefmt.toml` was the one project file authoring still opened by name with `std::fs`
(`specforge_formatter::config`), below every containment. Over MCP a configuration that was a symlink or a hard link to
a file outside the served project was read, and `specforge.format` quoted its first line back in W141; a typed edit and
`init` laid their text out with an outside file's values. The same lookup was done from scratch five times per `set` and
nine per `link` (a fresh `Configs` per `Block::open`, `finish` and `layout`). And the format run held each target but
read it with the unchecked `Held::read_to_string`, so a hard-linked source's text came back in `diffs`
(ADR 0146 D4 assumed the baseline read of `format` is never shown).

## Decision

- **D1.** The formatter parses only: `specforge_formatter::config::parse(path, io::Result<Vec<u8>>)` and
  `config::FILE`. It opens no file (`the_formatter_opens_no_file`). The lookup, its cache and its bound live in ops'
  `Configs`.
- **D2.** `Configs` reads through the `&dyn Disk` it is built with. Only `NotFound` is absence; any other read answer is
  the nearest configuration, a W141 naming the disk's reason (the landing, the count), never the content, and the
  defaults apply. Unbounded, `ContainedDisk` reads what `PlainDisk` reads, so no caller branches.
- **D3.** Which disk: the format run, `caller.containment().disk()`; the typed-edit plan, the view's
  (`ProjectView::containment`, which `read_report` shares); init, `caller.containment().narrowed(dir).disk()`;
  `format::document` (the LSP, `format --stdin`), `PlainDisk` (unbounded, ADR 0092 D6).
- **D4.** One `Configs` per run, plan or init. A `Site` carries its file's `FormatConfig`; `Block` never looks it up.
- **D5.** A bounded format run reads each target through `Containment::read`: a file with more than one name is a failed
  file (`invalid_input`), the others are formatted. The CLI is unchanged.

Rejected: D1, the formatter taking `&dyn Disk` (a new pipeline edge and the policy split across two crates) or closures
(a second port shape); D2, `Disk::exists` (a refused configuration is silently skipped) or `Disk::list` (a `read_dir`
per level, and a dangling CLI link becomes a W141); D3, a `Caller` on `edit::plan`; D4, a `RefCell<Configs>` on `Site`
or a configuration argument on every `Block` call; D5, keeping ADR 0146 D4 as written.

## Consequences

Over MCP a linked or hard-linked `.specforgefmt.toml` is a W141 and the defaults apply; the CLI and the LSP follow it. A
W141 over MCP may name an absolute landing outside the project (as E025 does, ADR 0145); it never names content. A
hard-linked target fails the MCP `specforge.format` call (`invalid_input`, `format_file_failed`) instead of being read,
diffed or rewritten: a bounded read refuses a file with more than one name.

## What would reopen it

A bounded caller of `format::document`; a `.specforgefmt.toml` written within a run or plan (the caches are not
invalidated); a configuration read outside ops.
