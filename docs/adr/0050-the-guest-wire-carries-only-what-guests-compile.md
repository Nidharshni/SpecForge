# The guest wire carries only what guests compile

**Status:** accepted (2026-10-11). Amends ADR 0036 D1, ADR 0041 D1, ADR 0044 D1, ADR 0012 D2 and D3, and ADR 0004
D5-a.

Every builtin blob is built from `specforge-protocol-types`, the SDK, its macros and the coverage rule: `xtask
build-builtins` records every file rustc reads for a blob and stales the blob when one changes. ADR 0044 D1 kept
the registry wire out of the protocol types for that reason ("a blob input of every builtin"), yet ADR 0036 D1 and
ADR 0041 D1 put the package names (`package.rs`, 726 lines) and the peer rule (`peers.rs`, 667 lines) there: the
host's reading of a declaration's name, a version and a peer's range, which no guest, no SDK item and no SDK
macro uses. Each of them staled all nine blobs when it was added, and every later edit to either would again.

The same crate described what an extension contributes three ways. `SUPPORTED_CATEGORIES` named 13 describe
categories, three of which (`fields`, `grammars`, `body_parsers`) the host never reads; `DeclaredCategory::ALL`
named the 10 it reads; `ContributionFlags` carried 10 booleans of another vocabulary (`validators` for
`validation_rules`, `entities` for four categories, `renderers`, `prompts` and `parsers` for nothing), derived from
the content twice (the declaration and the SDK) and read for one fact, `providers`. The outline printed the flags,
so an analyzer extension contributed nothing and passes and commands were never named.

## D1. The guest wire holds only what both sides compile

`specforge-protocol-types` is the declaration, the operation payloads (`calls`), the extension vocabulary and the
command-arg rule. An item that only the host reads lives host-side. A test holds every builtin's recorded inputs to
its own sources and the guest crates (protocol-types, the SDK, its macros, coverage).

## D2. Package names and the peer rule are `specforge-package`

`specforge_package` holds `PackageName`, `VersionRequirement`, `PackageRef` and `peers` (`PeerRequirement`,
`verdict`, `Peers`), moved unchanged, and re-exports `semver::{Version, VersionReq}`. A declaration's name is read
as a package name by `PackageName::declared_by`. `PeerDependency` stays the wire's text in the protocol types;
reading and judging it is the host's. The crate depends on the protocol types, serde and semver; the host crates
that read names or peers depend on it with plain path dependencies (the root manifest's inherited tables are a
blob input). It stays pure and diagnostic-free; `specforge_common::{package, peers}` build the diagnostics.

## Rejected

- A `host` cargo feature on the protocol types gating `package` and `peers`: a cfg-stripped module is not a blob
  input, but the guest/host split would be a build flag, the manifest would keep the optional `semver`, and one
  forgotten `#[cfg]` makes a host-only item a blob input again.
- `specforge-common` as the home: the registry wire and server would link ariadne, lasso, walkdir and the
  diagnostics catalog. `specforge-registry-wire`: ops and the LSP do not link it (ADR 0010).
- Hashing an extracted contract instead of source bytes: the blob is its sources (ADR 0013 D8).

## Consequences

- An edit to the package names or the peer rule stales no builtin blob; protocol-types drops `semver`; each blob
  has two fewer recorded inputs.

## What would reopen it

- A guest that needs the package or peer rule (an SDK check of its own peers at build time): it would then link
  `specforge-package` knowingly, and that crate would join the guest crates.
