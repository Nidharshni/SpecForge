# A kinds list written as text is read by one rule

**Status:** accepted (2026-10-22, architecture round 24, plan 08). Extends [ADR 0101](0101-an-operations-request-owns-its-meaning.md) D1 (a reader the surfaces call: `view::kinds_of_text`). Amends [ADR 0024](0024-one-mcp-request-pipeline.md) D4 (a graph view's `kinds` is a kinds list; what each entity template reaches). Holds [ADR 0177](0177-a-reads-selection-is-checked-by-the-project-view.md) S5 on the graph resources.

## Context

- A kind filter written as text had four readings. clap split `specforge export --kinds` and `specforge model --kinds` at
  commas and kept the spaces. `specforge query --kind` never split. The graph resources split, trimmed and refused an
  empty item. `type, event` was two kinds to `specforge://brief` and `type` plus I020 `' event'` to `specforge export`.
  `type,` was I020 `''` on the CLI (ADR 0177 S5) and `-32602` on the resource, which ADR 0177 rejected. ADR 0024 D4's
  byte-equality failed for those requests.
- ADR 0177 unified what a kind filter means (I020, `KindFilter`), not how a list written as text is read. The pairing
  table (ADR 0101 D5) paired tools only and never saw a resource's query.

## Decision

- **D1.** A kinds list written as text (`specforge export --kinds`, `specforge model --kinds`, `specforge query
  --kinds`, a graph resource's `kinds` query) is read by `specforge_ops::view::kinds_of_text`: the items between its
  commas, in order, each without the whitespace around it. An empty item is the empty kind: I020 `unknown entity kind
  ''`, keeping nothing (ADR 0177 S5). A flag given twice adds its items. Names stay exact (ADR 0015 Q3): a kind is an
  identifier, so the whitespace was never part of one.
- **D2.** A list that arrives as a list (an MCP tool's `kinds` array) is not text: its items are the kinds as given.
- **D3.** The surfaces spell (ADR 0101 D2). clap takes each `--kinds` as text, with no `value_delimiter`. The resource
  percent-decodes the value. Neither splits, trims nor refuses. A repeated `kinds` query key stays refused (ADR 0024 D4:
  a URI says a list with commas).
- **D4.** A list of kinds is spelled `--kinds` on the CLI and `kinds` over MCP; an argument that takes one kind stays
  `--kind` / `kind` (`schema`, `explore`, `coverage`, `list`).
- **D5.** `specforge://graph/{entity_id}` is the query of its entity (ADR 0015 Q2), at `query::DEFAULT_DEPTH`.
  `specforge://context/{entity_id}` is the context export scoped by its template, everything the entity reaches, as
  `context?scope=` and `specforge export --format context --scope` are. A `depth` query narrows or widens either, and
  each resource's description says which it is. A template's entity ID that no grammar admits stays `invalid_input`
  naming `entity_id` (the 400 case): it is the URI's own path segment. A `scope` query value is the export's scope,
  E003 when it names no entity, as on the CLI and the tool.

## Rejected

- Refusing whitespace or an empty item: reverses ADR 0177 S5 and needs a `Kinds` variant on three request field enums.
- Exact items (no trim): it reports as unknown a name the user wrote correctly inside a natural list.
- A clap `value_parser` per flag: clap would split a second time, or return a nested list.
- The reader beside `export::Request`: the model and the query read kinds lists too. It sits beside `KindFilter`.
- Reading an MCP tool's array items as text: a client that sends `"a,b"` as one item named that kind.

## Consequences

- CLI: `specforge export|model --kinds 'a, b'` keeps `b`; `specforge query --kind` is `--kinds` and splits.
- MCP: `specforge://graph|context|brief?kinds=a,` and `?kinds=` answer with I020 `''` instead of `-32602`.
- Rust: `specforge_ops::view::kinds_of_text` is new; `specforge_mcp`'s resource query no longer refuses `kinds`.
- The pairing table reads resources, and pairs `--kinds` text with `?kinds=` text.

```
┌─ text ───────────────┐   ┌──────────────────────────────┐   ┌────────────────────────┐
│ --kinds 'a, b,'      │──▶│ view::kinds_of_text          │──▶│ KindFilter: a, b, ''   │
│ ?kinds=a,%20b,       │   │ split ',' · trim · keep ''   │   │ I020 '' (ADR 0177 S5) │
└──────────────────────┘   └──────────────────────────────┘   └────────────────────────┘
```

## Reopen when

A kind name may hold whitespace or a comma (a grammar change), or a new surface writes a kinds list another way.
