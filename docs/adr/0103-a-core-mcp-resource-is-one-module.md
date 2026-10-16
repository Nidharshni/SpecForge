# A core MCP resource is one module, and the methods are one table

**Status:** accepted (2026-10-15, round 10, plan 12). Amends ADR 0049 D7 (who reads a placeholder's value) and ADR 0077
D3 (where `follow_root` is called). Applies ADR 0049 D1 and ADR 0058 D6 to resources. Amended by ADR 0111 (D4: a read
also carries notices).

A core tool and a core prompt are each one module implementing a trait, but a core resource was a row of a closure
table (`CORE_RESOURCES`). Each templated resource wrote its URI prefix twice (the row's `uri` and the reader's
`strip_prefix`) and read its placeholder its own way, so one malformed value read differently per resource:
`specforge://graph/` was refused, `specforge://entities/` listed every entity, and `specforge://entities/behavior/extra`
answered `[]` with no word, where `specforge.list {kind: "behavior/extra"}` reports I020. Beside it, the MCP method
names were written four times (the router's lifecycle arms and session arms, the stateless dispatch match, the
`CACHEABLE` list) and `follow_root` was called by both the stateless entry and the router.

## Decision

- **D1. One module per core resource.** `specforge_mcp::resource::CoreResource` states a resource once: `URI`, `NAME`,
  `DESCRIPTION`, `MIME_TYPE`, `CHANGES_WITH` and `read`. `ResourceSpec` is what the listing, the adapter, the
  subscriptions and the reflection tests read, implemented once for every `CoreResource`; `CORE_RESOURCES` is a
  `&[&dyn ResourceSpec]`, the shape of `CORE_TOOLS` and `CORE_PROMPTS`.
- **D2. The template reads its placeholders.** `uri_template::read` gives each `{placeholder}` of a core template as one
  non-empty, percent-decoded path segment (`Matched`), else `Malformed` naming it. `%2F` stays inside one segment.
  Naming is unchanged (ADR 0049 D7, first sentence): a URI the template names whose placeholder is malformed is
  `invalid_input` naming the placeholder, never "Unknown resource URI". Core resources only: extension resources still
  receive the URI whole.
- **D3. An empty placeholder is refused for every core template.** `specforge://entities/` and
  `specforge://entities/<a>/<b>` are `invalid_input` naming `kind`. `specforge.list` with no kind (or an empty one)
  still lists everything: it is an optional argument, not a URI segment.
- **D4. Content stays text.** `ReadOutcome` is `ResourceText { mime_type, text }`: the graph views are the export's
  bytes (ADR 0024 D4), so only two of eight resources serialize a typed value.
  *(ADR 0111: `ResourceText` also carries notices, sent as the read result's `_meta.diagnostics`.)*
- **D5. The methods are one table.** `protocol::method::METHODS` names each method, the revisions that serve it and
  whether the stateless result is cacheable. The router dispatches from it and returns the one -32601.
- **D6. `follow_root` is called once, by the router,** before every session method in both revisions. A stateless
  `server/discover` or an unknown method reads nothing and serves nothing; the invariant reads "every request that
  reads what is served".

## Consequences

`entities/` and `entities/a/b` are invalid input naming `kind`; the entity refusals read `Malformed entity_id: ...`
(was `Malformed entity ID: ...`); listings and every other read are byte-identical. A stateless request carrying an id
for `notifications/cancelled` reaches the cancel handler (it was -32601), as its table row says.

## Rejected

- The closure table kept: eight rows restating prefixes.
- One trait for tools, prompts and resources: their declared parts differ; `Surface` already shares the pipeline.
- Typed placeholder structs: every core template has at most one placeholder.
- Refusing at naming time: `specforge://graph/` would become "Unknown resource URI" (ADR 0049 Rejected).
- Giving extension resources `Matched`: it changes `McpResourceRequest` on the wire.

## What would reopen it

A core template with two placeholders or one that may span segments (RFC 6570 `{/var*}`, `{+var}`); an MCP revision
that matches templates differently.
