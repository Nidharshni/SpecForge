# One extension surface table; the host normalizes command args

**Status:** accepted (2026-10-06); D8's adapters are one module and D9's matching rule is the core resources' (ADR 0049).

An extension command is derived for each surface, and an extension surface was looked up five ways:

- **The derivation was split across three modules in three crates.** MCP's `registry.rs` flattened each
  declared arg to a `(name, "type")` string pair, the runtime crate (`specforge_wasm::surface`) turned
  those strings back into a JSON Schema and chose the tool's name, then `derived_input_schema` patched
  enum values, descriptions and `required` back in: never a default, never a minimum, and every
  required arg required, flags included. The CLI derived its own command line from the same
  declaration with other rules: it filled `default_value`, made a required flag optional and sent
  `false` for an unset flag. MCP passed the client's arguments as they came.
- **The SDK masked the default drift for SDK guests only.** It applied the default and checked the
  types guest-side (`CommandCall`); a guest declaring raw JSON got neither over MCP. Its host-option
  list duplicated the host's and compared names exactly, while the host spells `_` and `-` alike.
- **MCP served extension surfaces from four parallel structures joined by name at call time**
  (`tool_registry`, `resource_registry`, the registry's surfaces plus MCP's promoted ones, and the raw
  declarations for the command id and URI template). The joins were the bugs: no extension resource
  could be read (2fb7aa26: its URI was matched against the entry's name), a refused command was still
  promoted (49bfb73c), schemas came from a second lookup (d0f13f05, 375c23cf); a duplicate name was
  listed twice and dispatched once, and an extension tool named `specforge.validate` was listed beside
  the core tool and never dispatched. `registry.rs` had 17 fix commits in a month.
- **One tool answered with two error shapes.** A missing `milestone` was an McpError
  (`invalid_input`, a schema violation), `limit: -1` passed the schema (no `minimum`) and the guest
  answered its own `INVALID_INPUT`; the CLI's message for the same mistake differed again.
- **An extension resource failure was -32602 with its E028 code only in the message**, the one MCP
  path breaking `mcp_structured_error_responses`; a template outside `specforge://ext/` was listed and
  never readable.

## Decision

**D1. The derivation is one module in `specforge-ops`.** `specforge_ops::command::ExtensionCommand`
derives a declared command once, for every surface: its CLI name, its tool name
(`specforge.{short}.{id}`), each arg's value kind (`ArgValue`) and command-line shape (`ArgShape`: a
required arg that is not a flag is positional, every flag is a `--flag`), its typed default, the
tool's input schema, the args both surfaces send (`normalize`) and its refusal.
`ExtensionCommands` routes a project's commands by (short name, CLI name). The runtime crate keeps
the call seam and no naming or schema (`specforge_wasm::surface` is deleted).

**D2. The arg rule is one function, in `specforge-protocol-types`, run by the host and the SDK.**
`command_args::{normalize_arg, normalize_args, ArgError, refusal, HOST_OPTIONS}`: drift between guest
and host is impossible by construction. `CommandArgDescriptor` gains an optional `minimum` (wire
compatible both ways; a new closed-enum arg type would make an older host refuse a newer guest), set
to 0 by the SDK's `count()`. The builtins were re-vendored once.

**D3. Defaults are applied by the host, on both surfaces, and advertised.** An absent arg takes its
declared default, an unset flag is `false` (a flag is never required), each value is its declared
type (an integer or flag may come as the string a command line gives). The schema states each arg's
`default` (`false` for a flag) and `minimum`.

**D4. An undeclared argument is refused on every surface** (`additionalProperties: false`;
`unknown argument 'x'`, with the declared name it is close to).

**D5. A command tool's argument errors are the command's error object.** The host refuses before
the export runs with `ArgError::to_json()`, the `{code: INVALID_INPUT, message, suggestion?}` the CLI
writes under `--format json` for the same mistake (the CLI maps clap's usage errors onto the same
`ArgError`s). An explicit `mcp__` tool keeps the McpError schema checks: its schema is opaque JSON
to the host (ADR 0004 D4-a).

**D6. An extension resource failure is -32603 with an McpError in `data`** carrying the E028
diagnostic (`data.diagnostic.code`). "Unknown resource URI" stays -32602 for core and extension alike.
*(ADR 0024: an unknown URI is -32002 in a handshake session and -32602 in a 2026-07-28 request, as
each revision's server/resources says, with `data.uri`.)*

**D7. The table is the only authority on what MCP serves; each name once, first wins, I017 says why.**
`specforge_mcp::surface_table::ExtensionSurfaceTable`, built from the declarations whenever the
served environment loads (`McpState::applied`, `serve`) and emptied on shutdown, holds the tools
(an explicit `mcp__` tool with its schemas, or an `ExtensionCommand`) and the resources (with their
URI template). Precedence: core tools, explicit tools by extension load order, then commands by load
and declaration order. Not served, each with an I017 naming why: an explicit tool named as a core or
earlier tool, a resource whose URIs a core or earlier resource serves, a command whose tool name is
taken, a command the host refuses, a command another extension of the same short name routes first.
I017's title widens to "Extension surface not served under its name" (MCP-only: `check` keeps E039
for cross-extension duplicates).

