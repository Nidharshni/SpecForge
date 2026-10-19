# A link chain is every link the file system follows

**Status:** accepted (2026-10-19, architecture round 17, plan 09). Amends [ADR 0129](0129-a-source-read-is-a-recorded-read.md)
D2 (what a stamp records of the chain) and D3 (what reaches a source), and
[ADR 0133](0133-recorded-inputs-are-one-table.md) (what a recorded input keeps). Keeps ADR 0114's rejected "`read_link` on
the port", ADR 0146's rejected "a metadata method on the port", ADR 0104 (the containment's walk) and ADR 0073 D5 (a
symlinked directory is no source directory).

## Context

A read follows every symlink the file system meets on the way to a name. The ledger's walker (`link_hops`, in the `Disk`
adapter) followed only the links *at* the name: `alias/b.spec` with `alias` a symlinked directory and `b.spec` a plain
file had no hop. No stamp recorded `alias`, and every recorded input is keyed by its own name in its *canonical*
directory (ADR 0114 D3), which resolves `alias` away. Pointing the directory link elsewhere was therefore missed,
three ways, each reproduced on a build of `c4d7ed4a`:

- **R3**: `specforge watch` over `a.spec -> alias/b.spec`, `alias -> ../ext/v1`, then `alias -> ../ext/v2`: no event,
  while a cold `specforge check` sees the new source.
- **R4**: MCP `stale()` over the same layout with `ext/v1/b.spec` and `ext/v2/b.spec` of equal length and modification
  time: the served session keeps `one_a`, a fresh one sees `two_a`. The stamp compared the *spelled* chain
  `[proj/alias/b.spec]`, equal whatever `alias` pointed at.
- **R6**: a check input read through a directory link (`docs: ["../docs/linked/guide.md"]`, `linked` retargeted to a
  directory without the file): watch reports no E016, a cold check does. No link at all lies in the read path's target
  text here: the directory link is in the read path itself.

The chain's logic was spread over `disk.rs` (the walk), `stamps.rs` (`followed`, which imported the walker from the
`Disk` adapter) and `inputs.rs`/`recorded.rs`/`reach.rs` (what reaches a row), and each of the seven link fixes of 16 and
17 October patched the one that missed its shape. The contained disk's walk (`landing`, ADR 0104) already follows every
link; the two walkers agree on relative `..` targets under a symlinked directory (R1, R2: not reproduced as a
disagreement).

## Decisions

**D1. One module owns a name read through its links.** `ledger::chain` (crate-private) owns the **hops** (the links at
the name, in order, at most 40), what a stamp records of them, and the **way** (the directory links on the way to a
recorded input). It asks `std::fs`, as the read does.

**D2. A stamp records where each hop lands**: its own name in its canonical directory, as an input is keyed, not how it
is spelled. A directory link on the way to a hop pointed elsewhere changes the stamp whatever the length and the
modification time say. A plain file's stamp is still one `lstat`.

**D3. A recorded input keeps its way**: each directory link a read goes through before its own name, below the root as
opened and not directly under the file-system root (macOS `/tmp` and `/var` are the machine's spelling, not the
project's), by its own name, taken when the row is recorded. A changed path at one, or at a directory above one below
the root, is that input's change whatever the stamps say (a hop's: its source's). Watch watches each one's directory for
its own entries; the LSP registers each by name.

**D4. The containment's walk (`landing`, ADR 0104) is not shared.** It answers where a name lands, for a bound; the chain
answers which links a read followed, for change. They agree on `..` and on directory links.

**D5. A stamp's content hash reads through the disk the read goes through.** A racy stamp of a source read through a
bounded disk (MCP) hashed the bytes of a refused source with `std::fs::read`, below the bound (found by plan 02, R2; the
hash stayed in the process). The racy hash, taken when stamped and compared when checked, is now read through the source
disk, so a read the disk refuses is hashed as the refusal it is. This is not a metadata method on the port: only `read`
and `list`, which it has.

## Rejected

- A metadata or link method on `Disk` (ADR 0146's reasons).
- One walker in `kernel/common` for both walks: it couples a bound to change detection for no behaviour gain.
- Each directory link as a row of its own: its directory would be watched recursively (`/` for `/tmp`).
- A canonical landing in every stamp: a `realpath` per stamp, thousands per check run.
- Walking only each link's target text: it misses a directory link in the read path itself (R6).
- Comparing a way link's move through the row's stamp: an equal-stamp retarget is lost for environment and check rows,
  whose final path is a plain file with an empty chain.

## What would reopen it

Windows junctions behaving unlike symlinks; a profile showing `way`'s `lstat`s in a check run's renewal; symlinked
directories as sources (ADR 0073 D5).
