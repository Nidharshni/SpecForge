# The extension a path names is read by one module

**Status:** accepted (2026-10-20, architecture round 18, plan 03). Amends [ADR 0028](0028-installed-extensions-are-one-module.md) D7, [ADR 0045](0045-publish-is-one-operation-and-one-registry-serves-a-name.md) D3 and [ADR 0146](0146-a-file-with-more-than-one-name-is-not-read-through-a-bound.md) D4.

## Context

ADR 0028 D7 makes `specforge_ops::extension::candidate` the one module that reads an extension the project does not
load yet. Reading the extension a **path** names had drifted into five places: `LocalFile::read` (add, init),
`publish::read` plus `publish::validate` (publish, `extension validate`), the CLI build's own cargo layout, and the
load key `__candidate` that named an author's warnings. Each was reproduced:

- one missing file was E054 "file not found" from `add` and E040 "no extension binary at" from `publish`;
- one unreadable file was E054 "cannot read" from `add` and E040 "failed to read" from `publish`;
- `add ./ext` said "name the one to publish", and `add ./alias` named the link's target, not what was typed;
- a bounded `add_extension` installed a hard-linked binary that ADR 0146 refuses for every other bounded read;
- `publish` and `extension validate` printed `warning[W153]: extension '__candidate'`, and E028 for a non-component
  said `failed to compile component __candidate`.

## Decision

- **D1.** `specforge_ops::extension::candidate` is the one reader of the extension a request names by path
  (`LocalComponent::read`): add, init, publish, extension validate and the CLI build. It locates the component (a
  `.wasm` file, or `target/<WASM_TARGET>/release/<crate>.wasm`, else the only one there), reads it through the
  request's containment (publish and validate unbounded: CLI-only, ADR 0045 D6), a bounded read refusing a file with
  more than one name, the `Cargo.toml` it reads included (ADR 0146 D1), and names the path as spelled (ADR 0104).
- **D2.** One code per cause, in every verb: E040 for every refusal of the disk (nothing there, `file_not_found`; no
  built component or several, `precondition_failed`; it cannot be read); E054 is the syntax of a specifier only. One
  message per cause; add and init keep the specifier forms as the hint of "nothing there".
- **D3.** What a binary declares is the load's answer (`Loaded`), not a wrapper; the package a declaration names is
  derived once (`package`, and `registry_identity` for publish's scope and SemVer rules).
- **D4.** A load's warnings (W153, W138) name the extension by the name its handshake declares, whatever key it is
  loaded under (a candidate's `__candidate`, a file entry's text); a `WasmRuntime` adapter's load refusal names no
  key, its caller names the subject.
- **D5.** The crate layout (`WASM_TARGET`, `target_dir`) is the candidate module's; `specforge extension build` and
  `extension init`'s scaffold use it.

Rejected: E054 for a path that names nothing in every verb (publish has no specifier); keeping the split (an
unreadable file made it two codes for one disk state); a `Candidate` caching the identity (its E072 must come after the
declaration's errors in publish, before anything is written in add); a label parameter on `load_declaration`; renaming
the load key to the declared name (the name is only known after the handshake, which needs the key).

## Consequences

CLI and MCP: add, init and `add_extension` of a missing or unreadable local path answer E040 (was E054), "no extension
binary at ..." / "failed to read ..."; `extension validate` and `publish` of a missing path are `file_not_found` (was
`precondition_failed`); "name the component to use"; a file entry's W153/W138 name its declared name; E028's load
reason drops the key; a bounded add of a hard-linked component is E040. Rust API: `extension::Candidate` and
`binary_at` are gone; `LocalComponent`, `WASM_TARGET`, `target_dir` and `builtin_declaration` are added.

## What would reopen it

An MCP publish (ADR 0045 D6 reopened), which would pass its bound; a component that is no single `.wasm` file.
