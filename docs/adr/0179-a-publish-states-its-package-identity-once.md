# A publish states its package identity once

**Status:** accepted (2026-10-20, architecture round 18, plan 09). Amends [ADR 0036](0036-one-package-name.md) (D1),
[ADR 0044](0044-the-package-registry-contract-is-one-crate.md) (D2, D4) and
[ADR 0045](0045-publish-is-one-operation-and-one-registry-serves-a-name.md) (D2).

## Context

`specforge publish` settled a package's name and version in ops (E072), handed them to the `Registry` port beside the
declaration (`Upload { name, version, wasm, declaration }`), and the adapter routed by the name and dropped it. The
registry client then parsed the declaration's name and version again in `publish_to_registry` and in each
`RegistryClient` adapter (HTTP and in memory): the same identity at four sites, three times per publish. Those
re-derivations had a branch, `RegistryError::InvalidPackage`, that no production caller could reach and a weaker rule
than ops' (an unscoped name passed). Nothing tied the name a publish was routed by to the declaration it uploaded:
an `Upload` naming `@acme/x` with the declaration of `@other/y` was routed to `acme` and uploaded `@other/y` there,
and `MemoryRegistry` answered the same `Upload` with another package. The in-memory client stored a manifest that
names another version than the upload, which the registry refuses (`NAME_MISMATCH`).

## Decisions

**D1. A declaration read as a registry package is one value.** `specforge_package::DeclaredPackage`, built only by
`DeclaredPackage::of`: a scoped name and a full SemVer version parsed once, carried with the declaration they were read
from. Its error's `Display` is E072's message; the name is refused before the version.

**D2. It crosses both seams as itself.** `Registry::publish(&DeclaredPackage, wasm)` (`Upload` is gone);
`publish_to_registry(&DeclaredPackage, wasm, ..)`; `RegistryClient::publish(&DeclaredPackage, wasm, manifest_json,
signature, ..)`. No adapter parses a declaration's name or version; `RegistryError::InvalidPackage` is gone.

**D3. The registry still checks what it is sent.** The server parses the path and the manifest itself (INVALID_NAME,
INVALID_VERSION, INVALID_MANIFEST, NAME_MISMATCH, INVALID_SIGNATURE): a request may come from any client. The client
contract gains K12: the in-memory client refuses a manifest that is no declaration or names another version than the
upload, as the server does.

## Rejected

- Name and version as separate arguments: a disagreeing pair stays representable, every receiver re-checks or none
  does, and the client trait grows to eight inputs.
- The value in `specforge-registry-wire` (ops cannot depend on the adapters layer) or in `specforge-protocol-types`
  (the guest wire, a blob input, and below `PackageName`).
- The server reading the client's value: bytes from the network vouch for nothing.

## Consequences

The identity is parsed once per publish. A hand-built port call can no longer route one package and upload another.
The E072 messages are unchanged, and so is the request on the wire (the signed text is the typed values' text, equal
to the declaration's for every version `semver` accepts). A publish answers its `PublishDocument`: `PublishOutcome`
only copied into it and is gone ([ADR 0052](0052-an-operations-outcome-is-one-document-both-surfaces-print.md) D1).

## What would reopen it

A registry protocol that addresses an upload by something other than name and version (a content address), or a
second kind of publishable artifact.
