# Extension Inventory

This document catalogs the four official domain extensions after the entity audit and testability extraction. Together they provide 22 entity kinds, 57 edge types, and cross-extension enhancements through the Extension Protocol. Test concepts live in three more builtins that declare no entity kinds: `@specforge/testing` and the runner extensions `@specforge/cargo-test` and `@specforge/vitest` ([ADR 0002](adr/0002-test-runner-extensions.md); see their section). The `@specforge/rust` and `@specforge/typescript` builtins are source analyzers used by inference.

## Dependency Graph

Extensions declare peer dependencies that control load order and enable cross-extension references. The graph reads left-to-right: an arrow means "depends on."

```
                   ┌────────────────────┐
                   │  @specforge/product │  (root -- no dependencies)
                   │  9 kinds, 20 edges  │
                   └─────────┬──────────┘
                             │
              ┌──────────────┼──────────────┐
              │              │              │
              v              v              │
   ┌──────────────────┐  (implicit)        │
   │ @specforge/       │                    │
   │ software          │                    │
   │ 5 kinds, 14 edges │                    │
   └────────┬─────────┘                    │
            │                              │
     ┌──────┼──────┐                       │
     │             │                       │
     v             v                       v
┌──────────┐ ┌──────────────┐  ┌────────────────────┐
│ formal   │ │ governance   │  │ testing             │
│ 5 kinds  │ │ 3 kinds      │  │ 0 kinds             │
│ 12 edges │ │ 11 edges     │  │ 7 enhancements      │
└──────────┘ └──────────────┘  └─────────┬──────────┘
                                         │
                                  ┌──────┴───────┐
                                  v              v
                           ┌────────────┐ ┌────────────┐
                           │ cargo-test │ │ vitest     │
                           │ collector  │ │ collector  │
                           └────────────┘ └────────────┘
```

Load order: `product` first (no dependencies), then `software` (optionally depends on product), then `formal` and `governance` (depend on software), then `testing` (optionally depends on software and governance, whose kinds it makes testable), then the runner extensions, which require `testing`. Extensions with no peer between them keep the order `specforge.json` lists them in (ADR 0041).

## @specforge/product

**Root extension with no dependencies.** Provides the product planning and delivery vocabulary. Not every project needs this -- internal tools, scripts, and libraries may skip it entirely.

### Entity Kinds (9)

| Kind | Keyword | Singleton | Description |
|------|---------|-----------|-------------|
| Feature | `feature` | no | User-facing capability composed of behaviors |
| Journey | `journey` | no | UX flow mapping a persona and channel to features |
| Deliverable | `deliverable` | no | Shippable artifact bundling journeys and modules |
| Milestone | `milestone` | no | Planning phase with scheduled features and exit criteria |
| Module | `module` | no | Code package mapping features to ports |
| Term | `term` | no | Structured vocabulary for the project's ubiquitous language |
| Persona | `persona` | no | User archetype that interacts with the system |
| Channel | `channel` | no | Interaction medium through which personas access the system |
| Release | `release` | no | Coordinated shipment of deliverables targeting milestones |

### Edge Types (20)

| Edge Type | Source | Target | Semantics |
|-----------|--------|--------|-----------|
| `FeatureDependsOn` | feature | feature | This feature depends on that feature |
| `FeatureRelatesTo` | feature | feature | This feature is related to that feature |
| `JourneyExercisesFeature` | journey | feature | This UX flow exercises these features |
| `JourneyTargetsPersona` | journey | persona | This journey is performed by this persona |
| `JourneyUsesChannel` | journey | channel | This journey occurs through this channel |
| `DeliverableSupportsJourney` | deliverable | journey | This deliverable ships these journeys |
| `DeliverableContainsModule` | deliverable | module | This deliverable includes these modules |
| `DeliverableTrackedByMilestone` | deliverable | milestone | This deliverable targets these milestones |
| `DeliverableDependsOn` | deliverable | deliverable | This deliverable depends on that deliverable |
| `MilestoneDeliversFeature` | milestone | feature | This phase schedules these features |
| `MilestoneScopesModule` | milestone | module | This milestone includes these modules |
| `MilestoneDependsOn` | milestone | milestone | This milestone depends on that milestone completing first |
| `ModuleContainsFeature` | module | feature | This module implements these features |
| `ModuleDependsOn` | module | module | This module depends on that module |
| `TermReferencesRelatedTerm` | term | term | This term cross-references that term |
| `TermBelongsToModule` | term | module | This term belongs to this module's domain |
| `ReleaseIncludesDeliverable` | release | deliverable | This release ships these deliverables |
| `ReleaseCompletesMilestone` | release | milestone | This release targets these milestones |
| `ReleaseDependsOn` | release | release | This release depends on that release |
| `PersonaPrioritizesFeature` | persona | feature | This persona prioritizes this feature |

