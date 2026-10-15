# An extension command's name is judged by its route

**Status:** accepted (2026-10-15, round 10, plan 05). Narrows [ADR 0017](0017-one-extension-surface-table.md) D7 for
commands (its `check` keeps E039 is keyed by a command's route); D1's `ExtensionCommands` routes from the registry's
table. D1-D14 otherwise stand.

Who owns an extension command's name was decided in four places with three keys:

- The registry build deduplicated by the raw declared id and raised E039 (an Error, so `check` fails).
- Ops routed by `(short name, CLI name)`, the first command winning and the later being shadowed.
- MCP's surface table and the CLI's stderr note each re-matched `(short, CLI name)` to name the winner.

So `check` failed on two extensions that both declare `probe` under two short names, though `specforge greet probe`
and `specforge probe probe` both route and both MCP tools are served (reproduced with patched SDK fixtures). And it
said nothing when `he-lo` and `he_lo` under one short name routed to one command line, where only the first ever
ran. The review's After (ops raising E039) could not be built: `check`, the LSP and watch read the setup diagnostics
of `Environment`, which come from the registry build, and the registry cannot call ops (ADR 0084).

## Decision

- R1. The registry build routes the commands: `CommandRoutes`, keyed by `(ExtensionDeclaration::short(),
  option_name(id))`, both functions of leaf `protocol-types`. First in load order, then in declaration order.
- R2. A shadowed command is E039 (Error, Collision part), naming the route and the winner. One command id under two
  short names is two names: both route, with no E039.
- R3. Ops derives `ExtensionCommands` from the routes. The CLI note and MCP's I017 name the winner the routes
  recorded, and no surface rebuilds the key.
- R4. Explicit MCP tools and resources keep E039 by global name.
- R5. The registry's entry table (`SurfaceRegistryEntry`, `SurfaceType`), which no production code read, goes.

No new diagnostic code; E039's and I017's catalog text change.

## Consequences

A project that failed `check` only on a cross-short-name E039 now passes. A project with `a_b` and `a-b` under one
short name now fails `check`: it was already broken, one command never ran. No blob input changes, so no re-vendor.

## Rejected

- A warning (W159) for a shadowed command: a tool-name duplicate stays an Error, two severities for one class.
- No command diagnostic at `check`: `check`, doctor and the LSP go silent on a command that runs on no surface.
- Ops owning the table with `check` reading ops: violates the layering, and the LSP and watch would lose the
  diagnostic.
- Routing as a function in leaf `protocol-types`: a blob input, a re-vendor for nothing.

## Reopen

A surface that names commands by another key.
