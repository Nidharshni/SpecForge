# event

> **Module:** `core`

## Purpose

An `event` declares a **domain or system event** — something the system announces when a significant state change occurs. Events are the contracts between producers and consumers: a behavior emits an event, and other behaviors (possibly in different bounded contexts or services) react to it.

It answers: **"What does the system announce?"**

Events make the system's reactive behavior explicit and traceable. Without events, the connections between "user created" and "welcome email sent" are hidden in implementation code. With events, the compiler can trace the full chain: behavior produces event, event consumed by behavior, that behavior references invariants, etc.

## ID Pattern

```
identifier
```

Examples: `user_created`, `email_changed`, `order_placed`

## Syntax

```spec
use "types/user"
use "behaviors/user-crud"

event user_created "User Created" {
  channel   "users.created"

  payload {
    userId    string
    email     string
    role      UserRole
    timestamp timestamp
  }
}
```

## Fields

### Required

None. An event lists no behaviors: the behaviors name it, in `produces` (they emit it) and `consumes` (they react to it), and `trace <event>` lists them upstream.

### Optional

| Field | Type | Description |
|-------|------|-------------|
| `title` | string | Human-readable name (the string after the identifier). Optional — auto-derived from identifier if omitted. |
| `payload` | block | The event data shape — fields with types. Defines the contract between producer and consumers. |
| `channel` | string | Logical channel, topic, or queue name. Used for AsyncAPI generation and documentation. |
| `refs` | reference list | External references (issues, tickets, diagrams) linked to this event. |

### Payload Fields

Payload fields follow the same syntax as `type` fields:

```spec
payload {
  fieldName  fieldType
  fieldName  fieldType  @annotation
}
```

Supported annotations: `@readonly`, `@optional`.

## Relationships

### Incoming edges

| From | Edge Type | Meaning |
|------|-----------|---------|
| `behavior` | `BehaviorConsumesEvent` | "Behaviors that consume this event" (via `consumes` field) |
| `behavior` | `BehaviorProducesEvent` | "Behaviors that emit this event" (via `produces` field) |

### Outgoing edges

| To | Edge Type | Meaning |
|----|-----------|---------|
| `type` | `EventCarriesPayloadType` | "This event has as its payload these types" (via `payload` field) |
| `protocol` | `EventFollowsProtocol` | "This event follows these protocols" (via `follows_protocol` field) |
| `process` | `EventParticipatesInProcess` | "This event participates in these processes" (via `participates_in` field) |

## Validation Rules

| Code | Rule |
|------|------|
| E002 | No two events may share the same ID. |
| W007 | If no behavior produces the event, emit "event never produced" warning. |

## Design Guidance

### Good Events

Events should:
- **Describe what happened, not what to do** — "UserCreated" not "SendWelcomeEmail"
- **Be past tense** — "OrderPlaced", "PaymentProcessed", "SessionExpired"
- **Carry sufficient data** — consumers should not need to call back to the producer
- **Be immutable** — once published, the payload never changes

### Event vs. Behavior

| Event | Behavior |
|-------|----------|
| Describes what *happened* | Describes what the system *does* |
| Past tense ("UserCreated") | Imperative ("Create User") |
| Published after a state change | Defines the operation that causes the state change |
| Consumed by other behaviors | Referenced by features |

### Event Payload Design

Payloads should include:
- **Identity** — enough to identify the affected entity (e.g., `userId`)
- **Key data** — the data consumers need without round-tripping (e.g., `email`, `role`)
- **Timestamp** — when the event occurred
- **Minimal but sufficient** — don't include the entire entity; include what consumers need

### Cross-Context Events

Events are the primary mechanism for traceability across bounded contexts. When a behavior in the "Users" context produces `user_created`, and a behavior in the "Notifications" context consumes it, the compiler can trace the full chain even though the contexts are otherwise independent.

## Related Entities

| Direction | Entity | Edge | Meaning |
|-----------|--------|------|---------|
| outgoing | [type](type.md) | `EventCarriesPayloadType` | Types this event has as its payload |
| outgoing | `protocol` | `EventFollowsProtocol` | Protocols this event follows |
| outgoing | `process` | `EventParticipatesInProcess` | Processes this event participates in |
| incoming | [behavior](behavior.md) | `BehaviorConsumesEvent` | Behaviors that consume this event |
| incoming | [behavior](behavior.md) | `BehaviorProducesEvent` | Behaviors that emit this event |

## Examples

### Simple Event

```spec
event user_created "User Created" {
  channel   "users.created"

  payload {
    userId    string
    email     string
    role      UserRole
    timestamp timestamp
  }
}
```

### Event Without Consumers (Yet)

```spec
event email_changed "User Email Changed" {
  channel   "users.email-changed"

  payload {
    userId    string
    oldEmail  string
    newEmail  string
    timestamp timestamp
  }

  // No behavior produces it yet — will emit a W007 warning.
  // This is fine during early development.
}
```

### Cross-Service Event

```spec
use "behaviors/order-processing"

event order_placed "Order Placed" {
  channel   "orders.placed"

  payload {
    orderId     string
    customerId  string
    items       OrderItem[]
    totalAmount number
    currency    string
    timestamp   timestamp
  }

  refs [mermaid:order-event-flow]
}
```

The behavior `place_order` lists `order_placed` in `produces`. Four behaviors list it in `consumes`: `begin_fulfillment` (Fulfillment service: begin picking), `charge_payment` (Billing service: charge payment), `send_order_confirm` (Notification service: send confirmation) and `log_order_audit` (Audit service: log order event).

### Event Chain (A Consumer Produces the Next Event)

```spec
event payment_processed "Payment Processed" {
  channel   "billing.payment-processed"

  payload {
    orderId       string
    paymentId     string
    amount        number
    currency      string
    paymentMethod string
    timestamp     timestamp
  }
}
```

The behavior `charge_payment` lists `order_placed` in `consumes` and `payment_processed` in `produces`; `complete_fulfillment` and `send_payment_confirm` list `payment_processed` in `consumes`.
