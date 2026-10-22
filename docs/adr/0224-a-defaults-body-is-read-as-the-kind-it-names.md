# A defaults body is read as the kind it names

**Status:** accepted (2026-10-22, architecture round 25, plan 04). Extends [ADR 0147](0147-a-value-is-written-in-its-fields-one-form.md) D6 (a `defaults <kind>` body is formatted in the forms of the kind it names) and amends [ADR 0149](0149-a-file-states-a-kinds-shared-fields-once.md) D10 for W159 only; names D9's rule (an entry is inherited when its value span lies outside its node's span) in code, `checks::value_forms::inherited`. D4 of 0147 and D8 of 0149 hold.

## Context

- A `defaults behavior { category "auth" }` block parses as an entity block whose keyword is `defaults` and whose name is a kind. The graph (`FileDefaults::of`) and the editor (`kind_of_frame`) read its body as `behavior`; the formatter read it under the keyword, `defaults`, a kind no extension declares, so it rewrote nothing: `format --check` exited 0, MCP format answered all clean and the editor's formatting returned no edit, while `check` warned W159 for the same tokens.
- W159 for a default's value was reported once per block that takes it, each naming an innocent block: two values and three blocks gave six warnings, and three stacked squiggles per line in the editor.
- The rule "which kind is this body" had been written inline where it was needed (the editor, 2026-10-18), so the reader that did not need it on the first day never got it.

```text
BEFORE  formatter: kind = "defaults" -> no form          graph, editor: kind = "behavior"
        W159: one per rewrite per record (N takers -> N warnings naming N blocks)

AFTER   structural::body_kind(keyword, name)  <- formatter, editor (graph: the same answer, by name)
        W159: an inherited span is reported once, naming `defaults <kind>`
```

## Decisions

- **D1.** The kind a block's body is read as is `specforge_common::structural::body_kind`: the name a `defaults` header gives, else the keyword. The formatter and the editor ask it; the graph's defaults module reads a defaults block by its name, the same answer.
- **D2.** Format writes a defaults block's values in the forms of the kind it names (extends 0147 D6). D5 (kinds that take members) applies through the named kind; an undeclared kind has no forms, so nothing is rewritten.
- **D3.** A value a defaults block writes in the other form is one W159, at the default, naming `defaults <kind>` (amends 0149 D10 for W159). An own value is unchanged.
- **D4.** Every other diagnostic about an inherited value still names each block that takes it (0149 D10 stands).
- **D5.** A default no block takes is W161, not W159; format still rewrites it.

## Rejected

- A formatter-local `match` on the keyword: a second copy of the rule.
- A method on the parser's `Entity`: the formatter walks the CST and the editor its lexemes; neither builds one.
- An `inherited` flag on the record: 0149 already rejected it, the span says it.
- Moving W159 into the graph's defaults module: it would copy E061's admission rule into a second crate.

## Consequences

- Breaking diagnostics output: fewer W159 for an off-form default, its message names `defaults <kind>`.
- `format --check` fails on a project whose defaults hold an off-form value; `format` fixes it and the graph is unchanged.
- No new code, no re-vendor.

## What would reopen it

A second structural form whose body is read as another kind; a holder-dependent W159.
