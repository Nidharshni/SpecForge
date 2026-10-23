# A reviewer reads the graph export as Markdown

**Status:** accepted (2026-10-18, syntax round, plan 06). Amended by [ADR 0222](0222-what-an-entity-holds-is-written-once.md): the document lists methods and each field's annotations; the reopening case "methods in every export format" is answered for graph, context and markdown. Amends [ADR 0007](0007-emitter-is-export-formats.md) (the emitter's formats) and [ADR 0027](0027-an-enumerated-argument-is-one-option-table.md) (the export tool's table).

Every graph export was JSON or Graphviz. A person who signs a specification needed a tool to read it, and a
hand conversion drops a reference that does not resolve (no export carries it).

## Decision

- **D1. Markdown is a graph export format** (`EmitFormat::Markdown`, ops `Format::Markdown`, named `markdown`):
  `specforge export --format markdown`, `specforge.export`, and `specforge.render` (file `specification.md`).
  It is rendered beside DOT, before the JSON shapes: not budgeted, no schema, no coverage, no timestamp.
- **D2. It knows no kind.** A section is built from declarations only: a `normative` field is quoted, a field
  whose value `Reading::references` names entities is a Links row, a prose-shaped value (multi-line string,
  list of quoted strings, block) is a paragraph or list, anything else a Fields row. No template, no field name.
- **D3. Sections are selected, names are not.** `scope`/`depth`/`kinds` choose the sections; links, their titles
  and the referrers come from the whole graph.
- **D4. Order:** the scoped entity, then `(kind, id)` by bytes. Links in field and written order; referrers by
  `(kind, id, field)`; obligations numbered in declaration order.
- **D5. Text renders as written:** outside code spans `\`, `<` and `&` are escaped, so prose is CommonMark
  without raw HTML; table cells also escape `|` and keep one line.
- **D6. One quote:** the hover's statement quote is the emitter's `markdown::quote`, reached through
  `specforge_ops::read::inspect::quote`; the document escapes before it quotes, the hover does not escape.
- **D7. An unresolved reference is a row**, `*unresolved*`, never dropped.
- **D8. `specforge.export` reads `FORMAT`** (every export format), not `AGENT_FORMAT`, which stays the formats
  with a published JSON Schema (query, `schema --publish`).

## Rejected

- Extension-provided templates (a manifest field, an SDK type, a template language, a re-vendor) before any kind
  needs a layout the declarations cannot express.
- Reusing inspect's `EntityFacts`: it is an ops read view of one entity with coverage and diagnostics; the
  emitter is below ops.
- Grouping by file: each section shows its `file:line`.

## What would reopen it

A regulated layout with fixed headings per kind; a reviewer who needs coverage in the same document (then the
export takes `coverage` as query does); methods in every export format.
