# milestone

> **Module:** `@specforge/product`

## Purpose

A `milestone` declares a **planning phase** that schedules features and modules with exit criteria. Milestones provide the temporal dimension — when things ship — and connect planning to the compiler-validated traceability chain.

It answers: **"When does this ship and what must be true before it ships?"**

Milestones reference features and modules, making it possible to validate that a milestone is complete by checking if all features are implemented and all exit criteria are met.

## ID Pattern

```
identifier
```

Examples: `mvp_phase`, `search_analytics`, `foundation`

## Syntax

```spec
use "features/user-management"
use "features/auth"

milestone mvp_phase "Phase 1: Core" {
  status        in_progress
  features      [user_management, password_auth]
  exit_criteria [
    "All scheduled features implemented",
    "specforge check --strict passes",
  ]
  target_date   "2026-06-30"
}
```

## Fields

All fields are optional at the type level. The compiler emits warnings and info diagnostics when key fields are absent or inconsistent.

### Optional

| Field | Type | Description |
|-------|------|-------------|
| `title` | string | Human-readable name (the string after the entity ID). |
| `status` | MilestoneStatus | Current phase status: `planned`, `in_progress`, `completed`, `blocked`. Validated by W079. |
| `features` | EntityId[] | Features scheduled for this phase. Creates `MilestoneDeliversFeature` edges. |
| `modules` | EntityId[] | Modules included in this phase. Creates `MilestoneScopesModule` edges. |
| `depends_on` | EntityId[] | Other milestones that must complete before this phase can start. Creates `MilestoneDependsOn` edges. Cycles detected by E015. |
| `exit_criteria` | string[] | Exit criteria that must be satisfied before the phase is considered complete. Completed milestones without criteria emit W057. |
| `target_date` | string | Target completion date in ISO 8601 format (`YYYY-MM-DD`, e.g., `2026-06-30`). Not validated by the compiler. |
| `priority` | Priority | Importance level: `critical`, `high`, `medium`, `low`. |
| `reason` | string | Rationale for current status. A `blocked` milestone without `blockers` emits I060. |
| `tags` | string[] | Free-form tags for categorization. |
| `refs` | reference list | External references (issues, tickets, diagrams) linked to this milestone. |

> **Note:** To schedule individual behaviors within a milestone, install `@specforge/software`. The `MilestoneIncludesBehavior` edge type is provided by that extension via entity_enhancement, not by `@specforge/product`.

## Relationships

### Outgoing edges

| To | Edge Type | Meaning |
|----|-----------|---------|
| `feature` | `MilestoneDeliversFeature` | "This milestone schedules these features" (via `features` field) |
| `milestone` | `MilestoneDependsOn` | "This milestone depends on these milestones" (via `depends_on` field) |
| `behavior` | `MilestoneIncludesBehavior` | "This milestone delivers these behaviors" (via `behaviors` field) |
| `module` | `MilestoneScopesModule` | "This milestone includes these modules" (via `modules` field) |

### Incoming edges

| From | Edge Type | Meaning |
|------|-----------|---------|
| `deliverable` | `DeliverableTrackedByMilestone` | "Deliverables that are tracked by this milestone" (via `milestones` field) |
| `milestone` | `MilestoneDependsOn` | "Milestones that depend on this milestone" (via `depends_on` field) |
| `release` | `ReleaseCompletesMilestone` | "Releases that target this milestone" (via `milestones` field) |

## Validation Rules

| Code | Level | Rule |
|------|-------|------|
| W049 | warning | Milestone with no `features` field (modules don't count). |
| W057 | warning | Completed milestone without exit criteria. |
| E015 | error | Circular milestone dependency — `depends_on` edges form a cycle. |
| W079 | warning | Invalid `status` value (not in MilestoneStatus enum). |
| I053 | info | `target_date` present but not YYYY-MM-DD. |
| I057 | info | Blocked milestone without `depends_on`. |
| I060 | info | Blocked milestone without `blockers`. |
| I080 | info | No `owner`. |
| I087 | info | `start_date` present but not YYYY-MM-DD. |

## Design Guidance

### Phase Granularity

A milestone should be:
- **Time-bounded** — represents a planning period (sprint, milestone, release)
- **Verifiable** — has concrete exit criteria that can be checked
- **Progressive** — builds on previous milestones

### Status Values

| Value | Meaning |
|-------|---------|
| `planned` | Not yet started |
| `in_progress` | Actively being worked on |
| `completed` | All features done, exit criteria met |
| `blocked` | Cannot proceed (should include `reason` and `depends_on`) |

A typical lifecycle:

```
planned -> in_progress -> completed
                |
              blocked (requires reason + depends_on)
```

### Exit Criteria

Exit criteria should be:
- **Measurable** — "specforge check --strict passes", "coverage >= 90%"
- **Specific** — name the behaviors, features, or metrics
- **Automated where possible** — the compiler can verify behavior coverage and test presence
- **Anchored** — reference entity IDs (e.g., "all features in [user_management] done") to enable automated verification. Prose-only criteria are valid, but anchoring them to graph-verifiable properties is stronger

## Related Entities

| Direction | Entity | Edge | Meaning |
|-----------|--------|------|---------|
| outgoing | [feature](feature.md) | `MilestoneDeliversFeature` | Features this milestone schedules |
| outgoing | [milestone](milestone.md) | `MilestoneDependsOn` | Milestones this milestone depends on |
| outgoing | [behavior](behavior.md) | `MilestoneIncludesBehavior` | Behaviors this milestone delivers |
| outgoing | [module](module.md) | `MilestoneScopesModule` | Modules this milestone includes |
| incoming | [deliverable](deliverable.md) | `DeliverableTrackedByMilestone` | Deliverables that are tracked by this milestone |
| incoming | [milestone](milestone.md) | `MilestoneDependsOn` | Milestones that depend on this milestone |
| incoming | [release](release.md) | `ReleaseCompletesMilestone` | Releases that target this milestone |

## Examples

### Active Phase

```spec
use "features/user-management"
use "features/auth"

milestone mvp_phase "MVP" {
  status        in_progress
  depends_on    [foundation]
  features      [user_management, password_auth, user_profile]
  priority      high

  exit_criteria [
    "All scheduled features implemented",
    "Coverage >= 90%",
    "Zero open E-level diagnostics",
  ]

  refs [jira.epic:PROJ-001]
}
```

### Planned Phase

```spec
use "features/search"
use "features/analytics"

milestone search_analytics "Search & Analytics" {
  status        planned
  depends_on    [mvp_phase]
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
    "specforge init passes",
  ]
}
```
