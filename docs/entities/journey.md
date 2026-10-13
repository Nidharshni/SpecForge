# journey

> **Module:** `@specforge/product`

## Purpose

A `journey` declares a **UX flow** — a concrete interaction path that maps a specific persona on a specific channel to the features they use. Journeys sit at the top of the product model: they represent what a real user actually does with the system. Note (C12-14): journey steps are prose by deliberate decision (ADR pe_journey_flow_opaque) — journeys are documentation, not per-step traceability edges; the traceability chain runs behavior → verify → test record.

It answers: **"How does the user experience this?"**

While features describe *what* the system delivers, journeys describe *how* a specific type of user interacts with it. The same feature (e.g., "User Management") may surface as different journeys for different personas: an admin creates users through a web dashboard, while an API consumer creates users through a REST endpoint.

## ID Pattern

```
identifier
```

Examples: `create_user_web`, `create_user_api`, `search_records_cli`

## Syntax

```spec
use "features/user-management"
use "product/personas"
use "product/channels"

journey create_user_web "Create a New User" {
  persona  admin
  channels [web, cli, api]
  features [user_management]

  flow """
    1. Admin opens user management page
    2. Clicks "New User"
    3. Fills form (name, email, role)
    4. Submits — system validates uniqueness
    5. Success: user appears in list
    6. Failure: inline error on email field
  """
}
```

## Fields

All fields are optional at the type level.

### Optional

| Field | Type | Description |
|-------|------|-------------|
| `title` | string | Human-readable name (the string after the entity ID). Phrased as an action from the user's perspective. |
| `persona` | EntityId | The persona performing this journey. Must resolve to a declared entity (E003). |
| `description` | string | Brief summary of the journey's purpose. |
| `channels` | EntityId[] | The channel entities where this journey is available. Each must resolve to a declared entity (E003). |
| `features` | EntityId[] | Features this journey delivers. Creates `JourneyExercisesFeature` edges. |
| `flow` | string[] | Step-by-step interaction flow describing what the user does and what the system responds. |
| `priority` | Priority | Importance level: `critical`, `high`, `medium`, `low`. |
| `tags` | string[] | Free-form tags for categorization. |
| `refs` | reference list | External references (issues, tickets, diagrams) linked to this journey. |

## Relationships

### Outgoing edges

| To | Edge Type | Meaning |
|----|-----------|---------|
| `feature` | `JourneyExercisesFeature` | "This journey exercises these features" (via `features` field) |
| `persona` | `JourneyTargetsPersona` | "This journey is performed by these personas" (via `persona` field) |
| `channel` | `JourneyUsesChannel` | "This journey uses these channels" (via `channels` field) |

### Incoming edges

| From | Edge Type | Meaning |
|------|-----------|---------|
| `deliverable` | `DeliverableSupportsJourney` | "Deliverables that ship this journey" (via `journeys` field) |

## Validation Rules

| Code | Level | Rule |
|------|-------|------|
| E003 | error | `persona` and every `channels` entry must resolve to a declared entity. |
| W042 | warning | Journey not referenced by any deliverable (unreferenced journey). |
| I050 | info | `flow []`: a flow with no steps (a missing `flow` is E006). |
| I054 | info | No `persona`. |
| I055 | info | No channels (no `channels`, or an empty list). |

## Design Guidance

### Naming Journeys

Journeys should be named as user actions in the imperative mood:
- "Create a New User" (not "User Creation")
- "Search for Records" (not "Record Search")
- "Export Monthly Report" (not "Report Export")

### Writing Flows

Flows should:
- Be numbered steps (1, 2, 3...)
- Alternate between user actions and system responses
- Include both success and failure paths
- Be concrete enough to derive a UI wireframe or API sequence

### Persona and Channel References

Personas and channels are **first-class entities** declared with `persona` and `channel` blocks. Journeys reference them by entity ID, and the compiler validates these references (E003) and detects unreferenced entities (I046, I047).

