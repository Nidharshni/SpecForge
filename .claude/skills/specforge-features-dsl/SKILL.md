---
name: specforge-features-dsl
description: "Write feature blocks in .spec DSL files. Each feature declares a user-facing capability with free-form snake_case IDs and problem/solution framing; the behaviors that implement it name it in their `features` field. Features are NOT testable (testable=false). Use when grouping behaviors into coherent units of value for product planning."
---

# SpecForge Features DSL

Rules and conventions for authoring **`feature` blocks** in `.spec` files. Features bridge what users need (problem) and how the system delivers it (solution) and are implemented by behaviors, which name them in `features`.

**Requires:** `@specforge/software` plugin.

**Note:** Features are NOT testable entities (`testable=false` in the extension manifest). They do not support `verify` statements. Testing happens at the behavior level.

## When to Use

- Grouping related behaviors into a deliverable unit of value
- Defining what a feature solves (problem) and how (solution)
- Connecting behaviors to the product planning chain
- Creating the features that journeys will reference

## Block Syntax

```spec
feature user_management "User Management" {
  problem """
    Administrators need to manage user accounts
    with guaranteed data integrity.
  """

  solution """
    CRUD operations backed by PostgreSQL with
    unique email constraints and full audit trail.
  """
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
| `problem` | string / triple-string | User need or pain point. Written from the user's perspective. |
| `solution` | string / triple-string | How the system addresses the problem. Written from the system's perspective. |
| `refs` | reference list | External references linked to this feature. |

## Relationships

### Outgoing edges

| To | Edge Type | Meaning |
|----|-----------|---------|
| `ref` | `links_to` | External references linked to this feature |

### Incoming edges

A feature lists no behaviors: each behavior names the features it implements in its own `features` field, and `trace <feature>` lists them upstream.

| From | Edge Type | Meaning |
|------|-----------|---------|
| `behavior` | `BehaviorImplementsFeature` | Behavior implements this feature (via the behavior's `features` field) |
| `journey` | `JourneyFeature` | UX journey delivers this feature |
| `module` | `ModuleFeature` | Module implements this feature |
| `milestone` | `MilestoneFeature` | Milestone phase schedules this feature |

## Writing Rules

1. **A feature is implemented by multiple behaviors** -- if only one names it in `features`, it may be too granular.
2. **Problem is user-perspective** -- describe the pain point, not the solution.
3. **Solution is system-perspective** -- describe the approach at a high level.
4. **Write the relation on the behavior** -- a behavior lists the features it implements in `features`; a feature never lists behaviors.
5. **Features go on roadmaps, behaviors go in test suites** -- keep the distinction clear.
6. **Features are not testable** -- do not add `verify` statements; testing is at the behavior level.

## Validation Rules

| Code | Rule |
|------|------|
| E002 | No duplicate feature IDs across all `.spec` files. |
| W041 | Orphan feature -- not referenced by any journey. |
| W008 | Unimplemented feature -- no behavior names it in `features`. |

## Examples

### Simple Feature

```spec
feature password_authentication "Password Authentication" {
  problem """
    Users need to securely authenticate to access their accounts.
    The system must prevent unauthorized access while keeping
    the login experience fast and simple.
  """

  solution """
    Password-based authentication with bcrypt hashing,
    rate-limited login attempts, and secure session tokens.
  """
}
```

### Feature with Refs

```spec
feature full_text_search "Full-Text Search" {
  problem """
    Users with large datasets cannot find specific records quickly.
    Current filtering by exact field match is insufficient for
    free-form queries across multiple fields.
  """

  solution """
    Elasticsearch-backed full-text search with typo tolerance,
    field weighting, and faceted filtering. Results ranked by
    relevance with highlighting of matched terms.
  """

  refs [gh.issue:88, figma.frame:search-ui]
}
```

### Cross-Domain Feature

```spec
feature order_checkout "Order Checkout" {
  problem """
    Customers need to complete purchases with confidence that
    their order will be fulfilled, their payment processed correctly,
    and inventory accurately reserved.
  """

  solution """
    Orchestrated checkout flow: validate inventory, reserve items,
    process payment via Stripe, create order record. Compensating
    transactions on failure (release inventory, refund payment).
  """
}
```

## What NOT to Do

- Do not write a feature that a single behavior implements -- that is too granular
- Do not put MUST/SHOULD keywords in the problem/solution -- save those for behavior contracts
- Do not confuse features (system-perspective value) with capabilities (user-perspective UX flow)
- Do not write a `behaviors` field on a feature (E079): the behaviors name the feature in their `features`
- Do not put implementation details in the `problem` field -- it should be user-perspective
- Do not add `verify` statements -- features are not testable entities
