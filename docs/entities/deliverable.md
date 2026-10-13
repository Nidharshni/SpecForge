# deliverable

> **Module:** `@specforge/product`

## Purpose

A `deliverable` declares a **shippable artifact** that maps journeys to modules. Deliverables represent what actually ships to users — apps, services, CLIs, extensions — bridging the UX layer (journeys) to the code layer (modules).

It answers: **"What ships to users?"**

Deliverables complete the top of the traceability chain: `deliverable -> journey -> feature -> behavior -> invariant`. They make release planning explicit by bundling journeys into named artifacts with clear module dependencies.

## ID Pattern

```
identifier
```

Examples: `web_dashboard`, `cli_tool`, `rest_api`

## Syntax

```spec
use "journeys/admin-users"
use "journeys/developer-api"
use "modules/core"
use "modules/email"
use "product/milestones"

deliverable user_mgmt_mvp "User Management MVP" {
  artifact_type cli
  status        in_progress
  journeys      [create_user_web, create_user_api]
  modules       [core_mod, email_mod]
  milestones    [mvp_phase]
  version       "0.1.0"
}
```

## Fields

All fields are optional at the type level.

### Optional

| Field | Type | Description |
|-------|------|-------------|
| `title` | string | Human-readable name (the string after the entity ID). |
| `artifact_type` | ArtifactType | The artifact kind: `cli`, `service`, `library`, `web_app`, `mobile_app`, `api`, `extension`, `documentation`, `package`. Validated by W080. |
| `status` | DeliverableStatus | Lifecycle state: `draft`, `in_progress`, `shipped`, `deprecated`. Validated by W085. |
| `journeys` | EntityId[] | The UX journeys this deliverable ships. Creates `DeliverableSupportsJourney` edges. |
| `modules` | EntityId[] | The modules this deliverable is built from. Creates `DeliverableContainsModule` edges. |
| `milestones` | EntityId[] | Milestones this deliverable is tracked against. Creates `DeliverableTrackedByMilestone` edges. |
| `depends_on` | EntityId[] | Other deliverables this one depends on. Creates `DeliverableDependsOn` edges. Cycles detected by E052. |
| `version` | string | Semantic Versioning 2.0.0 string (e.g., `1.0.0`, `1.0.0-alpha.1`, `1.0.0+build.42`). |
| `reason` | string | Rationale for current status (required context for `deprecated` status, checked by I066). |
| `tags` | string[] | Free-form tags for categorization. |
| `refs` | reference list | External references (issues, tickets, diagrams) linked to this deliverable. |

## Relationships

### Outgoing edges

| To | Edge Type | Meaning |
|----|-----------|---------|
| `module` | `DeliverableContainsModule` | "This deliverable contains these modules" (via `modules` field) |
| `deliverable` | `DeliverableDependsOn` | "This deliverable depends on these deliverables" (via `depends_on` field) |
| `journey` | `DeliverableSupportsJourney` | "This deliverable ships these journeys" (via `journeys` field) |
| `milestone` | `DeliverableTrackedByMilestone` | "This deliverable is tracked by these milestones" (via `milestones` field) |

### Incoming edges

| From | Edge Type | Meaning |
|------|-----------|---------|
| `deliverable` | `DeliverableDependsOn` | "Deliverables that depend on this deliverable" (via `depends_on` field) |
| `release` | `ReleaseIncludesDeliverable` | "Releases that ship this deliverable" (via `deliverables` field) |

## Validation Rules

| Code | Level | Rule |
|------|-------|------|
| E052 | error | Circular deliverable dependency — `depends_on` edges form a cycle. |
| W080 | warning | Invalid `artifact_type` value (not in ArtifactType enum). |
| W085 | warning | Invalid `status` value (not in DeliverableStatus enum). |
| W043 | warning | Deliverable with no journeys (no `journeys`, or an empty list). |
| W046 | warning | Deliverable with no modules (no `modules`, or an empty list). |
| I061 | info | `version` present but not semver. |
| I066 | info | Deprecated deliverable without a `reason`. |
| I080 | info | No `owner`. |

