# Peer requirements are one module

**Status:** accepted (2026-10-08). Amends ADR 0001 and ADR 0012 D11.

An extension's declared peers (`PeerDependency { name, version, optional }`, the range as raw text)
were judged in five places that disagreed. A range that is not SemVer was a warning in `check`
(W062, and only when the peer was loaded), an error in `doctor`, satisfied in `add` and `update`,
and R-RES-003 when another requirer's range was unified. Doctor judged only lock entries, so a peer
a builtin satisfied was "not installed". The load order "dependencies first" was stated by the
spec, the docs and the code, and produced by nothing: declarations loaded in `specforge.json` order,
which decided E026, W018, enhancements, passes, rules and surfaces, so a dependent listed first took
its peer's keyword. The one sort, `specforge_wasm::topological_sort_extensions`, ran only inside
`migrate`'s hook run, so a cycle among required peers reached no compile, and failed a migration
only when a file was pending one.

## D1. One pure module, beside the package types

`specforge_protocol_types::peers` holds the rule: `PeerRequirement::read` reads a declared peer's
range as Cargo reads a SemVer requirement; `verdict(declared, installed)` is the one satisfaction rule
(`Satisfied`, `Missing`, `OutOfRange`, `NotSemver`, `Unreadable`); `Peers::of(members)` gives a set's
`load_order`, `cycles` and `unsatisfied`. It is pure and diagnostic-free like `package` (ADR 0036
D1); `specforge_common::peers` builds the diagnostics. The wire keeps the text: a declaration with an
unreadable range still loads and registers its kinds (as a bad rule costs one rule, ADR 0020 D6).

## D2. A range that can't be read satisfies nothing: E073

The range is read first, so an unreadable range is reported whether its peer is loaded, missing or
optional. It is the error E073, not W062 (retired, replaced by E073): no version can satisfy it, and
its fix is the dependent's, not the peer's. A peer installed at a version that is not SemVer is E027
(no range accepts it).

## D3. The registry build puts the declarations in load order

`build_registries` takes the declarations in entry order and loads them in load order before
anything else reads them. Load order is entry order, except that an extension comes after the peers
it declares: every required one, and an optional one unless its edge would close a cycle (optional
edges are taken in dependent-then-peer name order, after the required ones). Every first-wins rule
and every in-order list follows it. The order is a function of the entries, deterministic, and
idempotent: building from a build's declarations changes nothing. Ties keep the user's order rather
than names, so unrelated extensions keep the precedence `specforge.json` gives them.

## D4. A cycle among required peers is E027 in every compile

Each strongly connected set of required peers is one E027 naming its extensions, among the
declarations' own diagnostics (E030, W021, the peers' E073 and E027, the cycles' E027, W145). Its
members still load, together, in entry order. `migrate` runs hooks in the build's order and no longer
fails on a cycle; the sort leaves `specforge-wasm`.

## D5. Doctor reports the compile's verdicts

