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
