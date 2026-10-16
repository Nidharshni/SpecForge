# A graph is read only with the Registry build it was built in

**Status:** accepted (2026-10-16, architecture round 13, plan 01). Amends [ADR 0032](0032-one-graph-build.md) (the Registry
build builds the graph, taking the provider schemes per call; its graph inputs are held), [ADR 0115](0115-the-entity-snapshot-is-made-in-the-registry.md)
D2 (the pairing is checked at the snapshot, not left to a helper) and [ADR 0108](0108-a-project-view-comes-only-from-a-compiled-project.md) D4
(an assembled source is built in its environment).

## Context

- The entity snapshot takes any graph. `EntitySnapshot::of(&Graph, &RegistryBuild, spec_root)` documents that the graph was
  built with the Registry build's inputs; a `Graph` carries no trace of the inputs it was built with, so nothing checks it.
  ADR 0115 D2 says "production pairs them in `Environment`; tests through one helper". There was no one helper.
- The pairing was written by hand in four places (`Environment::graph_config`, and the snapshot, coverage and registry-text
  test harnesses), once more through `env.graph_config()` in `field_types.rs`, and was wrong in four more: `Assembled::source`
  and two ops tests built a graph with the default config and read it with real registries, and one coverage test read a
  graph with an environment of no declarations.
- `testing::Assembled::source` (38 call sites) builds with the default config: a declared single `reference` makes no edge
  and a quoted `"false"` of a `bool` field stays a string. No existing test depends on that reading (all 38 pass under
  production linking); the defect is latent.
- `RegistryBuild::graph_inputs` derived its value per call and read the candidate vocabulary, so its doc said "call it after
  `with_candidates`".
- The same rule was written four times on 2026-10-16 (`c148e4a9`, `04d0478a`, `49d26c4b`, `f5aaa05c`).

## Decisions

**D1. One maker.** `RegistryBuild::graph_of(files, schemes) -> GraphBuild` builds every graph a snapshot reads, with the
build's own inputs; `Environment::graph_of(files)` adds the providers' schemes. No caller outside `specforge-graph`'s own
tests writes a `GraphConfig`. The schemes are a per-call argument, so the build stays pure (ADR 0031 D7 rejected the build
holding providers, not a call taking a set of strings); this reverses ADR 0032's round-6 rejection of `graph_config(schemes)`.

**D2. The inputs are held.** `RegistryBuild::graph_inputs() -> &GraphInputs`, derived by `build_registries` and again by
`with_candidates`; there is no order to call them in.

**D3. The snapshot reads a `GraphBuild`, and only one built in its Registry build.**
`EntitySnapshot::of(&GraphBuild, &RegistryBuild, spec_root)` debug-asserts `build.inputs() == registries.graph_inputs()`.

**D4. A test's project is built as `check` builds it.** `testing::Assembled::source` is built in the project's environment
when it is compiled; a graph a test makes is held in that environment's empty build (`GraphBuild::holding`).

## Rejected

- The schemes kept in the session, the registry offering only `graph_inputs()`: two of the four hand-written literals stay.
- I005 moved out of the graph build into the session's import diagnostics: changes the report order, a different card.
- A registry newtype over `GraphBuild`: eight forwarding methods and a shallow module the session would hold instead of
  `GraphBuild`.
- Identity of an `Arc<GraphInputs>`: changes `GraphConfig.inputs`' type and refuses two equal builds from one declaration.
- A release `assert!`: a programming error would crash the LSP; the debug test profile already catches it.
- Building an assembled source at `source()`: leaves the order of the calls mattering.
- Converting every hand-made graph to sources (ADR 0108's rejection stands).

## Consequences

Rust API breaks in `specforge-registry` (`graph_inputs` returns a reference; `EntitySnapshot::of` takes `&GraphBuild`),
`specforge-graph` (`GraphBuild::holding` takes `self`) and `specforge-project` (`Environment::graph_config` is gone;
`entity_snapshot` takes `&GraphBuild`). No CLI, MCP, LSP, wire or output change; no diagnostic changes; no re-vendor.

## What would reopen it

A reader that needs a graph built with inputs other than its Registry build's (then the snapshot would take the inputs
explicitly); providers moving into the Registry build (then `graph_of` drops its argument).
