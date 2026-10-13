# failure_mode

> **Module:** `@specforge/governance`

## Purpose

A `failure_mode` declares an **FMEA (Failure Mode and Effects Analysis) entry** — a structured risk assessment tied to a specific invariant. It describes what can go wrong, how severe the consequences are, how likely it is to occur, and how detectable it is.

It answers: **"What can go wrong and how bad is it?"**

Failure modes close the loop between invariants and risk management. An invariant says "this must always be true"; a failure mode says "here's how it could become false, and here's what we do about it."

## ID Pattern

```
identifier
```

Examples: `write_loss`, `email_race`, `audit_corruption`

## Syntax

```spec
use "invariants/data"

failure_mode write_loss "Write Acknowledged but Lost" {
  invariant  data_persistence
  severity   8
  occurrence 2
  detection  3
  rpn        48

  cause      "Crash between ACK and fsync"
  effect     "Silent data loss — user believes write succeeded"
  mitigation "Write-ahead log with fsync before ACK"

  post_mitigation {
    severity   8
    occurrence 1
    detection  2
    rpn        16
  }
}
```

## Fields

### Required

| Field | Type | Description |
|-------|------|-------------|
| `invariant` | reference | The invariant this failure mode threatens. Must reference an existing `invariant`. |
| `severity` | number | Impact if the failure occurs (1-10 scale). 10 = catastrophic. |
| `occurrence` | number | Likelihood of the failure occurring (1-10 scale). 10 = near-certain. |
| `detection` | number | Ability to detect the failure before it impacts users (1-10 scale). 10 = undetectable. |

### Optional

| Field | Type | Description |
|-------|------|-------------|
| `title` | string | Human-readable name (the string after the entity ID). |
| `rpn` | number | Risk Priority Number = severity x occurrence x detection. Auto-computed if omitted; validated if provided. |
| `cause` | string | Root cause or trigger of the failure. |
| `effect` | string | Impact on the user or system when the failure occurs. |
| `mitigation` | string | Actions taken to reduce the risk. |
| `post_mitigation` | block | Reassessed scores after mitigation is applied. |
| `refs` | reference list | External references (issues, tickets, diagrams) linked to this failure mode. |

### Post-Mitigation Sub-Block

| Field | Type | Description |
|-------|------|-------------|
| `severity` | number | Reassessed severity after mitigation (1-10). |
| `occurrence` | number | Reassessed occurrence after mitigation (1-10). |
| `detection` | number | Reassessed detection after mitigation (1-10). |
| `rpn` | number | Reassessed RPN. Auto-computed if omitted; validated if provided. |

## FMEA Scoring Guide

### Severity (S)

| Score | Impact |
|-------|--------|
| 1-2 | Negligible — minor inconvenience, no data impact |
| 3-4 | Low — degraded experience, workaround available |
| 5-6 | Moderate — feature unavailable, manual recovery needed |
| 7-8 | High — data loss or corruption, significant business impact |
| 9-10 | Critical — safety risk, regulatory violation, catastrophic data loss |

### Occurrence (O)

| Score | Likelihood |
|-------|-----------|
| 1-2 | Rare — once per year or less |
| 3-4 | Uncommon — a few times per year |
| 5-6 | Occasional — monthly |
| 7-8 | Frequent — weekly |
| 9-10 | Near-certain — daily or continuous |

### Detection (D)

| Score | Detectability |
|-------|--------------|
| 1-2 | Immediate — automated monitoring catches it instantly |
| 3-4 | Quick — detected within minutes by alerts |
| 5-6 | Moderate — detected within hours by regular checks |
| 7-8 | Slow — detected only by user reports or audits |
| 9-10 | Undetectable — silent failure, no indication |

### RPN Thresholds

| RPN Range | Risk Level | Action |
|-----------|-----------|--------|
| 1-50 | Low | Accept, document, monitor |
| 51-100 | Medium | Mitigate, add monitoring |
| 101-200 | High | Mitigate urgently, add redundancy |
| 201+ | Critical | Block release until mitigated |

