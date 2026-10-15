# One claim rule for the extensions a change names

**Status:** accepted (2026-10-15, architecture round 10, plan 07). Amends ADR 0065 D4 (init claims its specifiers as
add does) and ADR 0092 D7 (where init reads a relative local specifier).

`add` decides what two specifiers that name one extension mean: the same way is a duplicate, another way is
`extension_conflict`. `init` had its own thinner loop that skipped a specifier only when its parsed `Source` equalled an
earlier one. So `specforge init --extensions a.wasm,b.wasm` (one extension, other bytes) exited 0 and the lock kept `b`,
where `specforge add a.wasm b.wasm` refused. The same held over MCP `init`.

A second divergence sat in the same resolution: MCP `add_extension` reads a relative local specifier from the project
root (ADR 0092 D7), MCP `init` read it from the server process's working directory, which a client cannot know.

## Decisions

- **D1. One module owns the rule.** `specforge_ops::extension::claims::Claims` owns "two specifiers, one extension" for
  every change that names extensions. The same way again (a builtin, the same registry version, the same SHA-256) is a
  duplicate, added once by the first; another way (other bytes, another version, another source) is
  `extension_conflict`, naming both specifiers. `add` and `init` both drive it, and claim before they ask their lock plan.
  `authoring` (init) uses `extension` (ADR 0086 D2), so the group DAG holds.
- **D2. What init does with each.** A duplicate is skipped (init's document has no per-specifier outcomes). A conflict
  refuses the init before anything is written, with add's code and wording, its subject `InitField::Extensions`
  (ADR 0072). No new diagnostic code.
- **D3. A guard in the lock plan.** `LockPlan::install` stages one install per name; a `debug_assert!` turns a caller
  that forgot to claim into a failing test. It is not a user-facing rule.
- **D4. `init` reads a relative local specifier from the directory it creates.** For the CLI that is the working
  directory, so nothing changes; over MCP it is the directory the call names, so `init` and a later `add_extension` read
  one spelling as one file.

## Rejected

- The lock plan refusing a second install of a name (D1 b): `add` must claim before it fetches, so this would be a second
  rule beside add's, and the plan knows asks, not the specifiers that spelled them.
- Sharing the whole `Resolver` with init (D1 c): init has no config, no registry, no `AlreadyPresent`.
- Reading init's relative specifiers from the server's root (U1 b): `init` and `add_extension` would read `../x.wasm`
  from different directories. Keeping the process's directory (U1 c): a client cannot predict it.

## Consequences

- `init` refuses what `add` refuses: two specifiers that add one extension differently exit 1 with `extension_conflict`.
- `specforge.init` reads a relative local path from the directory it creates, not the server's working directory.
- A missing relative file in `specforge init --extensions` is named by its joined path.

## What would reopen it

An operation that names extensions and is neither `add` nor `init`; a decision that MCP may install from outside the
project again (see "Closing ADR 0092 D7"; developing an extension means installing it from a sibling directory, which the CLI still does).

## Closing ADR 0092 D7 (2026-10-15)

This ADR left D7 open because installing from a sibling directory is how extensions are developed. The user closed it
after the round: over MCP a local `.wasm` an `add_extension` or `init` call names must resolve inside the project the
call installs into (for `init`, the served project, or the server's root while none is served), or the call is
`invalid_input` on `specifiers` / `extensions`, nothing installed. The check is the surface's, in
`specforge_mcp::containment::refuse_local_outside`, because MCP owns containment (ADR 0092); the shared resolver and
the CLI are unchanged (ADR 0092 D6), so developing an extension from a sibling directory still works with the CLI.
Consequence: an MCP client installs a vendored blob by copying it into the project first.
