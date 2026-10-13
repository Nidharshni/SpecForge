# behavior

> **Module:** `core`

## Purpose

A `behavior` declares a **behavioral contract** — a precise specification of what the system does in a specific situation. Behaviors are the atomic unit of specification: each one describes a single operation with its preconditions, guarantees, and verification strategy.

It answers: **"What exactly does the system do?"**

Behaviors use RFC 2119 keywords (MUST, MUST NOT, SHOULD, SHALL, MAY) to express the obligation level of each guarantee. They are the bridge between user-facing features (what value is delivered) and runtime invariants (what must always be true).

## ID Pattern

```
identifier
```

Examples: `create_user`, `read_user`, `update_email`

## Syntax

```spec
behavior create_user "Create User" {
  invariants [data_persistence, email_uniqueness]

  contract """
    When a valid CreateUserCommand is received,
    the system MUST create a user record with unique email
    and MUST return Result<User, DuplicateEmailError>.
  """

  refs [gh.issue:42, jira.epic:PROJ-123]

  verify unit        "insert user, retrieve by ID, assert equal"
  verify integration "insert user, restart process, retrieve persists"
  verify property    "email uniqueness holds under concurrent inserts"

  tests [
    "tests/user_test.go::TestCreateUser",
    "tests/user.test.ts:45",
    "tests/test_user.py::test_create_user",
  ]
}
```

## Fields

### Required

| Field | Type | Description |
|-------|------|-------------|
| `contract` | string or triple-string | The behavioral contract. MUST use RFC 2119 keywords to express obligation levels. |

### Optional

| Field | Type | Description |
|-------|------|-------------|
| `title` | string | Human-readable name (the string after the identifier). Optional — auto-derived from identifier if omitted. |
| `invariants` | reference list | Invariants that this behavior depends on or upholds. |
| `types` | reference list | Type definitions used by this behavior. |
| `ports` | reference list | Port interfaces used by this behavior. |
| `verify` | verify statement(s) | Test specifications: `verify {type} "{description}"`. Multiple allowed. |
| `tests` | string list | Paths to existing test files/functions that exercise this behavior. |
| `refs` | reference list | External references (issues, tickets, diagrams) linked to this behavior. |

### Verify Statement Types

| Type | Meaning | Example |
|------|---------|---------|
| `unit` | Isolated test, no external dependencies | `verify unit "insert user, get by ID, assert equal"` |
| `integration` | Tests with real dependencies (database, network) | `verify integration "insert user, restart, retrieve persists"` |
| `property` | Property-based / fuzz testing | `verify property "email uniqueness holds under concurrent inserts"` |
| `load` | Performance / load testing | `verify load "1000 concurrent creates in < 5s"` |
| `e2e` | End-to-end through the full system | `verify e2e "user created via API, visible in admin dashboard"` |

## Relationships

### Outgoing edges

| To | Edge Type | Meaning |
|----|-----------|---------|
| `event` | `BehaviorConsumesEvent` | "This behavior consumes these events" (via `consumes` field) |
| `invariant` | `BehaviorEnforcesInvariant` | "This behavior enforces these invariants" (via `invariants` field) |
| `feature` | `BehaviorImplementsFeature` | "This behavior implements these features" (via `features` field) |
| `event` | `BehaviorProducesEvent` | "This behavior emits these events" (via `produces` field) |
| `type` | `BehaviorReferencesType` | "This behavior uses these types" (via `types` field) |
| `behavior` | `BehaviorRefinesBehavior` | "This behavior refines these behaviors" (via `refines` field) |
| `property` | `BehaviorSatisfiesProperty` | "This behavior satisfies these properties" (via `satisfies` field) |
| `port` | `BehaviorUsesPort` | "This behavior uses these ports" (via `ports` field) |

### Incoming edges

| From | Edge Type | Meaning |
|------|-----------|---------|
| `behavior` | `BehaviorRefinesBehavior` | "Behaviors that refine this behavior" (via `refines` field) |
| `constraint` | `ConstraintConstrainsBehavior` | "Constraints that constrain this behavior" (via `constrains` field) |
| `constraint` | `ConstraintEnforcedByBehavior` | "Constraints that are enforced by this behavior" (via `enforced_by` field) |
| `failure_mode` | `FailureModeAffectsBehavior` | "Failure modes that affect this behavior" (via `affected_behaviors` field) |
| `milestone` | `MilestoneIncludesBehavior` | "Milestones that deliver this behavior" (via `behaviors` field) |
| `refinement` | `RefinementRefinesAbstract` | "Refinements that map from this behavior" (via `abstract_entity` field) |
| `refinement` | `RefinementRefinesConcrete` | "Refinements that map to this behavior" (via `concrete_entity` field) |

## Validation Rules

