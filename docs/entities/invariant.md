# invariant

> **Module:** `core`

## Purpose

An `invariant` declares a **runtime guarantee** — a property the system must never violate, regardless of input, load, or failure conditions. Invariants are the foundation of the traceability chain: behaviors reference them, decisions protect them, failure modes threaten them, and constraints scope them.

It answers: **"What must ALWAYS be true?"**

Invariants are not features or behaviors. They are the non-negotiable truths of the system. When an invariant is violated, the system is broken — not degraded, not slow, but fundamentally wrong.

## ID Pattern

```
identifier
```

Examples: `data_persistence`, `email_uniqueness`, `audit_integrity`

## Syntax

```spec
invariant data_persistence "Data Persistence" {
  guarantee """
    All committed writes survive process restarts.
    No acknowledged write may be silently dropped.
  """
  enforced_by [persist_committed_writes, replay_write_ahead_log]
  risk high
}
```

## Fields

### Required

| Field | Type | Description |
|-------|------|-------------|
| `guarantee` | string or triple-string | The precise statement of what the system guarantees. Uses RFC 2119 keywords (MUST, MUST NOT, SHALL). |

### Optional

| Field | Type | Description |
|-------|------|-------------|
| `title` | string | Human-readable name (the string after the identifier). Optional — auto-derived from identifier if omitted. |
| `enforced_by` | reference list | Behavior entity IDs that enforce this invariant. |
| `risk` | enum | `high`, `medium`, or `low`. Indicates the severity of consequences if this invariant is violated. |
| `refs` | reference list | External references (issues, tickets, diagrams) linked to this invariant. |

## Relationships

### Incoming edges

| From | Edge Type | Meaning |
|------|-----------|---------|
| `axiom` | `AxiomAssumesInvariant` | "Axioms that assume this invariant" (via `assumes` field) |
| `behavior` | `BehaviorEnforcesInvariant` | "Behaviors that enforce this invariant" (via `invariants` field) |
| `constraint` | `ConstraintProtectsInvariant` | "Constraints that protect this invariant" (via `protects` field) |
| `decision` | `DecisionProtectsInvariant` | "Decisions that protect this invariant" (via `invariants` field) |
| `failure_mode` | `FailureModeTargetsInvariant` | "Failure modes that target this invariant" (via `invariant` field) |
| `property` | `PropertyDependsOnInvariant` | "Properties that depend on this invariant" |

### Outgoing edges

None. No extension declares an outgoing edge type for `invariant`.

## Validation Rules

| Code | Rule |
|------|------|
| E001 | Every ID in `enforced_by` must resolve to a declared behavior entity. |
| E002 | No two invariants may share the same ID across all `.spec` files. |
| W003 | If no behavior references this invariant, emit "unused invariant" warning. |
| W005 | If `risk: high` and no `failure_mode` references this invariant, emit "unmitigated high-risk invariant" warning. |

## Design Guidance

### Good Invariants

Invariants should be:
- **Falsifiable** — you can write a test that would detect a violation
- **Universal** — they hold under all conditions, not just the happy path
- **Implementation-independent** — they describe what is true, not how it's achieved

### Examples of Good Invariants

- "No two active users share the same email address" (falsifiable, universal)
- "All committed writes survive process restarts" (falsifiable, universal)
- "The audit log is append-only — no entry may be modified or deleted" (falsifiable, universal)

### Bad Invariants (anti-patterns)

- "The system is fast" — not measurable, use a `constraint` instead
- "Users can log in" — this is a behavior, not an invariant
- "We use PostgreSQL" — this is a decision, not an invariant

## Related Entities

| Direction | Entity | Edge | Meaning |
|-----------|--------|------|---------|
| incoming | `axiom` | `AxiomAssumesInvariant` | Axioms that assume this invariant |
| incoming | [behavior](behavior.md) | `BehaviorEnforcesInvariant` | Behaviors that enforce this invariant |
| incoming | [constraint](constraint.md) | `ConstraintProtectsInvariant` | Constraints that protect this invariant |
| incoming | [decision](decision.md) | `DecisionProtectsInvariant` | Decisions that protect this invariant |
| incoming | [failure_mode](failure-mode.md) | `FailureModeTargetsInvariant` | Failure modes that target this invariant |
| incoming | `property` | `PropertyDependsOnInvariant` | Properties that depend on this invariant |

## Examples

### Simple

```spec
invariant email_uniqueness {
  guarantee "No two active users share the same email address."
  enforced_by [enforce_unique_email, create_user]
  risk medium
}
```

### Detailed

```spec
invariant audit_integrity "Audit Trail Integrity" {
  guarantee """
    The audit log is append-only.
    No audit entry may be modified or deleted after creation.
    Every state-changing operation MUST produce an audit entry
    before returning success to the caller.
  """
  enforced_by [append_audit_entry, intercept_state_changes]
  risk high
  refs [gh.issue:15, jira.epic:PROJ-200]
}
```

### Low-Risk

```spec
invariant display_name_length "Display Name Length" {
  guarantee "User display names MUST be between 1 and 100 characters."
  enforced_by [validate_display_name]
  risk low
}
```
