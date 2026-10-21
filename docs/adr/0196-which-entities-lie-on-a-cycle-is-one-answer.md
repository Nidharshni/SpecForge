# Which entities lie on a cycle is one answer

**Status:** accepted (2026-10-21, architecture round 21, plan 05). Affirms [ADR 0016](0016-navigation-is-one-module.md) D7 and [ADR 0074](0074-the-session-update-states-the-diagnostics-delta.md) D6 (W061 stays spanless). Moves the cycle walker named in [ADR 0020](0020-rules-are-a-typed-set.md) out of `specforge-common`; extends [ADR 0050](0050-the-guest-wire-carries-only-what-guests-compile.md) (a leaf crate the guests compile).

## Context

Five modules answered "which entities lie on a cycle" with their own depth-first walk: `Graph::detect_cycles` (W061), the shared walker the `cycle_detection` rules read (E007, E015, E052, W045, W092), the resolver's (W113), and the formal guest's two (E042, E041). Each recorded a cycle only when an edge closed back onto the walk's current path, so an entity whose only way into the cycle passes through an entity already finished was never reported. Product's `in_cycle` was a correct Tarjan; `module-depth` named the entity the diagnostics left out.

Reproduced at `9d05665cc`: on `alpha -> [beta, delta]`, `beta -> gamma`, `gamma -> alpha`, `delta -> beta`, W061 and E007 omitted `delta` while `product module-depth delta` put it on the cycle; W061 changed with the order the references were written in (`[beta, gamma]` against `[gamma, beta]`, the second omitting `beta`); W113 omitted `dd.spec` of `aa -> {bb, dd}`, `bb -> cc`, `cc -> aa`, `dd -> bb`; formal E042 omitted the same process, and E041 named `alpha` or `beta` from run to run, because the walk was seeded by a `HashMap`. The spec promised "zero false negatives" and "names all participants".

## Decision

- **D1.** A node is on a cycle exactly when it reaches itself. The cycles are the strongly connected sets that hold one: two or more nodes, or one with an edge to itself.
- **D2.** One diagnostic per set: W061, W113, E041 and E042. The cycle rules keep one per member.
- **D3.** `Cycle { members, path }`: `members` sorted; `path` a closed walk over real edges from the smallest member, each step to the nearest member not yet walked, then back. The answer depends only on the edge set, never on written, insertion or hash order. W061's data keeps `ReferenceCycle { path }` (the walk, entities repeating in a tangle); a simple cycle reads as before, from its smallest id.
- **D4.** W061 stays spanless (ADR 0016 D7: the LSP places it at the first entity's name with the others as related information; ADR 0074 D6: the delta ignores a move). W113 keeps its unclosed text, so a two-cycle still reads `a.spec -> b.spec`.
- **D5.** The answer is the std-only leaf crate `specforge-cycles` (`crates/leaf/cycles`), so the formal and product guests link what the host links (the coverage rule's precedent). `specforge-common` keeps no copy and no re-export. The graph, the registry, the resolver, formal E041/E042 and product's `in_cycle` read it; `find_cycles`, `CycleOptions` and the `report_paths` option (no caller) are deleted.
- **D6.** The package's peer-set cycles (E027) stay their own: they are correct and ordered by install order.

## Rejected

- The shortest cycle through the smallest member plus a `members` field on W061 (a breaking JSON shape for a rare tangle).
- A span on W061 (two ADRs and five tests rely on its absence).
- Leaving the guests' walks: two reproduced wrong answers would stay open.

## Consequences

- Breaking: `specforge_common::cycles` is `specforge_cycles`; `Graph::detect_cycles` is `Graph::cycles`; two cycles that share an entity are one W061 (one W113, E041, E042), whose text for a tangle is a longer walk; E007, E015, E052, W045 and W092 add the entities they missed; W113's newly named file joins the cyclic tail of the scope order.
- The repository's own spec has no cycle, so `dogfood` output does not move.
- The formal and product blobs' inputs include `crates/leaf/cycles`: re-vendored once by the round's coordinator.
- The spec's "zero false negatives" claim is proven exhaustively on every directed graph of four nodes.
