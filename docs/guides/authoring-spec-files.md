# Authoring `.spec` Files — Zero to Hero

A guided, end-to-end tutorial that takes you from never having seen a `.spec` file to
authoring a complete, validated specification an AI agent can consume.

This is the **tutorial**. Three companion guides go deeper once you've finished:

- **[Cookbook](spec-cookbook.md)** — task-oriented recipes ("how do I model X?").
- **[Best Practices](spec-best-practices.md)** — principles and named anti-patterns.
- **[Troubleshooting](spec-troubleshooting.md)** — diagnostic codes and how to fix them.

And for looking things up: **[Quick Reference](../quick-reference.md)** (cheat sheet) and
**[Entity Model](../entity-model.md)** (full field/edge/diagnostic reference).

> 💡 **Every code block in this guide compiles.** The worked example lives at
> [`examples/todo-app/`](../../examples/todo-app/) — clone it and run `specforge check`.

---

## Table of contents

- [Act I — Orientation](#act-i--orientation)
  - [What SpecForge is (and is not)](#what-specforge-is-and-is-not)
  - [The mental model: the graph is the product](#the-mental-model-the-graph-is-the-product)
  - [The five-minute win](#the-five-minute-win)
- [Act II — Core concepts](#act-ii--core-concepts)
  - [Anatomy of a `.spec` file](#anatomy-of-a-spec-file)
  - [Your first real entity, end to end](#your-first-real-entity-end-to-end)
  - [The edit → check → fix loop](#the-edit--check--fix-loop)
  - [Cross-linking: turning entities into a graph](#cross-linking-turning-entities-into-a-graph)
- [Act III — The entity tour](#act-iii--the-entity-tour)
  - [Which entity should I use?](#which-entity-should-i-use)
  - [`@specforge/software`](#specforgesoftware)
  - [`@specforge/product`](#specforgeproduct)
  - [`@specforge/governance`](#specforgegovernance)
  - [`@specforge/formal`](#specforgeformal)
- [Act IV — The worked project](#act-iv--the-worked-project)
- [Act V — Mastery](#act-v--mastery)

---

# Act I — Orientation

## What SpecForge is (and is not)

SpecForge compiles `.spec` files into a **validated, typed entity graph** that AI agents
and tools consume instead of re-reading your whole codebase. You write structured
declarations of intent — behaviors, types, features, guarantees — and the compiler checks
that they're consistent, then exports them as an open JSON schema (the *Graph Protocol*).

If you've used these tools, the analogy helps — but note the difference:

| Like… | …in that | Unlike, because SpecForge… |
|-------|----------|----------------------------|
| **Gherkin/Cucumber** | structured, human-readable intent | builds a *typed graph*; tests stay in your test runner |
| **TypeSpec / Protobuf** | a compiled schema language | spans requirements → behavior → architecture, not just APIs |
| **Terraform HCL** | declarative, validated, version-controlled | describes *intent for agents*, not infrastructure to apply |

**SpecForge is NOT:**

- ❌ a code generator — agents produce code; SpecForge gives them context.
- ❌ a test framework — it *traces* tests and consumes their results. The compiler never executes anything; `specforge collect` runs your runner's declared command only with your consent.
- ❌ a documentation format — the value is the *compiler*, not prose with conventions.

Here's the smallest meaningful `.spec` file. You'll understand every line by the end of
Act II:

```spec
spec "hello" {
  version "0.1.0"
}

behavior greet_user "Greet a user by name" {
  contract "Given a name, the system MUST return a personalized greeting."
  verify unit "returns 'Hello, Ada' for 'Ada'"
}
```

## The mental model: the graph is the product

This is the single most important idea in the guide. Internalize it and everything else
follows.

> 💡 **Mental model.** You are not writing documents. You are declaring **nodes** (entities
> like a behavior or a type) and **edges** (references between them, like a behavior that
> *enforces* an invariant). The compiler assembles these into a graph, validates it, and
> exports it. **An agent's accuracy is proportional to how well-connected that graph is.**

From the project's [principles](../vision/principles.md):

> **Structure is a spectrum, not a binary.** "One entity in a `.spec` file is better than
> zero. […] A single behavior with two verify declarations already gives an AI agent more
> to work with than a page of prose." You never need full coverage to get value.

> **Validation is the value.** "What separates SpecForge from a fancy comment format is the
> compiler." It catches dangling references, unreferenced entities, missing test coverage, cycles, and
> contradictions — every error caught at compile time is a round-trip saved with an agent.

The practical consequence, which we'll repeat throughout: **maximize edges.** A behavior
with `invariants`, `types`, `ports`, and `produces` filled in is far more valuable than one
with only a prose `contract`. The references *are* the product.

## The five-minute win

> ⏱️ **~5 minutes.** Goal: install, create a project, and see a validated graph. Don't try
> to understand everything yet — just get green output.

```bash
# 1. Create and enter a project directory
mkdir hello-spec && cd hello-spec

# 2. Scaffold a project (interactive extension selection; pick @specforge/software)
specforge init

# 3. Validate the starter spec
specforge check
#   => 0 errors

# 4. Export the graph an agent would consume
specforge export --format=context
```

`specforge init` writes a `specforge.json` config and a starter `spec/hello.spec`.
`specforge check` validates it. `specforge export` emits the graph. Three commands, and you
have a validated specification.

> ✅ **Checkpoint:** you have a project that compiles. Now let's understand what you wrote.

---

# Act II — Core concepts

## Anatomy of a `.spec` file

Every entity in SpecForge has the same universal shape. The compiler itself knows *no*
domain words — `behavior`, `type`, etc. all come from extensions — but they all parse as:

```spec
keyword  name  "Optional Title"  {
  field  value
}
```

Annotated:

```spec
behavior greet_user "Greet a user by name" {
//  │        │            │
//  │        │            └─ optional title (string); auto-derived from name if omitted
//  │        └─ entity name: letters/digits/underscores, 2–60 chars, globally unique
//  └─ keyword: which kind of entity (contributed by an installed extension)

  contract "Given a name, the system MUST return a greeting."
//  │        └─ a string value
//  └─ field key

  verify unit "returns a greeting"
//  └─ a verify declaration: kind + description
}
```

**Field value kinds** you'll use constantly:

```spec
behavior field_value_kinds "Field Value Kinds" {
  // Field value kinds you'll use constantly:

  status   draft                      // bare: a word from the field's set
  category auth                       // bare: an identifier field (a tag)

  contract """                        // triple-quoted multi-line string
    Multi-line text.
    Indentation is stripped sensibly.
  """

  invariants [data_persistence]       // reference list → creates graph EDGES (compiler-checked)
  acceptance ["fast", "secure"]       // string list → opaque values (not checked)
  refs       [gh.issue:42]            // a scheme ref ID links its `ref` entity
}
```

> 💡 The field's **declared type** decides both what a value references and how it is written:
> words and references bare, text quoted. `specforge format` writes every value so, and
> `specforge check` warns (W159) on the other form (ADR 0147).
> Every item of a `reference_list` field (and the value of a `reference` field) must resolve to
> a real entity or you get an error. A `string_list` field's items are opaque
> quoted strings; only a scheme ref ID (`gh.issue:42`) links its `ref`.
> Reference fields build the graph; string lists don't.

**Comments** are line-only (`//`). **Imports** wire files together:

```spec
use "types/user"                     // import a file (the .spec extension is implicit)
use { User, UserRole } from "types/user"   // selective import
```

## Your first real entity, end to end

Let's build something real and watch the compiler help us. Create `spec/tasks.spec`:

```spec
spec "first-steps" {
  version "0.1.0"
}

type Task "A unit of work" {
  id    string @readonly @unique
  title string
}

behavior create_task "Create a task" {
  category command
  types    [Task]
  contract "Given a title, the system MUST persist a Task with a unique id."
  verify unit "a created task has a non-empty id"
}
```

Run it:

```bash
specforge check
#   => 0 errors
```

Notice the edge: `create_task` lists `types [Task]`, which creates a
`BehaviorReferencesType` edge. The graph now *knows* this behavior operates on `Task`.

## The edit → check → fix loop

The core skill in SpecForge isn't memorizing fields — it's reading diagnostics and
iterating. Let's practice deliberately. **Break it on purpose:** change the behavior's
`types` to reference a type that doesn't exist:

```spec
behavior create_task "Create a task" {
  category command
  types    [Tsak]          // typo!
  contract "..."
  verify unit "..."
}
```

```bash
specforge check
```
```
[E003] Error: unresolved reference 'Tsak' in entity 'create_task'
   ╭─[ tasks.spec ]
   │  types    [Tsak]
   │           ──┬─
   │             ╰── 'Tsak' does not resolve to any entity
```

Read it like a pair programmer: **code** (`E003`), **message** (unresolved reference),
**span** (which line). Fix the typo, re-run, get `0 errors`. That loop —
*edit → check → read → fix* — is how you author specs.

> 💡 Run `specforge explain E003` for a full description of any diagnostic code. The
> compiler reports **all** errors at once (it doesn't stop at the first), so you can fix in
> batches. (`E001` is a parse/syntax error; `E003` is an unresolved reference.)

**Severity levels:**

| Level | Symbol | Meaning |
|-------|--------|---------|
| Error | `E003` | Blocks compilation. Must fix. |
| Warning | `W001` | A likely problem (unreferenced, missing coverage). Doesn't block. |
| Info | `I010` | Advice. Always reported; `--severity info` shows only it. |

Use `specforge check --strict` in CI to treat warnings as errors.

## Cross-linking: turning entities into a graph

A pile of unconnected entities is barely better than prose. The leap to *graph* happens
when entities reference each other. Let's add a guarantee and connect it:

```spec
invariant task_id_uniqueness "Task ID Uniqueness" {
  guarantee "No two tasks may share the same id."
  risk medium
  verify property "concurrent creation never produces duplicate ids"
}

behavior create_task "Create a task" {
  category   command
  types      [Task]
  invariants [task_id_uniqueness]   // ← new edge: behavior ENFORCES invariant
  contract   "Given a title, the system MUST persist a Task with a unique id."
  verify unit "a created task has a non-empty id"
}
```

Now `specforge trace task_id_uniqueness` shows the behavior that upholds it, and
`specforge query create_task --depth 1` returns the behavior *plus* its type and invariant.
That neighborhood — delivered in a few KB — is what an agent reads instead of grepping your
repo.

> ✅ **Checkpoint:** you can author linked entities and self-correct from diagnostics.
> You're past "config file" and into "graph." Now let's meet the full vocabulary.

---

# Act III — The entity tour

Vocabulary comes from **extensions**. Four ship as builtins. This tour goes deep on the two
you'll start with — `@specforge/software` and `@specforge/product` — and gives you a
complete-but-brief look at `@specforge/governance` and `@specforge/formal`.

## Which entity should I use?

| You're describing… | Use | Extension |
|--------------------|-----|-----------|
| A single operation's contract | **behavior** | software |
| A data shape (struct / union / error) | **type** | software |
| Something that happened (past tense) | **event** | software |
| An interface boundary / dependency | **port** | software |
| A rule that must always hold | **invariant** | software |
| A user-facing capability | **feature** | product |
| A UX flow (persona × channel → features) | **journey** | product |
| A kind of user | **persona** | product |
| A delivery surface (web, cli, api) | **channel** | product |
| A code package | **module** | product |
| A planning phase | **milestone** | product |
| A shippable artifact | **deliverable** | product |
| A versioned bundle | **release** | product |
| A glossary definition | **term** | product |
| Why a technical choice was made | **decision** | governance |
| A non-functional requirement | **constraint** | governance |
| A risk (FMEA) | **failure_mode** | governance |
| A temporal/safety property | **property** | formal |
| An assumed-true foundation | **axiom** | formal |
| A multi-party protocol | **protocol** | formal |
| Abstract→concrete mapping | **refinement** | formal |
| A communicating process | **process** | formal |

> 💡 **One concept per entity.** "Create a task and email the user" is *two* behaviors. If
> you're tempted to cram, split instead — see the [God-entity anti-pattern](spec-best-practices.md#anti-patterns).

Each subsection below follows the same shape: **use when → minimal example → key fields →
✅/❌ → links to.**

## `@specforge/software`

### behavior

**Use when:** describing one operation the system performs.

```spec
behavior create_task "Create a task" {
  category   command
  invariants [task_id_uniqueness]
  types      [Task, CreateTaskCommand, InvalidTitleError]
  ports      [TaskRepository]
  produces   [task_created]
  contract """
    When a CreateTaskCommand is received, the system MUST reject an empty title
    with InvalidTitleError, otherwise persist a Task with a unique id and emit
    task_created. It MUST return Result<Task, InvalidTitleError>.
  """
  verify unit        "empty title is rejected"
  verify integration "created task is retrievable"
}
```

**Key fields:** `contract` (the heart — use RFC 2119 MUST/SHOULD/MAY); `category` (routing
tag); and the edge-creating lists `invariants`, `types`, `ports`, `produces`, `consumes`,
`features`.

✅ **Good** — one operation, RFC 2119, edges filled in (above).
❌ **Bad** — `contract "handles tasks"` with no references. Vague, edgeless, low value.

**Links to:** invariant, type, port, event, feature.

### type

**Use when:** defining a data shape. Kind is inferred from syntax: struct `{ … }`, union
`= a | b`, error (tagged with `_tag @literal`). Annotations: `@readonly`, `@unique`,
`@optional`, `@literal`. Arrays: `Item[]`.

```spec
type Task {
  id     string @readonly @unique
  title  string
  status TaskStatus
}

type TaskStatus = open | in_progress | done | archived

type TaskNotFoundError {
  _tag   "TaskNotFoundError" @literal
  taskId string
}
```

✅ Use `_tag` + `@literal` for discriminated unions; `@readonly`/`@unique` for identity.
❌ Don't put operations on a type — those belong on a **port**.

**Links to:** other types (`composed_types`, `extends`).

### event

**Use when:** recording that something happened. Name in **past tense**. Payload is a
**type reference**, not inline fields.

```spec
event task_created "Task Created" {
  channel  "tasks.created"
  payload  TaskCreatedPayload
  category domain
  verify integration "emitted after a successful create"
}
```

✅ Past-tense name, payload references a type.
❌ `event create_task` (imperative) or inline payload fields.

**Links to:** type (payload). Produced/consumed *by behaviors* (the edge lives on the
behavior's `produces`/`consumes`).

### port

**Use when:** defining an interface boundary (hexagonal architecture). `direction` is
**required**: `inbound`, `outbound`, or `bidirectional`. Methods return `Result`.

```spec
port TaskRepository "Persistence boundary for tasks" {
  direction outbound
  category  "persistence/task"
  method create(cmd: CreateTaskCommand) -> Result<Task, InvalidTitleError>
  method findById(id: string)           -> Result<Task, TaskNotFoundError>
  method list(status: TaskStatus)       -> Result<Task[], never>
  verify integration "contract satisfied by the SQLite adapter"
}
```

Conventions: `{Entity}Repository` (persistence), `{Service}Gateway` (external), `{Domain}API`
(inbound).

**Links to:** used by behaviors (`ports [...]`).

### invariant

**Use when:** stating a rule the system must *never* violate. Fields: `guarantee`
(required), `risk` (`low`/`medium`/`high`/`critical`), `description`, `refs`.

```spec
invariant task_id_uniqueness "Task ID Uniqueness" {
  guarantee "No two tasks may share the same id."
  risk medium
  verify property "concurrent creation never produces duplicate ids"
}
```

> ⚠️ The builtin `invariant` has **no** `enforced_by` or `maintains` field. The link to a
> behavior lives on the *behavior* (`invariants [...]`); `enforced_by` is just the name of
> the inverse edge. (`maintains`/`requires`/`ensures` are added to *behaviors* by
> `@specforge/formal`.)

**Invariant vs. its neighbors:** "users can log in" is a *behavior*; "we use PostgreSQL" is
a *decision*; "p99 < 200ms" is a *constraint*. An invariant is falsifiable, universal, and
implementation-independent.

## `@specforge/product`

> Several product kinds require fields: `persona`/`channel` require `description`, `feature`
> requires `problem`, `journey` requires `flow`, `term` requires `definition`.

### feature

**Use when:** describing a user-facing capability. Behaviors point *up* to features.

```spec
feature task_management "Task Management" {
  status   in_progress              // proposed | accepted | in_progress | done | deferred | deprecated
  priority high                     // critical | high | medium | low
  effort   m                        // xs | s | m | l | xl
  problem  "Users need to capture, track, and complete work items."
  solution "Create, complete, and list tasks backed by a durable repository."
  acceptance ["A user can create a task", "A user can mark a task done"]
}
```

✅ `problem` framed from the user's perspective; `solution` from the system's.
❌ Inventing a status like `wip` — only the enum values above are valid (else `E061`).

**Links to:** behaviors implement it; journeys/modules/milestones/personas reference it.

### journey

**Use when:** mapping a persona on a channel through features. `flow` (required) is an
ordered list of steps.

```spec
journey capture_and_complete "Capture and complete a task" {
  persona  individual
  channels [cli]
  features [task_management]
  priority high
  flow [
    "User runs `todo add \"Buy milk\"`",
    "System creates the task and prints its id",
    "User runs `todo done <id>` and the task is marked done",
  ]
}
```

**Links to:** persona, channel, feature.

### persona & channel

```spec
persona individual "Individual user" {
  description     "A person managing their own personal to-do list."
  technical_level beginner
  status          active            // active | deprecated
  goals       ["Capture tasks quickly"]
  key_features [task_management]
}

channel cli "Command-line interface" {
  description       "A terminal CLI for managing tasks."
  interaction_model sync
  status            active
}
```

### module, milestone, deliverable

```spec
module core "todo-core" {
  family   core
  features [task_management]
}

milestone mvp "Minimum Viable Product" {
  status        in_progress         // planned | in_progress | completed | blocked
  features      [task_management]
  modules       [core]
  exit_criteria ["create/complete/list verified", "zero E-level diagnostics"]
}

deliverable todo_cli "todo CLI" {
  artifact_type cli                  // cli|service|library|web_app|mobile_app|api|extension|documentation|package
  status        in_progress          // draft | in_progress | shipped | deprecated
  journeys      [capture_and_complete]
  modules       [core]
  milestones    [mvp]
}
```

> 💡 `depends_on` lists on `module`, `milestone`, `deliverable`, and `release` are
> **DAG-validated** — a reference cycle is reported as `W061`.

### term

```spec
term task "task" {
  definition "A unit of work the user wants to track."
  aliases  ["to-do", "item"]
  see_also [done]                    // see_also targets OTHER TERMS only
}
```

### release

A versioned bundle of deliverables/milestones. `version` is required and must be semver
(else `W093`). Covered in the [Cookbook](spec-cookbook.md).

## `@specforge/governance`

Three kinds for architecture governance. (Concise coverage — one example each.)

```spec
decision sqlite_storage "SQLite for local storage" {
  status accepted                    // proposed | accepted | deprecated | superseded
  context "Single-user, local-first CLI needs durable storage without a server."
  decision "Use an embedded SQLite database file."
  consequences ["Zero-config persistence", "No multi-writer support"]
  invariants [task_id_uniqueness]    // links to the invariant it protects
}

constraint create_latency "Task creation is fast" {
  category    performance
  priority    medium                 // critical | high | medium | low
  description "Creating a task must feel instant on the CLI."
  metric      "create_task p99 < 50ms on local SSD"
  constrains  [create_task]
  verify load "benchmark create_task, assert p99 < 50ms"
}

failure_mode lost_task "Task acknowledged but not persisted" {
  invariant  task_id_uniqueness
  severity   high                    // critical | high | medium | low
  occurrence rare                    // certain | likely | occasional | unlikely | rare
  detection  moderate                // certain | likely | moderate | unlikely | undetectable
  cause      "Process killed before disk flush"
  effect     "User believes a task was saved but it disappears"
  mitigation "Write within a transaction; flush before acknowledging"
}
```

> ⚠️ FMEA scores are **enum words**, not 1–10 numbers (another value is `E061`).

## `@specforge/formal`

Five kinds for formal methods; requires `@specforge/software` as a peer and *enhances* its
entities (adds `requires`/`ensures`/`maintains`/`satisfies` to behaviors).

```spec
property no_lost_completion "Completion is never lost" {
  property_type safety               // safety | liveness | fairness
  expression "Once a task is `done`, it never silently reverts without a reopen."
  scope "task lifecycle"
}

// a behavior then declares it satisfies the property:
behavior complete_task "Complete a task" {
  // …
  satisfies [no_lost_completion]
}
```

The other four — `axiom`, `protocol`, `refinement`, `process` — are summarized in the
[Quick Reference](../quick-reference.md). A `refinement` example is in the
[Cookbook](spec-cookbook.md).

> ✅ **Checkpoint:** you know the full vocabulary and can pick the right tool. Time to build
> something complete.

---

# Act IV — The worked project

> ⏱️ **~20 minutes.** We build the **todo-app** from scratch, growing the spec one layer at
> a time and validating at each step. The finished project is at
> [`examples/todo-app/`](../../examples/todo-app/) — clone it to follow along or check your
> work.

We'll author in the order recommended by [spec-writing-flow.md](../spec-writing-flow.md):
**foundation → data → guarantees → boundaries → behavior → product → governance**.

### Step 1 — Foundation

`specforge.json` declares the extensions; `spec/main.spec` holds the singleton block:

```json
{ "name": "todo-app", "version": "0.1.0", "spec_root": "spec",
  "extensions": ["@specforge/software", "@specforge/product",
                 "@specforge/governance", "@specforge/formal"] }
```
```spec
spec "todo-app" { version "0.1.0" }
```
```bash
specforge check     # => 0 errors (an empty-but-valid project)
```

### Step 2 — Data (`spec/types/task.spec`)

Define the `Task`, its `TaskStatus` union, command payloads, error types, and event
payloads. (See the file in the example.) After adding it, `specforge check` reports
warnings like `W002` ("type not referenced by any behavior") — **expected**, because nothing
uses these types yet. The warnings will clear as we add behaviors. This is the graph telling
you about dangling nodes.

### Step 3 — Guarantees (`spec/invariants/task.spec`)

```spec
invariant task_id_uniqueness "Task ID Uniqueness" {
  guarantee "No two tasks may share the same id."
  risk medium
  verify property "concurrent creation never produces duplicate ids"
}
```

### Step 4 — Boundaries (`spec/ports/task_repository.spec`) and events

Add the `TaskRepository` outbound port and the `task_created` / `task_completed` events
(each importing the types it needs with `use`).

### Step 5 — Behavior (`spec/behaviors/task.spec`)

This is where it all connects. Each behavior imports the files it references and fills in
edges:

```spec
use "types/task"
use "invariants/task"
use "ports/task_repository"
use "events/task"
use "features/task_management"

behavior create_task "Create a Task" {
  category   command
  invariants [task_id_uniqueness]
  types      [Task, CreateTaskCommand, InvalidTitleError]
  ports      [TaskRepository]
  produces   [task_created]
  features   [task_management]
  contract """
    When a CreateTaskCommand is received, the system MUST reject an empty title
    with InvalidTitleError, otherwise persist a Task with a unique id and emit
    task_created. It MUST return Result<Task, InvalidTitleError>.
  """
  verify unit        "empty title is rejected"
  verify integration "created task is retrievable by id"
}
```

Once behaviors reference the types, ports, events, and invariants, the earlier `W002`
unreferenced warnings disappear. **The graph is now connected.**

### Step 6 — Product layer

Add the `feature`, `persona`, `channel`, `journey`, `module`, `milestone`, `deliverable`,
and `term` (see `spec/features/` and `spec/product/` in the example). The behaviors already
point up to `task_management` via `features [...]`, so the feature isn't an unreferenced.

### Step 7 — Governance & formal taste

Add the `decision`, `constraint`, `failure_mode`, and a `property` (wired to `complete_task`
via `satisfies`). Final validation:

```bash
specforge check
#   => 0 errors, 0 warnings, 0 infos
```

### See what an agent sees

```bash
specforge stats
#   Entities: 30   Edges: 35   Verified: 9

specforge trace task_management
```
```json
{
  "entity_id": "task_management", "entity_kind": "feature",
  "upstream": [
    { "entity_id": "create_task",   "entity_kind": "behavior", "edge_label": "features" },
    { "entity_id": "capture_and_complete", "entity_kind": "journey", "edge_label": "features" },
    { "entity_id": "individual",    "entity_kind": "persona",  "edge_label": "key_features" },
    { "entity_id": "core",          "entity_kind": "module",   "edge_label": "features" }
  ]
}
```

From one feature, the graph reaches the behaviors that implement it, the journey that
exercises it, the persona who wants it, and the module that houses it — then onward to the
types, ports, events, and invariants below. That traceable neighborhood, exported as a few
KB of JSON, is what replaces an agent's expensive codebase exploration.

> ✅ **Checkpoint:** you've shipped a complete, validated, multi-entity spec project.

---

# Act V — Mastery

You can now author and validate real specs. To go from competent to expert:

- **[Best Practices](spec-best-practices.md)** — the prescriptive rules (maximize edges,
  RFC 2119, one-concept-per-entity, naming) and **named anti-patterns** (God entity, unreferenced
  node, prose-only behavior, stringly-typed status) with fixes.
- **[Cookbook](spec-cookbook.md)** — copy-paste recipes for common modeling tasks.
- **[Troubleshooting](spec-troubleshooting.md)** — every common diagnostic code, what
  triggers it, and how to fix it.
- **[spec-writing-flow.md](../spec-writing-flow.md)** — the phased authoring flow in depth.
- **[Quick Reference](../quick-reference.md)** & **[Entity Model](../entity-model.md)** —
  the full field/edge/diagnostic reference of record.
- The per-kind **skills** under `.claude/skills/specforge-*-dsl/` carry the deepest
  field-by-field guidance for each entity kind.

And the commands you'll lean on as you author:

```bash
specforge check --strict          # CI gate: warnings become errors
specforge check --severity error  # only the errors (the exit code still counts everything)
specforge check --severity info   # only the info-level advice
specforge explain W004            # what does a code mean?
specforge trace <id>              # traceability chain for an entity
specforge query <id> --depth 2    # neighborhood at a zoom level
specforge model                   # render the logical data model
specforge format                  # auto-format .spec files
```

Welcome to hero status. The most expensive token is the one spent discovering what should
have been specified — and now you know how to specify it.