## Queries

| Query | Method | Description |
|-------|--------|-------------|
| Deliverable traceability | `queryDeliverableTraceability` | Transitive features via journey and module paths. |
| Deliverable completion | `queryDeliverableCompletion` | Aggregate milestone completion across referenced milestones. |
| Deliverable dependents | `queryDeliverableDependents` | Other deliverables that depend on this one. |
| Deliverable priority | `queryDeliverablePriority` | Derived priority from constituent milestones and journeys (max-priority). |
| Deliverable personas | `queryDeliverablePersonas` | Deduplicated personas served by this deliverable, traversed via deliverable→journey→persona edges. |

## Design Guidance

### Deliverable Granularity

A deliverable should be:
- **Shippable** — something you can deploy, publish, or release to users
- **Self-contained** — bundles all journeys needed for its use case
- **Persona-scoped** — targets specific user personas

### Status Lifecycle

```
draft -> in_progress -> shipped
                |
            deprecated (requires reason)
```

When `status` is `deprecated`, omitting `reason` emits I066.

### Deliverable vs. Journey

| Deliverable | Journey |
|-------------|---------|
| "Web Dashboard" | "Create a New User" |
| What ships | What a user can do |
| Groups journeys by artifact | Groups features by persona + channel |
| Has module dependencies | Has feature dependencies |

### Deliverable vs. Module

| Deliverable | Module |
|-------------|--------|
| "User Management MVP" / "Housing Policy Framework" | "@myservice/core" / "Advisory Workstream" |
| What users or stakeholders receive | What implementers build |
| Maps journeys to modules | Maps features to dependencies |
| Shippable artifact or outcome | Structural component |

## Related Entities

| Direction | Entity | Edge | Meaning |
|-----------|--------|------|---------|
| outgoing | [module](module.md) | `DeliverableContainsModule` | Modules this deliverable contains |
| outgoing | [deliverable](deliverable.md) | `DeliverableDependsOn` | Deliverables this deliverable depends on |
| outgoing | [journey](journey.md) | `DeliverableSupportsJourney` | Journeys this deliverable ships |
| outgoing | [milestone](milestone.md) | `DeliverableTrackedByMilestone` | Milestones this deliverable is tracked by |
| incoming | [deliverable](deliverable.md) | `DeliverableDependsOn` | Deliverables that depend on this deliverable |
| incoming | [release](release.md) | `ReleaseIncludesDeliverable` | Releases that ship this deliverable |

## Examples

### Web Application

```spec
use "journeys/admin-users"
use "journeys/admin-reports"
use "modules/core"
use "modules/reporting"
use "product/milestones"

deliverable web_dashboard "Web Dashboard" {
  artifact_type web_app
  status        in_progress
  journeys      [create_user_web, create_user_api, export_reports]
  modules       [core_mod, reporting_mod]
  milestones    [mvp_phase]
  refs          [jira.epic:PROJ-100]
}
```

### CLI Tool

```spec
use "journeys/developer-api"
use "modules/core"
use "modules/cli-utils"

deliverable cli_tool "CLI Tool" {
  artifact_type cli
  status        shipped
  version       "1.0.0"
  journeys      [search_records_cli, lint_specs_cli]
  modules       [core_mod, cli_utils_mod]
}
```

### Non-Software: Policy Framework

```spec
use "journeys/policy-review"
use "modules/advisory"

deliverable housing_policy "Housing Policy Framework" {
  artifact_type documentation
  status        draft
  journeys      [review_policy_draft, submit_public_comment]
  modules       [advisory_workstream]
}
```

### Minimal Deliverable

```spec
use "journeys/api-integration"

deliverable rest_api "REST API Service" {
  artifact_type api
  journeys [api_integration]
}
```
