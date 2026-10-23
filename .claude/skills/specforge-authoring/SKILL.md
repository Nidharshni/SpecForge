---
name: specforge-authoring
description: "Orchestrator for writing .spec DSL files that the SpecForge compiler parses. Delegates to 16 entity-specific child skills for block syntax, field reference, relationship edges, and validation rules. Use when writing new .spec files, adding entities to existing .spec files, auditing .spec file completeness, or converting markdown specs to .spec DSL."
---

# SpecForge Authoring (Orchestrator)

Orchestrator skill for writing and maintaining `.spec` DSL files -- the source-of-truth format that the SpecForge compiler parses into an in-memory graph, validates, and emits as markdown, JSON, traceability reports, and rendered outputs.

**This skill writes `.spec` files (compiled DSL). Not markdown spec documents.**

## Write through typed edits

Do not write `.spec` text by hand. Change a project with the typed edits, which write each value in the form its field
declares, lay the block out canonically, and refuse (E080) an edit that would leave the project with an error it does not
have:

```bash
specforge new behavior archive_task "Archive a Task" --in behaviors/task.spec --contract "…" --types Task --category command
specforge set archive_task category query          # specforge set <id> --title "…" changes the title
specforge link archive_task invariants task_id_uniqueness    # unlink removes; unset removes a field
specforge link archive_task --verify unit:"archiving an unknown task fails"
specforge delete archive_task --unlink-references
```

Over MCP, `specforge.edit` takes a list of edits (`create`, `set`, `unset`, `retitle`, `link`, `unlink`, `add_verify`,
`remove_verify`, `delete`) applied all or nothing, so an entity and the link to it go in one call. Add `--dry-run`
(`dry_run: true`) to see the diff first. The child skills below say what each field means; a union takes only `set --title`
and `delete` (every other edit is refused), and ports with `method` lines are not covered by typed edits yet, so
write those by hand.

## When to Use This Skill

- Writing a new `.spec` file for a SpecForge project
- Adding entity blocks (invariants, behaviors, features, etc.) to existing `.spec` files
- Auditing `.spec` file completeness against the entity model
- Converting existing markdown specs into `.spec` DSL format
- Determining which child skill to invoke for a specific entity type

## Delegation Table

| Entity Type | Child Skill | Naming Convention | Module |
|-------------|-------------|-------------------|--------|
| `spec` | **specforge-spec-block** | singleton | core |
| `ref` | **specforge-refs-dsl** | `scheme.kind:identifier` | core |
| `invariant` | **specforge-invariants-dsl** | `identifier` | @specforge/software |
| `behavior` | **specforge-behaviors-dsl** | `identifier` | @specforge/software |
| `feature` | **specforge-features-dsl** | `identifier` | @specforge/software |
| `event` | **specforge-events-dsl** | `identifier` | @specforge/software |
| `type` | **specforge-types-dsl** | `identifier` | @specforge/software |
| `port` | **specforge-ports-dsl** | `identifier` | @specforge/software |
| `journey` | **specforge-journeys-dsl** | `identifier` | @specforge/product |
| `deliverable` | **specforge-deliverables-dsl** | `identifier` | @specforge/product |
| `milestone` | **specforge-milestones-dsl** | `identifier` | @specforge/product |
| `module` | **specforge-modules-dsl** | `identifier` | @specforge/product |
| `term` | **specforge-terms-dsl** | `identifier` | @specforge/product |
| `decision` | **specforge-decisions-dsl** | `identifier` | @specforge/governance |
| `constraint` | **specforge-constraints-dsl** | `identifier` | @specforge/governance |
| `failure_mode` | **specforge-failure-modes-dsl** | `identifier` | @specforge/governance |

## Entity Naming

All entity IDs are **variable-name identifiers** (not sequential numeric prefixes):

- Any valid identifier (letters, digits, underscores, 2-60 chars): `data_persistence`, `UserRepository`, `validate_input`
- Titles are **optional** -- auto-derived from name if omitted (`auth_login` -> "Auth Login")
- Names must be 2-60 characters, letters/digits/underscores only
- Reserved words (the 16 entity keywords) cannot be used as names

## File Organization

Canonical `.spec` file structure for a SpecForge project. Example layout with `@specforge/software` + `@specforge/product` + `@specforge/governance` installed (your project may use a different combination of extensions):

