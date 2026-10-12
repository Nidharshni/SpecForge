# One extension declaration

**Status:** accepted (2026-10-05)

An extension's declaration (who it is, and the kinds, fields, edges, rules, enhancements,
surfaces, collectors, analyzers, passes and feature flags it contributes) was written once, in
the SDK, as `specforge-protocol-types` descriptors, then copied by the host into a second,
camelCase family (`ManifestV2`, `ManifestEntityKind`, `ManifestField`, …, a registry
`SandboxPolicy`), field by field, through a hand-written bridge. That family's only remaining
reason to exist was the `manifest.json` that `specforge publish` uploaded. Every copy lost
something: the SDK's `short` never reached the host, an extension declaring only commands (or
only passes) loaded with none of them, a field's `default_value` was dropped before the field
registry, passes were described twice more outside the load with their parse errors swallowed,
and `add`'s diamond gate trusted peers a publisher typed into `manifest.json` while the load's
E027 trusted the binary.

## D1. The protocol descriptors are the declaration

`specforge_protocol_types::ExtensionDeclaration` is the handshake plus every describe category
the host reads (`DECLARED_CATEGORIES`), typed. The SDK builds it
(`ContributionsBuilder::declaration`), the guest serves it (`describe_items`), the host loads it
(`ExtensionDeclaration::from_wire`), the registry build reads it and a package registry stores
it. There is no host-side mirror, no bridge and no `manifest.json`. A new declared field costs
one optional field on one type.

## D2. The loader asks every category, whatever the flags say

`specforge_wasm::protocol::load_declaration` reads the handshake (protocol major checked,
sandbox deadline applied) and then every declared category, unconditionally: 1 + 10 calls per
extension per environment load, and nothing describes a category again. Contribution flags are
derived from the content (`ExtensionDeclaration::contribution_flags`) and stay on the wire for
compatibility; only `providers`, which has no describe category, is read from them. Every guest
is SDK-built (ADR 0004, 0011) and the SDK answers every supported category, `[]` when empty, so
no guest breaks; command-only and passes-only extensions now load what they declare.

## D3. `fields` is derived

The `fields` category is every kind's fields, concatenated (`describe_items("fields")`); the
host never reads it and the builtins' hand-written copies (2,459 lines) are gone. What the host
reads as extension-wide fields is `shared_fields`.

## D6. Publish derives the stored declaration from the binary

`specforge publish [PATH]` (a `.wasm` component, or a crate whose
`target/wasm32-wasip2/release/*.wasm` is taken) loads the binary, loads its declaration, runs the
registry build over it alone, refuses on an error before any network call, and uploads the
declaration's JSON as the package's manifest. The publisher signature already binds the exact
manifest bytes and the binary's hash. The server parses an `ExtensionDeclaration`, checks its
identity against the URL and takes the description and
keywords it shows from the handshake. *(The `network_access` refusal was removed by
[ADR 0037](0037-the-sandbox-is-limits-the-host-enforces.md): no host grants network.)* Rejected: the server deriving the declaration itself (it
would link wasmtime and execute uploaded code; the check in D7 gives the same guarantee).

## D7. `add` verifies the served declaration against the binary

