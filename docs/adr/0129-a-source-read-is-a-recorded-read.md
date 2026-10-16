# A source read is a recorded read

**Status:** accepted (2026-10-16, architecture round 13, plan 07). Completes
[ADR 0114](0114-environment-inputs-are-recorded-as-read.md) D3 for sources. Amends
[ADR 0073](0073-a-batch-of-paths-narrows-the-stale-comparison.md) D2 (fourth bullet) and D5 (third bullet), and
[ADR 0106](0106-a-project-session-reads-the-disk-through-one-ledger.md) D5 ("the renewals of the links happen in
place"). Keeps ADR 0106 D2 and its rejected "stamping every discovered source at discovery".

## Context

Three rules answered "what did this read follow":

| Input | Read by | Hops recorded | Keyed and reached by |
|---|---|---|---|
| environment (`specforge.json`, lock, modules) | the recording adapter (`RunDisk`) | each hop, stamped | own name and each hop |
| check (`file_reference`, `file_exists`, cache) | `RunDisk` | each hop, stamped | own name and each hop |
| source (`.spec`) | `std::fs::read_to_string` | none; one canonical target | key region and final target only |

Reproduced against a debug build: a source behind `a.spec -> links/l1 -> real.spec` whose middle hop is pointed
elsewhere is missed by `specforge watch` and by the LSP (the hop's directory is not watched, the hop is not registered,
the event reaches nothing); a hop that is itself a source makes watch diverge from `check` (the event reaches `b.spec`,
never `a.spec`). The environment input with the same chain is seen everywhere. MCP sees the source too: `stale()`
compares the source's stamp, and `Entry::holds` follows the chain. So the stamp is right and the link record is wrong.

The source read had two entry points (the cold `Reads::source` and `Ledger::reread`), three stamp functions
(`source_with`, `stamp_source`, `stamp_sources`) and a link rule of its own (`Link`, `link_at`, `set_links`, `relink`:
one canonicalised target per symlinked source, taken at discovery and again at a reread).

## Decisions

**D1.** A source is read through a `specforge_registry::Disk` (`sources::read(disk, spec_root, key)`): `Unrecorded`
hands `PlainDisk`, the ledger the recording run's `RunDisk`. The UTF-8 mapping moves into `sources.rs`.

**D2.** The cold read and every reread are one ledger read (`Ledger::read_source`). It stamps each hop and the file
before reading; the source's stamp (by key) and its hops are exactly those of its last read. Discovery records
nothing about a source. `Link`, `link_at`, `set_links`, `relink`, `stamp_source` and `stamp_sources` are gone;
`Entry::stamp` is called only by `Run::read`.

**D3.** Each hop a symlinked source's read followed is a session input of that source, keyed by its own name in its
canonical directory (ADR 0114 D3): it reaches the source, its directory is watched, the LSP registers it.

**D4.** A held source's file is not read, so neither its stamp nor its hops are recorded until it is released
(ADR 0106 D2, extended to hops).

**D5.** Behaviour changes: a source whose link chain has a hop pointed elsewhere is that source's change in watch and
the LSP; watch watches the directory of every hop; the LSP registers every hop by its own name; a held symlinked
source's hops are recorded when it is released, so the LSP does not register its target while it is held.

## Rejected

- A hop walk beside a `std::fs` read: two read mechanisms again, and the walk could follow a path the read does not
  (the drift ADR 0107 and 0114 were written against).
- A stamp per hop, compared in `changes`: `std::fs::metadata` of a hop also follows the chain, so it gives the same
  answer with more state.
- Hops recorded at discovery for every discovered file: a second recording path, and a held key would be recorded
  without being read.
- Canonicalising each hop into the `Link`: it keeps a second rule.

## What would reopen it

Symlinked directories as sources (ADR 0073 D5), or a read the port cannot express for `.spec` files (streaming,
encodings other than UTF-8).
