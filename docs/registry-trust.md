# Registry trust model (v1)

How SpecForge keeps `specforge add` / `specforge update` trustworthy when
installing third-party extensions from a registry, what the system guarantees,
and what it deliberately does not (yet).

## Configuring a registry

SpecForge ships **no built-in registry** (ADR 0004 N1). A project lists the
registries it trusts in `specforge.json`:

```json
{
  "registries": [
    { "alias": "main", "url": "https://registry.example.com/v1", "default_registry": true },
    { "alias": "acme", "url": "https://extensions.acme.internal/v1", "scope_filter": "@acme" }
  ]
}
```

- A package whose scope matches an entry's `scope_filter` comes from that
  registry; every other package comes from the entry marked
  `default_registry`.
- With no `registries`, `specforge add @scope/name@version`, `update`,
  `search`, `publish` and `login` (and the MCP `add_extension` tool) fail with
  [E063](diagnostics.md#e063) **before any network call**.
- Builtin extensions (`specforge add @specforge/product`) and local `.wasm`
  files need no registry.

## What is verified

Every registry install performs four checks, in order:

1. **Integrity** — the downloaded wasm bytes must match the `sha256` the
   registry reports (transfer-level check).
2. **Publisher signature** — packages uploaded via `specforge publish` carry an
   Ed25519 signature object `{sig, public_key, signed_at}` over the canonical payload
   `{name, version, wasmSha256, manifestSha256, signedAt}`. The registry refuses an upload
   whose signature isn't one or doesn't verify over it (`INVALID_SIGNATURE`), so a stored
   package's signature verified once already. The client
   re-derives the payload from the **downloaded wasm bytes** and the
   **manifest the registry serves** (the package's extension declaration,
   which `specforge publish` derives from the binary; ADR 0012) and verifies the signature with the public
   key carried inside the signature object. A tampered binary *or* a swapped
   manifest breaks verification.
3. **Key policy (TOFU)** — on the first verified install the publisher's key id
   is **pinned** (recorded in `specforge.lock` and in
   `~/.specforge/known-keys.json`). Later installs must present the same key.
4. **Declaration** — once loaded, the binary's own declaration must equal the
   served one, field for field; the peers the served one declares are what the
   version-diamond check decided on before anything was loaded.

The registry stores and serves signatures, but is **not the trust anchor**:
even a fully compromised registry cannot forge a valid publisher signature —
it can only replay previously signed packages (see [residual risks](#residual-risks)).

## Failure behavior

| Situation | Behavior |
|---|---|
| Reply (or its manifest) names another package or version than requested | **Always refused** ([R-TRUST-004](diagnostics.md#r-trust-004)), before any key is pinned |
| Manifest missing or unreadable | **Always refused** ([R-OPS-004](diagnostics.md#r-ops-004)): it declares the peers the diamond check needs, so it is never read as "no peers" |
| Manifest published before extension declarations (it carries `manifestVersion`) | **Always refused** ([R-OPS-004](diagnostics.md#r-ops-004)), suggesting to re-publish the package with this version of `specforge publish` |
| Binary declares other than the served declaration | **Always refused** ([R-TRUST-004](diagnostics.md#r-trust-004)), naming the first category that differs |
| Package unsigned | Refused. `--allow-unsigned` accepts the risk explicitly |
| Signature doesn't verify (tampered wasm/manifest) | **Always refused** ([R-TRUST-002](diagnostics.md#r-trust-002)): `--allow-unsigned` does not bypass a broken signature |
| Registry answer isn't the protocol's JSON (e.g. a signature that isn't one) | **Always refused** ([R009](diagnostics.md#r009)), before any key is pinned |
| Key differs from the pinned key | Refused with both key ids; interactive re-pin offered (`y/N`); `--yes` accepts non-interactively for CI |
| Key on your `denied_keys` list | Refused outright (config-level revocation) |
| Key on your `trusted_keys` allowlist | Accepted without a prior pin (pre-seeded trust) |

## Managing keys

All publisher key state lives in `~/.specforge/known-keys.json`:

```json
{
  "pins": { "@acme/tools": "c0a2809dc5bb6c54" },
  "trusted_keys": ["9be31c0f77aa2d10"],
  "denied_keys": ["deadbeef00000000"]
}
```

- **Rotate your own key**: sign with the new key; consumers see the key-change
  flow and re-pin after explicit consent.
- **Revoke a key**: add its key id to `denied_keys` (yours or a compromised
  third party's). There is no server-side revocation in v1 — every consumer
  decides via their own config.
- **Pre-seed trust**: ship a `known-keys.json` with `trusted_keys` for
  air-gapped environments.

`specforge.lock` records the key id beside the wasm hash for every signed
registry install, so pins are auditable per project.

## Namespace ownership

- **First claim wins**: the first publish into a scope (`@acme/...`) registers
  that scope to the publishing token's publisher identity.
- **Only the owner publishes** into a claimed scope; anyone else gets
  `SCOPE_OWNED`. Tokens scoped by the registry admin (`@web`-style) are
  additionally limited to their own scope and cannot claim others.
- **Publisher identity**: the registry assigns an account id
  (`acct_...`) at first claim; `publisher` metadata is that id — never a
  self-chosen label. There are no Verified/Community badges; the signature
  key id is the trust signal.
- Scope reassignment (recovery after losing a token) is a registry-admin
  operation in v1.

## Residual risks (stated plainly)

- **Registry-controlled version lists**: `specforge update` resolves the
  newest version within the locked version's caret range (any version with
  `--major`) from the registry, and a compromised registry can serve an *older*, still
  validly signed version (a downgrade within the semver range you accept).
  Version-list signatures (TUF-style) are deferred past v1. Pin exact versions
  via `specforge.lock` when this matters.
- **Replay**: a signed package stays signed forever. If a publisher's *old*
  release is vulnerable, its signature is still valid; review the version you
  install, not just the signature.
- **TOFU first contact**: the first install trusts whichever key signs the
  first package you see. If your first install happens over a compromised
  channel, the pin anchors the wrong key. Pre-seed `trusted_keys` for
  high-assurance setups.
- **No key transparency**: v1 has no log proving a publisher uses one key
  consistently across all users. Compare pins with your team out of band.
- **Unsigned packages**: the reference registry refuses them, and refuses a
  signature that doesn't verify; another registry may not. `specforge add`
  refuses an unsigned package unless you pass `--allow-unsigned`.

## Runtime sandbox note

An extension runs with no capability and within the limits its handshake
declares, at most the host's ceiling; the host enforces both at execution time
(see Sandbox in the [extension protocol](extension-protocol.md), ADR 0037). The host embeds wasmtime 49.x
directly through the component model, with no intermediate plugin layer, so
runtime security fixes can be taken as soon as wasmtime ships them. CI runs
`cargo audit` against every committed `Cargo.lock` with no ignore list and
fails on vulnerabilities, unsound crates, and yanked versions.