| Code | Rule |
|------|------|
| E001 | Every ID in `invariants` must resolve to an existing `invariant`. |
| E001 | Every ID in `types` must resolve to an existing `type`. |
| E001 | Every ID in `ports` must resolve to an existing `port`. |
| E002 | No two behaviors may share the same ID. |
| W001 | If no `feature` references this behavior, emit "unreferenced behavior" warning. |
| W004 | If no `verify` statement is present, emit "unverified behavior" warning. |

## Design Guidance

### Writing Good Contracts

A contract should specify:
1. **Trigger** — "When X happens" or "Given Y"
2. **Action** — what the system does
3. **Guarantee** — what MUST be true after the action
4. **Error cases** — what happens when preconditions aren't met

Use RFC 2119 keywords precisely:
- **MUST** — absolute requirement; violation is a bug
- **MUST NOT** — absolute prohibition
- **SHOULD** — recommended; deviation requires justification
- **MAY** — truly optional behavior

### Contract Patterns

**Command (state change):**
```
When a valid {Command} is received,
the system MUST {perform action}
and MUST return Result<{Success}, {Error}>.
```

**Query (read):**
```
Given {precondition},
the system MUST return {result type}.
MUST NOT return stale data after a successful write.
```

**Validation (rejection):**
```
When {invalid input} is received,
the system MUST reject the request
and MUST return {ErrorType} with {details}.
```

### Behavior vs. Invariant

| Behavior | Invariant |
|----------|-----------|
| Describes an operation | Describes a property |
| Has a trigger (when X, do Y) | Has no trigger (always true) |
| "When a user is created, assign a unique ID" | "No two users share the same ID" |
| Scoped to a single operation | Universal across all operations |

### Granularity

Each behavior should describe **one logical operation**. If a behavior has multiple MUST clauses for different operations, split it. If two behaviors always happen together and can't be tested independently, merge them.

## Related Entities

| Direction | Entity | Edge | Meaning |
|-----------|--------|------|---------|
| outgoing | [event](event.md) | `BehaviorConsumesEvent` | Events this behavior consumes |
| outgoing | [invariant](invariant.md) | `BehaviorEnforcesInvariant` | Invariants this behavior enforces |
| outgoing | [feature](feature.md) | `BehaviorImplementsFeature` | Features this behavior implements |
| outgoing | [event](event.md) | `BehaviorProducesEvent` | Events this behavior emits |
| outgoing | [type](type.md) | `BehaviorReferencesType` | Types this behavior uses |
| outgoing | [behavior](behavior.md) | `BehaviorRefinesBehavior` | Behaviors this behavior refines |
| outgoing | `property` | `BehaviorSatisfiesProperty` | Properties this behavior satisfies |
| outgoing | [port](port.md) | `BehaviorUsesPort` | Ports this behavior uses |
| incoming | [behavior](behavior.md) | `BehaviorRefinesBehavior` | Behaviors that refine this behavior |
| incoming | [constraint](constraint.md) | `ConstraintConstrainsBehavior` | Constraints that constrain this behavior |
| incoming | [constraint](constraint.md) | `ConstraintEnforcedByBehavior` | Constraints that are enforced by this behavior |
| incoming | [failure_mode](failure-mode.md) | `FailureModeAffectsBehavior` | Failure modes that affect this behavior |
| incoming | [milestone](milestone.md) | `MilestoneIncludesBehavior` | Milestones that deliver this behavior |
| incoming | `refinement` | `RefinementRefinesAbstract` | Refinements that map from this behavior |
| incoming | `refinement` | `RefinementRefinesConcrete` | Refinements that map to this behavior |

## Examples

### Simple Query

```spec
behavior read_user "Read User by ID" {
  invariants [data_persistence]

  contract """
    Given a valid user ID,
    the system MUST return the user record or UserNotFoundError.
    MUST NOT return stale data after a successful write.
  """

  verify unit "insert then get by ID"
}
```

### Validation Behavior

```spec
behavior update_email "Update User Email" {
  invariants [data_persistence, email_uniqueness]

  contract """
    When an UpdateEmailCommand is received,
    the system MUST validate the new email is unique
    before committing the change.
    MUST return DuplicateEmailError if the email is already taken.
    MUST NOT modify the user record if validation fails.
  """

  verify unit        "update to unique email succeeds"
  verify unit        "update to taken email fails with DuplicateEmailError"
  verify integration "concurrent updates to same email — exactly one wins"
}
```

### Event-Producing Behavior

```spec
behavior place_order "Place Order" {
  invariants [data_persistence, idempotent_orders]

  contract """
    When a valid PlaceOrderCommand is received,
    the system MUST validate inventory availability,
    reserve items, create an order record,
    and MUST emit an OrderPlaced event.
    MUST return Result<Order, InsufficientInventoryError>.
  """

  verify unit        "place order with available items"
  verify unit        "reject order when items unavailable"
  verify integration "concurrent orders for last item — exactly one succeeds"
  verify e2e         "order placed via API triggers fulfillment workflow"

  tests [
    "tests/order.test.ts:12",
    "tests/order_test.go::TestPlaceOrder",
  ]
}
```
