# Todo App Example — `@specforge/software`

A minimal todo app specified with all 6 entity kinds from `@specforge/software`.

## Project setup

```json
// specforge.json
{
  "name": "todo-app",
  "version": "0.1.0",
  "spec_root": "spec/",
  "extensions": ["@specforge/software"]
}
```

## Spec file

```specforge
// spec/todo.spec

// ── Types ─────────────────────────────────────────────────

type TodoItem "Todo Item" {
  id          string    @readonly
  title       string
  completed   boolean
  created_at  string    @readonly
}

type TodoFilter "Todo Filter" = all | active | completed

type TodoError "Todo Error" = not_found | invalid_title | duplicate_id

// ── Ports ─────────────────────────────────────────────────

port todo_repository "Todo Repository" {
  direction outbound

  method save(item: TodoItem) -> Result<TodoItem, TodoError>
  method find_by_id(id: string) -> Result<TodoItem, TodoError>
  method list(filter: TodoFilter) -> Result<TodoItem[], TodoError>
  method delete(id: string) -> Result<void, TodoError>
}

port todo_api "Todo REST API" {
  direction inbound

  method create_todo(title: string) -> Result<TodoItem, TodoError>
  method list_todos(filter: TodoFilter) -> Result<TodoItem[], TodoError>
  method update_todo(id: string, completed: boolean) -> Result<TodoItem, TodoError>
  method delete_todo(id: string) -> Result<void, TodoError>
}

// ── Behaviors ─────────────────────────────────────────────

behavior create_todo "Create Todo" {
  category command
  features   [todo_management]
  types      [TodoItem, TodoError]
  ports      [todo_api, todo_repository]
  produces   [todo_created]
  invariants [todo_title_not_empty, todo_id_uniqueness]

  contract """
    When a valid title is received,
    the system MUST create a new TodoItem with completed=false,
    assign a unique id, and persist it via todo_repository.
    MUST return Result<TodoItem, TodoError>.
  """

  requires {
    title_present    "title is a non-empty string"
  }

  ensures {
    item_persisted   "TodoItem is saved via todo_repository"
    completed_false  "new item has completed=false"
    id_assigned      "new item has a unique id"
  }

  verify unit        "creates item with completed=false"
  verify unit        "rejects empty title"
  verify integration "persists item to repository"
}

behavior toggle_todo "Toggle Todo Completion" {
  category command
  features   [todo_management]
  types      [TodoItem, TodoError]
  ports      [todo_api, todo_repository]
  produces   [todo_toggled]

  contract """
    The system MUST flip the completed flag on an existing
    TodoItem and persist the change.
    MUST return Result<TodoItem, TodoError>.
  """

  requires {
    item_exists      "TodoItem with given id exists in repository"
  }

  ensures {
    flag_flipped     "completed is negated from its previous value"
    change_persisted "updated item is saved via todo_repository"
  }

  verify unit "flips completed from false to true"
  verify unit "flips completed from true to false"
  verify unit "returns error for nonexistent id"
}

behavior delete_todo "Delete Todo" {
  category command
  features   [todo_management]
  types      [TodoItem, TodoError]
  ports      [todo_api, todo_repository]
  produces   [todo_deleted]

  contract """
    The system MUST remove a TodoItem by id.
    MUST return Result<void, TodoError>.
  """

  requires {
    item_exists      "TodoItem with given id exists in repository"
  }

  ensures {
    item_removed     "TodoItem is deleted from todo_repository"
  }

  verify unit "deletes existing item"
  verify unit "returns error for nonexistent id"
}

behavior list_todos "List Todos" {
  category query
  features   [todo_management]
  consumes   [todo_created]
  types      [TodoItem, TodoFilter]
  ports      [todo_api, todo_repository]

  contract """
    The system MUST return todos filtered by the given TodoFilter.
    MUST return Result<TodoItem[], TodoError>.
  """

  ensures {
    filter_applied   "only items matching filter are returned"
    order_stable     "items are returned in creation order"
  }

  verify unit "filter=all returns all items"
  verify unit "filter=active returns only completed=false"
  verify unit "filter=completed returns only completed=true"
}

// ── Events ────────────────────────────────────────────────

event todo_created "Todo Created" {
  payload TodoItem
  channel "todos"
}

event todo_toggled "Todo Toggled" {
  payload TodoItem
  channel "todos"
}

event todo_deleted "Todo Deleted" {
  channel "todos"
}

// ── Invariants ────────────────────────────────────────────

invariant todo_title_not_empty "Todo Title Not Empty" {
  guarantee """
    A TodoItem MUST never have an empty or whitespace-only title.
  """
  risk medium

  verify property "no persisted item has an empty title"
}

invariant todo_id_uniqueness "Todo ID Uniqueness" {
  guarantee """
    Every TodoItem MUST have a globally unique id.
  """
  risk high

  verify property "no two items share the same id"
}

// ── Features ──────────────────────────────────────────────

feature todo_management "Todo Management" {
  problem """
    Users need to track tasks with the ability to add, complete,
    and remove items.
  """

  solution """
    CRUD operations on TodoItem entities with a filter for viewing
    active vs completed items. Events enable real-time UI updates.
  """
}
```

## Entity summary

| Kind | Count | Entities |
|------|-------|----------|
| **type** | 3 | `TodoItem`, `TodoFilter`, `TodoError` |
| **port** | 2 | `todo_repository` (outbound), `todo_api` (inbound) |
| **behavior** | 4 | `create_todo`, `toggle_todo`, `delete_todo`, `list_todos` |
| **event** | 3 | `todo_created`, `todo_toggled`, `todo_deleted` |
| **invariant** | 2 | `todo_title_not_empty`, `todo_id_uniqueness` |
| **feature** | 1 | `todo_management` |

## Graph edges produced

```
create_todo ──BehaviorImplementsFeature──> todo_management
toggle_todo ──BehaviorImplementsFeature──> todo_management
delete_todo ──BehaviorImplementsFeature──> todo_management
list_todos ──BehaviorImplementsFeature──> todo_management

create_todo ──Produces──> todo_created
toggle_todo ──Produces──> todo_toggled
delete_todo ──Produces──> todo_deleted

list_todos ──BehaviorConsumesEvent──> todo_created

create_todo ──UsesType──> TodoItem, TodoError
toggle_todo ──UsesType──> TodoItem, TodoError
delete_todo ──UsesType──> TodoItem, TodoError
list_todos  ──UsesType──> TodoItem, TodoFilter

todo_created ──UsesType──> TodoItem (payload)
todo_toggled ──UsesType──> TodoItem (payload)

create_todo ──UsesPort──> todo_api, todo_repository
toggle_todo ──UsesPort──> todo_api, todo_repository
delete_todo ──UsesPort──> todo_api, todo_repository
list_todos  ──UsesPort──> todo_api, todo_repository

create_todo ──BehaviorEnforcesInvariant──> todo_title_not_empty, todo_id_uniqueness
```

## What an AI agent sees

Running `specforge export --format=context` produces a structured JSON graph
with all entities, edges, contracts, and verify statements. An agent consuming
this context can generate:

- **Implementation code** — repository interface, domain logic, API handlers
- **Tests** — one test per `verify` statement, with `requires`/`ensures` as assertions
- **API docs** — from port method signatures and behavior contracts (transport details like HTTP verbs/routes belong in the adapter implementation, not the spec)
- **Architecture diagrams** — from the edge graph via `specforge export --format=dot`

The `requires`/`ensures` blocks use Design by Contract (progressive formality
Level 2 per RES-25). This is optional — removing them still leaves a valid spec
with prose contracts and verify statements.
