# Every committed blob is listed once and built one way

**Status:** accepted (2026-10-13, architecture round 8, plan 09). Amends ADR 0050's round-7 amendment ("the builtins are
listed once") and ADR 0013's consequence on `xtask snapshot-builtins`.

The repository commits 13 wasm components: nine builtins, three fixtures built with the SDK (greet, docref, the sandbox
probe) and one bridge component written without it. `BUILTINS` listed the nine. The greet fixture was named by hand in
three xtask places and CI, and docref and the probe in none. So no command rebuilt them after round 6, and they answered
protocol 1.2.0 while the host spoke 1.3.0, their sources changed after they were built, and every check passed. The wire
answers pinned for ten blobs were computed twice, and `revendor` checked the snapshots it had just written with the
function that wrote them.

## D1. One list

`specforge_component::committed` (feature `testing`) lists every committed blob: name, crate, artifact and whether it is
an SDK guest (with the describe keys it declares on purpose although the protocol no longer defines them) or a bridge
component (with the canned protocol it answers). It also computes an SDK blob's wire answers. `git ls-files '*.wasm'`
equals the list (an xtask test). The snapshot test, the typed round trip, the protocol freshness test, `build-builtins`,
`revendor` and every test that loads a fixture read it.

## D2. One layout, one build

A committed blob lives at `<crate>/wasm/<artifact>`, beside its `inputs.json`. `build-builtins` builds every listed blob
in a staged workspace and records its inputs; `--check` and `--verify` cover fixtures as they cover builtins. `revendor`
rebuilds every listed blob, writes every SDK blob's snapshots, then runs `build-builtins --check` and the declarations
test. It no longer runs a check of what it just wrote.

## D3. What may be old

No blob. An SDK blob answers the host's protocol; a test fails otherwise. The sandbox probe's surfaces keep the
pre-ADR-0037 `sandbox` key on purpose, declared as a legacy key and checked by the round trip. The bridge component's
canned handshake (1.0.0) is declared and pinned.

## Rejected

- Building fixtures in place and guarding them only by their protocol version: a fixture source change goes unseen, and
  bytes depend on the checkout path.
- A crate of its own for the list: it would hold one module, and the feature unification it avoids reaches no shipped
  build.
- Keeping `snapshot-builtins`: a second entry point to the snapshot write; `revendor` regenerates the same bytes.
- Raising the bridge component's canned protocol with the host's: it is no declaration, and would claim a compatibility
  nothing tests.

## Consequences

- Fixture blobs moved: `fixtures/<name>/wasm/<artifact>`; the bridge crate is `fixtures/test-component`; the workspace
  excludes `extensions` and `fixtures` by prefix.
- docref and the probe are pinned like greet; their snapshots join `crates/specforge-component/tests/declarations/`.
- `xtask snapshot-builtins` is gone; ADR 0013's raw caller of `call_export` for exact bytes is
  `committed::wire_answers`.
- The SDK's `golden.rs` keeps only its builder tests.
- `gate::blob_inputs` names a blob by its crate directory (`extensions/product`, `fixtures/greet-extension`), so a
  change to a fixture's recorded inputs selects the `builtins` check.
- `build-builtins` is renamed `build-blobs` (`target/blob-stage/`).

## What would reopen it

- A fixture that must stay on an old SDK on purpose: a third `Guest` kind with the version it pins, built from a pinned
  SDK, never left unlisted.
