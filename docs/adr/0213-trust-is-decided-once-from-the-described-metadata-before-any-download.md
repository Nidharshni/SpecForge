# Trust is decided once, from the described metadata, before any download

**Status:** accepted (2026-10-22, architecture round 24, plan 01). Reopens [ADR 0065](0065-every-extension-change-is-one-lock-plan-change.md)
D3 (a dry run decides the publisher as the add does). Amends [ADR 0059](0059-trust-pins-are-written-with-the-lock-plan.md) D5
(the decision is `TrustPolicy::decide`), [ADR 0044](0044-the-package-registry-contract-is-one-crate.md) D3 (the policy in ops is
narrowed), [ADR 0201](0201-a-locked-package-is-fetched-to-the-pin-its-lock-entry-records.md) D3 (the preview's R014 is the
adapter's) and [ADR 0010](0010-operations-reach-the-registry-through-a-port.md) (`published` takes the policy).
**D2 amended by [ADR 0221](0221-an-unsigned-package-is-held-to-its-names-expected-publisher.md) (round 25):** the adapter reads the user's store for an unsigned package too, and the expected key holds it.

## Context

A registry package's publisher was decided in one place, `trust_flow::check`, which ran after the fetch had downloaded the
binary, and a dry run never reached it.

- `specforge add --dry-run` (and MCP `add_extension` with `dry_run`) answered "would install" for an unsigned package, a
  signature that does not verify, a key change nobody can accept and a key on `denied_keys`; the real add refused each
  (R-TRUST-001/002/003/005) after `GET .../download`. ADR 0065 D3 said a preview cannot fail on "what only the binary or its
  signature can fail"; for the signature that premise is false.
- The signature covers the name, version, and the SHA-256 of the binary and of the manifest. The reply already states both
  digests (`PackageMetadata::sha256`, `manifest`), so the signature verifies without the bytes; the client only re-hashed the
  download to get what the reply says, once the integrity check had made the two equal.
- The in-memory adapter ops' tests use kept a second copy of the decision, with no allowlist and no deny list, so no ops test
  could say what R-TRUST-005 does to an add. Five commits in six days changed one decision in two or three places.

## Decision

- **D1. The publisher signature is verified over the digests the reply describes** (`SignedContent::described`), and the
  download is then held to that SHA-256 (R-OPS-002). The same proof, available before the binary.
- **D2. The decision is `TrustPolicy::decide(name, Signer, UserKeys)` in ops**, over what the adapter learned: the signer it
  verified (R-TRUST-002) and the user's pin, trusted and denied answers it read (R-TRUST-006). The signature check and the
  known-keys store stay in the adapter, so ADR 0010's guarantee (no ed25519, no `~/.specforge` read in the LSP's graph) is
  unchanged. R-TRUST-001, -003 and -005 have one home.
- **D3. Both adapters decide in one step before any download**, and `Registry::published` takes the policy: a dry run
  refuses R-TRUST-001/002/003/005/006 as the add does. What only the binary can fail (R-OPS-002, E028, a binary declaring
  other than its manifest) stays the commit's.
- **D4. The memory adapter holds pins, trusted and denied keys as data.** The contract gains C13 (`published` decides as
  `fetch` decides) and C14 (a denied key is refused whatever the consent).
- **D5. A dry run asks the add's own asker** about a key change: MCP and `--format json` never are asked, `--yes` consents,
  and on a terminal the same y/N is put and not recorded (nothing is pinned by a preview).

W155 is reported by a fetch only: a preview accepts nothing. No new diagnostic code.

## Rejected

- The decision in the adapter's `described`, moved before the download: the memory adapter (in ops, ADR 0044 D4) still cannot
  call it, so the copy and the gap in the contract stay.
- The decision in the lock plan: the port would grow a "what does the user's store say about this key" call, and the contract
  would no longer cover the decision.
- A new leaf crate for about a hundred lines: crate-map churn for no new dependency edge saved.
- A dry run that asks nobody (key change previewed as R-TRUST-003): one purpose branch for a difference only a terminal sees.
- W155 in a preview.

## Consequences

- `add --dry-run` and MCP `add_extension {dry_run}` exit 1 / `isError` where the add would; a refused package is never
  downloaded; a key-change question comes before the transfer, and nothing is pinned if the download then fails.
- A package that is doubly bad reports its trust refusal before R006 or R-OPS-002.
- `specforge-registry-client`'s `trust_flow::check` and its types are gone; ops' tests can express a deny list.

## Reopen when

A registry whose metadata does not carry the digests the signature covers; a signature over something only the binary holds.