```
specforge.json           # project config (preferred) -- created by `specforge init`
specforge.spec           # spec block (a graph entity; config is specforge.json)
invariants/
  data.spec              # invariant blocks grouped by domain
  auth.spec
behaviors/
  user-crud.spec         # behavior blocks grouped by capability
  auth.spec
  order-processing.spec
features/
  user-management.spec   # feature blocks
  auth.spec
events/
  user-events.spec       # event blocks
  order-events.spec
types/
  user.spec              # type definitions grouped by domain
  order.spec
  common.spec
ports/
  user.spec              # port interfaces grouped by domain
  email.spec
governance/
  decisions.spec         # decision blocks (ADRs)
  constraints.spec       # constraint blocks (NFRs)
  failure-modes.spec     # failure_mode blocks (FMEA)
product/
  journeys.spec          # journey blocks (UX flows)
  deliverables.spec      # deliverable blocks
  milestones.spec        # milestone blocks
  modules.spec           # module blocks
glossary.spec            # term blocks (individual entities)
```

Files can be split further by domain as the project grows. The directory structure is a convention, not a compiler requirement -- the compiler discovers all `.spec` files under the spec root.

## Import System

### `use` Directives

Files reference symbols from other files via `use` directives at the top of the file:

```spec
use "invariants/data"                      // imports all symbols from invariants/data.spec
use "invariants/data" { data_persistence } // selective import
use "behaviors/user-crud"
use "types/user"
```
### Resolution Rules

- `use "invariants/data"` resolves to `<spec_root>/invariants/data.spec`
- The spec root is `spec_root` in `specforge.json`, else the project root
- Paths use forward slashes (platform-independent)
- The `.spec` extension is implicit -- never include it
- All top-level declarations in a file are public by default
- A file MUST `use` another file to reference its symbols -- enforced at compile time

### When to Use Imports

- Reference an entity declared in another file -> add a `use` directive
- Reference an entity declared in the same file -> no import needed
- Import cycles are a compile error (`E003`)

## Common Patterns

### Entity Ordering Within Files

Group related entities together. Within a file, order entities by:

1. Types (data shapes used by behaviors below)
2. Invariants (guarantees referenced by behaviors below)
3. Behaviors (contracts that reference types and invariants)
4. Events (produced by behaviors above)
5. Features (compose behaviors above)

### Grouping Conventions

- **By domain**: `user.spec`, `order.spec`, `billing.spec` -- each file contains all entity types for that domain
- **By entity type**: `invariants/data.spec`, `behaviors/user-crud.spec` -- separate directories per entity type
- **Hybrid**: types and ports in `types/` and `ports/`, behaviors and features in domain files

The entity-type-per-directory pattern is recommended for projects with >20 entities.

### File Defaults

When two or more blocks of a kind in one file share a reference list, state it once above the first block:

```
defaults behavior {
  features [task_management]
}
```

Every behavior in the file now has `features [task_management]`; a block that writes its own `features` replaces it whole (`features []` means none). The next behavior you add inherits it too, so write a default only for what is true of every block of the kind. A default sets a field the kind declares (E077), once per kind (E078), and a block restating it is W161. `specforge migrate` hoists the repeats of an existing file. A block writes each field once (E081): give a list all its items in one `[...]`.

## Cross-Reference Conventions

### Entity Name References

Reference entities by their name in reference lists:

```spec
invariants [data_persistence, email_uniqueness]   // reference list
produces   [user_created, email_validated]   // a behavior's events
features   [user_management]
refs       [gh.issue:42, jira.epic:PROJ-123]
```

### Inline References in Prose

Use bracket syntax in triple-quoted strings:

```spec
contract """
  The system MUST validate email uniqueness per [email_uniqueness].
  See [gh.issue:42] for the original requirements.
"""
```

### Cross-File References

Always `use` the target file before referencing its entities:

```spec
use "invariants/data"
use "types/user"

behavior create_user "Create User" {
  invariants [data_persistence]    // declared in invariants/data.spec
  types      [User, UserRole]     // declared in types/user.spec
  contract "..."
}
```

## Validation Quick Reference

### Core Errors (compilation fails)

| Code | Description |
|------|-------------|
| E001 | Unresolved reference -- name not found |
| E002 | Duplicate entity name -- same name in multiple files |
| E003 | Circular import -- `use` statements form a cycle |
| E011 | Invalid ref target format |
| E012 | Unknown provider kind |
| E013 | Reserved word used as identifier |
| E014 | Invalid identifier characters |

### Core Warnings

| Code | Description |
|------|-------------|
| W012 | Orphan ref -- declared but never referenced |
| W013 | Vague entity name |

### Core Info

| Code | Description |
|------|-------------|
| I003 | Newer format features available |
| I004 | Unknown entity in reference field -- suggests installing a plugin |
| I005 | Unknown provider scheme -- suggests installing a provider |

### @specforge/software Errors

| Code | Description |
|------|-------------|
| E004 | Port method references invalid type |
| E030 | Always-false precondition in requires block |
| E031 | Liskov compliance violation -- strengthened precondition or weakened postcondition |
| E032 | Cycle in refinement chain |
| E033 | Behavior not satisfying feature requirements |
| E034 | Event deadlock detected (circular event dependency) |
| E035 | Channel type mismatch -- producer and consumer payload types differ |
| E041 | Refinement chain cycle -- cycle in RefinementChainLink DAG |
| E042 | Process composition cycle -- cycle in ProcessComposition DAG |

