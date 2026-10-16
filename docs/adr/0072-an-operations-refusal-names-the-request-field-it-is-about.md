# An operation's refusal names the request field it is about

**Status:** accepted (2026-10-13, architecture round 9, plan 02). Supersedes ADR 0033's round-8 amendment; amends
ADR 0029 D2. D1's last sentence is superseded by ADR 0132: no operation decides a subject by variant.

Round 8 had each core MCP tool declare which argument each refusal *code* of its operations is about
(`CoreTool::REFUSAL_ARGUMENTS`), applied by the dispatcher, and a debug build panic on an `invalid_input` refusal
the table missed. A tool cannot know which codes its operation's call graph raises: on the day it merged the table
missed E064, E028, E072, R-RES-003, R-RES-004 and R008 on `add_extension`, `init` and `remove_extension` (a typo
answered `-32603 Internal error` in a debug build, `invalid_input` with no argument in a release build), named
`E027` for an export refusal whose code is `incompatible_schema_version`, and no refusal of another kind about an
argument (`entity_not_found` for an `entity_id`, `conflict` for a `new_name`) named it, although
`read_mcp_arguments_as_declared` requires it.

## D1. The operation states the subject, typed

An operation that can refuse because of a value its caller gave returns `Result<T, specforge_ops::Refusal<F>>`.
`F` is its field enum: one variant per field of its request a refusal can be about, named as the request names it.
`Refusal { error: OpError, subject: Option<F> }`. The operation states the subject where it reads the field
(`error.about(AddField::Extensions)`, often once per field for everything reading it can refuse); an operation that
runs another maps the other's subject (`map_subject`). Typed errors that already name a variant (`CheckError`,
`AnalyzeError`, `TraceError`) decide the subject by variant in `From<…> for Refusal<XField>`.

## D2. Only a refusal of a value has a subject

`Refusal::about` keeps the field only when the error's kind rejects a value the caller gave
(`OpErrorKind::rejects_a_value`: invalid input, entity / file / extension not found, conflict). A refusal of the
project or the environment (precondition, schema mismatch, permission, timeout, internal, compilation) is about no
field. So `invalid_input` means a value the caller gave: a fetched registry binary that is not what was published,
a malformed declared peer name and a registry reply naming no package are `schema_mismatch`; an unknown collect
runner is `invalid_input`.

## D3. Each surface spells each field once, exhaustively

An MCP tool or prompt implements `ArgumentOf<F>` for each field enum of the operations it calls: one `match`, no
wildcard, to the argument that sends the field or to none for a field it fills itself or never sends.
`McpError::refused` / `Failed::refused` name it. A resource maps per call (its scope key is `scope`, `root` or the
template's `entity_id`). There is no `From<Refusal<F>>` into an MCP error: a tool that calls the operation does not
compile until it names the fields. The CLI and the LSP present the error and ignore the subject. The spelling of an
argument stays the surface's (ADR 0029 D2): `InitField::Dir` is `path` over MCP.

## D4. No table, no runtime guard

`REFUSAL_ARGUMENTS`, `McpError.refusal_code`, `name_argument`, the debug panic, the by-code naming of graph
resources and the call-site naming of `kind_guide`'s refusal are gone. The compiler checks the maps; tests pin each
raise site's subject. The call target's own arguments are `TargetArgument` (`path`, `use_cached`), and each
`TargetError` names one. A prompt's or resource's refusal that no argument fixes (an unusable test report) stays
JSON-RPC `-32603`, its `data` the McpError with its code and diagnostic.

## Consequences

- User-visible over MCP: `entity_not_found`, `extension_not_found`, `file_not_found` and `conflict` refusals about an
  argument name it (`entity_id`, `scope`, `name`, `new_name`, `test_results`, `session_id`, `schema_version`); a graph
  resource names the query key; the four kind corrections of D2 change `code`. JSON-RPC codes of prompts and
  resources are unchanged (a refusal no argument fixes stays -32603).
- 20 operations return `Refusal<F>`; their CLI and LSP callers read `refusal.error`.
- A read-view field enum shared by several operations (`EntityField`) and a kind guide's own (`KindGuideField`,
  since `GuideField` already names a field of the guide's document) are the two places the plan's names moved.

## Rejected

- **Keeping the table with the missing rows added**: the next refusal deep in the lock plan or the registry is
  missed the same way; the guard turns it into `-32603`.
- **A field name (`&'static str`) inside `OpError`, or one ops-wide `Subject` enum**: a tool's map needs a wildcard,
  so a new field is silently unnamed.
- **Deriving the map from `#[arg(…)]` attributes**: fields a surface fills itself or never sends need a second
  attribute vocabulary, and a tool calling two operations needs one attribute per operation per argument.

## What would reopen it

An operation whose refusal is about two fields at once (the subject becomes a set), or a surface that must name a
field differently per call beyond a resource's query key.
