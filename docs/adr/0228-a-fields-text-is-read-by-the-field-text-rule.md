# A field's text is read by the field-text rule

**Status:** accepted (2026-10-22, architecture round 25, plan 08). Extends [ADR 0019](0019-one-entity-snapshot-per-compile.md) (the read views read the field text too), [ADR 0093](0093-a-fields-declared-type-decides-what-its-value-references.md) D1 (the parser reads syntax only: what a value says, like what it references, is never the parse's shape) and [ADR 0015](0015-read-views-are-operations-over-the-project-view.md) Q6 (search's text tier reads what its filter reads) and I1 (the headline statement is owned text with its field).

## Context

- A string field takes a date, an expression group, a type union and a bare word as well as a quoted string ([ADR 0147](0147-a-value-is-written-in-its-fields-one-form.md)). Only the bare word is rewritten by `format`; a date or an expression group is canonical.
- Five reads asked the parser's shape instead of the rule: search's text tier and its snippet (a quoted string only), the inspect headline (a quoted string only), the context export's headline (a quoted string or a bare word) and the hover, which skipped the headline by pointer-comparing a quoted string.
- So `contract 2026-10-22` or `contract expr { latency < 10ms }` vanished from the context export, MCP inspect's `contract`, the context prompt's `contract_text`, the hover summary and the search's text tier, while markdown and the search field filter (which ask the rule) showed it. `c936eac8d` had fixed one reader of five.

```text
BEFORE  search tier | snippet | headline_statement | headline_fields | hover is_headline   FieldValue::String(..)
        markdown, field filter                                                           field_text   (the only two)

AFTER   stored FieldValue --> snapshot::field_text / field_items --> search, snippet, filter, Headline, hover, markdown
```

## Decisions

- **D1.** A read view that asks what a field says asks `snapshot::field_text` / `field_items`. A shape test stays only for display (the hover's value formatting), layout (markdown prose) and readers of structure (`prove`, a typed edit's comparison).
- **D2.** Search's text tier, its snippet and its field filter read one list of texts per field: a list-shaped value's items, else its one text. A snippet is the matching item.
- **D3.** The headline is the first field its extension declares headline and normative, whatever its shape, with its field text (`specforge_emitter::context::Headline { field, text }`). The context export lifts every headline field as its field text; the hover skips the headline by its field. An expression group reads as its field text, conjuncts joined by `", "`.

## Consequences

- A dated or expression contract appears in the context export, MCP inspect's `contract`, the context prompt and the hover summary; search finds field text in every field, at the field-text tier, after id and title matches.
- No schema change (the context node already admits any lifted string key), no Graph Protocol bump, no re-vendor. This repository's context export is unchanged: every headline here is a quoted contract or a status word.
- Rust API: `context::headline_statement` returns `Option<Headline>`; `EntityFacts::headline` is `Option<Headline>`.

## What would reopen it

- A read view that needs a field's structure rather than its text (it reads the value, as `prove` does).
- A headline that must not be text.
