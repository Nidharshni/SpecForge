# A core tool is one module; extension dispatch is its own module

**Status:** accepted (2026-10-11). Amends ADR 0024 A1 (what a tool spec is), ADR 0048 D2 (where
the reply type is named) and D9, ADR 0017 D8 (the adapters' module and names) and D9 (the URI rule),
and ADR 0015 P7 (how prompts name tools). D1 applied to prompts by ADR 0058. D7's placeholder reading for core
resources is the template's (ADR 0103).

A core MCP tool was stated in three code places: its module (`Args`, `Reply`, `call`), a `mod` line,
and an entry in `tools/table.rs` (502 lines) that restated the handler's input, arguments and reply
through five `macro_rules!` with eight near-identical arms. The arms built one of five
`Handler`/`MutationHandler` variants with identical fields but a target, each forwarding through
three `match` methods. Each change to what a tool declares (round 5: its effect, its target, its typed
reply, its hints) rewrote all 34 entries and the macros. The reply test kept its own table of each
tool's spec type (`REPLIES`) and the open-values test its own (`OPEN`); the prompts named tools
through a lookup that panicked at runtime on a typo. ADR 0024 A1 said a tool spec holds an output
schema, which the code derived from the handler, and ADR 0048 D2 named a `typed!` macro that did not
exist. The extension adapters lived in the core tools' module, with a second `schema_mismatch` builder
and a second duration helper, and core and extension resources matched URIs by two rules that
disagreed on an empty placeholder and on a query.

## Decision

- **D1. A core tool is one module.** It implements `CoreTool` on its unit struct `Tool`: `NAME`,
  `DESCRIPTION`, `EFFECT`, `SPEC` (where `spec/types` states its reply; `Text` by default),
  `OPEN_VALUES` (its outputSchema's open values), `type Args`, `type Input`, `type Returns` and
  `fn call`. `CORE_TOOLS` lists `spec(&x::Tool)` once per tool, in listing order. `tools/table.rs`,
  its macros, `Handler` and `MutationHandler` are deleted.
- **D2. The handler's input is a type, and it is the target** (ADR 0024 A2 kept): `Unscoped`,
  `View<R>`, `Project<R>` or `NewProject`, `R` one of `Served`, `ServedUnlessCached`, `AnyProject`,
  `AnyUnlessCached`. A project handler takes `ProjectRef` by value; init takes `NewProjectDir` (the
  directory and the runtime its extensions' declarations are read in).
  *(ADR 0077: `R` is `Served` or `AnyProject`; the `UnlessCached` markers are gone.)*
- **D3. What the handler returns is the reply kind** (`Handled`): `Answered<Text>`, `Answered<R>` or
  `Mutation<R>` (`R: MutationReply`). It gives the outputSchema (the one place a core tool's schema is
  built), whether the tool is typed, the refusal before the handler ran, and the rest of the call (a
  read's reply checked; a mutation completed by `mutation::complete`, ADR 0022). A mutation's
  arguments say whether it previews (`MutationArgs`), read before the handler runs through the bound
  `type Args: Arguments + Previews<Self::Returns>`.
- **D4. Contradictions do not build.** `spec()` asserts, when the table is built, that the effect is
  `Mutates` exactly when the handler answers a `Mutation`, and that the tool names a spec type exactly
  when its reply is typed; the bounds require a mutation's `Args` to be `MutationArgs` and its `Reply` a
  `MutationReply`. `cargo check` fails on any of these.
- **D5. One erased interface.** `ToolSpec`, implemented once for every `CoreTool`, is what listings,
  the tools adapter, events and the reflection tests read; tool names are typed constants
  (`validate::Tool::NAME`), so a prompt cannot name a tool that does not exist.
- **D6. Extension dispatch is one module** (`extension_dispatch`): `mcp_tool_adapter`,
  `command_adapter` and `resource_adapter`, one call each over the `WasmRuntime` seam, with one
  duration helper. A structured result its output schema refuses is one rule for core and extension
  tools (`reply::conforming`), its message naming the tool and, for an extension's, the extension.
- **D7. One URI rule** (`uri_template`): a template names every URI that starts with its text before
  its first placeholder, empty value included; a URI without one names itself, alone or followed by a
  query. A placeholder's value and a query are the reader's to read or refuse. This is the rule core
  resources followed; extension resources now follow it too.
- **D8. The reflection tests read each tool's declarations**: its spec type and its open values. No
  test keeps a per-tool table.

## Consequences

- `tools/list`, every core reply and every core refusal are byte-identical.
- User-visible over MCP: an extension resource read with an empty placeholder (`acme://doc/`) or with
  a query on an untemplated URI reaches its export instead of "Unknown resource URI", and
  `resources/subscribe` accepts it; an extension resource whose plain URI an earlier template names is
  not served (I017). An extension tool's `schema_mismatch` message reads "tool '<name>' of
  '<extension>' answered a reply its output schema refuses: …".
- `tools/call` no longer builds a tool's output schema to learn whether it has one; the remaining
  build per call is `Handled::output_schema`'s, the one place to memoize it.
- Adding a core tool: one module, one `mod` line, one table line, its spec type in
  `spec/types/mcp.spec`, and the re-blessed listing and reply snapshots.

## Rejected

- **A const descriptor struct with function pointers**, filled by a generic `const fn`: a hand-written
  vtable of seven function pointers in place of the language's, and two items per tool module.
- **Macros kept, one per module**: the glue and the five handler variants stay.
- **`const TARGET: ProjectTarget` with a default**: meaningless for explain and init.
- **`&ProjectRef` through a higher-ranked closure**: closure inference over an associated-type
  projection, for no handler that needs the reference.
- **Two traits, for reads and mutations**: overlapping blanket impls need wrapper types in the table,
  and name, description, arguments and input are stated in both.
- **Extension tools as `ToolSpec`s**: their arguments are opaque JSON (ADR 0004 D4-a), their target
  fixed, their run a dispatch with an event.
- **The extension URI rule for both**: `specforge://graph/` would stop being the entity resource's
  malformed-ID refusal and `specforge://graph?scope=a` would be unknown.
- **Runtime tests for the agreements**: a build check is stronger and needs no test.

## What would reopen it

An extension tool with typed arguments (D8); a core tool whose input is none of the four (a prompt-like
read of something other than a project); a reply kind beyond text, typed and mutation; an MCP revision
whose resource templates are matched by a different rule.
