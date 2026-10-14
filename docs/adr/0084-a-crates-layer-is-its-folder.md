# A crate's layer is its folder, and its dependencies point down

**Status:** accepted (2026-10-14, structure round, plan 04).

The workspace's 31 crates sit in `crates/<layer>/<name>/`, with eight layers from bottom to top: `leaf`, `kernel`,
`pipeline`, `runtime`, `session`, `app`, `adapters`, `surfaces` (ADR 0083). When the layout was introduced, its 118
normal and build dependencies between those crates pointed down (98) or stayed within a layer (20), and none pointed
up. Nothing kept it that way: one manifest line could make a kernel crate link the application, and the folder would
still claim otherwise.

## D1. The layer is the folder

A crate's layer is the folder under `crates/` it sits in. The order is `gate/src/layers.rs`'s `LAYERS`, the single
source: `gate::layers::{LAYERS, place}` are public so other checks read the same order. A crate under `crates/` that
is not in `crates/<layer>/<name>/`, with a layer from that list, is an error. No second map from package names to
layers exists.

## D2. Normal and build dependencies point down

A normal or build dependency (optional and target-specific ones included) points to a crate in its own layer or a
lower one. Within a layer any dependency is allowed; Cargo already rejects cycles.

## D3. Dev-dependencies are not judged

A dev-dependency builds only a crate's tests, benches and examples; it links into nothing that ships, and Cargo
allows dev cycles. Tests may run a crate against the real crate above it (the registry client against the server,
the emitter through the project compile). When this ADR was written, some dev-dependencies pointed up and 56 pointed
to test support.

## D4. The members outside `crates/` are in no layer

`gate`, `xtask`, `specforge-test` and `specforge-test-macros` are tools and test support. They may depend on any
crate; no crate in a layer may have a normal or build dependency on one. The guests and fixtures are outside the
workspace and are not judged: their dependency on the host is the extension SDK, and each blob records what it
compiles from.

## D5. An upward edge needs an ADR

`gate/src/layers.rs`'s `ALLOWED` lists the upward edges an ADR accepts, by package names and the ADR's number. It
is empty. An entry that accepts no upward edge fails the check, so the list never outlives its reason.

## D6. The `layers` gate check enforces it

`cargo gate` reads the `cargo metadata` it already runs, compiles nothing, and is reached by the pre-commit hook. It
runs when a manifest under `crates/`, the root manifest or the rule file changes, and in every full and CI run.

## Rejected

- **Judging dev-dependencies**, alone or with test support exempted: dozens of exceptions for edges that ship nothing.
- **An order within each layer**: a rank per crate that the folder tree does not show; Cargo already forbids cycles.
- **The rule in `gate/layers.toml`**: a parser and a schema for eight names and an empty list; a typo would be a
  runtime error instead of a compile error.
- **A layer map by package name** (the review's script): a second source of truth beside the folders.
- **`cargo-deny` bans or an xtask binary**: the first does not express folder layers, the second links the workspace
  (ADR 0066).

## Consequences

- A new crate goes in a layer folder, or the hook refuses the commit; moving a crate between layers is a `git mv`
  that the check judges.
- `cargo gate list` gains the `layers` row; the check's log prints the stack and the edge counts on every run.
- An upward edge is either removed (the code it needs moves down) or argued in an ADR and listed.
