# What an extension entry enables is the load's decision

**Status:** accepted (2026-10-19, architecture round 16, plan 03). Amends [ADR 0138](0138-the-lock-plan-answers-what-a-project-already-has.md)
**Amended by [ADR 0181](0181-the-lock-plan-predicts-the-peer-set-by-the-extension-loads-walk.md) (round 19):** D2: the set it asks about is predicted by the load's own walk (`Resolution::walk`), a locked entry with a missing or changed module included.
D1 and [ADR 0028](0028-installed-extensions-are-one-module.md) (amendment round 7, D3).

The lock plan answered "does an entry of `specforge.json` already enable X" by the entry's spelling
(`extension_entry_name`: `name=path.wasm` names `name`, a bare `path.wasm` names its path) in `append`, `unchanged`,
`Disabled::drops`, `Changing::of` and the companions, while the extension load and the plan's peer set answer it by
`Installed::resolve` and the component a `.wasm` file entry declares. So `add` of an extension a `name=path.wasm` entry
loads installed a module the project never loads and then answered `already_present`; over a bare `path.wasm` entry it
appended a bare name, and the file entry stopped loading (E028); over `["name", "path.wasm"]` it swapped one E028 for
another. Only `append` and `unchanged` differed from the load on a reachable input; the others parsed names the plan
wrote itself or reached a file arm no caller could. `builtin_name` read `@specforge/product=./a.wasm` as the builtin.

## Decision

- **D1.** A named entry names an extension by its text (`ExtensionEntry::names`: `name`, or a legacy `name@version`),
  as `Installed::resolve` reads it; a `.wasm` file entry names none by its text and enables only what its component
  declares, once the load admits it (`ProjectPeers::of`). Every lock-plan question ("does an entry already enable X":
  `append`, `unchanged`, `Disabled::drops`, the companions) reads that rule; `extension_entry_name` is gone.
- **D2.** An add or init (a verb that enables) of a name a `.wasm` file entry of the opened `specforge.json` loads is
  refused with `extension_conflict`, naming the entry's file, before anything is written; a dry run answers the same.
  `update` and `remove` are not affected.
- **D3.** `builtin_name` reads named entries only; a `name=path.wasm` specifier is a local path.

Rejected: replacing the file entry (the add would rewrite an entry the user wrote and gain a removal half); installing
without enabling (a new outcome on every surface, and the project still never loads it); a before/after "an entry stops
loading" rule (misses the `name=path.wasm` case and needs entry indices on the peer set); `Resolution::named` in
`installed` (the question needs no lock); counting `Done.enabled` by loads (an in-process event, ADR 0022 D7).

## Consequences

`add` over a file entry of the same name is refused (CLI, MCP); a `name=path.wasm` specifier is a local path;
`specforge_common::extension_entry_name` is removed.

What would reopen it: a way for one project to load an installed copy and a file of one name side by side; an add that
should take over a file entry.
