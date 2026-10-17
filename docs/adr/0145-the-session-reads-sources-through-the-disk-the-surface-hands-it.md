# The session reads sources through the disk the surface hands it

**Status:** accepted (2026-10-17, round 14 follow-up H1). Amends [ADR 0092](0092-a-path-an-mcp-request-names-resolves-inside-the-served-project.md) D8 (a read of a project source is bounded too, not only a mutation), [ADR 0129](0129-a-source-read-is-a-recorded-read.md) D1 (the recording run wraps the disk it is given), [ADR 0139](0139-a-compiled-project-reads-its-recorded-report-through-the-disk-it-is-handed.md) D2 (the surface decides the disk, now also for sources) and [ADR 0144](0144-a-project-file-is-written-by-replacing-its-name.md) D3 (the read exposure of a hard link now has one home, here).

Over MCP the session read every `.spec` source through `PlainDisk`, which follows symlinks wherever they lead. A
`spec/leak.spec` linked to a file outside the project was parsed: the parser quoted its text in E001 messages and its
entities (ids, kinds, field values) entered `validate`, the diagnostics resource, `list`, `search`, `export`, the graph
and the brief. ADR 0092 D8 bounded mutations; ADR 0139 bounded the recorded report; the sources are read in the
session, before any view is chosen. The same held for a symlinked spec root and for a `spec_root` that names a
directory outside the project.

## Decision

- **D1.** A surface hands the session the `Disk` its sources are read through:
  `ProjectSession::open_through` and `reopen_through` take `Option<SourceDisk>` (none reads the plain file system), and
  a reload keeps it. MCP hands `Containment::within(root).disk()` of the project it opens (the served project, and
  another project a call names). The CLI, watch and the LSP hand none and keep `PlainDisk` (ADR 0092 D6): their user
  is at the file system.
- **D2.** A source whose final target lands outside the root is unreadable: that source's E025 (`cannot read X: it
  resolves to Y, outside Z`), naming the landing and never the content. It is absent from the graph, and its stamp and
  link hops are still recorded, so a link retargeted inside is seen and heals on the next request. The rule is on the
  final target: a hop outside the project whose final target is inside, and an in-project chain, still read.
- **D3.** Only the source read takes the handed disk. The environment load and the checks read plain: checks
  (`file_reference`, `file_exists`) legitimately test files outside the root, and `specforge.json` and the lock gave no
  content leak (position-only errors; modules and lock are checked by hash).
- **D4.** A `spec_root` that lies outside the project root makes every source E025 over MCP; the CLI and the LSP still
  read it. Serve MCP at a root that contains the specs.
- **D5.** A hard link is not refused. It has no path that differs from a file in the project, so no containment sees it:
  an outside file hard-linked as a source is parsed and quoted over MCP. This is stated and pinned
  (`crates/surfaces/mcp/tests/source_links.rs`), not closed. A deployment that must keep an agent from outside content
  must not let it create names in the project, the same condition ADR 0092 stands on for symlinks.

## Rejected

- **Read and strip the content from messages.** The content is in entity ids, kinds, fields and titles; there is no
  message-only leak, and the outside entities would still steer `analyze` and `coverage`.
- **A refusal on `st_nlink > 1`.** `Disk` has no metadata method; false positives for `cp -l`, pnpm and Nix trees.
- **Bounding the environment and the checks too.** It breaks anchors to files outside the root (D3).
- **A `dyn Disk` in the compiled project** (ADR 0139's rejection stands): the source read is the session's, with no
  view yet, and the reload path needs the disk, so it lives in the ledger.
- **Every hop's location inside the root.** Buys nothing against a read that opens the canonical final file.

## Consequences

A legitimate shared-spec layout (a source link or `spec_root` into a sibling tree) works over the CLI and the LSP and
stops working over MCP. The message names the landing, and the suggestion for a refusal by the bound says to keep the
file and the link inside the served project, or to read it with the CLI (the "save it as UTF-8 text" suggestion stays for
a file that is not text). The pins are `source_links.rs` (MCP), `tests/source_disk.rs` (session) and `containment/disk_tests.rs`.

## What would reopen it

`Disk` gaining a metadata method; a stated promise that a sandboxed agent cannot learn the contents of outside files
(the `nlink` refusal returns); a content leak through `specforge.json` or the lock.
