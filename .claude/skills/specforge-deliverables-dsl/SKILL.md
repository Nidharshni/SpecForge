---
name: specforge-deliverables-dsl
description: "Write deliverable blocks in .spec DSL files (@specforge/product plugin). Each deliverable declares a shippable artifact with free-form snake_case IDs, mapping journeys to modules with artifact type and persona targeting. Use when describing what actually ships to users -- apps, services, CLIs, extensions."
---

# SpecForge Deliverables DSL

Rules and conventions for authoring **`deliverable` blocks** in `.spec` files. Deliverables represent what actually ships -- they bridge the UX layer (journeys) to the code layer (modules).

**Requires:** `@specforge/product` plugin.

## When to Use

- Defining what ships to users (apps, services, CLIs, extensions)
- Mapping journeys to module dependencies
- Planning release artifacts with persona targeting
- Completing the top of the traceability chain: deliverable -> journey -> feature -> behavior -> invariant

## Block Syntax

```spec
use "journeys/admin-users"
use "journeys/developer-api"
use "modules/core"
use "modules/email"
use "product/milestones"

deliverable user_management_mvp "User Management MVP" {
  artifact_type cli
  status        in_progress
  journeys      [create_new_user, create_user_via_api]
  modules       [core_lib, email_lib]
  milestones    [mvp]
  version       "0.1.0"
}
```

## Fields Reference

### Required

| Field | Type | Description |
|-------|------|-------------|
| `title` | string | Human-readable name (string after the entity ID). |
| `journeys` | reference list | UX journeys this deliverable ships. Omission emits W043. |

### Optional

| Field | Type | Description |
|-------|------|-------------|
| `artifact_type` | ArtifactType @optional | Artifact kind: `cli`, `service`, `library`, `web_app`, `mobile_app`, `api`, `extension`, `documentation`, `package`. Another value is E061. |
| `status` | DeliverableStatus @optional | Lifecycle state: `draft`, `in_progress`, `shipped`, `deprecated`. Absent: `draft`. Another value is E061. |
| `modules` | reference list | Libraries this deliverable is built from. Omission emits W046. |
| `milestones` | EntityId[] @optional | Milestones this deliverable is tracked against. Creates `DeliverableMilestone` edges. |
| `depends_on` | EntityId[] @optional | Other deliverables this one depends on. Creates `DeliverableDependsOn` edges. Cycles detected by E016. |
| `version` | string @optional | Semantic Versioning 2.0.0 string (e.g., `1.0.0`). Format validated by I061. |
| `reason` | string @optional | Rationale for current status (required context for `deprecated` status, checked by I066). |
| `personas` | identifier list | Personas this deliverable targets. |
| `tags` | string[] @optional | Faceted filtering tags. |
| `refs` | reference list | External references linked to this deliverable. |

## Relationships

### Outgoing edges

| To | Edge Type | Meaning |
|----|-----------|---------|
| `journey` | `DeliverableJourney` | Deliverable ships these journeys |
| `module` | `DeliverableModule` | Deliverable uses these modules |
| `milestone` | `DeliverableMilestone` | Deliverable tracked against these milestones |
| `deliverable` | `DeliverableDependsOn` | Deliverable depends on that deliverable |
| `ref` | `links_to` | External references linked to this deliverable |

### Incoming edges

| From | Edge Type | Meaning |
|------|-----------|---------|
| `deliverable` | `DeliverableDependsOn` | Another deliverable depends on this one |

## Writing Rules

1. **Shippable artifacts only** -- deliverables are things you can deploy, publish, or release.
2. **Bundle journeys, not features** -- journeys are the unit of user value in deliverables.
3. **List module dependencies** -- makes the code-to-product mapping explicit.
4. **Coverage validation** -- every journey's features should be reachable through the module chain (I049).
5. **Import journey and module files** -- `use` the files that declare referenced entities.
6. **One deliverable per artifact** -- a web app and a CLI are separate deliverables.

## Validation Rules

| Code | Level | Rule |
|------|-------|------|
| E016 | error | Circular deliverable dependency -- `depends_on` edges form a cycle. |
| W043 | warning | Deliverable with no journeys. |
| W046 | warning | Deliverable with no modules. |
| E061 | error | A value outside the field's vocabulary (`artifact_type`, `status`). |
| I049 | info | Journey features not a subset of module features (traceability gap). |
| I061 | info | Invalid `version` format (not Semantic Versioning 2.0.0). |
| I065 | info | Shipped deliverable has incomplete milestones. |
| I066 | info | Deprecated deliverable without a `reason`. |
| I073 | info | Deliverable references a journey using a deprecated persona. |
| I074 | info | Deliverable references a journey using a deprecated channel. |

## Examples

### Web Application

```spec
deliverable web_dashboard "Web Dashboard" {
  artifact_type web_app
  status        in_progress
  personas      [admin]
  journeys      [create_new_user, create_user_via_api, full_text_search_ui]
  modules       [core_lib, web_lib]
  milestones    [mvp]
  refs          [jira.epic:PROJ-100]
}
```

### CLI Tool

```spec
deliverable cli_tool "CLI Tool" {
  artifact_type cli
  status        shipped
  version       "1.0.0"
  personas      [developer]
  journeys      [manage_specs_cli, validate_project_cli]
  modules       [core_lib, cli_lib]
  milestones    [search_analytics]
}
```

### Minimal

```spec
deliverable rest_api_service "REST API Service" {
  artifact_type api
  journeys      [purchase_and_track_order]
}
```

## What NOT to Do

- Do not write deliverables without the `@specforge/product` plugin installed
- Do not confuse deliverables (what ships) with modules (what code implements)
- Do not bundle features directly -- bundle journeys, which trace to features
- Do not omit `modules` when you want coverage validation (I049)
- Do not reference journeys or modules from other files without `use` imports
