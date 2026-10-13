# SpecForge Entity Model

## Overview

SpecForge uses a **zero-entity core** architecture: the compiler is a pure typed-graph engine with zero domain knowledge. Two **structural kinds** (`spec` and `ref`) are parsed by the core grammar. All domain entity kinds (currently 22, declared by the four builtin extensions) come from **extensions** through their declarations (`ExtensionDeclaration`, ADR 0012).

Four official extensions provide the domain vocabulary:
- **@specforge/software** (5 kinds): behavior, invariant, event, type, port
- **@specforge/product** (9 kinds): journey, deliverable, milestone, module, term, feature, persona, channel, release
- **@specforge/governance** (3 kinds): decision, constraint, failure_mode
- **@specforge/formal** (5 kinds): property, axiom, protocol, refinement, process — plus structured conditions (inline requires/ensures/maintains fields), specification layering, event graph linting, coverage tracking via entity_enhancements on @specforge/software entities

Total: 2 structural + 22 domain = 24 entity kinds (no budget cap).

Every entity has a unique ID, compiler-checked cross-references, and a defined role in the traceability chain. Teams adopt only what they need — start with structural kinds, add extensions as projects grow.

## Architecture: Structural Core + Extensions

```
┌──────────────────────────────────────────────────────────────┐
│                    STRUCTURAL (2 kinds)                       │
│  spec (singleton config) · ref (external references)         │
│  + zero-entity core: ANY keyword parsed, extensions validate │
├──────────────────────────────┬───────────────────────────────┤
│  @specforge/software (5)     │  @specforge/governance (3)    │
│  behavior · invariant        │  decision · constraint        │
│  event · type · port         │  failure_mode                 │
├──────────────────────────────┤                               │
│  @specforge/product (9)      │                               │
│  journey · deliverable       │                               │
│  milestone · module · term   │                               │
│  feature · persona · channel │                               │
│  release                     │                               │
├──────────────────────────────┘                               │
│  @specforge/formal (5 kinds: property, axiom, protocol,      │
│  refinement, process) · structured conditions (inline) ·     │
│  specification layering · event graph linting · coverage     │
│  tracking · 10 edge types · 4 compiler passes · 3 feature    │
│  flags                                                       │
└──────────────────────────────────────────────────────────────┘
```

### Why This Split

**Structural core** contains two kinds parsed by the core grammar:
- `spec` — singleton project configuration (name, version, extensions, providers)
- `ref` — external resource references with scheme-based routing

The core grammar parses ANY `keyword name { fields }` block generically. Validation of which keywords are legal, what fields are allowed, and what edges exist comes entirely from extensions' declarations. If a new domain requires a compiler change, the architecture has failed.

