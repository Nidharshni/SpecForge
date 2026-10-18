# constraint

> **Module:** `@specforge/governance`

## Purpose

A `constraint` declares a **non-functional requirement** or **quality attribute** — a cross-cutting property that the system must satisfy. Unlike invariants (which are binary: violated or not), constraints are measurable qualities with thresholds and verification methods.

It answers: **"What quality must the system achieve?"**

Constraints cover the areas that IEEE 830 calls "non-functional requirements" and Arc42 calls "quality requirements": performance, security, reliability, scalability, compatibility, usability, maintainability, portability, legal compliance, and operational concerns.

## ID Pattern

```
identifier
```

Examples: `api_latency`, `pii_encryption_constraint`, `runtime_compat`

## Syntax

```spec
constraint api_latency "API Latency Under Load" {
  category    performance
  priority    must

  metric """
    response_time_p99 < 200ms
    at 1000 concurrent users
    sustained for 5 minutes
  """

  constrains  [create_user, read_user, update_email]
  protects    [data_persistence]

  verify load "k6 load test with 1000 VUs for 5 minutes, assert p99 < 200ms"
}
```

## Fields

### Required

| Field | Type | Description |
|-------|------|-------------|
| `category` | enum | The quality attribute category (see Category Taxonomy below). |
| `priority` | enum | `must`, `should`, or `may` — RFC 2119 priority level. |
| `description` | string or triple-string | The quality requirement description. For quantifiable constraints, use `metric` as an alias. |

### Optional

| Field | Type | Description |
|-------|------|-------------|
| `title` | string | Human-readable name (the string after the entity ID). |
| `metric` | string or triple-string | Alias for `description`. Use when the constraint is quantifiable with specific thresholds. Both field names are accepted by the compiler. |
| `constrains` | reference list | Behaviors this constraint applies to. |
| `protects` | reference list | Invariants this constraint helps protect. |
| `verify` | verify statement(s) | How to test this constraint (type + description). |
| `refs` | reference list | External references (issues, tickets, diagrams) linked to this constraint. |

## Category Taxonomy

| Category | Covers | Examples |
|----------|--------|----------|
| `performance` | Latency, throughput, resource usage | "p99 < 200ms", "handles 10k req/s" |
| `security` | Authentication, authorization, encryption, vulnerability management | "All PII encrypted at rest", "OWASP Top 10 mitigated" |
| `reliability` | Availability, fault tolerance, recovery | "99.9% uptime", "recovers from crash in < 30s" |
| `scalability` | Load capacity, elasticity, data volume | "Supports 100k concurrent users", "handles 1TB dataset" |
| `compatibility` | Platform support, backward compatibility, interoperability | "Node.js 18+", "backward-compatible API for 2 major versions" |
| `usability` | Accessibility, learnability, error prevention | "WCAG 2.1 AA", "new user completes onboarding in < 5 min" |
| `maintainability` | Modularity, testability, analyzability | "Test coverage > 90%", "no circular dependencies" |
| `portability` | Installability, adaptability, replaceability | "Runs on Linux/macOS/Windows", "Docker and bare-metal" |
| `legal` | Regulatory compliance, licensing, data residency | "GDPR compliant", "data stored in EU region only" |
| `operational` | Deployment, monitoring, logging, backup | "Blue-green deploy", "structured JSON logs", "daily backups" |

## Relationships

### Outgoing edges

| To | Edge Type | Meaning |
|----|-----------|---------|
| `behavior` | `ConstraintConstrainsBehavior` | "This constraint constrains these behaviors" (via `constrains` field) |
| `behavior` | `ConstraintEnforcedByBehavior` | "This constraint is enforced by these behaviors" (via `enforced_by` field) |
| `feature` | `ConstraintGovernsFeature` | "This constraint governs these features" (via `governs_features` field) |
| `invariant` | `ConstraintProtectsInvariant` | "This constraint protects these invariants" (via `protects` field) |

### Incoming edges

| From | Edge Type | Meaning |
|------|-----------|---------|
| `decision` | `DecisionImposesConstraint` | "Decisions that impose this constraint" (via `constraints` field) |

## Validation Rules

| Code | Rule |
|------|------|
| E001 | Every ID in `constrains` must resolve to an existing `behavior`. |
| E001 | Every ID in `protects` must resolve to an existing `invariant`. |
| E002 | No two constraints may share the same ID. |
| W006 | If a behavior has no constraint coverage for `security` or `performance` categories, emit "unconstrained behavior" warning. (Configurable — not all projects need this.) |

## Design Guidance

### Good Constraints

Constraints should be:
- **Measurable** — a test can pass or fail against the metric
- **Scoped** — they name the behaviors or invariants they apply to
- **Categorized** — using the standard category taxonomy

### Constraint vs. Invariant

| Invariant | Constraint |
|-----------|------------|
| Binary: violated or not | Graduated: a spectrum with a threshold |
| "Email addresses are unique" | "API responds in < 200ms" |
| A broken invariant means the system is *wrong* | A missed constraint means the system is *degraded* |
| Enforced by code | Verified by testing |

### Constraint vs. Behavior

| Behavior | Constraint |
|----------|------------|
| Describes what the system *does* | Describes *how well* it does it |
| "When a user submits a form, the system validates and saves" | "The form submission completes in < 500ms" |
| Scoped to a single operation | Cross-cuts multiple behaviors |

## Related Entities

| Direction | Entity | Edge | Meaning |
|-----------|--------|------|---------|
| outgoing | [behavior](behavior.md) | `ConstraintConstrainsBehavior` | Behaviors this constraint constrains |
| outgoing | [behavior](behavior.md) | `ConstraintEnforcedByBehavior` | Behaviors this constraint is enforced by |
| outgoing | [feature](feature.md) | `ConstraintGovernsFeature` | Features this constraint governs |
| outgoing | [invariant](invariant.md) | `ConstraintProtectsInvariant` | Invariants this constraint protects |
| incoming | [decision](decision.md) | `DecisionImposesConstraint` | Decisions that impose this constraint |

## Examples

### Performance

```spec
constraint api_latency "API Latency Under Load" {
  category    performance
  priority    must

  metric """
    response_time_p99 < 200ms
    at 1000 concurrent users
    sustained for 5 minutes
  """

  constrains  [create_user, read_user, update_email]

  verify load "k6 load test with 1000 VUs, assert p99 < 200ms"
}
```

### Security

```spec
constraint pii_encryption_constraint "PII Encryption at Rest" {
  category    security
  priority    must

  metric "All PII fields encrypted with AES-256 in the database."

  protects    [pii_encryption]
  constrains  [create_user, update_email]
  refs        [gh.issue:55]

  verify audit "Database column inspection confirms encryption"
}
```

### Compatibility

```spec
constraint runtime_compat "Runtime Compatibility" {
  category    compatibility
  priority    must

  metric "The CLI runs on Node.js 18+, Bun 1.0+, and Deno 2.0+."

  verify integration "CI matrix tests across all supported runtimes"
}
```

### Legal

```spec
constraint gdpr_residency "GDPR Data Residency" {
  category    legal
  priority    must

  metric "All EU user data stored in EU-West-1 region. No cross-region replication of PII."

  constrains  [create_user, update_email]
  protects    [data_residency]

  verify audit "Infrastructure audit confirms data residency"
}
```
