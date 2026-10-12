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

## D3. The declared categories are the only describe categories

`DeclaredCategory::ALL` is the protocol's one list. A guest answers those ten; `fields` (derived, ADR 0012 D3),
`grammars` and `body_parsers` (reserved, ADR 0004 D5-a) are gone, with `SUPPORTED_CATEGORIES`, the handshake
request's `supported_categories` (no guest read it), `GrammarDescriptor`, `BodyParserDescriptor` and
`ProtocolError::UnsupportedCategory`. `describe_items`, `describe_json`, `from_wire`'s callback,
`ExtensionCalls::describe` and the SDK's `raw_category` take a `DeclaredCategory`, so an undefined category does
not compile. `ExtensionDeclaration::declares(category)` says whether a declaration declares anything in a
category (an exhaustive match, so a new category cannot be forgotten); the package registry's search filter and
its in-memory adapter read it, and `specforge_registry_wire::declares` is gone.

## D4. What an extension contributes is what it declares; the handshake says `providers`

`ContributionFlags` is gone. `ExtensionDeclaration::declared_categories()` lists the categories a declaration
declares something in, in read order; the outline reads it. The one fact no category carries is the handshake's
`providers` (omitted when false), set by the SDK's `ContributionsBuilder::providers()`; provider registration
(W118) reads it. The protocol is 1.2.0: a 1.2 host loads every 1.x guest (it ignores an older guest's
`contribution_flags`, ADR 0013 D5); a 1.1 host cannot read a 1.2 guest's handshake, which pre-release hosts and
the SDK ship together to avoid. The same version renames the pass input's `exempt` to `owes_none` (it says the
entity owes no obligations of its own, not that it is exempt in the coverage view's sense, ADR 0019): an older
guest reading `owes_none` as absent would count unions toward coverage, so the rename rides in this version and
every builtin is rebuilt with it.

## D5. What stays though only the host calls it

`ExtensionDeclaration::from_wire` and its unknown-key walk (W138) stay in the protocol types: they define what the
wire accepts, and the SDK's raw categories and the host's load parse a category through the one function
(`set_category`). Moving them would split the declaration's read from its write (ADR 0012 D1).

## Rejected

- Deriving `SUPPORTED_CATEGORIES` from the declared ones plus a reserved list: it keeps three categories nothing
  reads.
- Keeping `ContributionFlags` derived in one place: it keeps a second vocabulary for the categories.
- A `host` cargo feature on the protocol types gating `package` and `peers`: a cfg-stripped module is not a blob
  input, but the guest/host split would be a build flag, the manifest would keep the optional `semver`, and one
  forgotten `#[cfg]` makes a host-only item a blob input again.
- `specforge-common` as the home: the registry wire and server would link ariadne, lasso, walkdir and the
  diagnostics catalog. `specforge-registry-wire`: ops and the LSP do not link it (ADR 0010).
- Hashing an extracted contract instead of source bytes: the blob is its sources (ADR 0013 D8).

## Consequences

- An edit to the package names or the peer rule stales no builtin blob; protocol-types drops `semver`; each blob
  has two fewer recorded inputs.
- `specforge outline` names the categories each extension declares, plus `providers`
  (`"contributes": ["entities", "edges", ...]` in JSON, was an object of booleans): `@specforge/rust` shows
  `analyzers`, product its `surfaces` and `passes`.
- An extension written with an older SDK still loads; one that raised `contribution_flags.providers` must call
  `providers()` and rebuild.
- Every pinned handshake loses `contribution_flags`.
- The pinned describe snapshots lose `describe_fields.json`, `describe_grammars.json` and
  `describe_body_parsers.json`.

## What would reopen it

- A contribution that no describe category carries besides `providers`: a second handshake field, or a category,
  not flags.
- A guest that needs the package or peer rule (an SDK check of its own peers at build time): it would then link
  `specforge-package` knowingly, and that crate would join the guest crates.

## Amendment (round 7, plan 16): a blob compiles no test code, and a guest depends on the SDK alone

- **A blob's inputs hold no test code.** Every test module of a crate a builtin compiles (the
  guests, the SDK and its macros, the protocol types, coverage) is declared out of line
  (`#[cfg(test)] mod tests;`), which a release build never reads; the SDK's test helpers
  (`testing`: `MockHost`, `call_every_command`) are behind its `testing` feature, which a guest
  enables only in `[dev-dependencies]`. `build-builtins`' `a_builtin_compiles_no_test_code`
  holds it. A test-only edit therefore re-vendors nothing.
- **`component_guest!` carries its own bindings** (`wit-bindgen` through the SDK, its
  `runtime_path` set), so a guest's only protocol dependency is the SDK.
- **The builtins are listed once**, `specforge_component::builtins::BUILTINS` (name, crate
  directory, artifact, bytes); `build-builtins` builds what it lists, and a test holds it to
  `extensions/*/` and to the `@specforge/<dir>` naming rule.

Rejected: hashing only non-test code in `inputs.json` (a Rust parser in xtask to keep a layout the
compiler makes free).