**@specforge/software** is a recommended-by-default extension (like Terraform's built-in providers). It adds the software engineering domain: behavioral contracts (`behavior → invariant`), the domain event model (`event`), and the code bridge (`type` + `port`). All 5 entity kinds are testable with verify support.

**@specforge/product** adds product planning and delivery entities. Not every project ships a product — internal tools, scripts, and libraries don't need journeys, deliverables, or milestones. This extension extends the chain upward (`deliverable -> journey -> feature`) and adds the structural bridge (`module -> feature`), temporal dimension (`milestone`), domain-neutral feature grouping (`feature`), user modeling (`persona`), interaction medium modeling (`channel`), and glossary (`term`).

**@specforge/governance** adds architecture governance, quality tracking, and risk assessment. Not every project formalizes ADRs, NFRs, or FMEA — early-stage startups and prototypes rarely do. This extension adds overlay entities that reference software entities (`decision → invariant`, `constraint → behavior`, `failure_mode → invariant`).

### Extension CLI

```bash
specforge init                          # structural core only (2 kinds: spec, ref)
specforge add @specforge/software       # + 5 software entities (recommended)
specforge add @specforge/product        # + 8 product entities
specforge add @specforge/governance     # + 3 governance entities
specforge add @specforge/formal         # + 5 formal entities (property, axiom, protocol, refinement, process)
specforge add @specforge/software @specforge/product  # several at once, as one change
specforge remove @specforge/governance  # remove extension
specforge extensions                    # list installed extensions
```

`specforge init` offers interactive setup with @specforge/software pre-selected:

```
? Which extensions do you want? (space to select)
  ● @specforge/software    — behavior, invariant, event, type, port (recommended)
  ○ @specforge/product     — journey, deliverable, milestone, module, term, feature, persona, channel, release
  ○ @specforge/governance  — decision, constraint, failure_mode
  ○ @specforge/formal      — property, axiom, protocol, refinement, process + structured conditions (inline)
```

## Entity Summary

### Structural (Core)

| # | Entity | ID Pattern | Question It Answers |
|---|--------|-----------|---------------------|
| 1 | [spec](entities/spec.md) | singleton | What project is this? |
| 2 | [ref](entities/ref.md) | `scheme.kind:identifier` | What external resource is this connected to? |

### @specforge/software

| # | Entity | ID Pattern | Question It Answers |
|---|--------|-----------|---------------------|
| 3 | [behavior](entities/behavior.md) | `identifier` | What exactly does the system do? |
| 4 | [invariant](entities/invariant.md) | `identifier` | What must ALWAYS be true? |
| 5 | [event](entities/event.md) | `identifier` | What does the system announce? |
| 6 | [type](entities/type.md) | `identifier` | What shape does the data have? |
| 7 | [port](entities/port.md) | `identifier` | What contracts exist between components? |

### @specforge/product

| # | Entity | ID Pattern | Question It Answers |
|---|--------|-----------|---------------------|
| 8 | [feature](entities/feature.md) | `identifier` | What value does this deliver? |
| 9 | [journey](entities/journey.md) | `identifier` | How does the user experience this? |
| 10 | [deliverable](entities/deliverable.md) | `identifier` | What ships to users? |
| 11 | [milestone](entities/milestone.md) | `identifier` | When does this ship? |
| 12 | [module](entities/module.md) | `identifier` | What component delivers this? |
| 13 | [term](entities/term.md) | `identifier` | What does this term mean? |
| 14 | [persona](entities/persona.md) | `identifier` | Who uses the system? |
| 15 | [channel](entities/channel.md) | `identifier` | Through which medium? |
| 16 | [release](entities/release.md) | `identifier` | What coordinated shipment is this? |

### @specforge/governance

| # | Entity | ID Pattern | Question It Answers |
|---|--------|-----------|---------------------|
| 17 | [decision](entities/decision.md) | `identifier` | Why was this built this way? |
| 18 | [constraint](entities/constraint.md) | `identifier` | What quality must the system achieve? |
| 19 | [failure_mode](entities/failure-mode.md) | `identifier` | What can go wrong and how bad is it? |

### @specforge/formal

| # | Entity | ID Pattern | Question It Answers |
|---|--------|-----------|---------------------|
| 20 | property | `identifier` | What temporal assertion must hold over time? |
| 21 | axiom | `identifier` | What is assumed true without proof? |
| 22 | protocol | `identifier` | What synchronization contract do events share? |
| 23 | refinement | `identifier` | What abstract-to-concrete mapping exists? |
| 24 | process | `identifier` | What communicating process do events participate in? |

#### @specforge/formal Entity & Edge Diagram

```
┌──────────────────────────────────────────────────────────────────────┐
│                    @specforge/formal (5 kinds, 10 edges)             │
├──────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  ┌─────────────────── Structured Conditions ──────────────────────┐  │
│  │                                                                │  │
│  │   ┌───────────┐                                               │  │
│  │   │ behavior  │  requires/ensures/maintains (inline fields)   │  │
│  │   │(enhanced) │  → ConditionEntry AST nodes                   │  │
│  │   │           │                                               │  │
│  │   │           │  Satisfies           ┌───────────┐            │  │
│  │   │           │─────────────────────▶│ property  │            │  │
│  │   └───────────┘                      │           │            │  │
│  │                                      └─────┬─────┘            │  │
│  │   ┌───────────┐  AssumedBy          PropertyDependsOn         │  │
│  │   │ invariant │────────────────┐           │                  │  │
│  │   │(enhanced) │                ▼           ▼                  │  │
│  │   └───────────┘           ┌───────┐  ┌───────────┐            │  │
│  │                           │ axiom │  │ invariant │            │  │
│  │                           └───────┘  └───────────┘            │  │
│  └────────────────────────────────────────────────────────────────┘  │
│                                                                      │
│  ┌─────────────────── Specification Layering ─────────────────────┐  │
│  │                                                                │  │
│  │   ┌────────────┐  RefinesAbstract,   ┌───────────┐            │  │
│  │   │ refinement │────────────────────▶│ behavior  │            │  │
│  │   │            │  RefinesConcrete    │           │            │  │
│  │   │            │                     └───────────┘            │  │
│  │   │            │  ChainsToRefinement                          │  │
│  │   │            │────────────────────▶┌────────────┐           │  │
│  │   └────────────┘                     │ refinement │           │  │
│  │                                      └────────────┘           │  │
│  │   field form: behavior ── refines ──▶ behavior (abstract true)│  │
│  └────────────────────────────────────────────────────────────────┘  │
│                                                                      │
│  ┌─────────────────── Event Graph Linting (CSP) ──────────────────┐  │
│  │                                                                │  │
│  │   ┌───────────┐  FollowsProtocol    ┌──────────┐             │  │
│  │   │   event   │────────────────────▶│ protocol │             │  │
│  │   │(enhanced) │                     └──────────┘             │  │
│  │   │           │  ParticipatesIn     ┌──────────┐             │  │
│  │   │           │────────────────────▶│ process  │             │  │
│  │   └───────────┘                     │          │             │  │
│  │                                     │          │             │  │
│  │                ProcessComposition   │          │             │  │
│  │                  ┌──────────┐──────▶│          │             │  │
│  │                  │ process  │       └──────────┘             │  │
│  │                  └──────────┘                                │  │
│  └────────────────────────────────────────────────────────────────┘  │
│                                                                      │
│  Entity kinds:  property · axiom · protocol · refinement · process   │
│  All: testable=false, supports_verify=false                          │
│  Conditions: inline fields (requires/ensures/maintains), not entities│
│  Errors:   E031, E041 (refinement cycle), E042 (process cycle)       │
│  Warnings: W029-W031, W035, W096, W110, W123-W134 (diagnostics.md)   │
│  Passes:   condition_check → layering_verify → event_graph_analyze   │
│            → coverage_tracking                                       │
└──────────────────────────────────────────────────────────────────────┘
```

## Cross-Extension References

When an entity references another entity from a different extension, the compiler uses **soft references** — a progressive enhancement model where spec files are valid with or without extensions installed.

### Resolution Rules

| Scenario | From | To | Behavior |
|----------|------|----|----------|
| **Same Extension** | extension entity | same extension entity | Always validated. |
| **Extension → Structural** | extension entity | spec/ref | Always validated. |
| **Cross-Extension** | extension entity | other extension entity | **Soft reference.** If target extension installed → validated (`E003` on miss). If not installed → `I004` info. |

### Diagnostic: I004 (Unknown Entity in Reference Field)

When a reference uses an identifier not found in any installed extension's entity registry:

```
info[I004]: Unknown entity 'use_postgresql' in field 'adrs'
  ┌─ behaviors/auth.spec:3:16
  │
3 │   adrs [use_postgresql]
  │         ^^^^^^^^^^^^^^ not found in installed extensions
  │
  = help: Install @specforge/governance to enable decision validation
```

### Example

```spec
// Structural core + @specforge/software only (no product/governance)
behavior create_user {
  invariants [data_persistence]    // ✅ Same extension: validated
  features   [user_management]     // ℹ️ I004: "Install @specforge/product"

  contract "..."
}
```

After `specforge add @specforge/product`:

```spec
// Same file — now product is installed
behavior create_user {
  invariants [data_persistence]    // ✅ Same extension: validated
  features   [user_management]     // ✅ Cross-extension: validated (E003 if not found)

  contract "..."
}
```

### Field-to-EntityKind Registry

The compiler uses the field name in which a reference appears to determine the expected target entity type. This replaces the prefix-based routing of the old ID system.

| Field Name | Target EntityKind | Extension |
|------------|-------------------|-----------|
| `invariants` | Invariant | @specforge/software |
| `enforces` | Invariant | @specforge/software |
| `types` | Type | @specforge/software |
| `ports` | Port | @specforge/software |
| `produces` | Event | @specforge/software |
| `trigger` | Behavior | @specforge/software |
| `features` | Feature | @specforge/product |
| `tests` | (file reference) | @specforge/software |
| `fieldType` | Type | @specforge/software |
| `refs` | Ref | core (structural) |
| `journeys` | Journey | @specforge/product |
| `modules` | Module | @specforge/product |
| `depends_on` | Module | @specforge/product |
| `adrs` | Decision | @specforge/governance |
| `invariant` (singular) | Invariant | @specforge/software |
| `affects` | Behavior | @specforge/software |

When the target kind's extension is not installed, the compiler emits `I004` instead of `E003`.

## Field Type Vocabulary (C2-08)

Extensions declare fields with one of eight typed vocabularies
(`specforge_protocol_types::FieldType`; every output names a type as below, ADR 0034). Author-facing meaning:

| Type | Value shape | Reference semantics |
|------|-------------|---------------------|
| `string` | free text | none |
| `integer` | whole number | none |
| `bool` | true/false | none |
| `enum` | one of the declared enum values | none |
| `string_list` | list of strings | none |
| `reference` | a single entity id | MUST resolve to a declared entity (E003) |
| `reference_list` | list of entity ids | each MUST resolve (E003) |
| `block` | nested structured block (requires/ensures-style clauses) | clause names are surfaced to extension passes |

A field may additionally declare `edge` + `target_kind` (its references
become typed graph edges), `required` (E006 enforcement), and
`file_reference=true` (path values validated to exist, e.g. `gherkin`), and
`normative=true` when the field states what the entity promises rather than
prose (a behavior's `contract`, an invariant's `guarantee`, a decision's
`decision`): `specforge export --format context` keeps normative fields and
drops the rest, without core knowing any field by name. `headline=true`
lifts the field to the context node's top level (a behavior's `contract`, a
feature's `status`), and `exempts_obligations=true` makes an entity that sets
it owe no `verify` obligations (formal's `abstract true`; W004, coverage and
stats leave it out). An entity kind may declare `contract_target` (references
to it are contract obligations, A010) and `declares_types` (its ids are the
type names custom validators resolve), and an extension a `theme_color` its
`model` and `outline` diagrams use (ADR 0007).
The per-kind vocabulary is the field registry's — `specforge model`
renders it from the same source.

## Naming Conventions

The grammar terminal accepts `[A-Za-z_][A-Za-z0-9_]*`; the compiler enforces
the documented contract on top of it. There is no enforced case convention.

```ebnf
identifier = ( letter | "_" ) , { letter | digit | "_" } ;   (* 2-60 chars — enforced, E014 *)
```

- **Length (E014):** identifiers MUST be 2-60 characters. The grammar's
  terminal is deliberately loose; the length contract is a graph-build check
  so the bound can evolve without a grammar change.
- **Reserved words (E013):** an identifier MUST NOT equal a reserved word —
  the structural keywords (`spec`, `ref`, `use`, `define`) or any
  extension-declared entity kind (e.g. `behavior`, `feature`). Enforced at
  graph build with a rename suggestion; a collision makes `refs [behavior]`-
  style entries ambiguous with the block introducer itself.

| Convention | Used By | Examples |
|------------|---------|----------|
| Free-form identifier | all named entities | `data_persistence`, `UserRepository`, `camelCase`, `SCREAMING_SNAKE` |
| Scheme-based | ref | `gh.issue:42`, `jira.epic:PROJ-123` |
| Singleton | spec | one per project, no ID |

### Flat Namespace

No two entities of ANY type can share the same name. `invariant data_check` and `behavior data_check` in the same project is an `E002` error. This prevents ambiguity in cross-references.

### Title Derivation

The title string after the identifier is optional. If omitted, the compiler auto-derives a title from the identifier:

- `auth_login` → "Auth Login"
- `data_persistence` → "Data Persistence"
- `UserRepository` → "User Repository"

Explicit titles override: `behavior auth_login "Login with Credentials" { ... }`

### Verify Statements

`verify` takes an optional kind: `verify unit "..."`, `verify contract "..."`, or the bare form
`verify "..."`. The bare form is accepted by the grammar and parses with an empty kind — the kind
word is preserved verbatim for downstream consumers (extensions decide which kinds they require).

### Reserved Words

Entity identifiers MUST NOT collide with reserved words. Two layers:

1. **Structural keywords** (`spec`, `ref`, `use`, `define`) are grammar
   block introducers — the parser itself rejects them as names (`E001`).
2. **Extension-declared keywords** (every kind in the KindRegistry, e.g.
   `behavior`, `feature`, `event`) are reserved as identifiers. Using one
   produces **`E013`** at graph build with a rename suggestion. Enforced
   since 2026-09; earlier revisions documented this rule without
   implementing it.

Reserved words remain valid as *kinds* (`behavior auth_login { ... }`) —
only the identifier position is restricted.

### Unicode

Identifiers allow Unicode letters (NFC-normalized). Bidirectional characters are forbidden. The `--ascii-only` lint restricts identifiers to ASCII.

### Backtick Escaping

For edge cases, backtick-escaped identifiers allow characters normally forbidden: `` `complex-name` ``.

## Traceability Chain

The entities form a directed acyclic graph. The @specforge/software chain is self-contained and useful on its own. Other extensions extend it:

### @specforge/software Chain

```
behavior ──enforces──→ invariant
    │
    │produces
    ▼
  event

type ←──TypeExtendsType── type (inheritance)
behavior / port ──references──→ type (code bridge)

any entity ──ExternalRef──→ URI
```

### Extended by @specforge/product

```
release ──ships──→ deliverable ──bundles──→ journey ──traces_to──→ feature
    │                  │            │
    │targets           │built_from  │persona / channels
    ▼                  ▼            ▼
  milestone          module     persona / channel
                     │
                     │provides
                     ▼
                   feature

milestone ──schedules──→ feature / module
milestone ──depends_on──→ milestone
deliverable ──targets──→ milestone
deliverable ──depends_on──→ deliverable
```

### Extended by @specforge/governance

```
decision ──protects──→ invariant (software)

constraint ──constrains──→ behavior (software) / invariant (software)

failure_mode ──mitigates──→ invariant (software)
```

**Full chain:** `release -> deliverable -> journey -> feature -> behavior -> invariant`
**Code bridge:** `deliverable -> module -provides-> feature`
**Temporal:** `milestone -> feature / module / milestone`
**Release coordination:** `release -> deliverable / milestone`
**Governance overlay:** `decision ─protects→ invariant`, `constraint → behavior`, `failure_mode → invariant`

## Edge Types

### @specforge/software Edges (15)

| Edge Type | From | To | Semantics |
|-----------|------|----|-----------|
| `ExternalRef` | any | any | Entity references an external URI or resource |
| `References` | any | any | General cross-reference between entities |
| `BehaviorConsumesEvent` | behavior | event | This behavior consumes these events |
| `BehaviorEnforcesInvariant` | behavior | invariant | This behavior enforces these invariants |
| `BehaviorImplementsFeature` | behavior | feature | This behavior implements these features |
| `BehaviorProducesEvent` | behavior | event | This behavior emits these events |
| `BehaviorReferencesType` | behavior | type | This behavior uses these types |
| `BehaviorUsesPort` | behavior | port | This behavior uses these ports |
| `EventCarriesPayloadType` | event | type | This event has as its payload these types |
| `MilestoneIncludesBehavior` | milestone | behavior | This milestone delivers these behaviors |
| `ModuleConsumesPort` | module | port | This module consumes these ports |
| `ModuleDefinesPort` | module | port | This module defines these ports |
| `PortReferencesType` | port | type | This port references these types |
| `TypeComposesType` | type | type | This type composes these types |
| `TypeExtendsType` | type | type | This type extends these types |

### @specforge/product Edges (20)

| Edge Type | From | To | Semantics |
|-----------|------|----|-----------|
| `DeliverableContainsModule` | deliverable | module | This deliverable contains these modules |
| `DeliverableDependsOn` | deliverable | deliverable | This deliverable depends on these deliverables |
| `DeliverableSupportsJourney` | deliverable | journey | This deliverable ships these journeys |
| `DeliverableTrackedByMilestone` | deliverable | milestone | This deliverable is tracked by these milestones |
| `FeatureDependsOn` | feature | feature | This feature depends on these features |
| `FeatureRelatesTo` | feature | feature | This feature relates to these features |
| `JourneyExercisesFeature` | journey | feature | This journey exercises these features |
| `JourneyTargetsPersona` | journey | persona | This journey is performed by these personas |
| `JourneyUsesChannel` | journey | channel | This journey uses these channels |
| `MilestoneDeliversFeature` | milestone | feature | This milestone schedules these features |
| `MilestoneDependsOn` | milestone | milestone | This milestone depends on these milestones |
| `MilestoneScopesModule` | milestone | module | This milestone includes these modules |
| `ModuleContainsFeature` | module | feature | This module implements these features |
| `ModuleDependsOn` | module | module | This module depends on these modules |
| `PersonaPrioritizesFeature` | persona | feature | This persona prioritizes these features |
| `ReleaseCompletesMilestone` | release | milestone | This release targets these milestones |
| `ReleaseDependsOn` | release | release | This release depends on these releases |
| `ReleaseIncludesDeliverable` | release | deliverable | This release ships these deliverables |
| `TermBelongsToModule` | term | module | This term belongs to these modules |
| `TermReferencesRelatedTerm` | term | term | This term cross-references these terms |

### @specforge/governance Edges (11)

| Edge Type | From | To | Semantics |
|-----------|------|----|-----------|
| `ConstraintConstrainsBehavior` | constraint | behavior | This constraint constrains these behaviors |
| `ConstraintEnforcedByBehavior` | constraint | behavior | This constraint is enforced by these behaviors |
| `ConstraintGovernsFeature` | constraint | feature | This constraint governs these features |
| `ConstraintProtectsInvariant` | constraint | invariant | This constraint protects these invariants |
| `DecisionAffectsFeature` | decision | feature | This decision affects these features |
| `DecisionImposesConstraint` | decision | constraint | This decision imposes these constraints |
| `DecisionProtectsInvariant` | decision | invariant | This decision protects these invariants |
| `DecisionSupersedesDecision` | decision | decision | This decision supersedes these decisions |
| `FailureModeAffectsBehavior` | failure_mode | behavior | This failure mode affects these behaviors |
| `FailureModeTargetsInvariant` | failure_mode | invariant | This failure mode targets these invariants |
| `FailureModeThreatensFeature` | failure_mode | feature | This failure mode threatens these features |

### @specforge/formal Edges (10)

| Edge Type | From | To | Semantics |
|-----------|------|----|-----------|
| `AxiomAssumesInvariant` | axiom | invariant | This axiom assumes these invariants |
| `BehaviorRefinesBehavior` | behavior | behavior | This behavior refines these behaviors |
| `BehaviorSatisfiesProperty` | behavior | property | This behavior satisfies these properties |
| `EventFollowsProtocol` | event | protocol | This event follows these protocols |
| `EventParticipatesInProcess` | event | process | This event participates in these processes |
| `ProcessComposesProcess` | process | process | This process composes these processes |
| `PropertyDependsOnInvariant` | property | invariant | This property depends on these invariants |
| `RefinementChainsToRefinement` | refinement | refinement | This refinement chains to these refinements |
| `RefinementRefinesAbstract` | refinement | behavior | This refinement maps from these behaviors |
| `RefinementRefinesConcrete` | refinement | behavior | This refinement maps to these behaviors |

### Cross-Extension Edges (Soft References)

| Edge Type | From | To | Semantics |
|-----------|------|----|-----------|
| `BehaviorImplementsFeature` | behavior (software) | feature (product) | "This behavior implements this feature" — via peer_dependency |
| `MilestoneIncludesBehavior` | milestone (product) | behavior (software) | "This milestone delivers these behaviors" — via entity_enhancement |

## Validation Rules

The compiler enforces structural invariants. Each rule belongs to the extension that owns the entities it validates. **Extension rules only fire when the extension is installed.** Cross-extension rules include `requires` guards for extension availability.

Every diagnostic code, its meaning, and the component that owns it (`core` or the
emitting `@specforge/<name>` extension) is listed in [docs/diagnostics.md](diagnostics.md).
That page is generated from the `specforge explain` catalog (`crates/specforge-diagnostics/src/lib.rs`),
which a test keeps in lockstep with the codes the compiler and extensions actually emit — so it is
the only registry; run `specforge explain <CODE>` for the same text in the terminal. Codes in the
`E900`–`E998`, `W900`–`W998` and `I900`–`I998` ranges are reserved for third-party extensions.

## DSL Scope Boundaries

### What belongs in the DSL (24 entity types: 2 structural + 22 across 4 extensions)

The 24 entity types above are the complete set of compiled block types. They were selected because they have high cross-reference density, benefit from compiler validation, and complete the traceability chain.

### What stays as markdown

These concepts gain nothing from compilation — they are prose documents with minimal cross-references:

| Concept | Reason |
|---------|--------|
| **research** | Exploratory narratives. Only `related_adr` and `outcome` are structural. |
| **product** | Pure prose: pitch, positioning, go-to-market strategy. Zero cross-references. |
| **process** | Governance docs: definition of done, test strategy, change control. |
| **references** | External links and tool references. Now partially handled by the `ref` entity for compiler-tracked external references; unstructured references remain as markdown. |
| **type-system** | Meta-documentation about type patterns. The `type` blocks handle actual types. |

### What is generated output

These are never source — they are produced by the compiler:

| Concept | Generated by |
|---------|-------------|
| **traceability** | `specforge trace` — auto-generated from graph traversal |
| **overview** | Compiler-generated from the graph |

### Meta-schema extensibility

For domain-specific entity types beyond the 23 shipped types, the `define` mechanism in the `spec` root block allows user-defined types with attribute validation, reference resolution, unreferenced detection, and LSP support. See [spec entity docs](entities/spec.md) for syntax.

## Progressive Adoption

SpecForge supports progressive adoption via its extension architecture. Teams start with structural core and add extensions as projects grow.

### Level 1: Structural Only

Just the compiler with zero domain knowledge. Parses any `keyword name { fields }` block generically. Useful for exploring the DSL. Custom entity kinds come from an extension: a `define` block is reported (W143) and ignored ([ADR 0005](adr/0005-define-blocks-removed.md)).

```bash
specforge init --no-extensions
# → 2 structural kinds: spec, ref
# → Core grammar parses any keyword, but no validation beyond structure
```

### Level 2: + Software Engineering (recommended starting point)

For any software project. Add behavioral contracts, domain events, type definitions, and port interfaces.

```bash
specforge init
# → @specforge/software pre-selected (recommended)
# → +5 entities: behavior, invariant, event, type, port
# → All 5 are testable with verify support
```

### Level 3: + Product Planning

For teams building products, add journeys, deliverables, milestones, modules, terms, features, personas, and channels.

```bash
specforge add @specforge/product
# → +9 entities: feature, journey, deliverable, milestone, module, term, persona, channel, release
```

### Level 3.5: + Formal Analysis

For teams using structured conditions, specification layering, or event graph linting. Requires @specforge/software.

```bash
specforge add @specforge/formal
# → +5 entity kinds: property, axiom, protocol, refinement, process
# → +4 compiler passes: condition_check, layering_verify, event_graph_analyze, coverage_tracking
# → +10 edge types (BehaviorSatisfiesProperty, BehaviorRefinesBehavior, EventFollowsProtocol, EventParticipatesInProcess, PropertyDependsOnInvariant, AxiomAssumesInvariant, RefinementRefinesAbstract/Concrete, RefinementChainsToRefinement, ProcessComposesProcess)
# → Block condition fields (requires/ensures/maintains) enhanced on behavior entities
# → Requires warning_level=strict in specforge.json for formal warnings
```

### Level 4: + Governance

For teams that need architecture rationale, quality tracking, and risk management.

```bash
specforge add @specforge/governance
# → +3 entities: decision, constraint, failure_mode
```

### Level 5: Domain-Specific

For regulated industries, complex domains, or custom workflows. Write an extension for your own entity kinds, or use the community extension ecosystem.

```bash
# Your own kinds: specforge new @you/my-kinds --extension
# Community: specforge add @specforge/compliance
#            specforge add @specforge/visual
```

A team using only @specforge/software gets full value from `specforge check` + `specforge trace` without ever touching product planning, governance, or compliance.

## Design Principles

1. **Zero domain knowledge in core** — the compiler is a pure typed-graph engine; ALL domain vocabulary comes from extensions; if a new domain requires a compiler change, the architecture has failed
2. **Every entity earns its place** — each answers a distinct question no other entity answers
3. **Compiler-checked references** — entity names are typed, resolved, and validated at compile time; cross-extension references degrade gracefully via soft references
4. **Traceability by construction** — the graph structure enforces traceability; unreferenced detection catches missing links
5. **Progressive adoption** — start with structural core, add @specforge/software (5), @specforge/product (8), @specforge/governance (3), @specforge/formal (5 kinds, 4 passes) as needed
6. **Language-agnostic** — the entity model works for any software project regardless of implementation language
7. **Bounded complexity** — the DSL balances expressiveness with readability (currently 24 entity kinds across 4 extensions); beyond official extensions, write your own or use community extensions
8. **Extensions don't break specs** — a spec file is always valid with structural core alone; extensions add validation, they don't remove it
