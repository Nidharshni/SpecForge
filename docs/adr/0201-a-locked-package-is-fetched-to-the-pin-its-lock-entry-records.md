# A locked package is fetched to the pin its lock entry records

**Status:** accepted (2026-10-21, architecture round 22, plan 02). Amends [ADR 0059](0059-trust-pins-are-written-with-the-lock-plan.md)
D3 (the lock's pin, not its key alone) and D5 (contract clause C12), [ADR 0065](0065-every-extension-change-is-one-lock-plan-change.md)
D3 (a dry run refuses a locked version's other bytes from the manifest) and [ADR 0138](0138-the-lock-plan-answers-what-a-project-already-has.md)
D1 (an entry that pins no hash is never already installed).

## Context

`specforge.lock` pins a registry install by version, SHA-256 and publisher key, but only the key reached the fetch
(`Installed::locked_key`); the hash decided "already installed" and was otherwise replaced by whatever the registry now
served.

- On a fresh clone, the E028 remedy `specforge add @sdk/beta@2.0.0` (CLI and MCP `add_extension` alike) installed another
  binary under the locked version (kind `beta` became `betab`) and rewrote the committed hash `e46e61c5...` to
  `6b950448...`, exit 0.
- The same happened to any module changed on disk, not only to a clone.
- The dry run of that add answered `planned`: the preview held the manifest's SHA-256 and compared it with nothing.
- The key half of the pin was held (R-TRUST-003 for a changed key); the hash half was not.
- W149's remedy for an entry that pins no hash is the same `specforge add`, which answered "already installed" and
  never pinned it.

## Decision

- **D1. The lock's pin is the fetch policy's.** `TrustPolicy` holds the `LockedPin` (version, SHA-256, key id) of the
  name's registry entry (`Installed::locked`). Its key is expected at any version (ADR 0059 D3, unchanged); its bytes
  only at its own version.
- **D2. Every adapter refuses a reply for the pinned version whose SHA-256 is another (R014)**, after the reply names the
  package and before the download, the signature check or any question (`TrustPolicy::admits`); the integrity check
  (R-OPS-002) then makes the downloaded bytes the admitted ones. No consent and no flag accept it: the remedy is
  `specforge remove` then `specforge add`, two deliberate commands a review sees as a lock diff.
- **D3. A dry run refuses it too**, from the published manifest's SHA-256: no download (ADR 0065 D3's "what only the
  binary or its signature can fail" is not this).
- **D4. An entry that pins no hash pins no bytes.** A fetch of its version is never "already installed", so W149's
  remedy pins it.
- **D5. Both adapters keep the rule**: `assert_registry_contract` clause C12.

## Rejected

- The check in the lock plan after the fetch: it downloads first and can ask a key-change question about bytes it then
  refuses.
- R-OPS-002 or R-TRUST-004 for the refusal: other causes (a corrupt download, a reply about another package), other
  remedies.
- Consent through `--yes`: `--yes` accepts a key change for CI, and CI's `--yes` would take swapped bytes silently.
- A `--relock` flag that drops the bytes from the pin: `remove` then `add` already is the explicit act.
- `TrustPolicy` carrying `&LockFileEntry`: a dependency edge for the adapter crates' tests.

## Consequences

- A clone restores the committed bytes or refuses; `update` is unchanged (it never fetches the locked version).
- `add` at another version pins new bytes, the lock's key still expected.
- MCP `add_extension` answers `isError`, `schema_mismatch`, R014.
- `docs/registry-trust.md`'s "pin exact versions via `specforge.lock`" becomes true.
- An add of a hashless registry entry needs its registry (E063 without one).

## What would reopen it

A registry protocol whose versions are mutable; a lock that pins bytes per registry; a `--force` reinstall (ADR 0138's
reopen clause).
