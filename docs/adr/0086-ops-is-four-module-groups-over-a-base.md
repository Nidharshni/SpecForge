# specforge-ops is four module groups over a base, and their edges are a DAG

**Status:** accepted (2026-10-14, structure round, plan 06). Amends ADR 0010, 0011, 0015, 0016, 0045 and 0076 (the
paths they name). Amended by [ADR 0161](0161-a-stub-fix-is-a-typed-edit.md): the fixes are in `authoring`.

`specforge-ops` (36,910 lines) declared 38 modules side by side, and `lib.rs` also held the failure types. Nothing
said which modules every operation is written with and which are operations. A proposed grouping had 60
cross-group references and cycles between five pairs of groups. Read one by one, 34 were dependencies, 19 came from
modules placed in the wrong group, 5 were items in the wrong module, 1 was a test of `init` inside `add`'s test,
and 1 was a doc link.

## D1. A base and four groups

The top-level modules are the **base**: `OpError` and `OpErrorKind` (`error`), `RunVerdict` (`verdict`),
`Refusal`, `Writes` (with `resolves_outside`, the one test of a path leaving a root), `options`, `config`, `report`, the registry port (`registry`, ADR 0010), the solver port
(`solver`, ADR 0079), `schema_version` (ADR 0067), the project view (`view`, ADR 0015) and `testing`. None is an
operation, and each is needed by more than one group. The operations live in four parent modules:

- `read`: the read views (ADR 0015), navigation (ADR 0016), inference, explain.
- `extension`: what a project installs, enables and lists, publish (ADR 0045), and the commands extensions
  declare (`extension::command`, ADR 0011).
- `analysis`: check, analyze with its passes and the prove pass, collect, doctor.
- `authoring`: init, format, rename, migrate.

## D2. The DAG

`read` and `extension` use only the base. `analysis` and `authoring` use the base, `read` and `extension`. The base
uses only itself. Test code obeys the same rule.

## D3. Written once, checked in the crate

`src/lib.rs` holds the table of groups and what each may use. `tests/operations/groups.rs` parses `src/` with syn
and fails on any `crate::`, `super::` or `self::` path (in a `use`, code, a test, or a macro's arguments) that
names a module of a group its file's group may not use. It also refuses a table with a cycle.

## D4. Items that moved to where the DAG puts them

`Trust` is the registry port's (`registry::Trust`): it is what the fetch policy carries (ADR 0059). The
unknown-entity refusal and its request field are the project view's (`view::{not_found, EntityField}`), and the
view raises it itself. `scan` is `infer`'s child, its only user.

## Rejected

- **Visibility** (`pub(in path)`): it names one ancestor, so it cannot allow two sibling groups and forbid a third.
- **A gate check**: the rule is one crate's inside, gate has no parser, and a path trigger can miss a change.
- **Read views at the top level**: 14 operations that look like base, and a group check that needs a list of
  names instead of the first path segment.
- **Crates per group**: Cargo would enforce the DAG, but whether a group earns a crate is a measured decision (the
  structure round's plan 09).

## Consequences

- 479 caller paths in the CLI, MCP, LSP and ops' tests change (`specforge_ops::trace` → `specforge_ops::read::trace`
  and so on). The crate-root types keep their paths.
- The extension group depends only on the base, so it can leave the crate if the base comes with it or moves below
  it.

## What would reopen it

An operation that needs two groups that may not use each other (a read view that needs extension management), or a
fifth kind of operation (one that writes the project's extensions and its sources at once).

## Placements after plan 13

`format::run_within` and `migrate::run_within` (the root a surface binds them to) live with their modules, at
`authoring::format` and `authoring::migrate`. `migrate::SOURCE_OUTSIDE_ROOT` and the inference session's
`SOURCE_OUTSIDE_ROOT` stay with their operations. `resolves_outside` is a write-side check shared by authoring and
by `read::infer`, so it stays in the base beside `Writes` (`specforge_ops::resolves_outside`, a crate-root name).

**Amended by ADR 0104:** `resolves_outside` and the `run_within`s are gone; `containment` is a base module (`specforge_ops::Containment`), and format, migrate and rename take it in their request.