`Installed::health` checks modules only. Doctor reports the compile's E027 and E073 as its peer
findings (code, message, the diagnostic's suggestion), so doctor and check cannot disagree.

## D6. The gate judges by the same rule and covers every add

`check_diamonds` reads `verdict`: an unreadable range (the candidate's or a requirer's) is E073; a
locked peer out of range is unified against the registry (R-RES-006/005, ADR 0001) or, for a local
install, refused with E027. Both add paths check the locked extensions that require the package
they install, as update checks the dependents of what it updates.

## Not done

- `add` does not install a missing peer (dependency resolution); `check`'s E027 reports it.
- The gate does not consult builtins or `.wasm` file entries: their versions are not installable
  choices, so there is nothing to unify; `check`'s E027 reports a mismatch.
- The SDK does not panic on a malformed range; authors meet E073 in `extension validate`, `publish`
  and `check`.
- `PeerDependency.version` keeps its wire name and peer names stay text.

## Consequences

- A malformed peer range fails `check`, the LSP and MCP (was a warning), `doctor`, `add`, `update`
  and `publish`.
- A project listing a dependent before its peer now loads the peer first: E026/W018 name the
  dependent as the later registration, and passes, rules, surfaces and the schema's extension list
  follow dependency order. The repo's own config loads governance before testing.
- `doctor --format json`: peer problems are in `peers` with the diagnostic's code; `issues` holds
  only binary problems; `peer_mismatch` is gone.
- A local `add`, and an add that would break a locked dependent, can be refused.

**What would reopen it:** a need for user-chosen precedence that contradicts the peers (load order
overrides), or a dependency resolver for `add` (a new ADR, superseding D6's "missing peers are
check's").

## Amendment (2026-10-11): one lock plan applies the rule to every install

D6 put the peer rule in one module but left applying it to five call sites in `add`, `update` and `init`.
Each chose which lock to judge against (the lock as read, as staged, or minus the package itself), whether
a registry unifies a diamond, and whether the gate ran before or after the binary loaded. They disagreed:

- `add` of a new version of a locked package counted the package's previous lock entry as another
  requirer: R-RES-005 naming the package against itself, where `update` gave R-RES-006.
- A locked extension an install breaks was named twice among the requirers.
- `update` judged each upgrade against the lock as the upgrades before it in lock order left it, and
  skipped every pair of extensions it moved: one lock order refused a coupled upgrade the other accepted,
  and it could write a lock whose upgraded extension's peer was unsatisfied.
- `init` judged its local files one change at a time while it wrote them.

- **A1. One lock plan.** `specforge_ops::extension::lock_plan` is the only way an operation installs:
  `add` stages the install of each of its specifiers (amended 2026-10-11: an add takes several, and a
  refusal names the specifier; among several failures it reports the first the peer rule refused before
  any binary that did not load), `update` every upgrade, `init` its local files. Staging writes the install's
  entry into the lock as the operation leaves it, replacing the package's previous entry. The plan owns the
  change (ADR 0028 D4), the pin and the wording.
- **A2. Judged once, after every install is staged.** The peer rule (`Peers::unsatisfied` over the staged
  lock) judges every declared peer that touches a staged package: a staged extension's own, and every
  locked extension's on a staged one. A peer that is not locked stays `check`'s. An unsatisfied one is
  unified against the registry the staged install came from (R-RES-006/005, ADR 0001), or, for a local
  file, refused with E027. A version diamond names each requirer once. Lock order decides nothing.
- **A3. Before anything fetched is loaded.** A registry install's binary is loaded only once the plan has
  passed, both directions included, and must then declare what was published (ADR 0012). Only a checked
  plan can be committed.
- **A4. One wording.** A locked extension an install breaks: "installing '<package>' <version> breaks
  '<dependent>': ..." (`updating` for `update`). R-RES-006 for a peer the same plan moves says "but this
  update locks <version>".
- **A5. Init validates before it writes.** `init`'s plan judges the lock plan of its local files before any
  file is written; `apply` installs them as one change.

Rejected: gating each install as it is staged (it is what made `update` order-dependent); a
caller-chosen unification source (the install's own provenance decides it); a dependency resolver for
coupled upgrades (ADR 0001 still holds: the plan refuses and names the version that unifies, and
`update --major` moves locked registry extensions together).

ADR 0001's `check_diamonds` and ADR 0010's `diamond` module are now `lock_plan`; `LockFile::requirers_of`
is gone (its one use is the plan's).

**What would reopen it:** an install that must be judged against something other than the lock (a
builtin's or a `.wasm` file entry's version, today `check`'s), or a resolver for `add`.

**Amended by ADR 0050 (2026-10-11):** D1's module is `specforge_package::peers`, beside the package types in the
host-side crate `specforge-package`; `PeerDependency`, the wire's text, stays in the protocol types.

**Amended by ADR 0059 (2026-10-12), to amendment A1:** the plan also owns the trust pins its fetches decided: it
pins them through the registry port as the last step of its change, and a refused plan pins nothing.

## Amendment (round 7): the gate judges the project's peer set

The round-6 lock plan applied the rule once per change, but to its own set: the lock's entries, every
one of them, a missing peer dropped. "Not done" bullets 1 and 2 left the rest to `check`. Three sets
met the one rule: the lock plan's, `required_builtins`' walk of the embedded builtins (run only when
the thing added was a builtin, with its own E027), and the registry build's loaded declarations. They
disagreed:

- `add ./x.wasm` requiring `@specforge/software ^9` succeeded and every `check` failed E027; so did a
  registry package, `init --extensions`, MCP's `add_extension`, and an install requiring a builtin no
  entry enabled ("which is not installed").
- `add @specforge/software` succeeded though an installed extension's range rejected it: a builtin's
  add judged nothing.
- An install that broke a `.wasm` file entry's peer, or that a file entry did not satisfy, succeeded.
- An install that "broke" a locked extension no entry enables was refused, though no compile loads it.

Bullet 2's reason, "their versions are not installable choices, so there is nothing to unify", mixed
unifying with judging: a local install has nothing to unify either, and the gate refused it with E027.

- **B1. One set.** The project peer set is the extensions a project's `specforge.json` entries load,
  each entry resolved by the extension load's one rule (`Installed::resolve`, ADR 0028): a builtin by
  its embedded declaration, an installed extension by its lock entry (the version and peers its pinned
  module declares), a `.wasm` file entry by its file's declaration. An entry that loads nothing and a
  locked extension no entry enables are not members. The registry build judges the set as loaded (its
  input, unchanged); the lock plan judges it as the change leaves it (`ProjectPeers`), before anything is
  written.
- **B2. Judged whole, a missing peer included.** Every declared peer whose dependent or peer the change
  touches (a staged install, an appended entry) is judged: E073 for an unreadable range; E027 for a
  required peer no member provides, with `check`'s message and suggestion; a peer installed from a
  registry, out of range, unified against the registry the touching install came from (R-RES-006/005),
  its requirers the set's members; any other out of range (a local install, a builtin, a `.wasm` file
  entry) E027. A member a change breaks is named: "installing|updating|enabling '<name>' <version>
  breaks '<dependent>': ...".
- **B3. `add` and `init` enable the builtins a change requires.** Before judging, a verb that enables
  appends each required peer of what it adds that is a builtin no entry enables, and theirs,
  dependencies first, from any source (ADR 0028 D7's builtin-only walk is gone). `update` enables
  nothing: an upgrade's missing peer is refused, and the refusal names the `add` that enables it.
- **B4. A plan that only enables accepts any lock.** It opens no change; an unreadable lock hides the
  installed extensions from the set, as from the compile.
- **B5. Every specifier of an add is one plan** (A1 as amended 2026-10-11): a builtin one specifier
  enables is judged with the installs the others stage, and with what the project already loads.

Supersedes "Not done" bullet 2 and, for a missing peer, bullet 1: a missing peer is refused, a
builtin one enabled; a missing registry or local peer is still not installed for the user.

Rejected: a dependency resolver for `add` (a missing registry peer has a version, a trust decision
and a source to choose; still the trigger below); refusing a missing builtin rather than enabling it
(`add` of a builtin already enables its peers, and a builtin's version is no choice); keeping the lock
as the set and adding builtins and file entries to it (it judges extensions no compile loads); the
round-6 extension inventory as the set (it needs a compiled project, which `add` would have to load
into the caller's runtime, ADR 0028 D7); the registry build judging the predicted set (members that
failed to load would add E027s beside their E028).

**What would reopen it:** a dependency resolver for `add`; an operation that must change a builtin's
version; a peer whose installed module may declare other peers than its lock entry records.

## Amendment (round 8): remove and the dry runs are lock-plan changes

`remove` judges the project peer set through the plan (B1 for removals; a locked extension no entry enables keeps
nothing from being removed), the dry runs of `add` and `remove` are the plan checked and not committed (a registry
package judged by its published manifest), and A5's init is one plan, committed once (ADR 0065).

**Amended by [ADR 0165](0165-the-lock-a-change-leaves-is-built-when-its-plan-is-checked.md) (2026-10-19):** B4: a plan that stages no install or uninstall accepts any lock; it opens no change.

**Amended (2026-10-19, architecture round 17, plan 08):** B2: what a removal touches is the members that differ between
the project peer set before it and after it (one it removes, or one a `.wasm` file entry it un-shadows loads instead,
judged at that entry's version); what an add, init or update touches is what it stages and appends, which includes every
member it moves. Reopened by a verb that both installs and drops an entry: its drops must be judged by the difference.