`Package` carries the served `declaration` (replacing its `peers`). The diamond gate decides on
`declaration.peers()` before anything is loaded (ADR 0001's order); after loading, the binary's
declaration must equal the served one, field for field, else `METADATA_MISMATCH` naming the first
differing category. This refines ADR 0010: `Registry::fetch` returns a `Package` whose
declaration is checked against its binary, after the identity check.

## D8. No camelCase form is kept

The stored manifest is the snake_case protocol JSON. A package published before this change (its
stored manifest has `manifestVersion`) is refused by `add` and `update` with
`UNREADABLE_MANIFEST` and the suggestion to re-publish it with this version of `specforge
publish`. Rejected: a read-only legacy reader (it keeps a camelCase type alive for packages no
public registry holds, ADR 0004 N1) and a server-side rewrite (it would invalidate the publisher
signature over the manifest's hash).

## D11. The registry build owns declaration validation; the environment keeps runtime failures

*(Amended by ADR 0041: the build first puts the declarations in load order, dependencies first; the
declarations' own diagnostics are E030, W021, the peers' E073 and E027, the cycles' E027, then W145.)*

*(Amended in architecture round 6, plan 08: `Environment::diagnostics()` reports the environment's own
first — the load's failures and warnings, provider registration (W118/E057), I002 — then the registry
build's whole list in its one order, `RegistryBuild::diagnostics()`, which ends with the surfaces' E055
and E039; see the amendment below.)*

*(Amended by ADR 0054: the extension load's diagnostics come entry by entry — an entry's failure, or
W149 then W153/W138 — with the lock's E033 once before the first entry it left unloaded; every
diagnostic of the environment and the build carries its setup part.)*

`build_registries(Vec<ExtensionDeclaration>) -> RegistryBuild` checks the declarations
themselves (E030 identity and shape, W021 consistency against the loaded peers, E027 peer
dependencies, W145 pass order cycles, in that order, extension by extension within each),
orders each extension's passes (`RegistryBuild::passes`, `check_passes()`, `analyze_passes()`)
and populates the registries; it is pure. `Environment::diagnostics()` reports, in order: the
extension load's failures (E033 for the lock once, then E028/E070/W149 per entry, ADR 0028) in
entry order, load warnings (W153, then W138, ADR 0037), the declarations' own diagnostics, provider
registration (W118/E057), I002, then the registry build's. The runtime keeps no load state:
`Installed::load` returns it, and the environment keeps it. E028 and E030 used to interleave
extension by extension; a consumer diffing `check` output sees them reordered only when both occur
in one project.

## D13. W138 and E030 are re-scoped, not retired; W145 is new

W138 is a describe item key the protocol does not define (a typo in a hand-written
`raw_category` item, or SDK/host skew), reported at load. E030 is an invalid extension
declaration: an empty name or version, an `ext_short` that is not lowercase kebab case, an
analyzer without a language, file extensions or exports. W145 is a pass order cycle, which keeps
declaration order (it was an `eprintln!` the LSP and MCP sent to stderr).

## Consequences

- One type family from the SDK to the package registry; the registries embed the descriptor
  they were built from (`FieldRegistryEntry::declared`, …), so a declared field reaches every
  reader (`default_value`, `inference_guide`, a kind's `incremental`).
- The SDK's `short` routes commands (`specforge <short> <command>`, MCP `specforge.<short>.*`);
  the macro refuses a `short` that is not lowercase kebab case at compile time.
- The handshake carries the extension's `description` and `keywords`; every builtin describes
  itself.
- The nine builtins declare with the SDK builders instead of ~7.9k lines of hand-written
  describe JSON; their exact wire answers are pinned under
  `crates/specforge-component/tests/declarations/` (`xtask snapshot-builtins` writes them).
- `manifest.json` is no longer read anywhere; `specforge extension init|build|validate` work on
  an SDK crate and its built component.
- The host-side code that only the mirror family fed (the manifest bridge, sidecar discovery,
  contribution-export checks, query extensions, upgrade) is deleted with it, and the diagnostics
  only it emitted are retired: E017, E018, E020, E023, E035, W028, W116, W117.
- `specforge-wasm` no longer depends on `specforge-registry`: the loader produces the declaration
  the registry build consumes.

## What would reopen it

- A host that needs a declaration without loading the extension's code: then a signed,
  server-verified derivation of the declaration.
- A category too large to describe on every load: then a batched describe and a minor protocol
  version.

## The registry build reports its diagnostics in one order (amendment, architecture round 6, plan 08)

D11 put the build's diagnostics in three public lists (declaration, registry, surface) and left the
order to whoever read them. Three readers stitched them in two orders: the environment interleaved
provider registration and I002 between the declaration and registry lists and reported the surface
list last, after the graph and the checks; `publish` and the registry's tests chained the three. The
environment also appended the custom rules' load-time probes (W112) to the registry list from
outside.

- **One order, owned by the build.** `RegistryBuild::diagnostics()` is every diagnostic the build
  reports: the declarations' own (E030, W021, E073 and E027, W145), the registries' (E026, W018,
  W019, W021, I004; W017; the rule set's W150, W147 or W112; W023), the custom rules' probes (W112),
  then the surfaces' (E055, E039), each group extension by extension in load order. The lists are
  private.
- **The build runs its probes.** `RegistryBuild::probe(&dyn CustomVerdicts)` asks the port once per
  custom rule and keeps the W112s in their place; probing again replaces them. The environment
  probes when its extensions loaded in a runtime.
- **The environment's own, then the build's.** `Environment::diagnostics()` reports, in order: E069,
  the extension load's failures (E033 once, then E028/E070/W149 per entry) and warnings (W153, then
  W138), provider registration (W118/E057), I002, then `RegistryBuild::diagnostics()`.
  `CompiledProject::diagnostics()` follows with the imports', the graph build's and the checks'.
- **Read through methods.** The build's registries, rule set, passes and surfaces are read through
  `kinds()`, `fields()`, `edges()`, `rules()`, `passes()` and `surfaces()`; none is a public field.

Consequences: provider registration (W118/E057) now comes before the declarations' own diagnostics
instead of between them and the registries'; E055 and E039 now come with the environment's
diagnostics, before the imports', instead of last. A consumer diffing `check` output sees either only
when both kinds occur in one project. I002 is unaffected: it is reported only when no declaration
loaded, when the build reports nothing.

**Amended by ADR 0050 (2026-10-11):** D2: contribution flags are gone; what a declaration contributes is its
content (`ExtensionDeclaration::declares`), and the handshake's `providers` says it implements configured
providers. D3: the `fields` category is gone; the declared categories are the
protocol's only describe categories.
