# SpecForge Documentation

This directory contains the entity model reference and design documentation for SpecForge. For a project overview, see the [root README](../README.md). For the full architecture — core + plugins, edge types, validation rules, and design principles — see [entity-model.md](entity-model.md).

## Entity Reference

### Core (8 entities)

| Entity | ID Pattern | Purpose |
|--------|-----------|---------|
| [spec](entities/spec.md) | singleton | Project root configuration — name, infix, version, codegen settings |
| [invariant](entities/invariant.md) | `INV-{infix}-{n}` | Runtime guarantee the system must never violate |
| [behavior](entities/behavior.md) | `BEH-{infix}-{n}` | Behavioral contract for a single operation with RFC 2119 keywords |
| [feature](entities/feature.md) | `FEAT-{infix}-{n}` | User-facing capability composed of behaviors |
| [event](entities/event.md) | `EVT-{infix}-{n}` | Domain event emitted by a behavior, consumed by others |
| [type](entities/type.md) | identifier | Data type definition — structs, unions, errors, commands |
| [port](entities/port.md) | identifier | Interface contract — hexagonal architecture boundary |
| [ref](entities/ref.md) | `scheme.kind:identifier` | External reference — typed link to issues, tickets, designs |

### @specforge/product (5 entities)

| Entity | ID Pattern | Purpose |
|--------|-----------|---------|
| [journey](entities/journey.md) | `identifier` | UX flow mapping a persona + surface to features |
| [deliverable](entities/deliverable.md) | `identifier` | Shippable artifact bundling journeys and modules |
| [milestone](entities/milestone.md) | `identifier` | Planning phase with scheduled features and exit criteria |
| [module](entities/module.md) | `identifier` | Code package mapping features to ports |
| [term](entities/term.md) | `identifier` | Structured vocabulary defining the project's ubiquitous language |

### @specforge/governance (3 entities)

| Entity | ID Pattern | Purpose |
|--------|-----------|---------|
| [decision](entities/decision.md) | `ADR-{n}` | Architecture Decision Record — rationale for technical choices |
| [constraint](entities/constraint.md) | `CON-{infix}-{n}` | Non-functional requirement with measurable thresholds |
| [failure_mode](entities/failure-mode.md) | `FM-{infix}-{n}` | FMEA risk assessment tied to an invariant |

### @specforge/formal (5 entity kinds + enhances @specforge/software)

| Entity | ID Pattern | Purpose |
|--------|-----------|---------|
| property | `identifier` | Temporal/behavioral assertion (safety/liveness/fairness) |
| axiom | `identifier` | Assumed-true foundation (no proof required) |
| protocol | `identifier` | Shared synchronization contract across events |
| refinement | `identifier` | Abstract-to-concrete behavior mapping with condition deltas |
| process | `identifier` | CSP-style communicating process with alphabet and composition |

Provides structured conditions (inline requires/ensures/maintains fields on behaviors), specification layering, event graph linting, and coverage tracking. Conditions are inline fields that reference invariants, not standalone entities. Contributes 8 edge types (AssumedBy, Satisfies, FollowsProtocol, PropertyDependsOn, RefinesTo, RefinementChainLink, ParticipatesIn, ProcessComposition), 4 compiler passes (condition_check, layering_verify, event_graph_analyze, coverage_tracking), and formal analysis diagnostics (E031, E041, E042, W029-W031, W035, W096, W110, W123, W126, W128, W131, W134; see [Diagnostic Codes](diagnostics.md)). Requires `warning_level=strict` in specforge.json.

## Traceability Chain

### Core

```
feature ──implements──→ behavior ──references──→ invariant
  FEAT-XX-N              BEH-XX-N     │           INV-XX-N
                                      │produces
                                      ▼
                                    event
                                    EVT-XX-N

                         type / port (code bridge)

          any entity ──links_to──→ ref (external reference bridge)
                                   scheme.kind:id
```

### Extended by @specforge/product

```
deliverable ──bundles──→ journey ──traces_to──→ feature (core)
                │
                │built_from
                ▼
              module ──provides──→ feature (core)
                        │
                        │defines_port
                        ▼
                      port (core)

milestone ──schedules──→ feature (core) / deliverable
```

