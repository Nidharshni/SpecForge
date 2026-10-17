# One reader for the extensions a change names

**Status:** accepted (2026-10-16, architecture round 11, plan 02). Amends ADR 0098 (D1 c narrowed: add and init share the
naming step, not the Resolver) and ADR 0065 D4. Applies ADR 0101 D1 and D2. Builds on ADR 0104.

"The extensions a change names" was read in four modules with two request shapes. `AddRequest` took parsed specifiers, so
the CLI and MCP each parsed; `init::Request` took raw strings, and ops init split them on commas, joined a relative path
onto its directory and dropped `..` by name. MCP add joined onto the root, the CLI add joined nothing, and MCP init parsed
unsplit strings only to run a check. The per-source loop and the local read were written twice. The copies disagreed, each
reproduced:

- R1: `init --extensions a,b` names two extensions; `add a,b` names one specifier and is refused.
- R2: MCP `add_extension ["a,b"]` is refused (E054); MCP `init ["a,b"]` enables two extensions.
- R3: `link/../x.wasm`, `link` a symlink: add follows the file system, init drops `link` by name.
- R4: the same disagreement was an escape in MCP init (the check followed the link, the read went by name); closed by ADR
  0104 and kept closed here by reading what is checked.
- R7: init's split turns the requirement `@acme/tool@>=1, <2` into a package and a local file `<2`.

## Decision

- **D1. One module reads them.** `specforge_ops::extension::names` takes the raw strings, a place (the project root, the
  base and the containment) and a step for registry packages. It owns parsing, the base, claims (ADR 0098), the local read
  and the git refusal, and returns one claimed value per string. add keeps its config, registry resolution,
  `AlreadyPresent`, the dry run and `apply`; init keeps its starter templates and E063. The whole `Resolver` is still not
  shared (ADR 0098 D1 c): none of its parts enters the reader, and a registry package enters only as the caller's step.
- **D2. No operation splits a string.** Each string is one specifier (ADR 0036): a requirement may hold commas
  (`>=1, <2`). The command line spells several to `init --extensions` (clap's `value_delimiter`), as `EntityIds` does
  (ADR 0101 D2). MCP `init ["a,b"]` is E054 on `extensions`, as `add_extension` answers.
- **D3. `..` is the file system's.** The reader hands the joined path to the containment-aware read (ADR 0104 D6), which
  resolves component by component and reads what it resolved. The one reading serves `add` and `init`, a directory not
  made yet included.
- **D4. The base is stated once.** `AddRequest.base: Option<&Path>`, `None` is the project root (MCP); the CLI passes its
  working directory. init's base is the directory it creates (ADR 0098 D4).
- **D5. Surfaces only spell.** `extension::{specified, Specified, Source, parse}` and `Specified::relative_to` leave the
  public interface; no surface parses a specifier.

## Rejected

- Splitting in ops (U1 a): breaks version requirements, makes add's hint wrong too. Splitting outside a requirement: a
  second grammar beside `PackageRef::parse`. Splitting nowhere: breaks `init --extensions a,b`.
- Sharing the whole `Resolver`; sharing parse and base only and keeping two loops.
- `..` by name everywhere; a required `base`; always the root as the base.

## Consequences

- Breaking: `AddRequest.extensions` is `Vec<String>` and gains `base`; MCP `init ["a,b"]` is refused; CLI `init
  --extensions link/../x.wasm` follows the link; a missing relative file in `specforge add` is named by its joined path.
- `init --extensions a,b` is unchanged.

## What would reopen it

A third operation that names extensions (update naming new sources, a lockfile import), or a specifier grammar that
contains a list.

**Amended by ADR 0134 (2026-10-17):** D1: add's `apply` is gone: `extension::names` asks the opened lock plan for each specifier and names the plan's refusal; add keeps its config, registry step, dry run and outcomes.