| Persona | Typical Meaning |
|---------|----------------|
| `admin` | System administrator with full access |
| `user` | Regular authenticated user |
| `viewer` | Read-only access |
| `api_consumer` | Machine-to-machine integration |
| `operator` | DevOps / infrastructure team |

| Channel | Typical Meaning |
|---------|----------------|
| `web` | Browser-based web application |
| `cli` | Command-line interface |
| `api` | REST/gRPC/GraphQL API endpoint |
| `mobile` | Native mobile app |

### Journey vs. Feature

| Journey | Feature |
|---------|---------|
| User-perspective | System-perspective |
| "Admin creates a new user via web dashboard" | "User Management" |
| Includes persona, channels, interaction flow | Includes problem, solution, behaviors |
| One journey may use one feature | One feature may serve many journeys |

### Multiple Journeys per Feature

A single feature often surfaces as multiple journeys:

```
feature user_management
  |-- journey create_user_web (admin, web)
  |-- journey create_user_api (api_consumer, api)
  |-- journey view_user_list (admin, web)
  +-- journey search_users (admin, web)
```

## Related Entities

| Direction | Entity | Edge | Meaning |
|-----------|--------|------|---------|
| outgoing | [feature](feature.md) | `JourneyExercisesFeature` | Features this journey exercises |
| outgoing | [persona](persona.md) | `JourneyTargetsPersona` | Personas this journey is performed by |
| outgoing | [channel](channel.md) | `JourneyUsesChannel` | Channels this journey uses |
| incoming | [deliverable](deliverable.md) | `DeliverableSupportsJourney` | Deliverables that ship this journey |

## Examples

### Web UI Journey

```spec
use "features/user-management"
use "product/personas"
use "product/channels"

journey create_user_web "Create a New User" {
  persona  admin
  channels [web]
  features [user_management]

  flow """
    1. Admin navigates to Settings > Users
    2. Clicks "Add User" button
    3. Modal opens with form: name, email, role dropdown
    4. Admin fills fields and clicks "Create"
    5. System validates email uniqueness
    6. Success: modal closes, user appears in table, toast notification
    7. Failure: inline error below email field, "Email already in use"
  """

  refs [figma.frame:user-create-modal]
}
```

### API Journey

```spec
use "features/user-management"
use "product/personas"
use "product/channels"

journey create_user_api "Create User via API" {
  persona  api_consumer
  channels [api]
  features [user_management]

  flow """
    1. Consumer sends POST /api/v1/users with JSON body
    2. System validates request body schema
    3. System validates email uniqueness
    4. Success: 201 Created with user object in response body
    5. Validation failure: 422 Unprocessable Entity with error details
    6. Duplicate email: 409 Conflict with error message
  """
}
```

### CLI Journey

```spec
use "features/search"
use "product/personas"
use "product/channels"

journey search_records_cli "Search Records from Terminal" {
  persona  operator
  channels [cli]
  features [full_text_search]

  flow """
    1. Operator runs: specforge search "user email:alice@"
    2. System performs full-text search with field filtering
    3. Results displayed in table format with highlighting
    4. Operator can pipe output to jq or other tools
    5. No results: informative message with suggestion to broaden query
  """
}
```

### Multi-Feature Journey

```spec
use "features/order-checkout"
use "features/order-tracking"
use "product/personas"
use "product/channels"

journey purchase_and_track "Purchase and Track Order" {
  persona  user
  channels [web, mobile]
  features [order_checkout, order_tracking]
  priority high

  flow """
    1. User reviews cart and clicks "Checkout"
    2. System validates inventory for all items
    3. User confirms shipping address and payment method
    4. System processes payment and creates order
    5. Success: confirmation page with order number and ETA
    6. User can track order status on "My Orders" page
    7. System sends email notification at each fulfillment stage
    8. Payment failure: error page with retry option, cart preserved
  """
}
```
