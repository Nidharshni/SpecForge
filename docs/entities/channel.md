# channel

> **Module:** `@specforge/product`

## Purpose

A `channel` declares an **interaction medium** — a first-class entity modeling how users interact with the system. Channels are graph-addressable nodes that journeys reference to describe the medium through which an interaction flow occurs.

It answers: **"Where does the interaction happen?"**

While journeys describe *how* users interact and personas describe *who* they are, channels describe *through what medium* the interaction occurs. Declaring channels as entities (rather than bare identifiers or config enums) enables unreferenced detection, interaction model validation, and graph queries across the product model.

## ID Pattern

```
identifier
```

Examples: `web`, `cli`, `api`, `storefront`, `mobile`

## Syntax

```spec
channel web "Web Application" {
  description "Browser-based web application accessed via HTTPS."
  interaction_model request_response
  tags ["digital"]
}
```

## Fields

### Required

| Field | Type | Description |
|-------|------|-------------|
| `description` | string or triple-string | What this channel is and how users access it. |
| `interaction_model` | InteractionModel enum | The communication pattern: `request_response`, `event_driven`, `batch`, `streaming`, `bidirectional`, `manual`. Another value is E061. |

### Optional

| Field | Type | Description |
|-------|------|-------------|
| `title` | string | Human-readable name (the string after the entity ID). |
| `url` | string | Canonical documentation, API endpoint, or configuration URL for this channel. Informational only — no URL format validation in v1. |
| `status` | ChannelStatus enum | Lifecycle state: `active`, `deprecated`. Another value is E061. Absent status treated as `active` for incremental adoption. |
| `reason` | string | Rationale for deprecation (expected when `status=deprecated`, checked by I070). |
| `tags` | string list | Free-form tags for categorization. |

## Relationships

### Outgoing edges

None. Channels are leaf nodes in the product graph.

### Incoming edges

| From | Edge Type | Meaning |
|------|-----------|---------|
| `journey` | `JourneyUsesChannel` | "Journeys that use this channel" (via `channels` field) |

## Validation Rules

| Code | Level | Rule |
|------|-------|------|
| E003 | error | Journey `channels` entries must resolve to declared entities (E022 when the target isn't a channel). |
| E061 | error | A value outside the field's vocabulary. |
| I047 | info | Channel not referenced by any journey (unreferenced channel). |
| I070 | info | Deprecated channel without a `reason`. |

## Design Guidance

### InteractionModel Selection

| Model | Use When | Example |
|-------|----------|---------|
| `request_response` | User sends a request, system responds synchronously | Web form submission, REST API call |
| `event_driven` | System pushes updates to the user asynchronously | WebSocket notifications, push notifications |
| `batch` | User submits work to be processed later | File upload for processing, scheduled reports |
| `streaming` | Continuous data flow from system to user | Live dashboards, log tailing |
| `bidirectional` | Real-time two-way communication | Chat, collaborative editing |
| `manual` | No digital interaction — physical or paper-based | Storefront counter, paper forms |

### Channel vs. Deployment Environment

Channels describe **interaction mediums**, not deployment targets:
- `web` is a channel (how users interact)
- "production" is a deployment environment (where code runs)
- `api` is a channel (how machines interact)
- "staging" is a deployment environment

### When to Declare a Channel

Declare a channel when:
- Users interact with the system through a distinct medium
- The interaction model differs from existing channels
- Multiple journeys share the same medium

## Related Entities

| Direction | Entity | Edge | Meaning |
|-----------|--------|------|---------|
| incoming | [journey](journey.md) | `JourneyUsesChannel` | Journeys that use this channel |

## Examples

### Web Channel

```spec
channel web "Web Application" {
  description """
    Browser-based web application accessed via HTTPS.
    Supports modern browsers (Chrome, Firefox, Safari, Edge).
    Responsive design for desktop and tablet viewports.
  """
  interaction_model request_response
  url "https://app.example.com"
  tags ["digital", "primary"]
}
```

### CLI Channel

```spec
channel cli "Command-Line Interface" {
  description """
    Terminal-based interface for power users and automation.
    Supports piped output, JSON formatting, and shell completion.
  """
  interaction_model request_response
  url "https://docs.example.com/cli"
  tags ["digital", "developer"]
}
```

### Deprecated Channel

```spec
channel legacy_api "Legacy REST API v1" {
  description "Original REST API, superseded by v2 GraphQL endpoint."
  interaction_model request_response
  url "https://api.example.com/v1"
  status deprecated
  reason "Replaced by GraphQL API (channel: graphql_api) — sunset date 2026-12-31"
}
```

### Non-Software Channel

```spec
channel storefront "Physical Storefront" {
  description """
    A brick-and-mortar retail location where customers interact
    with staff face-to-face. No digital interface — all interactions
    are manual and recorded by staff into the system afterward.
  """
  interaction_model manual
  tags ["physical", "retail"]
}
```
