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
    { "alias": "main", "url": "https://registry.example.com", "default_registry": true },
    { "alias": "acme", "url": "https://extensions.acme.internal", "scope_filter": "@acme" }
  ]
}
```

- `url` is the registry's root: SpecForge adds the protocol's `/v1` to every
  request. A url ending in `/v1`, or one that isn't an `http`/`https` URL, is refused with
  [E067](diagnostics.md#e067), and so is every registry operation while any entry can't be read.
- A package whose scope is an entry's `scope_filter` (`@acme`) comes from that
  registry. A `scope_filter` that isn't a scope (`acme`, `@acme/`, `@Acme`) is
  [E067](diagnostics.md#e067), and no registry is asked. Every other package comes from the entry marked
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
3. **Key policy (TOFU)** — a package must be signed by its pinned key: yours in
   `~/.specforge/known-keys.json`, or, when you have none, the key the project's
   `specforge.lock` records for it. With neither, the first verified key is
   trusted on first use. The key is pinned in both places when the install is
   written; an install that is refused or fails pins nothing.
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
| Registry answer isn't the protocol's (e.g. a signature that isn't one, or a 404 without the protocol's NOT_FOUND error: the url isn't a registry's root) | **Always refused** ([R009](diagnostics.md#r009)), before any key is pinned |
| The version `specforge.lock` pins, served as other bytes | **Always refused** ([R014](diagnostics.md#r014)) before the download and before any key question; `--yes` does not accept it; `--dry-run` reports it. Remove the extension and add it again to take the new bytes |
| Key differs from the pinned key | Refused with both key ids; interactive re-pin offered (`y/N`); `--yes` accepts non-interactively for CI |
| Key differs from the key `specforge.lock` records (no pin of yours) | Refused like a pinned key change, naming `specforge.lock`; `--yes` or a yes at the prompt accepts it |
| `known-keys.json` can't be read | **Always refused** ([R-TRUST-006](diagnostics.md#r-trust-006)); the file is never overwritten |
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
registry install. It is the project's pin: a machine with no pins of its own (a
new teammate, CI) holds the package to it, and the version it pins to the bytes it pins.

## Namespace ownership

- **First claim wins**: the first publish into a scope (`@acme/...`) registers
  that scope to the publishing token's account.
- **Only the owner publishes or yanks** in a claimed scope; a token issued to
  another account gets `SCOPE_OWNED`. Tokens scoped by the registry admin
  (`--scope @web`, a scope; `@web/ui` or `web` is refused) are additionally
  limited to their own scope and cannot claim others. A token issued without
  `--scope` covers every scope.
  A token stored with a scope filter that is not a scope (issued before the
  filter was checked) authenticates nothing, and `specforge-registry token list`
  refuses naming it and the `token revoke` command that removes it.
- **Accounts**: every token is issued to an account (`specforge-registry token
  create --account`), so a rotated or expired token's replacement keeps the
  scope. `publisher` metadata is the account name the operator chose — never a
  label the publisher sends. There are no Verified/Community badges; the
  signature key id is the trust signal.
- Scope reassignment (a hand-over, a takedown) is `specforge-registry scope
  assign`, run by the operator against the data directory.

## Residual risks (stated plainly)

- **Registry-controlled version lists**: `specforge update` installs only a
  version newer than the locked one (within its caret range, any newer one
  with `--major`), so a registry cannot make an update install an older
  version. A compromised registry can still withhold newer versions (a
  freeze), or serve a newer, validly signed but malicious one. Version-list
  signatures (TUF-style) are deferred past v1. `specforge add
  <name>@<version>` installs exactly what it names, older included; pin exact
  versions via `specforge.lock` when this matters.
- **Replay**: a signed package stays signed forever. If a publisher's *old*
  release is vulnerable, its signature is still valid; review the version you
  install, not just the signature.
- **TOFU first contact**: the first install trusts whichever key signs the
  first package you see. If your first install happens over a compromised
  channel, the pin anchors the wrong key. Pre-seed `trusted_keys` for
  high-assurance setups. A project's lock carries its pins, so only a machine
  installing a package the lock doesn't yet hold trusts on first use.
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
