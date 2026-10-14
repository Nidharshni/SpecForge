# One project, four surfaces: the migration's decisions

**Status:** accepted (2026-09-30); amended the same day after the decision research

The architecture review in `.planning/architecture/` found that the CLI, `specforge watch`, the LSP
and the MCP server each assemble the compiler on their own, and so drift apart. It plans six
deepenings: a compiled-project module, one coverage rule, shared operations, an MCP tool table,
one registry build, and a diagnostic catalog without gaps. Each plan stops at decisions that change
linked obligations or user-visible output. These are the answers; the spec is edited to match in
the step that needs each one.

The first version adopted each plan's recommendation as written. A research pass per group
(`.planning/architecture/decisions/`, with the code, spec, tests, history and outside precedent for
every answer) then confirmed most of them, refined some, and changed seven; this text is the result.
Changed answers are marked *(amended)*.

## Compiled project (plan 01)

- **D1-a** An import cycle is **W113**, as the code reports; the spec text saying E003 is wrong (E003
  already means "unresolved reference"). References resolve across the project without `use`, so a
  cycle changes nothing. W113's message and cycle order become deterministic.
- **D1-b** `exclude` in `specforge.json` applies to `check`, watch, the LSP and MCP alike, relative to
  the spec root. The spec type and the JSON schema (which rejects the key today) gain it.
  *(ADR 0055: each entry is a gitignore-style pattern relative to the spec root, not a substring; an
  entry that can match no source is E069; an excluded file is no source, so no import names it.)*
- **D1-c** One diagnostic type with **two presenters** for now: the CLI's JSON nests `span`, MCP's
  is flat. When they converge, it is one additive shape carrying both, so no reader breaks.
  *(Converged, recorded by ADR 0052: `specforge_common::DiagnosticJson` is that additive shape,
  nesting `span` and repeating its start flat as `file`, `line` and `column`; every surface's JSON
  and every outcome document carry diagnostics in it, `DiagnosticList`, ADR 0048 D6.)*
- **D1-d** `CompiledProject` and `ProjectSession` live in a **new crate, `specforge-project`**; the
  emitter stays free of the Wasm runtime.

## Coverage (plan 02)

- **D2-a** An entity with zero obligations is **uncovered** even when tests pass: a test that proves
  nothing declared is not proof. Proven means at least one obligation, all proven, no failing test.
- **D2-b** *(amended)* The entities W004 exempts are exempt from A001, stats and the gate denominator
  too: union types, `abstract true`, and governance kinds that declare no obligations. The exemption
  is decided from the registry, not from field names. `--min 100` becomes reachable.
- **D2-c** *(amended)* An entity is "declared" when it has at least one `verify` statement; the spec's
  file-reference clause is dropped (no extension declares such a field). Stats gains `declared_pct` and
  `proof_pct`; `coverage_pct` stayed as a deprecated alias of `declared_pct` until ADR 0048 D7 removed it.
- **D2-d** `inspect.testable` means the kind's testability; a separate `declared` field says whether
  the entity declares obligations.
- **D2-e** A malformed `specforge-report.json` is an **error** in MCP, as in the CLI, reported as an
  `isError` tool result (D4-a). A missing report still means "no results".
- **D2-f** The coverage rule is a **shared pure crate, `specforge-coverage`, owned by
  `@specforge/testing`**, linked by the Wasm pass and the host surfaces (this ADR records it; ADR 0002
  stands for the runner split).

## Operations (plan 03)

- **D3-a** The MCP export tool emits **Graph Protocol V2**, through the same function as the CLI and
  with its schema policy: the schema is embedded for a full graph, referenced when the export is
  scoped, and left out under a token budget or for the context and brief formats unless asked for.
- **D3-b** *(amended)* Installed extensions load from the **lock file**, hash-checked, on every
  surface; the `specforge.json` entry is the bare name. The lock records a local install's **own
  declared version**, with `source: "local:<path>"`, so peer and diamond checks work and `update`
  never replaces it from a registry. *(Amended by ADR 0028)* A binary that is not the pinned one is
  E070, an entry without a hash loads with W149, and every surface loads through
  `Installed::load`.
- **D3-c** Provider configuration follows the spec's `ProviderConfig`: an **array** of
  `{scheme, alias, extension, settings}` with `extension` required. Order is kept, because the first
  declared scheme wins (E057). Two instances of one provider use distinct schemes.
- **D3-d** *(amended)* MCP doctor **compiles fresh by default**, like `validate`, with a `use_cached`
  option. Doctor reports extension load failures (E028, E070) on both surfaces.
  *(ADR 0077: `use_cached` is gone.)*
- **D3-e** *(amended)* `init` enables **builtins, and extensions it installs through the shared add
  operation**; it never writes an entry `check` cannot load. Until the shared add exists, builtins only.
