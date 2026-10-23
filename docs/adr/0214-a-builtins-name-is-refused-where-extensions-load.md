# A builtin's name is refused where extensions load

**Status:** accepted (2026-10-22, architecture round 24, plan 02). Extends [ADR 0028](0028-installed-extensions-are-one-module.md) D7 to the extension load; reads with [ADR 0160](0160-what-an-extension-entry-enables-is-the-loads-decision.md) D1 and [ADR 0181](0181-the-lock-plan-predicts-the-peer-set-by-the-extension-loads-walk.md) D1.

**Amended by [ADR 0223](0223-a-file-entrys-declared-name-is-a-package-name.md) (round 25):** D2: a package name is checked first, and "already loaded" has no exception for the entry's own text; Rejected's "every .wasm file-entry refusal is E028": one that declares no package name is E072.

## Context

- `add`, `update`, `init`, `publish`, `extension validate` and `extension init` refuse a binary that declares a builtin's name
  (`not_a_builtins_name`, ADR 0028 D7); the extension load did not: its admit rules let a `.wasm` file entry load under a
  builtin's name whenever the builtin was not also enabled by name.
- `extensions`, `doctor` and `check` then presented `@specforge/product` from `file:./impostor.wasm` as healthy.
- The lock plan walks the load's resolution (ADR 0181 D1), so it followed the load: `add @specforge/product` was refused
  because the file "loaded" it, and the hint of `add ./impostor.wasm` ("enable the builtin instead") was circular.

## Decisions

- **D1. The load's admit refuses a `.wasm` file entry whose component declares a builtin's name** (`LoadProblem::BuiltinsName`,
  E028 on the entry, nothing loaded under its key), whether or not the builtin is enabled. Only a builtin loads under a
  builtin's name.
- **D2. Order:** the name written, a builtin's name, a name already loaded.
- **D3. `Builtins::contains` is the one predicate** for a declared name. `Resolution` keeps the builtins it was resolved
  over, and `not_a_builtins_name` reads the same predicate.

## Rejected

- Loading the builtin in place of the file, or loading the file with a warning: a second meaning for a file entry (ADR 0160).
- Refusing the whole project's load: every other entry refusal is per entry.
- Checking at `resolve`: the declared name is known only after the read.
- A new diagnostic code: every `.wasm` file-entry refusal is E028.

## Consequences

- Such a project's `check`, `doctor`, LSP and MCP report E028 on the entry; the builtin is not loaded unless named. The file
  is never rewritten.
- `add <builtin>` over it enables the builtin. The builtin-plus-file case says "the name of a builtin" instead of "already
  loaded". `remove <builtin>` over the file alone is `extension_not_found`.
- `specforge-installed` gains a `LoadProblem` variant.

## What would reopen it

- A host that lets a user replace a builtin with a local build (a development override) needs an explicit, named mechanism,
  not a file entry.
