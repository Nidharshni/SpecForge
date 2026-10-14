# The crate map is generated from the workspace

**Status:** accepted (2026-10-14, structure round, plan 12). Builds on ADR 0084 (the layer rule) and ADR 0066 (one
check list).

The workspace's crates sit in `crates/<layer>/<crate>/` (ADR 0083, ADR 0084). Three things described them, and each had
drifted. `ARCHITECTURE.md` named 3 of the crates. `spec/product/modules.spec` declared 12 of 31, and 12 of its 14
dependency lists were wrong. Two manifest descriptions contradicted their crate docs. Fourteen crate roots had no crate
doc, and nineteen members had no description, so no line per crate could be quoted from the crates themselves.

## D1. A crate's line is its manifest's `description`

One line of plain text in the crate's own `Cargo.toml`, the line `cargo metadata`, docs.rs and crates.io show. Each
crate root also opens with a `//!` doc saying what the crate is, at whatever length that takes.

## D2. Two blocks are rendered, never written

`ARCHITECTURE.md`'s layer stack and `crates/README.md`'s crate table sit between `crate-map` markers. `gate` renders
them from `cargo metadata` (each member's folder and description) and from one line per layer kept beside the layer
order (`gate::layers::{LAYERS, place}`: no second copy of the order). `cargo gate crate-map` writes them. The prose
around the markers is hand-written and states no counts.

## D3. The gate check `crate-map` holds them

At hook reach, triggered by the two files, the root manifest, or a member's manifest or crate root, it fails when:
a member under `crates/` is not in a layer folder; a member has no one-line description; a crate root opens with no
doc; or a block differs from its rendering. Links in the two files are checked by `doc-links` (plan 02), which reads
every tracked Markdown file.

## D4. The repository's spec models every crate

Every member under `crates/` and `integrations/rust/` is one `module` in `spec/product/modules.spec`, titled with its
package name. Its description is the manifest's, and its `depends_on` is the manifest's normal and build dependencies
on modelled members. `xtask check-modules` compiles the project and compares them; the gate check `spec-modules` runs
it at quick reach.

## D5. "Layer" means the crate layer

The LSP's published diagnostics were described as "layers" (ADR 0056 D5). They are now its two parts, the compile's and
the formatter's, and `CONTEXT.md` defines **Layer** once. ADR 0056 is a dated record and keeps its words.

## Rejected

- **The first sentence of the crate doc as the line**: crate docs open with multi-line paragraphs and intra-doc links.
- **Hand-written tables checked for their crate set**: their lines could still drift from the descriptions.
- **The check in `xtask check-links`**: that checks test-to-spec links, compiles the project, and links wasmtime.
- **A test in a crate**: a document belongs to no member, so a quick run would not reach it.
- **Reading `modules.spec` as text in gate**: gate has no SpecForge dependency (ADR 0066 D1), and the language has one
  reader (ADR 0038).
- **rustc's `missing_docs` for the crate doc**: it fires on every public item.

## Consequences

- Adding, moving or describing a crate means: its description and doc, its module in `modules.spec`, then `cargo gate
  crate-map`. `crates/README.md` lists the steps.
- A description is quoted in three places (manifest, crate table, module); two are checked against the first.
- The crates new to `modules.spec` carry no `features` yet.
