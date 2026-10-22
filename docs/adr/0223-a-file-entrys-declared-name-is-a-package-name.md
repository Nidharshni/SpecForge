# A .wasm file entry's declared name is a package name

**Status:** accepted (2026-10-22, architecture round 25, plan 03). Extends [ADR 0214](0214-a-builtins-name-is-refused-where-extensions-load.md) D1-D2, [ADR 0036](0036-one-package-name.md) D1 and [ADR 0160](0160-what-an-extension-entry-enables-is-the-loads-decision.md) D1; reads with [ADR 0028](0028-installed-extensions-are-one-module.md) D7.

## Context

- The load's admit was the one place a declared name entered without the package-name rule: a `.wasm` file entry declaring `@specforge/Product` or `@SDK/greet` loaded and listed healthy, while `add ./x.wasm` refused it (E072).
- A file entry's component loaded under the entry's text (`first.wasm`), itself a valid package name. A component declaring another entry's text was renamed onto that key, and that entry's load replaced it: listed loaded, check clean, its commands and passes "not loaded". "Already loaded" exempted the entry's own text, so two entries were admitted under one name.
- `extension validate` called such a binary valid.

```text
BEFORE  admit(declared): written? builtin? loaded(unless declared == own text)?  -- any text is Ok
        load key of a file entry = its entry text, a valid package name

AFTER   admit(declared): package name? (E072)  written?  builtin?  loaded?       -- no exception
        load key of a file entry = "file:<entry>"   ':' is in no package name; "__candidate" starts with '_'
```

## Decisions

- **D1.** The admit reads a `.wasm` file entry's declared name as a package name (`PackageName::parse`); one that is none is `LoadProblem::NotAPackageName`, E072 naming the entry, nothing loaded; the other entries load.
- **D2.** Order: a package name, the name written, a builtin's name, a name already loaded, with no exception for the entry's own text (amends 0214 D2).
- **D3.** Every name the load admits is a package name; its temporary keys are not: a file entry's component loads under `file:<entry>` (`load_key`), a candidate under `__candidate`. So no load replaces what another entry loaded, and no candidate read evicts a project extension.
- **D4.** `extension validate` lists E072 for such a name; publish refuses it at its declaration step.

## Rejected

- E028 (0214's "every `.wasm` file-entry refusal is E028"): E072 is the rule's code at every entry point.
- Refusing the whole project's load, or loading with a warning.
- A fifth rule refusing a declared name equal to another entry's text: a user-visible rule for an internal key.
- `Declared.name: PackageName`: no caller needs the type.
- Reserving `@specforge/` for file entries (round 22 X3).

## Consequences

- A specforge.json naming a `.wasm` whose component declares no package name: that entry stops loading, E072 on check, doctor, the LSP and MCP, which exit 1; the file is never rewritten. Rebuild with a package name.
- No new code and no `LoadProblem` variant; the E072 catalog text lists the file entry and `extension validate`.

## What would reopen it

- A host that must load components whose declared names come from another ecosystem (mixed case): an alias at the entry, never a looser `PackageName` (as 0036 says for registries).