### Extended by @specforge/governance

```
decision ──protects──→ invariant (core)
  ADR-N

constraint ──constrains──→ behavior (core) / invariant (core)
  CON-XX-N

failure_mode ──mitigates──→ invariant (core)
  FM-XX-N
```

**Full chain:** `deliverable -> journey -> feature -> behavior -> invariant`

## Authoring Guides

Learn to write `.spec` files:

- **[Zero to Hero Tutorial](guides/authoring-spec-files.md)** — the guided, end-to-end learning path (orientation → core concepts → entity tour → a complete worked project).
- **[Cookbook](guides/spec-cookbook.md)** — task-oriented recipes ("how do I model X?").
- **[Best Practices](guides/spec-best-practices.md)** — prescriptive rules and named anti-patterns.
- **[Troubleshooting](guides/spec-troubleshooting.md)** — diagnostic codes and how to fix them.
- **[Product Getting Started](guides/product-getting-started.md)** — `@specforge/product` walkthrough.
- **[Extending SpecForge](guides/extending-specforge.md)** — the extension-authoring tutorial: scaffold a Wasm extension, contribute kinds/fields/rules, write compiler passes, build and install.
- **[Formal Verification](guides/formal-verification.md)** — machine-checkable bounds and claims: the expression language, `metric expr { }`, SMT-proven consistency and entailment, counterexamples, discharge linkage.
- **[Worked Example: todo-app](../examples/todo-app/)** — a complete, validated reference project.
- **[Rust test tracing](guides/rust-test-tracing.md)** and **[vitest test tracing](guides/vitest-test-tracing.md)** — link tests to the obligations they prove; `specforge collect` and `analyze coverage`.
- **[Demo script](demo.md)** — a 10-minute walkthrough on `examples/shop`: spec, check, agent context, tests, coverage.

## Quick Reference

See **[quick-reference.md](quick-reference.md)** for a single-page cheat sheet covering every entity kind, edge type, and validation code.

## AI Agent Token Economics

AI coding agents spend 60-80% of their tokens on *discovery* — not *building*. SpecForge provides structured, machine-readable context that eliminates this waste:

- **90-95% fewer tokens** for context gathering (spec graph query vs. 20-50 file reads)
- **75-86% total token reduction** per agent task
- **70% fewer rework cycles** (first-shot accuracy from precise contracts)
- **Developer time savings exceed token savings by 10x** (~4,000 hours/year for a 100-dev org)

The industry has independently converged on structured context files (CLAUDE.md, .cursor/rules, copilot-instructions.md). SpecForge is the **compiled, cross-referenced, validated** version of what these tools approximate.

> Full analysis with citations: **[RES-18: AI Agent Token Economics](../spec/research/RES-18-ai-agent-token-economics.md)**

## Research

| ID | Title |
|----|-------|
| [RES-11a](../spec/research/RES-11a-spec-dsl-core-compiler.md) | Core compiler architecture |
| [RES-11b](../spec/research/RES-11b-spec-dsl-codegen-plugins.md) | Code generation & test plugins |
| [RES-12](../spec/research/RES-12-dsl-concept-expansion.md) | DSL concept expansion analysis |
| [RES-13](../spec/research/RES-13-README.md) | Market landscape 2026 |
| [RES-18](../spec/research/RES-18-ai-agent-token-economics.md) | **AI agent token economics — cost reduction analysis** |
| [RES-19](../spec/research/RES-19-market-position-success-estimation.md) | **Market position & success estimation** |
| [RES-25](../spec/research/RES-25-formal-methods-integration.md) | Formal methods integration |
| [RES-26](../spec/research/RES-26-zero-entity-core-architecture.md) | Zero-entity core architecture |
| [RES-27](../spec/research/RES-27-software-eng-entity-redesign.md) | Software engineering entity redesign |
| [Extension Model](extension-model.md) | Extensions, providers, and generators architecture |

## Validation Codes

Every diagnostic code, with its meaning, owner and fix, is listed in [Diagnostic Codes](diagnostics.md), generated from the catalog that `specforge explain <CODE>` prints.
