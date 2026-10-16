# The entity snapshot is made in the registry crate

**Status:** accepted (2026-10-16, architecture round 12, plan 04). Reopens [ADR 0019](0019-one-entity-snapshot-per-compile.md)
"Placement"; completes [ADR 0031](0031-the-registry-build-checks-the-entities.md) D1's "Tests run them through the build".

ADR 0019 placed the maker of the entity records in the project crate because "the registry, which must not depend on
the graph, reads it". The registry has depended on `specforge-graph` since 74cac053 (2026-10-11), and reads
`specforge_graph::Node` and `ValueShape` in six source files. The maker therefore sat two layers above the checks that
read its records, and 16 commits changed both crates together (67d23001's edge typing is the clearest). The registry's
check tests could not reach the maker, so 178 sites built records by hand, and two of their shapes no compile
produces: an integer on an enum field (the graph build stores a string) and a `define` entity (no such node exists).
The snapshot's own tests paired a default-config graph with real registries, so a quoted `"false"` on an exempting flag
stayed a string and counted as set; production coerces it to a boolean and reports W004.

## Decisions

**D1. Placement.** `specforge_registry::snapshot::EntitySnapshot::of(&Graph, &RegistryBuild, spec_root)` holds the
records, the field text, edge typing, `Standing` and the pass, validator and coverage adapters, beside the checks that
read them. The session keeps `Environment::entity_snapshot`, which knows the spec root. `EntityRecord` stays in
`specforge_registry::entity`.

**D2. The pairing invariant.** A snapshot reads a graph built with its registry build's own graph inputs
(`GraphConfig::inputs == build.graph_inputs()`). Production pairs them in `Environment`; tests through one helper.

**D3. Check tests start from text.** A test of what the checks report on what a project writes builds its records
through the graph build and the snapshot. Hand-built records (`EntityRecord::new` and the `with_*` builders) stay for
tests whose subject is the rule engine or a wire shape; they are no producer in production (ADR 0031 "What would
reopen this").

**D4. Two renderings of a value, side by side.** The field text (ADR 0019) serves rules, validators and passes; the
written value (`field_form::write`, ADR 0064 D3) serves outputs and inputs. They differ only for an expression group
(`", "` against one conjunct per line); each names the other.

## Rejected

- A new crate between registry and session (a crate-map change for one production caller).
- Merging the snapshot into `entity.rs` (over the file-size cap).
- Unifying the expression joiner: `"\n"` for the field text is a wire change and a re-vendor; `", "` for the written
  value breaks ADR 0064 D3 and the prove pass.
- Converting every rule-engine test to text.

## Consequences

`specforge_project::snapshot` is gone (a Rust API break in `specforge-project`); the registry depends on
`specforge-parser` and `specforge-coverage`, both below it. No behaviour, wire or output change; no re-vendor.

## What would reopen this

A reader of the snapshot below the registry (it would move down), or a reason for the field text to carry structure
the written value has (ADR 0019's own clause).
