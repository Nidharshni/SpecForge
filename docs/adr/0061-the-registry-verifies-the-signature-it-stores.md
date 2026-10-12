# The package signature is a wire type, and the registry verifies the one it stores

**Status:** accepted (2026-10-12). Amends ADR 0044 (D1, D2).

A package's publisher signature was a type only in the client. The wire carried it as a string, and the reference
registry checked only that the form field was not blank before storing it, reading the key id out of it as untyped
JSON. So a signature that was not one, or a well-formed signature over other bytes or another version, was stored;
every client refused that version (R-TRUST-002, which `--allow-unsigned` does not bypass), and since a published
version is immutable (ADR 0045 D4) the version number was lost. The registry's refusals carried a code no client read,
so every 400 reached the user as "network error, check your connection".

## D1. The signature is one module of the contract

`specforge_registry_wire::signature` holds the signature's JSON (`{sig, public_key, signed_at}`, hex and RFC 3339, no
other key), the content it covers (`SignedContent`: the name and version as declared, the SHA-256 of the exact binary
and manifest uploaded), its key id (the first 16 hex of SHA-256 of the public key), and how it is made, parsed and
verified (strict Ed25519; a small-order key is refused). Only a well-formed `PackageSignature` exists. The publisher,
the registry and the installer call the same functions. The signed payload's bytes are unchanged; the envelope lost
`keyId` (a copy of a function of the public key) and became snake_case like every other body. A second signature
algorithm is a protocol change, served beside `/v1` (ADR 0044's reopen clause), not a field.

## D2. The registry parses and verifies before it stores

An upload with no signature is UNSIGNED_PACKAGE; one whose signature is not a `PackageSignature`, or does not verify
over the name and version in its path, its binary and its exact manifest, is INVALID_SIGNATURE. Both are refused
before anything is stored, so the version stays publishable. The registry is still not the trust anchor: it judges no
key. The client verifies what it downloads again, and which keys it trusts is its pins, `trusted_keys` and
`denied_keys`.

Rejected: parsing without verifying (a well-formed signature over other bytes still burns the version, and the registry
holds everything verification needs); binding a scope to its first key on the server (it makes the registry the trust
anchor and blocks key rotation; key transparency is a consumer-side matter, `docs/registry-trust.md`).

## D3. The metadata carries the typed signature; the key id is derived

`PackageMetadata.signature` is an optional `PackageSignature` object; `key_id` left the wire, and with it the client's
check that the two copies agree. A reply carrying a signature that isn't one is not a `PackageMetadata`.

## D4. A refusal and an unreadable answer are their own errors

The error body's code is `ErrorCode`, named the same by both ends. A 400 is `RegistryError::Refused` (R008) carrying
the code and the registry's message. An answer that is not the call's wire type, a malformed signature among them, is
`RegistryError::InvalidReply` (R009), which no flag accepts. The client reads the publish receipt and refuses one that
describes another upload (R009).

## D5. A publish is always signed

`publish_to_registry` takes the signing key and `RegistryClient::publish` the signature, not options. The in-memory
client refuses a signature that does not verify as the registry does, held to the client contract (K11).

## Consequences

- A field of the signature is one change, which publisher, registry and installer compile against.
- The registry links `ed25519-dalek` (through the wire crate); the LSP still links none of it (ADR 0010).
- Registry data written before this ADR is refused (`PRAGMA user_version` 1): serve from a new data directory.
- R-TRUST-002 means "doesn't verify"; R-TRUST-004 no longer has a key-id case.

## What would reopen it

- A second signature algorithm or a signing service: a protocol version beside `/v1`.
- A registry that must decide trust itself (a curated mirror that re-signs): the trust source then becomes
  configuration, as ADR 0044's reopen clause says, and this ADR's D2 rejection is revisited.
