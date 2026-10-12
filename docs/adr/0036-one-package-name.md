# One package name and version requirement

**Status:** accepted (2026-10-07)

What an extension package is called, and which of its versions `specforge add` asks for, was read by
twelve pieces of code in seven crates: three `rfind('@')` splitters (the wasm specifier, the registry
client, the `specforge.json` entry), an ops pre-check for `@scope/name`, an `is_range` test, two
resolvers (the client's and the ops test fake's, which disagreed on pre-releases), the server's
inline name check and scope extraction, the registry chooser, the token scope check, the scaffold's
name check and the default short name. The string crossed the ops ↔ client seam twice (ops parsed
it, the adapter joined it back, the client parsed it again and chose the registry again). Six live
bugs followed: ops and the client asked for different packages (`foo@/bar`); a version reached the
URL unescaped (`1.0.0/x`, `1.0.0?x=1`); `1.x` and `1.2` were fetched as one version; a module
declaring `../../../outside1` was installed outside the project and `remove` deleted that directory;
the server published `@acme/..`; and `latest` was a pre-release in production but not in the fake.

## D1. One module, in the protocol types

`specforge_protocol_types::package` holds `PackageName`, `VersionRequirement` and `PackageRef`. A
string is parsed into one of them where it enters (the `add` argument, a `specforge.json` entry, a
lock entry, a declaration being installed or published, a registry URL) and passed as itself. It
lives in `specforge-protocol-types` because every consumer already depends on that crate except
`specforge-common`, which gains the edge, and because a declaration's name is the package's name.
The crate stays pure and diagnostic-free; callers map its errors (E054, R-RES-003, E072, E065,
`INVALID_NAME`/`INVALID_VERSION`).

## D2. A name is a path

A package name is `@scope/name` or `name`, each part `a-z 0-9 . _ -` starting with a letter or
digit, at most 214 bytes: npm's rule. So it is always one or two normal path components and one URL
segment once its `/` is encoded. A registry holds only scoped names. The installed layout joins
only `PackageName::relative_path()`.

## D3. A requirement is resolved by ops, by one rule

`latest`/`*`, a full version (exact, fetched directly) or a SemVer requirement read as Cargo reads
one. `VersionRequirement::pick` chooses: the highest matching version; for `latest` the highest
release, a pre-release only when there is no release. The `Registry` port (ADR 0010) lists versions
and fetches one by `PackageName` and `Version`; it does not resolve. The registry client fetches the
typed name and version from the one registry the adapter chose.

## Consequences

- Accepted that was refused: `@a/x` (one-character parts). Refused that was accepted: unscoped
  registry names, uppercase, any character outside the set, `.`/`..` parts, more than one `/`, a
  version part that is no version or requirement, a declared name that is not a package name, and a
  `specforge.json` entry that names no package (E072). `1.x`/`1.2` resolve as requirements.
  `latest` skips pre-releases while a release exists.
- A registry that already stores a name the rule refuses can no longer serve it: no client could
  install it safely.
- Not done: splitting the rest of the server's publish handler; that is about uploads, not names.

**What would reopen it:** a registry that must hold unscoped or mixed-case names (an import from
another ecosystem). The rule would then grow a registry-side alias, not a looser `PackageName`.

**Amended by ADR 0045 (2026-10-08):** D3's "the one registry the adapter chose" is
`Configured::registry_for`: the scope's entry, else the default entry, else R-OPS-001 before any
request; there is no fallback to the first entry, and `publish` asks the same registry.

**Amended by ADR 0050 (2026-10-11):** D1's module is the host-side crate `specforge-package`
(`specforge_package::{PackageName, VersionRequirement, PackageRef}`, `Version` re-exported): the protocol types
are a blob input of every builtin and keep only the guest wire. A declaration's name is read as a package name by
`PackageName::declared_by`; `specforge-common` gains the edge to `specforge-package`, not to the protocol types.

## Amendment (architecture round 7, plan 08): what an update moves to, and what a search lists

D3 chose the version a requirement asks for. `update` asked `pick` for `^locked`, or for `latest` under
`--major` or when the locked version was not SemVer, and counted any version other than the locked one as an
update. So `update --major` moved a lock down: from a pre-release (`2.0.0-beta.1`, with `1.4.0` published,
went to `1.4.0`, since `latest` skips pre-releases) and from a version no longer published (`0.3.0`, with only
`0.1.0` left, went to `0.1.0`). Without `--major` the second lock failed R-RES-004 and, an update being all or
nothing, held back every other extension. A registry's search listed each package at its highest version,
pre-releases included (the server ranked pre-release identifiers as text), where `add` installs `latest`'s
pick.

- **A1. One upgrade rule beside `pick`.** `specforge_package::upgrade_target(locked, published, reach)` is
  what an update moves a locked package to: `pick` over the range `>locked` (and `^locked` unless
  `Reach::Major`, `--major`). So it is the highest published version above the locked one by precedence,
  and semver's pre-release rule applies as to every range: a pre-release is a target only on the locked
  pre-release's own `MAJOR.MINOR.PATCH`, a release never moves to a pre-release, and `--major` reaches no
  pre-release of another version. `None` is up to date, never a lower version; a locked version no longer
  published, with nothing newer, is up to date. `update` lists once and asks it (`resolve::upgrade`);
  `VersionRequirement::compatible_with` and `resolve_requirement` are gone.
- **A2. A locked version that is not SemVer** moves to what `latest` picks, the version `specforge add <name>`
  installs and the command `Installed::reinstall` names, in either reach.
- **A3. A search lists the version `add` installs.** Each registry lists a package once, when any of its
  versions matches the term, at what `pick` chooses for `latest` (`VersionRequirement::pick_from` over the
  stored records); `--contributes` is judged on that version. `pick`, `upgrade_target` and a version
  diamond's unification all read `highest_matching(published, requirements)`: the highest version every
  requirement matches. The server, `MemoryClient` and `MemoryRegistry` all do; the port contract holds them
  to it (R-S2).

Rejected: `--major` as `latest` floored at the locked version (from `2.0.0-beta.1` it would reach less than
plain `update`, which moves to `2.0.0-beta.2`); "the highest release above the lock, else a pre-release"
(it moves a stable lock onto another major's beta); failing an unreadable locked version (one hand-edited
entry would block every upgrade); failing a locked version no longer published (moving forward or not at all
is the update's whole job; saying the version was yanked is a listing's or `doctor`'s); an `outdated` command
(none exists; it would read `upgrade_target`).

**What would reopen it:** an opt-in to pre-releases (`update --pre`, `add` of a new major's pre-release by
default). It would widen the admitted range, not add a second rule.
