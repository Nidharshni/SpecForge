# Quick Reference

Single-page lookup for the CLI, the configuration file, and every entity kind, edge type, and validation code (provided by the four builtin extensions). For full details, see the [entity model](entity-model.md) or individual [entity docs](entities/).

---

## CLI Commands

```bash
# Project lifecycle
specforge init                       # scaffold a new project
specforge check                      # validate .spec files
specforge check --strict             # promote warnings to errors
specforge check --severity error     # show only errors (the exit code still counts everything)
specforge check --cache              # record statuses in specforge-cache.json (history rules)

# Graph access (for agents and humans)
specforge export --format=context    # token-efficient context for an agent
specforge export --format markdown --scope <id> --depth 1   # a review document for people who sign
specforge query <id> --depth 2       # multi-resolution neighborhood query
specforge query <id> --format context --include-coverage   # an agent's slice with coverage
specforge trace <id>                 # traceability chain for an entity
specforge trace --plan plan.json     # check an agent plan (AgentPlan JSON) against the graph
specforge schema                     # emit the Graph Protocol schema
specforge model                      # render the logical data model
specforge outline                    # render the extension architecture
specforge stats                      # project statistics
specforge explore [<id>]             # starting points, hubs and unconnected entities
specforge review [<id>]              # coverage gaps around an entity (or the whole project)
specforge infer-guide [<kind>]       # what to look for in code to write a kind's entities

# Extensions & registry
specforge extensions                 # list enabled builtins and installed extensions
specforge add @specforge/product     # enable a builtin, or install @scope/name@version from a registry
specforge add <spec> <spec>...       # several at once: one change, all or nothing
specforge remove <name>              # disable a builtin, drop an entry, or uninstall an extension
specforge search <query>             # search registries
specforge publish                    # publish an extension to a registry
# There is no built-in registry: list yours under "registries" in specforge.json
# (see registry-trust.md); without one, add/update/search/publish fail with E063.

# Tooling
specforge format                     # format .spec files
specforge collect                    # run the test runners (with your approval) and record what they prove
specforge doctor                     # health-check installed extensions
specforge mcp                        # start the MCP server (stdio)
specforge explain E001               # explain a diagnostic code
```

## Configuration

Projects are configured via **`specforge.json`** (like `tsconfig.json`):

```json
{
  "name": "payments",
  "version": "0.1.0",
  "spec_root": "spec",
  "extensions": ["@specforge/software"]
}
```

`specforge init --extensions @specforge/software` creates this for you; `specforge add` / `specforge remove` edit the `extensions` list afterwards.

---

## Entities

### spec
> Module: core | ID: singleton | "What project is this?"

| Required | Optional |
|----------|----------|
| name, version, extensions | persona, surface — *(planned: not yet implemented)* providers, gen |

No graph edges. Root configuration declaring which extensions the project uses.

---

### invariant
> Module: core | ID: author-chosen identifier | "What must ALWAYS be true?"

| Required | Optional |
|----------|----------|
| title, guarantee | enforced_by, risk |

Outgoing: behavior (`enforces`)
Incoming: behavior (`references`), decision (`protects`), constraint (`constrains`), failure_mode (`mitigates`)

---

### behavior
> Module: core | ID: author-chosen identifier | "What exactly does the system do?"

| Required | Optional |
|----------|----------|
| title, contract | invariants, adrs, types, ports, verify, tests |

Outgoing: invariant (`references`), event (`produces`), type (`uses_type`), port (`uses_port`), decision (`shaped_by`)
Incoming: feature (`implements`), event (`consumes`), constraint (`constrains`)

---

### feature
> Module: core | ID: author-chosen identifier | "What value does this deliver?"

| Required | Optional |
|----------|----------|
| title, behaviors, problem, solution | roadmap |

Outgoing: behavior (`implements`)
Incoming: capability (`traces_to`), library (`provides`), roadmap (`schedules`)

---

### event
> Module: core | ID: author-chosen identifier | "What does the system announce?"

| Required | Optional |
|----------|----------|
| title, trigger | payload, channel, consumers |

Outgoing: behavior (`consumes`)
Incoming: behavior (`produces`)

---

### type
> Module: core | ID: identifier | "What shape does the data have?"

| Required | Optional |
|----------|----------|
| name, fields or variants | — |

No outgoing edges.
Incoming: behavior (`uses_type`), event (payload reference), port (method signatures)

---

### port
> Module: core | ID: identifier | "What contracts exist between components?"

| Required | Optional |
|----------|----------|
| name, direction, methods | category |

