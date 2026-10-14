# SpecForge Cookbook

Task-oriented recipes for common modeling problems. Each recipe is a goal, a
copy-paste-ready snippet, and the result. For a guided introduction, start with the
**[zero-to-hero tutorial](authoring-spec-files.md)**; for principles, see
**[best practices](spec-best-practices.md)**.

> 💡 Every snippet here is validated against `specforge check`. Imports (`use "..."`) are
> shown where a snippet references entities defined elsewhere.

## Contents

- [Model a CRUD feature](#model-a-crud-feature)
- [Add an event-driven flow](#add-an-event-driven-flow)
- [Define an interface boundary (port)](#define-an-interface-boundary-port)
- [Express a non-functional requirement (constraint)](#express-a-non-functional-requirement-constraint)
- [Record an architecture decision (ADR)](#record-an-architecture-decision-adr)
- [Assess a risk (FMEA / failure_mode)](#assess-a-risk-fmea--failure_mode)
- [Map an abstract behavior to a concrete one (refinement)](#map-an-abstract-behavior-to-a-concrete-one-refinement)
- [Declare a foundational assumption (axiom)](#declare-a-foundational-assumption-axiom)
- [Model a synchronization protocol](#model-a-synchronization-protocol)
- [Organize a multi-file project with imports](#organize-a-multi-file-project-with-imports)
- [Add an extension](#add-an-extension)
- [Wire up traceability (verify → tests → results)](#wire-up-traceability-verify--tests--results)

---

## Model a CRUD feature

**Goal:** capture a create/read/update/delete capability as a feature plus behaviors.

Group the operations under one **feature**, and write one **behavior** per operation
(one concept per entity). Behaviors point *up* to the feature via `features [...]`.

```spec
use "features/task_management"

behavior create_task "Create a task" {
  category command
  features [task_management]
  contract "Given a valid title, the system MUST persist a Task and return it."
  verify unit "empty title rejected"
}

behavior delete_task "Delete a task" {
  category command
  features [task_management]
  contract "Given a task id, the system MUST remove the Task or return TaskNotFoundError."
  verify unit "deleting an unknown id returns TaskNotFoundError"
}
```

> Use `category command` for state-changing operations and `category query` for reads. The
> category is a routing hint agents use to pick the right operation.

---

## Add an event-driven flow

**Goal:** model one behavior emitting an event that another consumes.

The producer lists `produces`; the consumer lists `consumes`. The event itself stays
neutral — the direction lives on the behaviors.

```spec
use "events/task"

behavior complete_task "Complete a task" {
  category command
  produces [task_completed]            // ← emits
  contract "Marks a task done and emits task_completed."
  verify unit "status becomes done"
}

behavior send_completion_email "Notify on completion" {
  category handler
  consumes [task_completed]            // ← reacts
  contract "On task_completed, the system SHOULD send a confirmation email."
  verify integration "an email is queued when task_completed fires"
}
```

```spec
// events/task.spec
event task_completed "Task Completed" {
  channel "tasks.completed"
  payload TaskCompletedPayload
  verify integration "emitted once per completion"
}
```

---

## Define an interface boundary (port)

**Goal:** declare a dependency the system talks to (database, external service, API).

```spec
use "types/task"

port TaskRepository "Persistence boundary for tasks" {
  direction outbound                   // outbound = the system depends on it
  category  "persistence/task"
  method create(cmd: CreateTaskCommand) -> Result<Task, InvalidTitleError>
  method findById(id: string)           -> Result<Task, TaskNotFoundError>
  method list(status: TaskStatus)       -> Result<Task[], never>
  verify integration "adapter satisfies the contract"
}
```

Every method returns `Result<Success, Error>` (use `never` when it can't fail, `void` when
there's no value). Then a behavior declares it uses the port: `ports [TaskRepository]`.

---

## Express a non-functional requirement (constraint)

**Goal:** capture a measurable quality bar (latency, throughput, security).

```spec
use "behaviors/task"

constraint create_latency "Task creation is fast" {
  category    performance               // performance | security | reliability | ...
  priority    high                      // critical | high | medium | low
  description "Creating a task must feel instant on the CLI."
  metric      "create_task p99 < 50ms on local SSD"
  constrains  [create_task]             // links the behaviors it governs
  verify load "benchmark create_task over 10k iterations, assert p99 < 50ms"
}
```

> `description` is required. Use `constrains [...]` to attach the constraint to behaviors,
> and `protects [...]` to attach it to invariants.

---

## Record an architecture decision (ADR)

**Goal:** document *why* a technical choice was made, with consequences.

```spec
use "invariants/task"

decision sqlite_storage "SQLite for local storage" {
  status accepted                        // proposed | accepted | deprecated | superseded
  date   2026-01-15
  context "Single-user, local-first CLI needs durable storage without a server."
  decision "Use an embedded SQLite database file in the user's config directory."
  consequences [
    "Zero-config persistence",
    "No concurrent multi-writer support (acceptable for a single-user CLI)",
  ]
  invariants [task_id_uniqueness]        // the invariant this decision protects
}
```

When a decision is replaced, set the old one's `status superseded` and point the new one's
`superseded_by` at it.

---

## Assess a risk (FMEA / failure_mode)

**Goal:** record a failure scenario and its mitigation against an invariant.

```spec
use "invariants/task"

failure_mode lost_task "Task acknowledged but not persisted" {
  invariant  task_id_uniqueness
  severity   high                        // critical | high | medium | low
  occurrence rare                        // certain | likely | occasional | unlikely | rare
  detection  moderate                    // certain | likely | moderate | unlikely | undetectable
  cause      "Process killed between in-memory insert and disk flush"
  effect     "User believes a task was saved but it disappears on restart"
  mitigation "Write within a SQLite transaction; flush before acknowledging"
}
```

> ⚠️ `severity`, `occurrence`, and `detection` take **enum words**, not 1–10 numbers.
> Another value is `E061`.

---

## Map an abstract behavior to a concrete one (refinement)

**Goal:** (formal) link a high-level behavior to its concrete implementation.

```spec
use "behaviors/task"

refinement create_to_sql "Abstract create → SQL" {
  abstract_entity  create_task          // the high-level behavior
  concrete_entity  create_task_sqlite   // the concrete one
  invariant_deltas ["adds: a UNIQUE index enforces task_id_uniqueness"]
}
```

Both `abstract_entity` and `concrete_entity` are required and must reference behaviors.

---

## Declare a foundational assumption (axiom)

**Goal:** (formal) state something assumed true that the system relies on.

```spec
use "invariants/task"

axiom clock_monotonic "Wall clock is monotonic" {
  expression    "The system clock never moves backward during a request."
  assumes       [task_id_uniqueness]
  justification "We rely on a monotonic clock source provided by the OS."
}
```

---

## Model a synchronization protocol

**Goal:** (formal) describe a multi-party message protocol as a state machine.

```spec
protocol task_sync "Task sync protocol" {
  alphabet      ["request", "ack", "commit"]
  states        ["idle", "pending", "committed"]
  initial_state "idle"
  transitions   ["idle -> request -> pending", "pending -> ack -> committed"]
}
```

`alphabet` and `initial_state` are required. Link an event to it with
`follows_protocol [task_sync]` on the event (a field `@specforge/formal` adds to events).

---

## Organize a multi-file project with imports

**Goal:** split a growing spec across files and wire them with `use`.

Organize **by entity kind**, then import what each file references:

```
spec/
  main.spec              # the spec { } block
  types/task.spec
  invariants/task.spec
  ports/task_repository.spec
  events/task.spec
  behaviors/task.spec    # imports the four above
```

```spec
// behaviors/task.spec
use "types/task"                         // full import (the .spec extension is implicit)
use { Task, CreateTaskCommand } from "types/task"   // selective import
use * as repo from "ports/task_repository"          // namespace import

behavior create_task "Create a task" {
  types [Task, CreateTaskCommand]
  contract "..."
  verify unit "..."
}
```

> ⚠️ Imports must form a DAG. A circular `use` chain raises `E003`. Forgetting to import a
> file whose entity you reference raises `E001` (unresolved reference).

---

## Add an extension

**Goal:** bring more vocabulary into a project.

```bash
specforge add @specforge/product       # adds journey, feature, milestone, …
specforge extensions                    # list what's installed
specforge remove @specforge/formal      # uninstall (use --force if depended upon)
```

This edits the `extensions` array in `specforge.json`. New entity kinds become available
immediately on the next `specforge check`.

---

## Wire up traceability (verify → tests → results)

**Goal:** connect a spec to the tests that prove it (the core value loop).

There are three layers:

1. **Intent** — `verify` declarations state *what* should be tested:
   ```spec
   behavior create_task "Create a task" {
     contract "..."
     verify unit        "empty title rejected"
     verify integration "created task is retrievable"
   }
   ```
2. **Linkage** — each test names the entity it proves, in its runner's idiom
   (see [ADR 0002](../adr/0002-test-runner-extensions.md)); the spec carries no test paths.
3. **Proof** — `specforge collect` runs each detected runner extension's
   test command (`@specforge/cargo-test` runs `cargo test`), once you've
   approved that command for the project, and records the results in
   `specforge-report.json`, which feeds coverage back into the graph.

```bash
specforge collect                        # detect runners, run them, record results
specforge collect --no-run               # record the runners' existing reports
specforge analyze coverage               # see which entities are proven
```

> The compiler never runs anything: runner extensions declare their command,
> and `collect` runs it on their behalf with your consent. See
> [principles §5](../../vision/principles.md) on traceability as a feedback loop.

---

**See also:** [tutorial](authoring-spec-files.md) · [best practices](spec-best-practices.md)
· [troubleshooting](spec-troubleshooting.md) · [quick reference](../quick-reference.md).
