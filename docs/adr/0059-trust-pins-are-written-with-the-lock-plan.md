# Trust pins are written with the lock plan

**Status:** accepted (2026-10-12). Amends ADR 0010, ADR 0028 D4, ADR 0041 (amendment A1), ADR 0044 D4.
**D3 and D5 extended by [ADR 0201](0201-a-locked-package-is-fetched-to-the-pin-its-lock-entry-records.md) (round 22):** the fetch is held to the lock entry's whole pin (its bytes at its version, R014), and contract clause C12 holds both adapters to it.
**D5 amended by [ADR 0213](0213-trust-is-decided-once-from-the-described-metadata-before-any-download.md) (round 24):** the decision is `TrustPolicy::decide`; the allowlist and the deny list are in the contract (C14).
D2's `commit_with(config, last)` sentence amended by [ADR 0128](0128-one-project-change-over-held-files.md): `last` is the operation's project change (`specforge.json`, the `Also` files, then the pins); the change no longer names the files.
**D3 and D4 extended by [ADR 0221](0221-an-unsigned-package-is-held-to-its-names-expected-publisher.md) (round 25):** the lock's key and the user's pin hold against an unsigned package (R-TRUST-007), and an unreadable store refuses an unsigned install.

The fetch policy pinned a publisher key while it checked the signature (`check_and_pin`), that is, while
the lock plan was still building its installs and before its gate. An install the gate, the binary load or
the commit then refused still left its key pinned: a consented key change re-pinned the new key (W156,
"now pinned"), and a first use pinned the refused package's key. Known-keys then disagreed with
`specforge.lock`, and re-installing the locked version, which R-RES-006 tells the user to do, failed
R-TRUST-003. `update` documented the leak as policy. The in-memory registry pinned nothing, so the port's
contract could not say "a fetch writes nothing". The lock's `key_id` was written for every signed install
and read by nothing, though the trust docs called it a pin. A `known-keys.json` that did not parse was read
as empty, so `denied_keys` was ignored and the next pin overwrote the file.

## D1. A fetch decides; the lock plan's commit pins

`Registry::fetch(name, version, TrustPolicy)` verifies and decides the publisher (`Publisher`) and writes
nothing. `Registry::pin(&[KeyPin])` saves what a committed plan accepted (`Publisher::pin`: a first use or a
consented change), all or none, and the configured registry reports a W156 per re-pin then. Rejected: ops
writing the store (ADR 0010), a pending-trust handle returned by the fetch (a second type, the same
effect).

## D2. Pins are the change's last step

`Change::commit_with(config, last)` writes the binaries, the lock, then runs `last`: the `specforge.json`
edit and the pins. A pin that can't be saved (R-TRUST-006) puts every file back, and the store is replaced
whole (temp file, rename), so a failed save leaves it as it was. A crash between the lock and the pins leaves
the old pin, so the next fetch is a key change: it fails closed. Rejected: pins first (the crash leaves
this ADR's bug), pins after the change (a failed pin can't undo it).

## D3. The lock's key id is the project's pin

With no pin of the user's for a name, the fetch expects the key the lock records for its registry install
(`Installed::locked_key`). The user's own pin always wins. A machine with no pins (a teammate, CI, the
reinstall command doctor prints) is held to the team's key instead of trusting any on first use; a lock
edited to name another key replaces only what first use would have accepted. Rejected: deleting the field
(the docs promise a per-project pin and nothing else gives one); accepting either key (a lock edit would
override a user's pin). The key id stays a `String`: it is compared for equality in two places and never
parsed.

## D4. The store fails closed

A known-keys file that exists and can't be read refuses the fetch with R-TRUST-006 and is never
overwritten. A missing file is an empty store.

## D5. Both adapters keep the pin policy

`MemoryRegistry` decides first use, match, key change and the lock's key as the configured registry does,
and pins only through `pin`. `assert_registry_contract` states it: a fetch pins nothing (C3), a pin is
recorded (C8), a consented key change is decided and not written (C9), the lock's key is the pin when there
is none (C10). Allowlist and deny list are store features with their own tests, outside the contract.

## D6. Consent is still asked at fetch

Asking about a key change after the gate would need the fetch to return undecided changes and the port to
grow a decision call. Once nothing is written for a refused plan, the cost of asking first is one question the
user didn't need.

## Consequences

- A refused or failed `add`, `update` or `add_extension` pins nothing and prints no W156.
- W156 is printed after a written install only. W155 is unchanged (reported at fetch).
- On a machine with no pin, a package signed by another key than the lock records is a key change:
  refused by `--format json` and MCP, asked on a terminal; R-TRUST-003 names `specforge.lock`.
- A malformed `known-keys.json` refuses registry installs (R-TRUST-006, retitled "Known-keys file
  unusable") until it is fixed or moved aside.
- `update`'s note that a pin "stays pinned: it records trust, not a change to the project" is gone.

## What would reopen it

- Asking about a key change after the gate (the fetch would return undecided changes, and the port would
  grow a decision call).
- A registry the policy must trust per source (ADR 0044's reopening), which would add a third pin source.
- Two processes changing one user's pins at once needing more than last-writer-wins.

## Amendment (architecture round 11, plan 07)

- **D6.** Consent is asked at fetch, of the policy's `asking` (`TrustPolicy.asking`); the adapter holds no asker
  ([ADR 0110](0110-who-can-be-asked-is-one-value-handed-to-an-operation.md)).
