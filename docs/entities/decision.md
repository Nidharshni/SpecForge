# decision

> **Module:** `@specforge/governance`

## Purpose

A `decision` is an **Architecture Decision Record (ADR)** — a documented rationale for a significant technical choice. Decisions capture the *why* behind the system's construction: what alternatives were considered, what was chosen, and what consequences follow.

It answers: **"Why was this built this way?"**

Decisions prevent knowledge loss. When a new team member asks "why do we use PostgreSQL instead of MongoDB?", the answer lives in `use_postgresql`, not in someone's memory.

## ID Pattern

```
identifier
```

Examples: `use_postgresql`, `event_sourcing_audit`, `migrate_grpc`

## Syntax

```spec
decision use_postgresql "PostgreSQL over MongoDB" {
  status   accepted
  date     2025-03-01

  context """
    We need a primary datastore. Team has SQL expertise.
    Document model not needed — data is relational.
  """

  decision """
    Use PostgreSQL with typed schemas.
  """

  consequences [
    "Migrations required for schema changes",
    "Strong ACID guarantees",
    "Team can leverage existing SQL knowledge",
  ]

  invariants [data_persistence]
}
```

## Fields

### Required

| Field | Type | Description |
|-------|------|-------------|
| `status` | enum | Lifecycle status of the decision (see Status Lifecycle below). |
| `context` | string or triple-string | The situation that motivated this decision. What forces are at play? |
| `decision` | string or triple-string | The choice that was made. What are we doing? |

### Optional

| Field | Type | Description |
|-------|------|-------------|
| `title` | string | Human-readable name (the string after the entity ID). |
| `number` | integer | Optional sequential number for display in reports (e.g., `number 1`). |
| `date` | date | When the decision was made (YYYY-MM-DD). |
| `consequences` | string list | Known consequences of this decision — both positive and negative. |
| `invariants` | reference list | Invariants that this decision protects or enables. |
| `refs` | reference list | External references (issues, tickets, diagrams) linked to this decision. |

## Status Lifecycle

| Status | Meaning |
|--------|---------|
| `proposed` | Under discussion, not yet accepted. |
| `accepted` | Approved and in effect. The system is built according to this decision. |
| `deprecated` | Superseded by a newer decision. Still present for historical context. |
| `superseded` | Replaced by another ADR. The `superseded_by` field should reference the replacement. |

## Relationships

### Outgoing edges

| To | Edge Type | Meaning |
|----|-----------|---------|
| `feature` | `DecisionAffectsFeature` | "This decision affects these features" (via `affects_features` field) |
| `constraint` | `DecisionImposesConstraint` | "This decision imposes these constraints" (via `constraints` field) |
| `invariant` | `DecisionProtectsInvariant` | "This decision protects these invariants" (via `invariants` field) |
| `decision` | `DecisionSupersedesDecision` | "This decision supersedes these decisions" (via `superseded_by` field) |

### Incoming edges

| From | Edge Type | Meaning |
|------|-----------|---------|
| `decision` | `DecisionSupersedesDecision` | "Decisions that supersede this decision" (via `superseded_by` field) |

## Validation Rules

| Code | Rule |
|------|------|
| E003 | Every ID in `invariants` must resolve to a declared entity. |
| E002 | No two decisions may share the same ID. |
| E061 | `status` must be one of the recognized decision statuses. |

## Design Guidance

### Good Decisions

Decisions should capture:
- **Context** — what problem or force prompted this decision
- **Alternatives considered** — what options were evaluated (can be in context or consequences)
- **Rationale** — why this option was chosen over others
- **Consequences** — both benefits and trade-offs

### When to Write an ADR

Write a decision when:
- Choosing between technologies (database, framework, language)
- Making an architectural pattern choice (monolith vs. microservices, sync vs. async)
- Establishing a convention that affects multiple components
- Making a choice that would be expensive to reverse

### When NOT to Write an ADR

Skip a decision for:
- Trivial implementation choices (variable names, formatting)
- Standard practices that don't need justification
- Choices that are easily reversible with no consequences

## Related Entities

| Direction | Entity | Edge | Meaning |
|-----------|--------|------|---------|
| outgoing | [feature](feature.md) | `DecisionAffectsFeature` | Features this decision affects |
| outgoing | [constraint](constraint.md) | `DecisionImposesConstraint` | Constraints this decision imposes |
| outgoing | [invariant](invariant.md) | `DecisionProtectsInvariant` | Invariants this decision protects |
| outgoing | [decision](decision.md) | `DecisionSupersedesDecision` | Decisions this decision supersedes |
| incoming | [decision](decision.md) | `DecisionSupersedesDecision` | Decisions that supersede this decision |

## Examples

### Technology Choice

```spec
decision use_postgresql "PostgreSQL over MongoDB" {
  status   accepted
  number   1
  date     2025-03-01

  context """
    We need a primary datastore. Team has SQL expertise.
    Document model not needed — data is relational.
    ACID transactions are required for financial data integrity.
  """

  decision """
    Use PostgreSQL 15+ with typed schemas and row-level security.
  """

  consequences [
    "Migrations required for schema changes",
    "Strong ACID guarantees",
    "Team can leverage existing SQL knowledge",
    "No native document storage — complex JSON queries are slower",
  ]

  invariants [data_persistence]
  refs [gh.discussion:12]
}
```

### Architectural Pattern

```spec
decision event_sourcing_audit "Event Sourcing for Audit Trail" {
  status   accepted
  number   5
  date     2025-06-15

  context """
    Regulatory requirements demand a complete, immutable audit trail
    of all state changes. A traditional update-in-place model would
    require separate audit logging with risk of drift.
  """

  decision """
    Use event sourcing for the billing domain.
    All state changes are stored as an append-only event log.
    Current state is derived by replaying events.
  """

  consequences [
    "Complete audit trail by construction",
    "Event replay enables point-in-time recovery",
    "Increased storage requirements",
    "Eventually consistent read models required",
    "Team needs training on event sourcing patterns",
  ]

  invariants [audit_integrity, idempotent_orders]
}
```

### Proposed (Not Yet Accepted)

```spec
decision migrate_grpc "Migrate from REST to gRPC for Internal Services" {
  status   proposed
  number   12
  date     2026-02-15

  context """
    Internal service-to-service communication uses REST/JSON.
    Serialization overhead is measurable at current scale (>15% of p99 latency).
    Type safety between services relies on manual OpenAPI maintenance.
  """

  decision """
    Adopt gRPC with Protocol Buffers for all internal service communication.
    External API remains REST/JSON.
  """

  consequences [
    "~40% reduction in serialization latency (estimated)",
    "Compile-time type safety between services",
    "Requires protobuf tooling in CI",
    "Debugging is harder — binary protocol requires specialized tools",
    "Two API paradigms to maintain (external REST + internal gRPC)",
  ]
}
```
