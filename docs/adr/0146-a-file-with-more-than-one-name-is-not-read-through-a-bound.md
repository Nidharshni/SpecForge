# A file with more than one name is not read through a bound

**Status:** accepted (2026-10-17, round 14 follow-up J1). Amends [ADR 0145](0145-the-session-reads-sources-through-the-disk-the-surface-hands-it.md) D5 (a hard link was accepted and pinned; its "What would reopen it" condition, a stated promise that a sandboxed agent cannot learn the contents of outside files, is met by the user's request). Amends [ADR 0144](0144-a-project-file-is-written-by-replacing-its-name.md) D3 and [ADR 0139](0139-a-compiled-project-reads-its-recorded-report-through-the-disk-it-is-handed.md) (their hard-link read sentences). D1-D4 of ADR 0145 stand. **Amended by ADR 0158 (2026-10-19):** D4: the migration's source read joins what is read through the bound. **Amended by [ADR 0169](0169-the-format-configuration-is-read-through-the-callers-disk.md) (2026-10-19):** D4: `format`'s targets and the format configuration join the reads through the bound; a bounded format run fails a file with more than one name. **Amended by [ADR 0172](0172-the-extension-a-path-names-is-read-by-one-module.md) (round 18):** D4: the extension a path names (add, init) is read through the bound too; a bounded read refuses a file with more than one name. **Amended by [ADR 0183](0183-a-project-scoped-call-reads-and-writes-through-one-containment.md) (2026-10-21):** D4: the schema cache joins the reads through the bound.

A hard link inside the project to a file outside is another directory entry for the same inode, so its canonical path
is inside and a bounded read opened it. Over MCP `ln outside/secret.txt proj/spec/leak.spec` was parsed: the parser
quoted its text in E001 messages, its entities (ids, kinds, field values, also on later lines) entered `validate`, the
diagnostics resource, `list`, `search`, `export`, the graph and the brief, and a hard-linked recorded report steered
`coverage`. The symlink spelling of the same file was already E025.

## Decision

- **D1.** `Containment::read` opens the resolved file once and, only when the containment is bounded (`root` is set:
  MCP), asks the open handle for its link count. A regular file with `nlink > 1` is `InvalidInput`
  ("it has N names, so no bound can say where its content is"), which `ContainedDisk` passes through and the session
  reports as that source's E025 (the recorded report as E045, infer sources as unreadable). The message names the count,
  never the content nor the other name (unknown).
- **D2.** The check is on the handle that is read (`File::open`, `metadata`, `read_to_end` on one `File`): the inode
  checked is the inode read, so there is no window between a metadata call and a read. The `Disk` port is unchanged.
- **D3.** The rule is on the count, so a source whose other name is also inside the project (a `cp -al` snapshot served
  directly) is refused too. Accepted: the suggestion says to copy the file in.
- **D4.** Uniform for what is read through `Containment::read` or `ContainedDisk`: `.spec` sources, the recorded report,
  infer sources, the anchors and infer manifests, analyze's named report. Not changed (stated, not closed):
  `specforge.json` and the lock (read through the plain environment disk; position-only errors, modules and lock are
  hash-checked), the collector report file (parsed, silently ignored), and the `Held::read*` callers that read the
  project's own state before a write (`rename`'s and `format`'s baseline, `init`, the lock plan). A write replaces the
  name (ADR 0144), so a hard-linked file read there is never written through. Consequence: `format` over MCP still
  rewrites a hard-linked source and so leaves it with one name; `rename` needs the session's graph, which no longer
  holds the entity of a refused source, so it is refused and writes nothing.
- **D5.** The CLI, watch and LSP are unbounded (ADR 0092 D6) and read a hard link as before.
- **D6.** Non-unix: the check is skipped (`shortcut:` comment; std's link count is unstable there), so the guard is
  absent on Windows. Not run there.
- **D7.** E025's suggestion is one text for the three causes of a bounded refusal: "keep the file inside the served
  project as a plain file (not a symlink that leaves it, not a hard link: copy it in), or read this project with the CLI".

## Rejected

- **`Disk::link_count` or a metadata method on the port.** Three implementations change, the recording disk must decide
  what a metadata call stamps, and a separate `read` then `metadata` is a race between two path lookups.
- **Tracking per device or per inode.** A hard link has no original; the kernel keeps the count, not the names. Finding
  the other name is a scan of the file system. A per-device rule fails the common case (same volume).
- **Copy on read.** The bytes are the leak.
- **An opt-out.** A key in `specforge.json` is a project file the planter of a link can write, so it defeats the guard;
  a per-file allow list is the same. No environment switch either: a second knob for a layout estimated as negligible.

## Consequences

A project served over MCP that is itself a hard-linked snapshot (`cp -al`, `rsync --link-dest`) or shares sources by
hard link gets an E025 per source and works over the CLI and the LSP. Pins: `crates/surfaces/mcp/tests/source_links.rs`,
`crates/surfaces/mcp/tests/hard_links.rs` (rename), `containment/disk_tests.rs`.

## What would reopen it

A legitimate layout that cannot copy the file (then an opt-out in the environment of the `specforge mcp` process); a
content leak through `specforge.json` or the lock; a file system where `st_nlink` is a constant above 1; the link count
stabilising in std on Windows.
