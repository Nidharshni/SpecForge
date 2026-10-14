# SpecForge Best Practices

Prescriptive guidance for writing high-value `.spec` files, plus the named anti-patterns to
avoid. This assumes you've worked through the
**[zero-to-hero tutorial](authoring-spec-files.md)**.

The throughline: **the graph is the product, and validation is the value.** A spec is good
in proportion to how well-connected and how validated it is — not how much prose it
contains.

---

## Best practices

### 1. Maximize edges

**Do:** fill in every reference field that legitimately applies — `invariants`, `types`,
`ports`, `produces`, `consumes`, `features`.

**Why:** edges are what an agent traverses. A behavior with five edges gives an agent a rich
neighborhood; a behavior with a prose contract and no edges gives it a sentence. Two specs
with identical prose but different edge density produce very different agent accuracy.

```spec
// ✅ rich — the agent can traverse to types, ports, events, guarantees
behavior create_task "Create a task" {
  category   command
  types      [Task, CreateTaskCommand, InvalidTitleError]
  ports      [TaskRepository]
  produces   [task_created]
  invariants [task_id_uniqueness]
  features   [task_management]
  contract   "…"
  verify unit "…"
}
```

### 2. Write contracts in RFC 2119

**Do:** use MUST / MUST NOT / SHOULD / MAY in `contract` and `guarantee` text.

**Why:** these words have precise, shared meaning for humans *and* agents. "The system
should probably validate" is ambiguous; "the system MUST reject an empty title" is testable.

### 3. One concept per entity

**Do:** split "create a task and notify the assignee" into a `create_task` behavior and a
`notify_assignee` handler.

**Why:** small entities are individually testable, independently traceable, and
reusable. Big ones can't be verified precisely and bloat every query that touches them. See
the [God-entity anti-pattern](#the-god-entity).

### 4. Always declare at least one `verify`

**Do:** add a `verify` line to every behavior, invariant, and port.

**Why:** the `verify` declaration is the *intent* layer of traceability. A testable entity
with no `verify` raises a coverage warning (e.g. `W004`) and gives agents no signal about how
to validate the work.

### 5. Use enum values exactly

**Do:** check the allowed set before inventing a status/priority/severity. (See the
[quick reference](../quick-reference.md) or the tutorial's entity tour.)

**Why:** a value outside a field's vocabulary is an error (`E061`) and degrades the
graph's queryability. `status wip` is not the same node-state as `status in_progress` to a
consumer.

### 6. Link, don't duplicate

**Do:** reference a shared `type` or `invariant` from many behaviors instead of restating it.

**Why:** the graph deduplicates meaning. One `Task` type referenced by ten behaviors is one
source of truth; ten inline restatements drift apart and can't be traced.

### 7. Name for humans, identify for machines

**Do:** use clear, consistent identifiers (`create_task`, not `ct` or `BEH-001`) and add a
title. Pick one casing convention — `snake_case` for behaviors/events/invariants,
`PascalCase` for types/ports is a common split.

**Why:** identifiers are the graph's primary keys; they appear in every query an agent reads.
Sequential prefixes (`BEH-001`) are obsolete in SpecForge and carry no meaning.

### 8. Organize files by kind, wire with `use`

**Do:** `spec/types/`, `spec/behaviors/`, `spec/invariants/`, … and import across them.

**Why:** predictable layout makes specs navigable, keeps imports explicit, and mirrors how
the tutorial and the [todo-app example](../../examples/todo-app/) are structured.

### 9. Adopt progressively

**Do:** start with one feature and its behaviors. Expand where the pain is.

**Why:** [Principle 1](../vision/principles.md) — "structure is a spectrum." You never
need comprehensive coverage to get value. One connected behavior already beats a page of
prose.

### 10. Treat the compiler as a pair programmer

**Do:** run `specforge check` constantly; fix warnings, don't suppress them; read the
info-level advice it always reports (`--severity info` shows only it) periodically; use
`--strict` in CI.

**Why:** every diagnostic fixed at author time is a round-trip saved with an agent later.

---

## Anti-patterns

Each is named so you can recognize and discuss it.

### The God entity

**Looks like:** one behavior whose contract describes a whole workflow ("create the task,
validate the assignee, send an email, update the dashboard, and log an audit event").

**Why it's wrong:** it can't be verified precisely, every query that touches it drags in
unrelated concerns, and it hides the real edges (which event? which port?).

**Do instead:** one behavior per operation, each with its own contract, edges, and `verify`.
Compose them via a feature or an event chain.

### The unreferenced node

**Looks like:** a `type`, `feature`, or `invariant` that nothing references — flagged by
`W002`, `W008`, `W003`, `W041`, `I010`, etc.

**Why it's wrong:** an unconnected node is invisible to graph traversal, so it adds zero
value to an agent and usually signals a missing edge.

**Do instead:** connect it (a behavior should reference the type; a behavior should implement
the feature) — or delete it. An unreferenced node is either a missing edge or dead weight.

### The prose-only behavior

**Looks like:** `behavior do_stuff { contract "handles the user thing" }` — no `category`,
no `types`/`ports`/`produces`, no `verify`.

**Why it's wrong:** it's a comment with extra syntax. It triggers `W006` (no category) and
coverage warnings, and gives an agent nothing to traverse — defeating the entire point.

**Do instead:** add the category, the reference edges, and at least one `verify`. If you
don't know the edges yet, that's a signal the design is unfinished.

### Stringly-typed status

**Looks like:** `status wip`, `priority p1`, `severity 8` — values outside the allowed enum.

**Why it's wrong:** consumers can't reason about an unknown state; it raises enum-constraint
warnings and breaks status-based queries and rollups.

**Do instead:** use the canonical enum value (`in_progress`, `high`, `high`). If the enum
genuinely lacks a state you need, that's an extension concern — don't smuggle it in as a
string.

### Quoted references (the silent disconnect)

**Looks like:** `invariants ["task_id_uniqueness"]` — a *string list* where a *reference
list* was meant.

**Why it's wrong:** quoting turns a compiler-checked edge into an opaque string. No edge is
created, no error is raised, and the graph is silently poorer.

**Do instead:** drop the quotes for entity references: `invariants [task_id_uniqueness]`.
Reserve quotes for opaque values (`acceptance ["…"]`, `refs ["gh.issue:42"]`).

### Reinventing vocabulary

**Looks like:** modeling a domain concept as a generic `type` with a stringly-typed `kind`
field because "there's no entity for it."

**Why it's wrong:** you lose validation, edges, and queryability that a real entity kind
provides.

**Do instead:** use the right builtin entity, or — if the concept is genuinely new to a
domain — author an extension. [Principle 7](../vision/principles.md): extensions over
built-ins, always.

---

**See also:** [tutorial](authoring-spec-files.md) · [cookbook](spec-cookbook.md) ·
[troubleshooting](spec-troubleshooting.md) · [vision principles](../vision/principles.md).