### Shared Fields

| Field | Type | Description |
|-------|------|-------------|
| `tags` | string_list | Freeform labels applied to all entity kinds |

### Notes

- No testable or verify concepts: `@specforge/testing` contributes those.
- Feature includes a `deprecated` status (terminal state, reachable from `done`).
- Persona and channel are first-class entity kinds, not configuration values.

## @specforge/software

**Optionally depends on: @specforge/product** (optional peer dependency: without it, `features` references are I004 hints and the product enhancements are skipped). Provides the software engineering vocabulary: behavioral contracts, domain events, type definitions, and port interfaces.

### Entity Kinds (5)

| Kind | Keyword | Singleton | Description |
|------|---------|-----------|-------------|
| Behavior | `behavior` | no | Behavioral contract for a single operation |
| Invariant | `invariant` | no | Runtime guarantee the system must never violate |
| Event | `event` | no | Domain or system event emitted by behaviors |
| Type | `type` | no | Data type definition (struct, union, error, command) |
| Port | `port` | no | Interface contract (hexagonal architecture boundary) |

### Edge Types (15)

| Edge Type | Source | Target | Semantics |
|-----------|--------|--------|-----------|
| `References` | any | any | General cross-reference |
| `BehaviorImplementsFeature` | behavior | feature (product) | This behavior implements this feature |
| `BehaviorProducesEvent` | behavior | event | This behavior emits these events |
| `BehaviorConsumesEvent` | behavior | event | This behavior reacts to these events |
| `BehaviorReferencesType` | behavior | type | This behavior uses these type definitions |
| `EventCarriesPayloadType` | event | type | This event carries this payload type |
| `TypeComposesType` | type | type | This type composes or extends that type |
| `BehaviorUsesPort` | behavior | port | This behavior uses these port interfaces |
| `BehaviorEnforcesInvariant` | behavior | invariant | This behavior enforces these invariants |
| `TypeExtendsType` | type | type | This type extends that type (inheritance) |
| `PortReferencesType` | port | type | This port names this type in a method parameter or return type |
| `ExternalRef` | any | URI | This entity links to an external reference |
| `MilestoneIncludesBehavior` | milestone (product) | behavior | This milestone delivers these behaviors |
| `ModuleConsumesPort` | module (product) | port | This module consumes these ports |
| `ModuleDefinesPort` | module (product) | port | This module defines these ports |

### Entity Enhancements

| Target Kind | Owner Extension | Added Fields |
|-------------|----------------|-------------|
| `module` | @specforge/product | `ports` (reference_list -> port), `ports_defined` (reference_list -> port) |
| `milestone` | @specforge/product | `behaviors` (reference_list -> behavior) |

### Notes

- No testable or verify concepts: `@specforge/testing` contributes those.
- Cross-extension edges to `feature` (product) use the peer dependency mechanism.
- Enhancement edges (`MilestoneIncludesBehavior`, `ModuleConsumesPort`, `ModuleDefinesPort`) are declared with `c.enhance(..)` on product entity kinds.

## @specforge/governance

**Depends on: @specforge/software** (peer dependency). Provides architecture governance, quality tracking, and risk assessment.

### Entity Kinds (3)

| Kind | Keyword | Singleton | Description |
|------|---------|-----------|-------------|
| Decision | `decision` | no | Architecture Decision Record with rationale |
| Constraint | `constraint` | no | Non-functional requirement with measurable thresholds |
| FailureMode | `failure_mode` | no | FMEA risk assessment tied to an invariant |

### Edge Types (11)

