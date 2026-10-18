---
name: specforge-events-dsl
description: "Write event blocks in .spec DSL files. Each event declares a domain or system event with free-form snake_case IDs, payload type reference, channel name, sync blocks for CSP concurrency, and verify statements (integration/deadlock_free/liveness). Use when making the system's reactive behavior explicit and traceable across bounded contexts."
---

# SpecForge Events DSL

Rules and conventions for authoring **`event` blocks** in `.spec` files. Events make the system's reactive behavior explicit -- a behavior emits an event, and other behaviors consume it.

**Requires:** `@specforge/software` plugin.

## When to Use

- Declaring what the system announces when a state change occurs
- Defining event payload contracts between producers and consumers (via type references)
- Tracing reactive chains across bounded contexts or services
- Linking behaviors via publish-subscribe relationships
- Defining CSP synchronization constraints with `sync` blocks

## Block Syntax

```spec
use "types/user"
use "behaviors/user-crud"

event user_created "User Created" {
  channel   "users.created"
  payload   UserCreatedPayload

  verify integration "event emitted after user creation"
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
| `payload` | reference | Type reference for the event data shape (e.g., `payload UserCreatedPayload`). |
| `channel` | string | Logical channel, topic, or queue name. |
| `sync` | block | CSP synchronization constraints: `barrier` (behavior references) and `timeout` (duration). |
| `verify` | verify statement(s) | Test specifications: `verify {kind} "{description}"`. Kinds: integration, deadlock_free, liveness. |
| `refs` | reference list | External references linked to this event. |

### Sync Block

The `sync` block defines CSP (Communicating Sequential Processes) synchronization constraints:

```spec
sync {
  barrier [behavior_a, behavior_b, behavior_c]
  timeout 30s "all sub-analyses must complete within 30 seconds"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `barrier` | reference list | Behaviors that must all complete before this event fires. |
| `timeout` | string | Duration with description -- maximum wait time for barrier completion. |

### Verify Kinds for Events

| Kind | Meaning |
|------|---------|
| `integration` | Event emitted correctly with proper payload |
| `deadlock_free` | No circular dependency between event participants |
| `liveness` | Event processing eventually completes |

## Relationships

### Incoming edges

| From | Edge Type | Meaning |
|------|-----------|---------|
| `behavior` | `BehaviorProducesEvent` | Behavior emits this event (via the behavior's `produces` field) |
| `behavior` | `BehaviorConsumesEvent` | Behavior reacts to this event (via the behavior's `consumes` field) |

An event lists no behaviors: the behaviors that emit or react to it name it, and `trace <event>` lists them upstream.

### Outgoing edges

| To | Edge Type | Meaning |
|----|-----------|---------|
| `type` | `EventCarriesPayloadType` | Event carries data shaped like this type (via `payload`) |
| `ref` | `links_to` | External references linked to this event |

## Writing Rules

1. **Past tense names** -- "user_created", "order_placed", "payment_processed" -- events describe what happened.
2. **Write the relation on the behavior** -- a behavior lists the events it emits in `produces` and the events it reacts to in `consumes`; an event never lists behaviors (E079).
3. **Payload is a type reference** -- use `payload UserCreatedPayload`, not inline field declarations. Define the payload shape as a `type` block.
4. **Use `sync` for concurrency constraints** -- when multiple behaviors must complete before the event fires.
5. **Import files** -- `use` the files that declare the types a block references.
6. **An event no behavior produces is OK during development** -- `W007` warning will remind you to add a `produces` later.
7. **Add verify statements** -- events are testable entities (integration, deadlock_free, liveness).

## Validation Rules

| Code | Rule |
|------|------|
| E002 | No duplicate event IDs across all `.spec` files. |
| E034 | Circular event dependency detected (deadlock). |
| E035 | Channel type mismatch -- producer and consumer disagree on payload type. |
| W007 | Event never produced -- no behavior lists it in `produces`. |
| W029 | Unmatched producers -- event has producers but no consumers. |

## Examples

### Simple Event

```spec
event user_created "User Created" {
  channel   "users.created"
  payload   UserCreatedPayload

  verify integration "event emitted after user creation"
}
```

### Event Without Consumers

```spec
event user_email_changed "User Email Changed" {
  channel   "users.email-changed"
  payload   EmailChangedPayload

  // No behavior consumes it yet -- fine during early development.

  verify integration "event emitted after email update"
}
```

### Event with Sync Block

```spec
event concurrency_analysis_complete "Concurrency Analysis Complete" {
  channel   "analysis.concurrency"
  payload   ConcurrencyAnalysisReport

  sync {
    barrier [detect_deadlocks, detect_channel_mismatch, detect_unmatched_producers, detect_livelock_risk]
    timeout 30s "all concurrency sub-analyses must complete within 30 seconds"
  }

  verify integration "event emitted after process analyze pass completes"
  verify deadlock_free "no circular dependency between concurrency sub-analyses"
}
```

### Cross-Service Event

```spec
use "behaviors/order-processing"

event order_placed "Order Placed" {
  channel   "orders.placed"
  payload   OrderPlacedPayload

  verify integration "event emitted with correct order details"
  verify liveness "all consumers eventually process the event"
}
```

## What NOT to Do

- Do not name events as commands ("send_welcome_email" -- that is a behavior, not an event)
- Do not use future tense ("user_will_be_created" -- events describe what happened)
- Do not write `trigger` or `consumers` on an event (E079): the behaviors list the event in `produces` and `consumes`
- Do not define payload fields inline -- use a type reference (`payload MyPayloadType`)
- Do not ignore sync constraints for events with multiple concurrent dependencies