- **D3-f** MCP analyze passes **no proved claims unless prove ran**. An opt-in `prove` argument comes
  after the prove code is shared and z3 runs with a timeout.
- **N1** *(new)* SpecForge does not own `specforge.dev`. There is **no hard-coded default registry**:
  a registry must be configured before `add`, `update`, `search` or `publish` reach one. Schema URLs
  move off `specforge.dev` to the canonical repository (D6-b).

## MCP tool table (plan 04)

- **D4-a** Only malformed requests, unknown tools and server faults are **JSON-RPC errors**. Argument
  validation, lookups, business rules and execution failures are **`isError` tool results** carrying
  `McpError { code, message, diagnostic }`, with the diagnostic code in `diagnostic.code` (MCP
  2025-11-25, SEP-1303). A validation run that finds errors is a successful call.
- **D4-b** A tool's category is its role (`core`, `navigation`, `mutation`, `management`); its origin
  is a separate `source` field, and the MCP `annotations` (`readOnlyHint`, …) derive from the same
  definition. `infer_session` is a **mutation** tool; `infer_progress` and `infer_gaps` are core.
  Extension tools are purged by `source`, never by category.
- **D4-c** *(new)* The server **negotiates** protocol versions 2025-11-25, 2025-06-18 and 2025-03-26,
  accepts JSON-RPC batches from 2025-03-26 clients, and returns `structuredContent` alongside the text
  block. Per-tool output schemas follow the typed tool table; the stateless 2026-07-28 revision later.
  *(2026-10-01)* The stateless revision is served too, dual-era: a request whose `_meta` names
  `io.modelcontextprotocol/protocolVersion` is answered on its own, with or without `initialize`
  (`specforge_mcp::modern`); every other request follows the negotiated handshake revision.
  *(ADR 0077: a stateless request never changes what the server serves.)*
  `server/discover` lists only `2026-07-28` (the handshake revisions are reached through
  `initialize`). Cacheable results carry `ttlMs: 0` and `cacheScope: "private"`: they describe the
  project on disk, which any request may recompile. `subscriptions/listen` honours resource
  subscriptions and sends `notifications/resources/updated`; the server offers no list-changed
  notifications, and the handshake era's `specforge/graphChanged` never goes on a listen stream.
- **D4-d** *(2026-10, plan 07)* Prompts follow the tool table: one Prompt spec per prompt, typed
  arguments whose listing derives from the type, and one envelope. A prompt failure is a JSON-RPC
  error (-32602 for client input, -32603 server-side) whose `data` is the McpError, which gains
  `prompt`. Every prompt result is two user messages, instruction then JSON payload.
- **D4-e** *(2026-10, ADR 0024)* Tools, resources and prompts run through one request pipeline
  (`specforge_mcp::surface_call`); a resource read refuses like a prompt (JSON-RPC error, McpError in
  `data`), both under one code rule; core resources are `specforge export` over the call's view.

## Registry build (plan 05)

- **D5-a** Compile wires **I002** and **peer-dependency checks** (honouring `optional` peers, after the
  governance manifest's peer is corrected); I005 follows D3-c. *(amended)* The **I004** keyword hint is
  removed: E024 already names the extension to install, and both would report every entity twice.
  **Define blocks** and **grammar and body-parser contributions** are removed from the spec and the
  registry (define blocks warn when used); the protocol flags stay reserved. *(Amended by ADR 0050: the
  reserved protocol flags and the `grammars` and `body_parsers` describe categories are removed; a kind
  keeps `has_body_parser`.)*

## Diagnostic catalog (plan 06)

- **D6-a** E047 is **renumbered to W139** and stays a warning (it was one by design); E047 is recorded
  as retired.
- **D6-b** The canonical repository URL is **`github.com/leaderiop/SpecForge`**, defined once: the
  crates inherit Cargo's `repository`, and the LSP reads it from `CARGO_PKG_REPOSITORY`.
- **D6-c** *(amended)* The 30 registry codes (`R###`, `R-XXX-###`) are **catalogued as they are**. The
  18 codes that don't fit a family are renumbered into E/W, and duplicates fold into the codes they
  repeat. E048 is split in two, and W097 is catalogued.
- **D6-d** *(2026-10-01, Phase B)* The catalog moves to its own crate, `specforge-diagnostics`, with
  no dependencies, once four consumers outside the CLI were specified: the MCP `specforge.explain`
  tool, the catalogue title on every diagnostics JSON entry, doctor quoting the explanation when a
  diagnostic has no suggestion, and the LSP hover on a diagnostic. The LSP's docs links come from
  `specforge_diagnostics::docs_href`, not from a copy of `docs/diagnostics.md` in the binary.

## Repository (new)

- **N2** The gate checks this repository's spec **with its extensions loaded** (`specforge check` from
  the root), not `spec/` alone, which loads none. What that reveals is fixed or honestly unlinked.
