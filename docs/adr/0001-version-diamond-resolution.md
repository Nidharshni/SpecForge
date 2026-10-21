# Version diamond resolution is intersection-based unification, not backtracking (C8-07)

**Status:** accepted; amended by [ADR 0041](0041-peer-requirements-are-one-module.md) (2026-10-08), and by plan 08 of round 19 (2026-10-21)

When two installed extensions each declare a peer dependency on the same
third package at different semver ranges (a "version diamond"), `specforge
add` must decide whether a single locked version can satisfy both, instead of
silently installing and leaving `doctor` to discover the conflict later.

We resolve this by **intersection**: collect every requirer's range for the
shared peer, fetch the peer's published versions from the registry, and pick
the highest version that satisfies every range simultaneously
(`specforge_ops::extension::check_diamonds`; ADR 0010 moved it out of the registry client).
If no version satisfies every requirer, `add` fails with a diagnostic
(`R-RES-005`/`R-RES-006`) naming each conflicting requirer and its range,
rather than guessing.

We deliberately do **not** implement general backtracking — searching for an
alternative combination of *other* packages' versions to unblock an
otherwise-unsatisfiable diamond (the way Cargo's resolver can downgrade an
unrelated dependency to make room). SpecForge's install model is flat (one
locked version per extension name, no lockfile-wide SAT search), and the
extension graph is shallow enough in practice that unresolvable diamonds are
rare and best fixed by the user upgrading the narrower requirer. Backtracking
would add real complexity (a proper resolver algorithm, non-deterministic
output ordering, harder-to-explain failures) for a case that hasn't shown up
yet. If it does, that's its own design doc and its own ADR — this one only
covers unification.

## Amended by ADR 0041

The gate judges a peer by the one peer rule (`specforge_protocol_types::peers::verdict`). A peer
range that is not SemVer, the new extension's or another requirer's, is refused with E073 (it was
let through, or R-RES-003). A local install is gated too: with no registry to unify against, a
locked peer outside its range is E027. Every add checks the locked extensions that require the
package it installs, as `update` checks the dependents of what it updates.

## Amendment (round 7): one rule picks the unified version

The intersection is `specforge_package::highest_matching`: the highest published version every
requirer's range matches, each range read on its own with semver's pre-release rule (a pre-release
only when that range names its `MAJOR.MINOR.PATCH`). The ranges are never merged into one
requirement, which would admit a pre-release one requirer's range names and another's does not.
`pick` and `update`'s upgrade target read the same function. The requirers are the members of the
project peer set that declare the peer (ADR 0041, amendment round 7); the gate is
`specforge_ops::extension::lock_plan`.

**Amended (2026-10-21, architecture round 19, plan 08):** the peer's published versions are read by
the one reader `add` and `update` use (`specforge_ops::extension::resolve::published`): a peer its
registry publishes no version of is R-RES-002, not an R-RES-005 with nothing available. R-RES-005
names requirers whose ranges no *published* version satisfies.
