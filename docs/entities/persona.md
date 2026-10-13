# persona

> **Module:** `@specforge/product`

## Purpose

A `persona` declares a **user role** — a first-class entity modeling who interacts with the system. Personas are graph-addressable nodes that journeys reference to describe the type of user performing an interaction flow.

It answers: **"Who uses this?"**

While journeys describe *how* users interact and features describe *what* the system delivers, personas describe *who* the users are. Declaring personas as entities (rather than bare identifiers or config enums) enables unreferenced detection, cross-reference validation, and graph queries across the product model.

## ID Pattern

```
identifier
```

Examples: `admin`, `api_consumer`, `patient`, `landlord`

## Syntax

```spec
persona admin "System Administrator" {
  description """
    A user with full system access responsible for managing
    user accounts, configuring system settings, and monitoring
    operational health.
  """
  technical_level expert
  goals ["manage user accounts", "configure system settings", "monitor operations"]
  tags ["internal"]
}
```

## Fields

### Required

| Field | Type | Description |
|-------|------|-------------|
| `description` | string or triple-string | Who this persona is and what role they play in the system. |

### Optional

| Field | Type | Description |
|-------|------|-------------|
| `title` | string | Human-readable name (the string after the entity ID). |
| `technical_level` | TechnicalLevel enum | Technical proficiency: `expert`, `advanced`, `intermediate`, `beginner`, `non_technical`. |
| `goals` | string list | What this persona wants to achieve with the system. |
| `pain_points` | string list | Frustrations or obstacles this persona faces. |
| `status` | PersonaStatus enum | Lifecycle state: `active`, `deprecated`. Validated by W083. Absent status treated as `active` for incremental adoption. |
| `reason` | string | Rationale for deprecation (expected when `status=deprecated`, checked by I069). |
| `tags` | string list | Free-form tags for categorization. |
| `refs` | reference list | External references (issues, tickets, diagrams) linked to this persona. |

## Relationships

### Outgoing edges

| To | Edge Type | Meaning |
|----|-----------|---------|
| `feature` | `PersonaPrioritizesFeature` | "This persona prioritizes these features" (via `key_features` field) |

### Incoming edges

| From | Edge Type | Meaning |
|------|-----------|---------|
| `journey` | `JourneyTargetsPersona` | "Journeys that are performed by this persona" (via `persona` field) |

## Validation Rules

| Code | Level | Rule |
|------|-------|------|
| E003 | error | Journey `persona` field must resolve to a declared entity (E022 when the target isn't a persona). |
| W083 | warning | Invalid `status` value (not in PersonaStatus enum). |
| I046 | info | Persona not referenced by any journey (unreferenced persona). |
| I069 | info | Deprecated persona without a `reason`. |

## Design Guidance

### When to Declare a Persona

Declare a persona when:
- A distinct type of user interacts with the system through journeys
- The role has different permissions, goals, or technical proficiency
- Multiple journeys share the same user type

### Granularity

Personas represent **roles**, not individuals:
- One persona per distinct role (e.g., `admin`, `viewer`, `operator`)
- Do NOT create per-person personas (e.g., `alice`, `bob`)
- If two roles have the same permissions and goals, they are one persona

### Technical Level

Use `technical_level` to communicate UI/UX expectations:

| Level | Meaning |
|-------|---------|
| `expert` | Power user, comfortable with CLI, APIs, advanced features |
| `advanced` | Technically proficient, prefers efficient workflows |
| `intermediate` | Familiar with the domain, needs clear guidance |
| `beginner` | New to the system, needs onboarding and tooltips |
| `non_technical` | No technical background, needs fully guided experience |

### Persona vs. Journey

| Persona | Journey |
|---------|---------|
| "System Administrator" | "Create a New User" |
| Describes who | Describes how they interact |
| One persona, many journeys | One journey, one persona |
| Reusable across journeys | References a persona |

## Related Entities

| Direction | Entity | Edge | Meaning |
|-----------|--------|------|---------|
| outgoing | [feature](feature.md) | `PersonaPrioritizesFeature` | Features this persona prioritizes |
| incoming | [journey](journey.md) | `JourneyTargetsPersona` | Journeys that are performed by this persona |

## Examples

### Minimal Persona

```spec
persona viewer "Read-Only Viewer" {
  description "A user with read-only access to reports and dashboards."
}
```

### Full Persona

```spec
persona admin "System Administrator" {
  description """
    A user with full system access responsible for managing
    user accounts, configuring system settings, and monitoring
    operational health. Typically part of the IT or DevOps team.
  """
  technical_level expert
  goals [
    "manage user accounts and permissions",
    "configure system settings",
    "monitor operational health",
    "respond to incidents",
  ]
  tags ["internal", "privileged"]
}
```

### Deprecated Persona

```spec
persona legacy_admin "Legacy Admin" {
  description "Former admin role before RBAC migration."
  technical_level expert
  status deprecated
  reason "Replaced by role-based personas (admin, operator) in v2"
}
```

### Non-Software Persona

```spec
persona patient "Healthcare Patient" {
  description """
    A person receiving healthcare services. Interacts with the
    system to schedule appointments, view medical records, and
    communicate with care providers.
  """
  technical_level beginner
  goals [
    "schedule appointments",
    "view medical records",
    "communicate with care team",
  ]
}

persona landlord "Property Owner" {
  description """
    A property owner who manages rental units, reviews tenant
    applications, and tracks maintenance requests.
  """
  technical_level intermediate
  goals [
    "list rental properties",
    "review tenant applications",
    "track maintenance requests",
  ]
}
```
