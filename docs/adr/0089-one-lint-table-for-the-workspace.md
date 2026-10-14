# One lint table for the workspace; an exception says why where it is

**Status:** accepted (2026-10-14, structure round, plan 10).

At `0113232f` five crates each denied `print_stdout` in their own `[lints]` table, the CLI denied it again in
`main.rs`, and nothing else was linted: 200 `unwrap()`s in non-test code (186 of them `writeln!` into a `String` in
the emitter), `unsafe` with no stated argument, and 30 `#[allow]`s of which 11 silenced nothing. Two of the four
crashes the round reproduced (an LSP position past `u32::MAX - n`, and text nested deeper than the parser's stack)
ended a server that an editor or an agent was talking to; plan 10's first tickets fixed them.

## Decision

- **D1. One table, inherited whole.** The root manifest's `[workspace.lints]` holds every level; each member has
  `[lints] workspace = true` and no table of its own (cargo refuses a member that does both). A crate stricter than
  the workspace says so in an inner attribute (`specforge-cli`'s `#![deny(clippy::print_stderr)]`).
- **D2. The levels.** `unsafe_code`, `unwrap_used`, `print_stdout`, `dbg_macro`, `todo`, `unimplemented` and
  `allow_attributes_without_reason` are denied. `expect("...")` is allowed: its message names the invariant it
  relies on, and input a user controls returns an error instead. `panic!`, indexing and string slicing are not linted
  workspace-wide: of the 48 hits sampled, the ones that were bugs are fixed where they were, and the rest are
  invariants of local loops. The module that converts client positions to byte offsets (`specforge-lsp`'s line
  index) denies `arithmetic_side_effects`, `indexing_slicing` and `string_slice` itself.
- **D3. Tests state their `unwrap` once, at the root of the test target, and add no line to a listed file.** A
  unit test's `unwrap` is its assertion. `clippy.toml`'s `allow-unwrap-in-tests` covers `#[test]` functions and
  `#[cfg(test)]` modules; the helper functions of an integration-test crate fall outside it (1,118 hits at the
  tip). Each test and bench target's root starts with
  `#![allow(clippy::unwrap_used, reason = "a test's unwrap is its assertion: a None or an Err fails the test")]`,
  which every module the root declares inherits, and so do the `testing` feature modules that unwrap. ADR 0085
  forbids a listed file (53 over 800 lines) to gain or lose a line, so the options were measured against it:

  | Option | Listed files touched (of the 46 holding unwrap hits) | Verdict |
  |---|---|---|
  | Root `#![allow]` in each target | 2: the only listed files that are themselves a target root (`ops-registry/tests/fetch.rs`, `registry-server/tests/server_contracts.rs`, 24 helper unwraps). The other 44 are modules of an unlisted `main.rs` | chosen; the 2 get `.expect("...")` in place of `.unwrap()`, line count unchanged |
  | `allow-unwrap-in-tests` alone | 0, but leaves 1,118 hits | insufficient |
  | A `[lints.clippy]` copy per crate with listed tests | 0, but cargo forbids it beside `workspace = true`, so the table is copied into ~13 crates and drifts | rejected |
  | Strict lints on `--lib --bins` in the gate only | 0, but the workspace table cannot carry `unwrap_used` (a table applies to every target), so the editor and a local `cargo clippy` stop seeing it | fallback, not needed |
  | `.expect("...")` for all 547 hits in the 46 listed files | 46 | rejected: 547 meaningless messages, and any rustfmt rewrap makes a row rise |

  A test file that is only a module of another target's root needs no attribute.
- **D4. Exceptions are `#[expect(lint, reason = "...")]` at the item.** An `expect` fails the gate when what it
  excuses goes away. `#[allow(lint, reason = "...")]` remains only where an `expect` cannot be fulfilled everywhere
  it expands: proc-macro output, a module shared by several test binaries, and the roots of D3.
- **D5. Unsafe is denied, not forbidden.** `forbid` cannot be lifted at an item, and unsafe code lives in five
  crates' targets (the tree-sitter FFI, specforge-test's `atexit`, three tests). Each unsafe item carries
  `#[expect(unsafe_code, reason)]` stating why it is sound.
- **D6. The guests and the blob stage.** The guests (`extensions/*`, `fixtures/*`) are their own workspaces and keep
  their own lint settings; the gate lints them with `-D warnings`. The four crates every blob compiles (the SDK, its
  macros, protocol-types, coverage) inherit the table, so the blob stage's root manifest carries
  `[workspace.lints.rust]` and `[workspace.lints.clippy]` (`gate::blob_inputs::INHERITED_TABLES`). Lint levels do
  not change a blob's bytes; they change its recorded inputs, so inheriting them is a re-vendor.

## Consequences

- A new `unwrap()` in non-test code fails `cargo clippy` locally, in the editor and in the gate.
- A new integration-test target has to start with the attribute of D3, or its first helper `unwrap` fails.
- The lint tables sit after `[workspace]` and before `[workspace.package]`: `gate::blob_inputs` keeps every line
  under an inherited header, comments included, so a comment placed after `[workspace.dependencies]` would change
  every blob's record.
- A `#[cfg_attr(<cfg>, allow(...))]` is linted only under its cfg: the ones that remain carry a reason too, and
  `project/tests/session.rs`'s (a listed file, live only off unix) cannot take one without growing.
- Every `#[allow]` that remains carries its reason; every other exception disappears with the code it excused.

## Rejected

- **`expect_used = "deny"`:** about a hundred attributes repeating the message beside them.
- **`indexing_slicing` workspace-wide:** 389 local-loop invariants become `get()` chains; the one reproduced slicing
  crash was a string cut and is fixed where it is.
- **A gate step running clippy with extra flags:** invisible to editors and to a local `cargo clippy`.