| Edge Type | Source | Target | Semantics |
|-----------|--------|--------|-----------|
| `DecisionProtectsInvariant` | decision | invariant (software) | This decision protects these invariants |
| `ConstraintEnforcedByBehavior` | constraint | behavior (software) | This constraint applies to these behaviors |
| `DecisionSupersedesDecision` | decision | decision | This decision supersedes that decision |
| `ConstraintConstrainsBehavior` | constraint | behavior (software) | This quality requirement constrains these behaviors |
| `ConstraintProtectsInvariant` | constraint | invariant (software) | This constraint protects these invariants |
| `FailureModeTargetsInvariant` | failure_mode | invariant (software) | This failure mode threatens this invariant |
| `ConstraintGovernsFeature` | constraint | feature (product) | This constraint governs these features |
| `DecisionAffectsFeature` | decision | feature (product) | This decision affects these features |
| `FailureModeThreatensFeature` | failure_mode | feature (product) | This failure mode threatens these features |
| `DecisionImposesConstraint` | decision | constraint | This decision imposes these constraints |
| `FailureModeAffectsBehavior` | failure_mode | behavior (software) | This failure mode affects these behaviors |

### Notes

- All cross-extension edges use soft references. If `@specforge/software` or `@specforge/product` is not installed, references to their entity kinds produce `I004` diagnostics.
- The `failure_mode` kind declares `severity`, `occurrence` and `detection` (and their `post_` forms) as vocabularies (E061); it doesn't check a declared RPN against their product.

## @specforge/formal

**Depends on: @specforge/software** (peer dependency). Provides formal analysis constructs: structured conditions, specification layering, event graph linting, and coverage tracking.

### Entity Kinds (5)

| Kind | Keyword | Singleton | Description |
|------|---------|-----------|-------------|
| Property | `property` | no | Temporal or behavioral assertion (safety, liveness, fairness) |
| Axiom | `axiom` | no | Assumed-true foundation (no proof required, no coverage tracking) |
| Protocol | `protocol` | no | Shared synchronization contract across events |
| Refinement | `refinement` | no | Abstract-to-concrete behavior mapping with condition deltas |
| Process | `process` | no | CSP-style communicating process with alphabet and composition |

### Edge Types (10)

| Edge Type | Source | Target | Semantics |
|-----------|--------|--------|-----------|
| `AxiomAssumesInvariant` | axiom | invariant (software) | This axiom assumes this invariant |
| `BehaviorSatisfiesProperty` | behavior (software) | property | This behavior satisfies this property |
| `BehaviorRefinesBehavior` | behavior (software) | behavior (software) | This concrete behavior refines this abstract behavior |
| `EventFollowsProtocol` | event (software) | protocol | This event follows this protocol |
| `PropertyDependsOnInvariant` | property | invariant (software) | This property depends on this invariant |
| `RefinementRefinesAbstract` | refinement | behavior (software) | This refinement maps from this abstract behavior |
| `RefinementRefinesConcrete` | refinement | behavior (software) | This refinement maps to this concrete behavior |
| `RefinementChainsToRefinement` | refinement | refinement | This refinement chains to that refinement |
| `EventParticipatesInProcess` | event (software) | process | This event participates in this process |
| `ProcessComposesProcess` | process | process | This process composes that process |

### Entity Enhancements

| Target Kind | Owner Extension | Added Fields |
|-------------|----------------|-------------|
| `behavior` | @specforge/software | `requires` (block), `ensures` (block), `maintains` (block), `satisfies` (reference_list -> property), `sync` (block) |
| `event` | @specforge/software | `follows_protocol` (reference_list -> protocol), `participates_in` (reference_list -> process), `sync` (block) |

### Compiler Passes (4)

| Pass | After | Description |
|------|-------|-------------|
| `condition_check` | resolve | Validate structured condition consistency |
| `layering_verify` | condition_check | Verify specification layering constraints |
| `event_graph_analyze` | resolve | Analyze event graph for synchronization issues |
| `coverage_tracking` | layering_verify | Track coverage of formal verification obligations |

### Notes

- Requires `warning_level=strict` in `specforge.json` for formal warnings to fire.
- Structured conditions are inline blocks (requires/ensures/maintains) within behavior bodies. Conditions are not standalone entities; shared constraints are modeled as invariant entities.
- Cycle detection on refinement chains (E041) and process composition (E042) prevents infinite layering.

## @specforge/testing and the runner extensions

**`@specforge/testing`** owns the test vocabulary ([ADR 0002](adr/0002-test-runner-extensions.md)). Core keeps `verify [kind] "..."` only as reserved syntax; this extension gives it meaning. It declares no entity kinds or edge types.