No outgoing edges.
Incoming: behavior (`uses_port`), library (`defines_port`), invariant (`enforces`)

---

### ref
> Module: core | ID: `scheme.kind:identifier` | "What external resource is this connected to?"

| Required | Optional |
|----------|----------|
| scheme, identifier | title, provider-specific fields |

No outgoing edges. Leaf node.
Incoming: any entity (`links_to`)

---

### journey
> Module: @specforge/product | ID: `identifier` | "How does the user experience this?"

| Required | Optional |
|----------|----------|
| title, persona, features, flow | surface |

Outgoing: feature (`traces_to`)
Incoming: deliverable (`bundles`)

---

### deliverable
> Module: @specforge/product | ID: author-chosen identifier | "What ships to users?"

| Required | Optional |
|----------|----------|
| title, journeys | modules, milestone, personas, type |

Outgoing: journey (`bundles`), module (`built_from`)
Incoming: milestone (`schedules`)

---

### milestone
> Module: @specforge/product | ID: `identifier` | "When does this ship?"

| Required | Optional |
|----------|----------|
| title, status | features, criteria |

Outgoing: feature (`schedules`), deliverable (`schedules`)
Incoming: feature (`milestone` field), deliverable (`milestone` field)

---

### module
> Module: @specforge/product | ID: `identifier` | "What component delivers this?"

| Required | Optional |
|----------|----------|
| title, features | depends_on, description, family |

Outgoing: feature (`provides`), module (`depends_on`)
Incoming: deliverable (`built_from`), module (`depends_on`)

---

### term
> Module: @specforge/product | ID: `identifier` | "What does this term mean?"

| Required | Optional |
|----------|----------|
| definition | title, aliases, context, see |

No graph edges. The `see` field is informational only — it does not create compiler-tracked edges.

---

### decision
> Module: @specforge/governance | ID: `ADR-{n}` | "Why was this built this way?"

| Required | Optional |
|----------|----------|
| title, status, context, decision | date, consequences, invariants |

Outgoing: invariant (`protects`)
Incoming: behavior (`shaped_by`)

---

### constraint
> Module: @specforge/governance | ID: author-chosen identifier | "What quality must the system achieve?"

| Required | Optional |
|----------|----------|
| title, category, priority, description/metric | behaviors/affects, invariants, verify |

Outgoing: behavior (`constrains`), invariant (`constrains`)
No incoming edges.

---

### failure_mode
> Module: @specforge/governance | ID: author-chosen identifier | "What can go wrong and how bad is it?"

| Required | Optional |
|----------|----------|
| title, invariant, severity, occurrence, detection | rpn, cause, effect, mitigation, post_mitigation |

Outgoing: invariant (`mitigates`)
No incoming edges.

---

## Edge Types

### Core (9 edges)

| Edge | From | To | Meaning |
|------|------|----|---------|
| `references` | behavior | invariant | Behavior depends on invariants |
| `implements` | feature | behavior | Feature is composed of behaviors |
| `produces` | behavior | event | Behavior emits events |
| `consumes` | event | behavior | Event triggers behaviors |
| `uses_type` | behavior | type | Behavior uses type definitions |
| `uses_port` | behavior | port | Behavior uses port interfaces |
| `enforces` | invariant | behavior | Invariant enforced by behaviors |
| `imports` | file | file | File uses symbols from another file |
| `links_to` | any entity | ref | Entity links to external reference |

### @specforge/product (7 edges)

| Edge | From | To | Meaning |
|------|------|----|---------|
| `traces_to` | journey | feature | UX flow maps to features |
| `bundles` | deliverable | journey | Deliverable ships journeys |
| `built_from` | deliverable | module | Deliverable uses modules |
| `depends_on` | module | module | Module depends on another module |
| `provides` | module | feature | Module implements features |
| `schedules` | milestone | feature/deliverable | Phase schedules features or deliverables |
| `FeatureDependsOn` | feature | feature | Feature depends on another feature |

### @specforge/governance (4 edges)

| Edge | From | To | Meaning |
|------|------|----|---------|
| `protects` | decision | invariant | Decision protects invariants |
| `constrains` | constraint | behavior/invariant | Quality requirement applies to entities |
| `mitigates` | failure_mode | invariant | Failure mode threatens invariant |
| `shaped_by` | behavior | decision | Behavior shaped by decisions (soft ref) |

---

## Validation Codes

Every diagnostic code, with its meaning, owner and fix, is listed in [Diagnostic Codes](diagnostics.md), generated from the catalog that `specforge explain <CODE>` prints.
