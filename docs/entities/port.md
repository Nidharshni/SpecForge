# port

> **Module:** `core`

## Purpose

A `port` declares an **interface contract** — a set of operations that a component provides or requires. Ports define the boundaries between the domain and the outside world: databases, APIs, message brokers, external services. They are the "holes" in the hexagonal architecture that adapters plug into.

It answers: **"What contracts exist between components?"**

Ports are Phase 2 entities. The v1 parser recognizes the syntax but treats method signatures as opaque until code generation is implemented. When codegen is active, ports produce language-specific interfaces: TypeScript interfaces with `ResultAsync`, Python ABCs with `Result`, Go interfaces with `context.Context`.

## ID Pattern

```
identifier
```

Examples: `UserRepository`, `EmailService`, `PaymentGateway`, `NotificationPort`

By convention, ports typically use PascalCase, but any valid identifier is accepted.

## Syntax

```spec
use "types/user"

port UserRepository "User Repository" {
  direction outbound
  category  "persistence/user"

  method create(cmd: CreateUserCommand) -> Result<User, DuplicateEmailError>
  method findById(id: string) -> Result<User, UserNotFoundError>
  method findByEmail(email: string) -> Result<User?, never>
  method update(id: string, cmd: UpdateUserCommand) -> Result<User, DuplicateEmailError | UserNotFoundError>
  method delete(id: string) -> Result<void, UserNotFoundError>
}
```

## Fields

### Required

| Field | Type | Description |
|-------|------|-------------|
| `name` | identifier | The port name (the identifier after `port`). |
| `direction` | enum | `inbound` or `outbound`. See Direction below. |
| `methods` | method list | One or more method signatures defining the port's operations. |

### Optional

| Field | Type | Description |
|-------|------|-------------|
| `category` | string | Organizational category for grouping related ports (e.g., `"persistence/user"`, `"messaging"`, `"external/payment"`). |
| `refs` | reference list | External references (issues, tickets, diagrams) linked to this port. |

### Direction

| Direction | Meaning | Examples |
|-----------|---------|----------|
| `inbound` | The outside world calls into the system. The port defines what the system offers. | `UserAPI`, `WebhookHandler`, `CLICommands` |
| `outbound` | The system calls out to the world. The port defines what the system requires. | `UserRepository`, `EmailService`, `PaymentGateway` |

In hexagonal architecture terms:
- **Inbound** ports are "driving" ports (primary adapters connect to these)
- **Outbound** ports are "driven" ports (secondary adapters implement these)

### Method Signatures

```ebnf
method_decl  = "method" , identifier , "(" , [ param_list ] , ")" , "->" , return_type ;
param_list   = param , { "," , param } ;
param        = identifier , ":" , type_expr ;
return_type  = type_expr ;
type_expr    = identifier [ "?" ] [ "<" , type_expr , { "," , type_expr } , ">" ]
             | type_expr , "|" , type_expr ;
```

Common patterns:
- `Result<Success, Error>` — operation that can fail with a typed error
- `Result<Success, ErrorA | ErrorB>` — multiple possible error types
- `Type?` — nullable return (may return null/nil/None)
- `void` — no return value
- `never` — error channel is impossible (operation cannot fail)

## Relationships

### Incoming edges

| From | Edge Type | Meaning |
|------|-----------|---------|
| `behavior` | `BehaviorUsesPort` | "Behaviors that use this port" (via `ports` field) |
| `module` | `ModuleConsumesPort` | "Modules that consume this port" (via `ports` field) |
| `module` | `ModuleDefinesPort` | "Modules that define this port" (via `ports_defined` field) |

### Outgoing edges

| To | Edge Type | Meaning |
|----|-----------|---------|
| `type` | `PortReferencesType` | "This port references these types" (via `types` field) |

## Validation Rules

| Code | Rule |
|------|------|
| E002 | No two ports may share the same identifier. |
| E001 | Every type name used in method signatures must resolve to a declared type or be a primitive. |

## Design Guidance

### Port Naming

| Pattern | Convention | Examples |
|---------|-----------|----------|
| Repository | `{Entity}Repository` | `UserRepository`, `OrderRepository` |
| External service | `{Service}Service` or `{Service}Gateway` | `EmailService`, `PaymentGateway` |
| API surface | `{Domain}API` or `{Domain}Port` | `UserAPI`, `SearchPort` |
| Event bus | `{Domain}EventBus` or `EventPublisher` | `OrderEventBus`, `EventPublisher` |

### Result Types

All port methods should return `Result<Success, Error>` types, not throw exceptions. This makes error handling explicit and type-safe. The specific `Result` implementation depends on the target language:

| Language | Result Type |
|----------|-------------|
| TypeScript | `ResultAsync<T, E>` (from hex-di, neverthrow, or Effect) |
| Python | `Result[T, E]` (from result library) |
| Go | `(T, error)` (idiomatic Go error handling) |

### Port vs. Type

| Port | Type |
|------|------|
| Defines operations (methods) | Defines data shape (fields) |
| Has a direction (inbound/outbound) | Has no direction |
| Generates interfaces | Generates structs/classes |
| "What can the system do?" | "What data does the system use?" |

### Port vs. Behavior

| Port | Behavior |
|------|----------|
| The interface contract (signature) | The behavioral contract (semantics) |
| "This method exists with this signature" | "When this method is called, MUST do X" |
| No MUST/SHOULD/MAY — just type signatures | MUST/SHOULD/MAY define obligation levels |
| Implementation-level | Specification-level |

A behavior references the invariants that the port's implementation must uphold. The port defines the shape; the behavior defines the rules.

## Related Entities

| Direction | Entity | Edge | Meaning |
|-----------|--------|------|---------|
| outgoing | [type](type.md) | `PortReferencesType` | Types this port references |
| incoming | [behavior](behavior.md) | `BehaviorUsesPort` | Behaviors that use this port |
| incoming | [module](module.md) | `ModuleConsumesPort` | Modules that consume this port |
| incoming | [module](module.md) | `ModuleDefinesPort` | Modules that define this port |

## Examples

### Outbound: Database

```spec
use "types/user"

port UserRepository "User Repository" {
  direction outbound
  category  "persistence/user"

  method create(cmd: CreateUserCommand) -> Result<User, DuplicateEmailError>
  method findById(id: string) -> Result<User, UserNotFoundError>
  method findByEmail(email: string) -> Result<User?, never>
  method update(id: string, cmd: UpdateUserCommand) -> Result<User, DuplicateEmailError | UserNotFoundError>
  method delete(id: string) -> Result<void, UserNotFoundError>
  method list(page: integer, pageSize: integer) -> Result<UserPage, never>
}
```

### Outbound: External Service

```spec
use "types/email"

port EmailService "Email Service" {
  direction outbound
  category  "external/email"

  method send(to: string, template: EmailTemplate, data: EmailData) -> Result<void, EmailDeliveryError>
  method sendBulk(recipients: BulkEmailRequest) -> Result<BulkEmailResult, EmailDeliveryError>
}
```

### Inbound: API Surface

```spec
use "types/user"

port UserAPI "User API" {
  direction inbound
  category  "api/user"

  method createUser(cmd: CreateUserCommand) -> Result<User, DuplicateEmailError>
  method getUser(id: string) -> Result<User, UserNotFoundError>
  method updateUser(id: string, cmd: UpdateUserCommand) -> Result<User, DuplicateEmailError | UserNotFoundError>
  method deleteUser(id: string) -> Result<void, UserNotFoundError>
  method listUsers(query: UserQuery) -> Result<UserPage, never>
}
```

### Outbound: Message Broker

```spec
use "types/order"

port OrderEventBus "Order Event Bus" {
  direction outbound
  category  "messaging/order"

  method publish(event: OrderEvent) -> Result<void, PublishError>
  method subscribe(channel: string, handler: EventHandler) -> Result<Subscription, SubscriptionError>
}
```

## Code Generation Output

### TypeScript

```typescript
// src/generated/ports/user-repository.ts (auto-generated)
import type { ResultAsync } from "@hex-di/core";
import type { User, CreateUserCommand, DuplicateEmailError, UserNotFoundError } from "../types/user";

export interface UserRepository {
  create(cmd: CreateUserCommand): ResultAsync<User, DuplicateEmailError>;
  findById(id: string): ResultAsync<User, UserNotFoundError>;
  findByEmail(email: string): ResultAsync<User | null, never>;
}
```

### Python

```python
# src/generated/ports/user_repository.py (auto-generated)
from abc import ABC, abstractmethod
from result import Result
from ..types.user import User, CreateUserCommand, DuplicateEmailError, UserNotFoundError

class UserRepository(ABC):
    @abstractmethod
    async def create(self, cmd: CreateUserCommand) -> Result[User, DuplicateEmailError]: ...

    @abstractmethod
    async def find_by_id(self, id: str) -> Result[User, UserNotFoundError]: ...
```

### Go

```go
// internal/generated/ports/user_repository.go (auto-generated)
package ports

import (
    "context"
    "myservice/internal/generated/types"
)

type UserRepository interface {
    Create(ctx context.Context, cmd types.CreateUserCommand) (types.User, error)
    FindByID(ctx context.Context, id string) (types.User, error)
}
```
