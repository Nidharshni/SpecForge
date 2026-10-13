# One check list for CI, the gate and the hook

**Status:** accepted (2026-10-13, architecture round 8, plan 04).

CI (`.github/workflows/ci.yml`), the local gate (`scripts/gate.sh`, quick and full) and the pre-commit hook each wrote
down which checks the repository runs, and each missed what another ran: CI never ran `specforge check` against the
warning baseline, the dogfood coverage analysis or the doctests; the gate and the hook never ran grammar freshness, the
zero-entity check, rustfmt on the crates outside the workspace or cargo-audit; quick mode ran the blob freshness check
for `extensions/*` and the SDK only, though every blob records protocol-types and the root manifest's inherited tables.
Each new check landed in one script.

## D1. The list is one module, `gate`

`gate/src/checks.rs`'s `CHECKS` writes every check once: its name, what passing proves, the smallest run that reaches
it (hook, quick, full, or the cold-clone bootstrap), the paths that trigger it in a hook or quick run, whether it
compiles the workspace, the checks it runs after, and its fix. `cargo gate list` prints it. `gate` is a workspace
package with no SpecForge dependency, so the hook never waits on a workspace build to start.

## D2. Every caller runs a mode of the list

The hook runs `cargo gate hook` (the staged files; checks that compile nothing of the workspace; never the blob check,
since a round's branches carry stale blobs until its one re-vendor). `cargo gate` runs what the working tree changed;
`cargo gate full` every check but the bootstrap; CI runs `cargo gate ci`, which selects what `full` selects (a test
holds it) and `cargo gate bootstrap` in a job without a cache. No check is named in the workflow.

## D3. A blob's trigger is what it recorded

Which builtin blobs a change feeds is read from `extensions/*/wasm/inputs.json` by `gate::blob_inputs`, the module
`build-blobs` writes and checks those records through. No list of crates triggers the blob check.

## D4. Which dogfood numbers are pinned

This repository's own projects have no error and the warning counts of `gate/baseline.json` (the `specs` check);
infos are pinned nowhere. A test pins a rule's meaning on a fixture project, never a count over `spec/`.

## Rejected

- The list in bash with CI calling it: its selection cannot be tested, and reading JSON needed Python.
- The list as an `xtask` binary: `xtask` links the workspace and wasmtime; every hook after a core edit would rebuild
  them first.
- One CI job or step per check: the list in YAML again.
- Ratcheting info counts: every new spec entity would need a baseline edit.

## Consequences

- `scripts/gate.sh` is gone; `gate/baseline.json` replaces `scripts/gate-baseline.json`.
- CI's main job runs the doctests, `specforge check` with the baseline and the dogfood analysis; the hook formats the
  guests and fixtures and checks grammar freshness and the zero-entity rule.
- A new check is one entry in `CHECKS`.
