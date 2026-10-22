# An unsigned package is held to its name's expected publisher

**Status:** accepted (2026-10-22, architecture round 25, plan 01). Amends
[ADR 0213](0213-trust-is-decided-once-from-the-described-metadata-before-any-download.md) D2 (the adapter reads the user's
store whatever the signer). Extends [ADR 0059](0059-trust-pins-are-written-with-the-lock-plan.md) D3 (the lock's key holds
against no key as against another) and D4 (the store fails closed for an unsigned package too). Amends
[ADR 0135](0135-a-remedy-names-a-command-or-the-requests-words-never-a-flag.md) D1 (R-TRUST-001's text) and
[ADR 0110](0110-who-can-be-asked-is-one-value-handed-to-an-operation.md) (the MCP server accepts no risk on the user's
behalf).

## Context

A name the user pinned, or one whose key `specforge.lock` records, was held to that key against another key (R-TRUST-003)
but not against no key: `TrustPolicy::decide` returned for an unsigned package before it read the expected key, and the
configured adapter did not even read the user's store for one. A mirror serving a pinned package's tampered bytes with
the signature stripped installed with `--allow-unsigned` alone, or with MCP `allow_unsigned: true`, which R-TRUST-001's
own suggestion invited. The install wrote the lock entry without its `key_id`, so the next teammate trusted any key on
first use, and a `known-keys.json` that can't be read did not refuse it.

## Decision

- **D1. The expected key is decided for every package**: the user's pin, else the key the lock records, else none. An
  unsigned package with an expected key is refused with **R-TRUST-007**, before R-TRUST-001, whatever `--allow-unsigned`,
  `--yes`, the prompt or `trusted_keys` say. Its remedy is deliberate: check the registry; if the publisher really stopped
  signing, remove the extension and its pin, then add it again.
- **D2. Both adapters hand `decide` the store's answers whatever the signer**: the name's pin always, trusted and denied
  by the signer's key (false when unsigned), and R-TRUST-006 when the store can't be read. Registry contract clause C15
  holds the memory registry and the configured one (over HTTP and over memory) to it.
- **D3. An unsigned install happens only for a name with no expected key**, so the lock plan, which writes the
  publisher's key, never replaces a lock key with none.
- **D4. MCP `specforge.add_extension` takes no `allow_unsigned`.** Accepting an unsigned package is a person's decision
  in a terminal, as accepting a key change is. R-TRUST-001's suggestion names `specforge add --allow-unsigned` (or
  `specforge update --allow-unsigned`) in a terminal.

```
  expected = user pin | lock key | none        (every package)
        │
        ├── unsigned, expected ──▶ R-TRUST-007   (nothing accepts it)
        ├── unsigned, none     ──▶ --allow-unsigned (CLI only) ? W155 : R-TRUST-001
        └── signed             ──▶ denied R-TRUST-005, then same / first use / key change R-TRUST-003
```

## Rejected

- An unsigned package for a pinned name as a key change "to no key", asked like one (R-TRUST-003): `--yes` in CI would
  silently accept a stripped signature, and the lock would have to record a key that did not sign the bytes it pins.
- Reusing R-TRUST-001 with a message naming the pin: its explanation and suggestion ("install it with
  `--allow-unsigned`") would be wrong for exactly this case.
- A shared `UserKeys` constructor for the two adapters: two lines each; C15 is the seam's guard (ADR 0044).
- Accepting an unsigned package whose bytes equal the lock's hash: the lock is project text a PR can edit (hash and
  registry URL together); one rule is simpler to state and audit.
- W155 in a preview: ADR 0213 stands.

## Consequences

- `specforge add` and `update --allow-unsigned` exit 1 with R-TRUST-007 for a pinned or locked name; a dry run and MCP
  refuse it the same way, downloading nothing.
- An unsigned install with an unreadable `known-keys.json` is refused (R-TRUST-006).
- A lock whose key an earlier unsigned install erased stays keyless until a signed install; the user's pin is then the
  only guard. A `known-keys.json` pin for a name the project holds unsigned refuses its next fetch.
- An MCP call that sends `allow_unsigned` gets `invalid_input`; no file format changes.

## Reopen when

A registry protocol carries a signature-free provenance proof (a transparency log), or a `specforge trust`/`unpin`
command manages `known-keys.json`.