**D8. Listing order is unchanged:** core in table order, explicit extension tools, then commands;
resources core then extension. `McpState` keeps one field, `surfaces`; listings are
`CORE_TOOLS`/`CORE_RESOURCES` then the table; a call is one lookup in it, then one of two adapters
(`mcp_tool_adapter`, `command_adapter`; plus the resource adapter), each one call across the
`WasmRuntime` seam through `ExtensionCalls` (ADR 0013).

**D9. An extension resource is matched by its template alone, after the core resources**, whatever
its scheme (the `specforge://ext/` gate goes). A template whose URI (each `{..}` probed with `x`) a
core resource or an earlier extension resource matches is not served.

**D10. Tests go through the seam.** `FakeExtension` declares tools and resources (`with_tool`,
`with_resource`, `declaring`) beside commands; no test injects descriptors into the state.

**D11. `short` is the declaration's** (`ExtensionDeclaration::short`, the SDK's `short` as
`ext_short`, else the name's last segment, ADR 0012).

**D12. Declaration contradictions are refusals on both surfaces**: a default its type refuses, a flag
with a default, a required arg with a default (with the host options and two args spelling one
option). The SDK panics on the same `refusal` when the extension is built, and on a required flag or
an empty `one_of`; the host holds a raw-JSON guest to the same rule, its required flag a flag.

**D13. Two extensions with one short name:** the first in load order routes a CLI name on both
surfaces; the later is reported (I017 over MCP, a note on stderr on the CLI).

**D14. Every project a call reaches has an extension runtime.** The served session's (the host's, or the
project's own) or the one-shot compile's for another project: so
`target::ProjectRef::runtime` is not optional, and the "the project has no extension runtime"
branches of the tool, resource, migrate, collect and gaps handlers (each answering differently: an
internal error, invalid params, a skipped pass) were unreachable and are deleted. Analyze always runs
the extensions' passes in the project's runtime. With nothing served there is no project:
an extension call is `precondition_failed` (-32602 with that McpError for a resource); ADR 0025.
*(ADR 0024: -32603, the code rule every refusal without `isError` follows.)*

## Consequences

- Over MCP an auto-promoted command now receives its declared defaults and `false` for an unset flag
  (a non-SDK guest too); `call.is_set("<flag>")` is `true` over MCP as on the command line.
- A required flag is no longer required over MCP. An agent sending an undeclared argument (such as
  `format`) gets `INVALID_INPUT` instead of having it silently ignored; a command tool's argument
  errors are `INVALID_INPUT` objects with the CLI's messages, not `invalid_input` McpErrors with
  `violations`, and no `surface_command_dispatched` event is recorded for a refused call.
- `tools/list` shows each command arg's `default`, its `minimum`, and `additionalProperties: false`
  (the product listing: 40 tools, 29 counts with `"minimum": 0`; its order is byte-identical).
- `specforge product features --limit -1` is refused by the command line (exit 2, `INVALID_INPUT`),
  and a count's message is "must be a non-negative integer, got 'abc'" on every surface (the SDK used
  to quote JSON-style `"abc"`).
- Duplicate tool names disappear from `tools/list`, and new I017 infos say why (none for the
  builtins). An extension resource template of any scheme is readable; one a core resource serves is
  not listed. An extension resource failure is -32603 with an McpError carrying E028.
- `resources/list` spells MCP's `mimeType` (it wrote `mime_type`).
- The builtins were re-vendored once (product's describe payload gains `minimum`).

## Rejected

- **A schema builder kept in the runtime crate**: the runtime would keep a protocol of string types
  for surface knowledge it does not need.
- **Defaults applied guest-side only**: works for SDK guests alone.
- **Omitting unset flags**: the CLI has always sent `false`; product's `--details` reads either way,
  but `is_set` would change on the command line.
- **A new diagnostic code for "not served"**: I017 is already MCP's "not served" report; an E-level
  code would make MCP's validate report errors `check` does not.
- **MCP's -32002 for unknown resources**: it touches every core resource; out of scope. *(Reopened and
  adopted by ADR 0024: one envelope writes every resource error, and the handshake revisions had
  -32002 all along.)*

## What would reopen it

A third surface needing a derivation `ExtensionCommand` cannot give (it grows a method, not a copy),
or MCP adopting -32002 for unknown resources.