## Relationships

### Outgoing edges

| To | Edge Type | Meaning |
|----|-----------|---------|
| `behavior` | `FailureModeAffectsBehavior` | "This failure mode affects these behaviors" (via `affected_behaviors` field) |
| `invariant` | `FailureModeTargetsInvariant` | "This failure mode targets these invariants" (via `invariant` field) |
| `feature` | `FailureModeThreatensFeature` | "This failure mode threatens these features" (via `threatens_features` field) |

### No incoming edges

Failure modes are leaf nodes in the traceability chain. They are referenced by invariants (via the reverse lookup) but nothing explicitly points *to* a failure mode.

## Validation Rules

| Code | Rule |
|------|------|
| E003 | The `invariant` reference must resolve to a declared entity. |
| E002 | No two failure modes may share the same ID. |
| W051 | `severity` must be a recognized value. |
| W052 | `occurrence` must be a recognized value. |
| W121 | `detection` must be a recognized value. |

The compiler doesn't check that a declared `rpn` equals `severity * occurrence * detection`.

## Design Guidance

### When to Write Failure Modes

Write a failure mode for:
- Every invariant with `risk: high`
- Any invariant where the failure consequences affect data integrity or user safety
- Known operational risks discovered through experience or incident reviews

### Cause-Effect-Mitigation Pattern

The three narrative fields tell a complete story:
1. **Cause** — What triggers the failure? (technical root cause)
2. **Effect** — What does the user or system experience? (impact)
3. **Mitigation** — What do we do about it? (technical countermeasure)

### Post-Mitigation Assessment

Always reassess after mitigation. Good mitigations should:
- Reduce occurrence (prevent the failure from happening)
- Reduce detection score (make the failure more visible if it happens)
- Severity rarely changes (the impact of the failure is inherent)

## Related Entities

| Direction | Entity | Edge | Meaning |
|-----------|--------|------|---------|
| outgoing | [behavior](behavior.md) | `FailureModeAffectsBehavior` | Behaviors this failure mode affects |
| outgoing | [invariant](invariant.md) | `FailureModeTargetsInvariant` | Invariants this failure mode targets |
| outgoing | [feature](feature.md) | `FailureModeThreatensFeature` | Features this failure mode threatens |

No incoming edges. Failure modes are leaf nodes in the traceability chain.

## Examples

### Data Integrity

```spec
failure_mode write_loss "Write Acknowledged but Lost" {
  invariant  data_persistence
  severity   8
  occurrence 2
  detection  3
  rpn        48

  cause      "Crash between ACK and fsync"
  effect     "Silent data loss — user believes write succeeded"
  mitigation "Write-ahead log with fsync before ACK"

  post_mitigation {
    severity   8
    occurrence 1
    detection  2
    rpn        16
  }
}
```

### Security

```spec
failure_mode email_race "Email Uniqueness Bypass via Race Condition" {
  invariant  email_uniqueness
  severity   6
  occurrence 3
  detection  4
  rpn        72

  cause      "Two concurrent requests create users with the same email before the unique constraint check"
  effect     "Two active users share an email — login ambiguity, potential account takeover"
  mitigation "Database-level UNIQUE constraint + serializable isolation for user creation"
  refs       [gh.issue:31]

  post_mitigation {
    severity   6
    occurrence 1
    detection  1
    rpn        6
  }
}
```

### Operational

```spec
failure_mode audit_corruption "Audit Log Corruption" {
  invariant  audit_integrity
  severity   9
  occurrence 1
  detection  5
  rpn        45

  cause      "Disk corruption or software bug overwrites existing audit entries"
  effect     "Regulatory non-compliance — audit trail integrity compromised"
  mitigation "Append-only table with no UPDATE/DELETE grants, WAL archiving, daily integrity checksums"

  post_mitigation {
    severity   9
    occurrence 1
    detection  2
    rpn        18
  }
}
```
