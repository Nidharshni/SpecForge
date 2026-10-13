---
name: specforge-milestones-dsl
description: "Write milestone blocks in .spec DSL files (@specforge/product plugin). Each milestone declares a planning phase with free-form snake_case IDs, status lifecycle, feature scheduling, and exit criteria. Use when defining when things ship and what must be true before a phase is complete."
---

# SpecForge Milestones DSL

Rules and conventions for authoring **`milestone` blocks** in `.spec` files. Milestone phases provide the temporal dimension -- when things ship -- with verifiable exit criteria.

**Requires:** `@specforge/product` plugin.

## When to Use

- Defining planning phases (MVP, v2, etc.) with verifiable exit criteria
- Scheduling features and deliverables into release milestones
- Connecting planning to the compiler-validated traceability chain

## Block Syntax

```spec
use "features/user-management"
use "features/auth"

milestone mvp "Phase 1: Core" {
  status        in_progress
  features      [user_management, password_authentication]
  target_date   "2026-06-30"
  exit_criteria [
    "All exit criteria met",
    "specforge check --strict passes",
  ]
}
```

## Fields Reference

### Required

| Field | Type | Description |
|-------|------|-------------|
| `title` | string | Human-readable name (string after the entity ID). |

### Optional

| Field | Type | Description |
|-------|------|-------------|
| `status` | MilestoneStatus | Phase status: `planned`, `in_progress`, `completed`, `blocked`. Absent: `planned`. Another value is E061. |
| `features` | reference list | Features scheduled for this phase. |
| `modules` | reference list | Module dependencies for this phase. |
| `depends_on` | reference list | Other milestones that must complete before this phase. Forms a DAG validated by E015 (cycle detection). |
| `exit_criteria` | string list | Exit criteria for phase completion. Completed milestones without criteria emit W057. |
| `target_date` | string @optional | Target completion date in ISO 8601 format (`YYYY-MM-DD`). Validated by I053. |
| `priority` | Priority @optional | Importance level: `critical`, `high`, `medium`, `low`. |
| `reason` | string @optional | Rationale for current status (expected when `blocked`, checked by I060). |
| `tags` | string[] @optional | Faceted filtering tags. |
| `refs` | reference list | External references linked to this milestone phase. |

**Note:** If `@specforge/software` is installed, milestones gain a `behaviors` field (reference list -> behavior) via entity_enhancement, with `MilestoneBehavior` edges.

### Status Values

| Value | Meaning |
|-------|---------|
| `planned` | Not yet started |
| `in_progress` | Actively being worked on |
| `completed` | All features done, exit criteria met |
| `blocked` | Cannot proceed (should include `reason` and `depends_on`) |

## Relationships

### Outgoing edges

| To | Edge Type | Meaning |
|----|-----------|---------|
| `feature` | `MilestoneFeature` | Phase schedules these features |
| `module` | `MilestoneModule` | Phase includes these modules |
| `milestone` | `MilestoneDependsOn` | Phase depends on these phases completing first |
| `ref` | `links_to` | External references linked to this phase |

### Incoming edges

| From | Edge Type | Meaning |
|------|-----------|---------|
| `milestone` | `MilestoneDependsOn` | Another phase depends on this phase |
| `deliverable` | `DeliverableMilestone` | A deliverable tracks against this milestone |

## Writing Rules

1. **Verifiable criteria** -- "specforge check --strict passes", "coverage >= 90%", not vague goals.
2. **Progressive phases** -- each phase builds on previous ones.
3. **Import feature files** -- `use` the files that declare referenced features.

## Validation Rules

| Code | Level | Rule |
|------|-------|------|
| E015 | error | Circular milestone dependency -- `depends_on` edges between milestones form a cycle. |
| W049 | warning | Empty milestone -- no features AND no modules. |
| W057 | warning | Completed milestone without exit criteria. |
| E061 | error | A value outside the field's vocabulary (`status`, `priority`). |
| I051 | info | Milestone features not reachable from milestone modules (gap). |
| I053 | info | Invalid `target_date` format (not ISO 8601 `YYYY-MM-DD`). |
| I057 | info | Blocked milestone without a `depends_on` reference. |
| I060 | info | Blocked milestone without a `reason`. |
| I064 | info | Milestone temporal inconsistency (depends on a later-dated milestone). |
| I075 | info | Exit criterion not anchored to a graph entity (prose-only). |

## Examples

### Active Phase

```spec
milestone mvp "MVP" {
  status        in_progress
  depends_on    [foundation]
  features      [user_management, password_authentication, basic_search]
  priority      high

  exit_criteria [
    "All MVP features verified",
    "Coverage >= 90%",
    "Zero open E-level diagnostics",
  ]

  refs [jira.epic:PROJ-001]
}
```

### Planned Phase

```spec
milestone search_analytics "Search & Analytics" {
  status        planned
  depends_on    [mvp]
  features      [full_text_search, analytics_dashboard]
  target_date   "2026-09-30"

  exit_criteria [
    "Full-text search returns results in < 200ms",
    "Analytics dashboard renders within 3 seconds",
  ]
}
```

### Completed Phase

```spec
milestone foundation "Foundation" {
  status        completed
  features      [project_scaffold]

  exit_criteria [
    "Project scaffold complete",
    "CI/CD pipeline operational",
  ]
}
```

### Blocked Phase

```spec
milestone external_integration "External Integration" {
  status        blocked
  depends_on    [mvp]
  features      [third_party_sync]
  reason        "Waiting on partner API access credentials"
}
```

## What NOT to Do

- Do not write milestones without the `@specforge/product` plugin installed
- Do not set vague criteria like "system is ready" -- use measurable, automatable checks
- Do not reference features from other files without a `use` import
- Do not use status values outside the enum (`planned`, `in_progress`, `completed`, `blocked`)