### @specforge/software Warnings

| Code | Description |
|------|-------------|
| W001 | Orphan behavior -- not in any feature |
| W003 | Unused invariant -- not referenced by any behavior |
| W004 | Unverified behavior -- no `verify` statement |
| W007 | Orphan event -- no consumers |
| W010 | Unknown annotation on type field |
| W029 | Unmatched event producers -- no consumers |
| W030 | Incomplete refinement chain -- abstract with no concrete |
| W031 | Deep refinement chain (depth > 4) |
| W032 | Livelock risk -- re-triggering without backoff |
| W033 | Starvation risk -- unfair port access |
| W034 | Unbounded channel buffer -- no sync timeout |
| W035 | Undischarged proof obligation |
| W036 | Port-behavior condition incompatibility |
| W037 | Unverifiable condition |
| W038 | Unreachable postcondition -- contradicts preconditions |
| W039 | Redundant precondition -- implied by sibling |
| W040 | Invariant without formal property -- no maintains block |
| W058 | Feature coverage mismatch (downgraded from E033) |
| W059-W060 | Condition entity validation (orphan, empty description) |
| W061-W063 | Property entity validation (orphan, empty description, missing kind) |
| W064-W065 | Axiom entity validation (orphan, empty description) |
| W066-W068 | Protocol entity validation (orphan, empty description, ordering conflict) |
| W069-W071 | Refinement entity validation (orphan, empty description, missing delta) |
| W072-W074 | Process entity validation (orphan, empty description, missing alphabet) |

### @specforge/software Info

| Code | Description |
|------|-------------|
| I007 | Proof obligation verified by test |
| I008 | Deadlock freedom verified |
| I009 | Formal analysis available -- suggests `specforge analyze` |
| I011 | Ensures without requires -- info diagnostic |

### @specforge/product Errors

| Code | Description |
|------|-------------|
| E007 | Circular module dependency |
| E008 | Persona not defined in spec root (journey) |
| E009 | Surface not defined in spec root (journey) |

### @specforge/product Warnings

| Code | Description |
|------|-------------|
| W041 | Orphan feature -- not in any journey |
| W042 | Orphan journey -- not in any deliverable |
| W043 | Deliverable with no journeys |
| W044 | Orphan module -- not referenced by any deliverable |
| W045 | Circular feature dependency |

### @specforge/governance Errors

| Code | Description |
|------|-------------|
| E005 | RPN mismatch -- severity x occurrence x detection != rpn |

### @specforge/governance Warnings

| Code | Description |
|------|-------------|
| W005 | Unmitigated high-risk invariant |
| W006 | Unconstrained behavior |

### @specforge/governance Info

| Code | Description |
|------|-------------|
| I001 | Stale proposal -- decision proposed >30 days ago |

## Authoring Workflow

1. **Start with `specforge init`** -- creates `specforge.json` (project config: name, version, plugins, providers, personas, surfaces)
2. **Add refs** -- invoke **specforge-refs-dsl** for external issue/ticket/design references
3. **Add software entities** (if @specforge/software installed):
   - **specforge-types-dsl** for domain data shapes
   - **specforge-ports-dsl** for interface contracts
   - **specforge-invariants-dsl** for runtime guarantees
   - **specforge-behaviors-dsl** for behavioral contracts referencing invariants, types, and ports
   - **specforge-events-dsl** for domain events produced by behaviors
   - **specforge-features-dsl** to group behaviors into user-facing capabilities
4. **Add product entities** (if @specforge/product installed):
   - **specforge-journeys-dsl** for UX flows
   - **specforge-deliverables-dsl** for shippable artifacts
   - **specforge-modules-dsl** for code packages
   - **specforge-milestones-dsl** for planning phases
   - **specforge-terms-dsl** for domain vocabulary
5. **Add governance entities** (if @specforge/governance installed):
   - **specforge-decisions-dsl** for ADRs
   - **specforge-constraints-dsl** for NFRs
   - **specforge-failure-modes-dsl** for FMEA entries
11. **Validate** -- run `specforge check` to verify all references resolve and no orphans exist

## What NOT to Do

- Do not mix markdown spec conventions with `.spec` DSL syntax -- they are different formats for different purposes
- Do not duplicate entity names across files -- each name is globally unique
- Do not use `use` for entities in the same file -- only for cross-file references
- Do not hand-write `index.yaml` files -- the compiler generates them
- Do not hand-write traceability matrices -- `specforge trace` generates them
- Do not add traceability or overview as source entities -- they are compiler outputs
- Do not confuse plugins (entity model), providers (ref validation), and renderers (output formats)
- Do not include the `.spec` extension in `use` paths -- it is implicit
  
