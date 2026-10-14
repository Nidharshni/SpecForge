# type

> **Module:** `core`

## Purpose

A `type` declares a **data type definition** — the shape of a domain entity, command, error, or value object. Types are the data vocabulary of the specification: they define what fields an entity has, what annotations apply, and how the type maps to generated code.

It answers: **"What shape does the data have?"**

Types are Phase 2 entities. The v1 parser recognizes the syntax but treats type expressions as opaque until code generation is implemented. When codegen is active, types produce language-specific output: TypeScript interfaces, Python dataclasses, Go structs, JSON Schema.

## ID Pattern

```
identifier
```

Examples: `User`, `CreateUserCommand`, `DuplicateEmailError`, `UserRole`

By convention, types typically use PascalCase, but any valid identifier is accepted.

## Syntax

### Struct Type

```spec
type User {
  id        string      @readonly
  email     string      @unique
  name      string
  role      UserRole
  createdAt timestamp   @readonly
  updatedAt timestamp   @readonly
}
```

### Union Type (Enum)

```spec
type UserRole = admin | editor | viewer
```

### Error Type (Tagged)

```spec
type DuplicateEmailError {
  _tag    "DuplicateEmailError"   @literal
  email   string
  message string
}
```

### Command Type

```spec
type CreateUserCommand {
  email  string
  name   string
  role   UserRole
}
```

## Fields

### Type-Level

| Field | Type | Description |
|-------|------|-------------|
| `name` | identifier | The type name (the identifier after `type`). |
| `fields` | field list | For struct types: the fields with their types and annotations. |
| `variants` | identifier list | For union types: the possible values (`= a \| b \| c`). |
| `refs` | reference list | *(Optional)* External references (issues, tickets, diagrams) linked to this type. |

### Field-Level

Each field in a struct type has:

| Part | Description |
|------|-------------|
| `name` | The field identifier. |
| `type` | The field's type: primitive (`string`, `number`, `boolean`, `timestamp`), another type name, or an array (`TypeName[]`). |
| `annotations` | Zero or more annotations (see below). |

### Field Annotations

| Annotation | Meaning | Code Generation Effect |
|------------|---------|----------------------|
| `@readonly` | Field is set at creation and never modified. | TypeScript: `readonly`. Python: frozen dataclass. Go: no setter. |
| `@unique` | Field value must be unique across all instances. | Generates unique constraint hints. |
| `@literal` | Field has a fixed literal value (used for tagged unions). | TypeScript: literal type. Python: `Literal["..."]`. |
| `@optional` | Field may be absent. | TypeScript: `?`. Python: `Optional`. Go: pointer. |

### Primitive Types

| Type | Meaning | TypeScript | Python | Go |
|------|---------|------------|--------|----|
| `string` | Text | `string` | `str` | `string` |
| `number` | Numeric value | `number` | `float` | `float64` |
| `integer` | Integer value | `number` | `int` | `int` |
| `boolean` | True/false | `boolean` | `bool` | `bool` |
| `timestamp` | Date/time | `Date` | `datetime` | `time.Time` |
| `void` | No value (for return types) | `void` | `None` | — |

## Relationships

### Incoming edges

| From | Edge Type | Meaning |
|------|-----------|---------|
| `behavior` | `BehaviorReferencesType` | "Behaviors that use this type" (via `types` field) |
| `event` | `EventCarriesPayloadType` | "Events that have as their payload this type" (via `payload` field) |
| `port` | `PortReferencesType` | "Ports that reference this type" (via `types` field) |
| `type` | `TypeComposesType` | "Types that compose this type" (via `composed_types` field) |
| `type` | `TypeExtendsType` | "Types that extend this type" (via `extends` field) |

### Outgoing edges

| To | Edge Type | Meaning |
|----|-----------|---------|
| `type` | `TypeComposesType` | "This type composes these types" (via `composed_types` field) |
| `type` | `TypeExtendsType` | "This type extends these types" (via `extends` field) |

## Validation Rules

| Code | Rule |
|------|------|
| E002 | No two types may share the same identifier. |
| E001 | Every type name used in a field type must resolve to a declared type or be a primitive. |

## Design Guidance

### Naming Conventions

| Kind | Convention | Examples |
|------|-----------|----------|
| Domain entity | PascalCase noun | `User`, `Order`, `Product` |
| Command | PascalCase with `Command` suffix | `CreateUserCommand`, `PlaceOrderCommand` |
| Error | PascalCase with `Error` suffix | `DuplicateEmailError`, `UserNotFoundError` |
| Enum | PascalCase | `UserRole`, `OrderStatus` |
| Value object | PascalCase noun | `EmailAddress`, `Money`, `DateRange` |

### Tagged Unions for Errors

Use the `_tag` pattern for discriminated unions:

```spec
type DuplicateEmailError {
  _tag    "DuplicateEmailError"   @literal
  email   string
  message string
}

type UserNotFoundError {
  _tag    "UserNotFoundError"     @literal
  userId  string
  message string
}
```

This generates TypeScript discriminated unions, Python tagged classes, and Go error types with a `Tag()` method.

### Type vs. Port

| Type | Port |
|------|------|
| Data shape (what the data looks like) | Interface contract (what operations exist) |
| `User { id, email, name }` | `UserRepository { create(), findById() }` |
| Passive (no behavior) | Active (defines operations) |

## Related Entities

| Direction | Entity | Edge | Meaning |
|-----------|--------|------|---------|
| outgoing | [type](type.md) | `TypeComposesType` | Types this type composes |
| outgoing | [type](type.md) | `TypeExtendsType` | Types this type extends |
| incoming | [behavior](behavior.md) | `BehaviorReferencesType` | Behaviors that use this type |
| incoming | [event](event.md) | `EventCarriesPayloadType` | Events that have as their payload this type |
| incoming | [port](port.md) | `PortReferencesType` | Ports that reference this type |
| incoming | [type](type.md) | `TypeComposesType` | Types that compose this type |
| incoming | [type](type.md) | `TypeExtendsType` | Types that extend this type |

## Examples

### Domain Entity

```spec
type Order {
  id         string      @readonly
  customerId string
  items      OrderItem[]
  status     OrderStatus
  total      number
  currency   string
  createdAt  timestamp   @readonly
  updatedAt  timestamp   @readonly
}
```

### Value Object

```spec
type OrderItem {
  productId  string
  name       string
  quantity   integer
  unitPrice  number
  total      number
}
```

### Union / Enum

```spec
type OrderStatus = pending | confirmed | shipped | delivered | cancelled
```

### Error with Tag

```spec
type InsufficientInventoryError {
  _tag       "InsufficientInventoryError"   @literal
  productId  string
  requested  integer
  available  integer
  message    string
}
```

### Command

```spec
type PlaceOrderCommand {
  customerId string
  items      OrderItemInput[]
  currency   string
}

type OrderItemInput {
  productId  string
  quantity   integer
}
```