### Entity Enhancements (7)

Each enhancement makes its target kind testable with the verify kinds listed. An enhancement whose owner extension isn't enabled is skipped silently.

| Target Kind | Owner Extension | Verify Kinds | W004 when no obligations |
|-------------|----------------|--------------|--------------------------|
| `behavior` | @specforge/software | unit, contract, integration, property, performance | yes |
| `invariant` | @specforge/software | unit, integration, property, performance, mutation | yes |
| `event` | @specforge/software | integration, unit, deadlock_free, liveness | yes |
| `type` | @specforge/software | unit, property | yes |
| `port` | @specforge/software | integration, unit | yes |
| `constraint` | @specforge/governance | unit, integration, property, load, contract | no |
| `failure_mode` | @specforge/governance | unit, integration, property | no |

### Validation Rules

| Code | Severity | Rule |
|------|----------|------|
| W004 | warning | Testable entity declares no verify obligations (union types and abstract entities are exempt) |
| W009 | warning | Verify kind outside the kind's allowed set |

### Compiler Passes (1)

`coverage` (`specforge analyze coverage`): scores intent (A001/A002), counts enforcement (invariants nothing references, which `check` reports as W003) and proof from the results `specforge collect` records: failing tests (A014), obligations no passing test names (A015), and tests naming obligations the spec doesn't declare (A016), plus formal discharge. `--min` gates CI on proof coverage.

### Runner extensions

Each test runner has its own extension that requires `@specforge/testing` and contributes one collector: the files that select it, the command `specforge collect` runs (after the user approves it), where the report lands, and a pure export mapping the report to entities.

| Extension | Detected by | Command | How a test links to its entity |
|-----------|-------------|---------|--------------------------------|
| `@specforge/cargo-test` | `Cargo.toml` | `cargo test --workspace --no-fail-fast` (stdout captured) | `#[specforge_test(behavior = "...", verify = "...")]`, or plain tests named `entity_id__obligation_slug` or inside `mod entity_id` |
| `@specforge/vitest` | `vitest.config.*`, `vitest.workspace.*` | `npx --no vitest run` with the JSON reporter | `meta: { specforge: { behavior: "...", verify: "..." } }` |

### Notes

- Spec files carry no test paths: the retired `tests` field is replaced by linkage in the tests themselves.
- `init` enables `@specforge/testing` with `@specforge/software`, and a runner extension when it finds that runner in the project.

## What Was Removed (Entity Audit)

The entity audit identified four redundant entity kinds and one DSL construct that were removed or retargeted:

| Removed | Reason | Replacement |
|---------|--------|-------------|
| `library` | Identical fields to module (family, features, depends_on) | Use `module` |
| `capability` | Identical fields to journey (persona, channels, features, flow) | Use `journey` |
| `roadmap` | Covered by milestone + release (planning via milestones, shipping via releases) | Use `milestone` and `release` |
| `condition` | Retargeted to invariant; edges renamed `*Condition` to `*Invariant` | Use `invariant` with requires/ensures/maintains references |
| `verify` as core grammar | `verify [kind] "..."` stays as reserved syntax; its meaning moved out of core | `@specforge/testing` (ADR 0002) |

## Entity Count Summary

| Extension | Entity Kinds | Edge Types | Enhancements | Dependencies |
|-----------|-------------|------------|--------------|-------------|
| @specforge/product | 9 | 20 | -- | none |
| @specforge/software | 5 | 15 | module, milestone | product |
| @specforge/governance | 3 | 11 | -- | software |
| @specforge/formal | 5 | 10 | behavior, event | software |
| @specforge/testing | 0 | 0 | 7 entity kinds | software, governance (optional) |
| @specforge/cargo-test | 0 | 0 | -- | testing |
| @specforge/vitest | 0 | 0 | -- | testing |
| **Total (shipped)** | **22** | **56** | -- | -- |

## Related Documentation

- [Extension Protocol](extension-protocol.md) -- the Wasm protocol extensions implement
- [Extension SDK](extension-sdk.md) -- the SDK for building extensions
- [Extension Model](extension-model.md) -- the broader extension architecture
- [Entity Model](entity-model.md) -- entity kinds, edge types, and validation rules
