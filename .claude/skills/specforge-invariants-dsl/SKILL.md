---
name: specforge-invariants-dsl
description: "Write invariant blocks in .spec DSL files. Each invariant declares a runtime guarantee with free-form snake_case IDs, a guarantee statement using RFC 2119 keywords, a risk level (low/medium/high/critical), verify statements (property/unit/mutation), and optional maintains blocks for formal properties. Use when defining what must ALWAYS be true in the system."
---

# SpecForge Invariants DSL

Rules and conventions for authoring **`invariant` blocks** in `.spec` files. Invariants are the foundation of the traceability chain -- they declare non-negotiable runtime guarantees.

**Requires:** `@specforge/software` plugin.

## When to Use

- Defining a runtime guarantee the system must never violate
- Naming a guarantee that behaviors enforce (they list it in their `invariants`)
- Establishing risk levels for failure mode analysis
- Adding formal properties via `maintains` blocks
- Creating the invariants that behaviors will reference

## Block Syntax

```spec
invariant data_persistence "Data Persistence" {
  guarantee """
    All committed writes survive process restarts.
    No acknowledged write may be silently dropped.
  """
  risk high

  maintains {
    write_durability   "every ACKed write is fsync'd before response"
    crash_recovery     "WAL replay restores all committed state"
  }

  verify property "committed writes survive simulated crash"
  verify unit     "WAL replay restores all entries"
}
```

## Fields Reference

### Required

| Field | Type | Description |
|-------|------|-------------|
| `title` | string | Human-readable name (string after the entity ID). |
| `guarantee` | string / triple-string | Precise statement of what the system guarantees. Uses RFC 2119 keywords. |

### Optional

| Field | Type | Description |
|-------|------|-------------|
| `risk` | enum | `low`, `medium`, `high`, or `critical`. Severity if violated. |
| `maintains` | block | Frame invariants -- formal properties that must hold before AND after any operation. |
| `verify` | verify statement(s) | Test specifications: `verify {kind} "{description}"`. Kinds: property, unit, mutation. |
| `refs` | reference list | External references linked to this invariant. |

### Verify Kinds for Invariants

| Kind | Meaning |
|------|---------|
| `property` | Property-based testing to verify guarantee holds |
| `unit` | Unit test verifying enforcement mechanism |
| `mutation` | Mutation testing to verify detection of violations |

## Relationships

### Incoming edges (entities that reference this invariant)

An invariant lists no behaviors: the behaviors that enforce it name it in their `invariants` field, and `trace <invariant>` lists them upstream.

| From | Edge Type | Meaning |
|------|-----------|---------|
| `behavior` | `BehaviorEnforcesInvariant` | Behavior enforces this invariant (via the behavior's `invariants` field) |
| `decision` | `protects` | Decision protects this invariant |
| `failure_mode` | `mitigates` | Failure mode threatens this invariant |
| `constraint` | `constrains` | Quality requirement protects this invariant |

### Outgoing edges

| To | Edge Type | Meaning |
|----|-----------|---------|
| `ref` | `links_to` | External references linked to this invariant |

## Writing Rules

1. **One guarantee per invariant** -- do not combine unrelated guarantees in a single block.
2. **Falsifiable** -- you must be able to write a test that detects a violation.
3. **Universal** -- the guarantee holds under all conditions, not just the happy path.
4. **Implementation-independent** -- describe what is true, not how it is achieved.
5. **Use RFC 2119 keywords** -- MUST, MUST NOT, SHALL in the guarantee text.
6. **Write the relation on the behavior** -- a behavior lists the invariants it enforces in `invariants`; an invariant never lists behaviors (E079).
7. **Every high-risk invariant needs a failure_mode** -- or `W005` warning fires.
8. **Add `maintains` for formal properties** -- enables automated consistency checking.
9. **Add `verify` statements** -- invariants are testable (property, unit, mutation).

## Validation Rules

| Code | Rule |
|------|------|
| E002 | No duplicate invariant IDs across all `.spec` files. |
| W003 | Unused invariant -- not referenced by any behavior. |
| W005 | Unmitigated high-risk invariant -- `risk: high` or `risk: critical` with no `failure_mode`. |
| W040 | Invariant without formal property -- has prose guarantee but no `maintains` block. |

## Examples

### Simple

```spec
invariant email_uniqueness "Email Uniqueness" {
  guarantee "No two active users share the same email address."
  risk medium

  verify property "concurrent user creation preserves email uniqueness"
}
```

### With Formal Properties

```spec
invariant audit_trail_integrity "Audit Trail Integrity" {
  guarantee """
    The audit log is append-only.
    No audit entry may be modified or deleted after creation.
    Every state-changing operation MUST produce an audit entry
    before returning success to the caller.
  """
  risk critical

  maintains {
    append_only        "no UPDATE or DELETE operations on audit table"
    complete_coverage  "every state-changing behavior produces an audit entry"
    ordering_preserved "audit entries ordered by wall-clock time"
  }

  verify property "audit log is append-only under concurrent writes"
  verify unit     "state-changing operation produces audit entry"
  verify mutation "tampering with audit entry is detected"

  refs [gh.issue:15, jira.epic:PROJ-200]
}
```

### Low-Risk

```spec
invariant display_name_length "Display Name Length" {
  guarantee "User display names MUST be between 1 and 100 characters."
  risk low

  verify unit "names within bounds accepted"
  verify unit "names outside bounds rejected"
}
```

## What NOT to Do

- Do not use an invariant for something that is a behavior ("Users can log in" is a behavior)
- Do not use an invariant for a technology choice ("We use PostgreSQL" is a decision)
- Do not use an invariant for a performance target ("The system is fast" is a constraint)
- Do not leave a high-risk invariant without a behavior that lists it in `invariants` -- name the enforcement mechanism
- Do not skip the `risk` field when a failure_mode will reference this invariant
