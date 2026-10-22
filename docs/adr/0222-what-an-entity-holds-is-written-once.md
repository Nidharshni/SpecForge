# What an entity holds is written once

**Status:** accepted (2026-10-22, architecture round 25, plan 02). Amends [ADR 0152](0152-a-reviewer-reads-the-graph-export-as-markdown.md) (the document lists methods and each field's annotations; the reopening case "methods in every export format" is answered for graph, context and markdown). Extends [ADR 0094](0094-obligations-are-an-entitys-own-member.md) (the node's methods and annotations are written beside its `verify`), [ADR 0064](0064-a-declared-field-type-decides-one-wire-form.md) (annotations beside `FieldJson`, which is unchanged), [ADR 0006](0006-one-incremental-rebuild.md) (the delta diffs the written members) and [ADR 0058](0058-every-mcp-message-is-a-spec-type.md) (inspect and the context prompt carry `annotations` and `methods`).

## Context

- Eleven places wrote "what an entity holds", each its own way. The graph export, the context export, the Markdown review document, MCP inspect, the context prompt, the hover, the outline and the graph delta each kept a private copy of the field writer; the delta and the outline each had a private method writer.
- A method (`method save(order: Order @optional) -> Result<void, never>`) and an annotation after a field (`id string @optional @readonly`) reached no export: a node carried its fields' values and nothing of how they were written. The outline showed a method without its parameters' annotations; the delta saw a top-level annotation change but not one inside a block field's entry.
- A key written twice was written differently by different readers.

```text
BEFORE  delta.rs field_contents()/methods()   files.rs format!("{}{?}: {}")   json.rs / context.rs / markdown / inspect / prompt / hover
        each: write_fields(&node.fields) or its own walk; no annotations, no methods in an export

AFTER   specforge_graph::Members::of(node) -- fields (FieldJson) | annotations (by path) | methods (MethodJson)
        graph, context, markdown, inspect, context prompt, hover, outline and delta read it; write_fields is crate-private
```

## Decisions

- **D1.** One module, `specforge_graph::members`, beside `field_form` and `Obligation` (moved in): `Members::new`/`of`, `MethodJson::signature`, `AnnotationJson::spelled`. No position and no parse tree is in any of its types.
- **D2.** Annotations are at node level, by path: a field's name, or `outer.key` for an entry of the block field `outer`, at any depth. A field with none is absent.
- **D3.** Methods are structured (`MethodJson`: name, parameters with type, `?` and annotations, return type); `signature` is the line `specforge format` writes.
- **D4.** Every export of a node but the brief and DOT, MCP inspect, the context prompt, the hover and the outline read it. The context export keeps the annotations of the fields it keeps.
- **D5.** The delta diffs the members: an annotation is its field's change at any depth, a method edit the `methods` aspect, a quoting-only edit nothing. A key written twice is its first entry everywhere.
- **D6.** No version moves. `GraphNode`, `PassEntity`, the validator context and the command input are unchanged; the new keys are optional and absent when empty.

## Rejected

- Annotations as a field's `FieldJson`: it would change every `fields` object and the published `fields_<kind>` schemas.
- A node aspect for annotations: an annotation is its field's, as the delta already reported at the top level.
- A `Serialize` for `Members`: each reader places its three parts under its own keys.
- Methods as text only; a second annotation spelling (it is `spell::quoted`'s).

## Consequences

- `specforge export`, `query` and the graph resources gain optional `annotations` and `methods` keys on a node that has them; a node with neither is byte-for-byte what it was. The published schema gains the two properties and three `$defs`.
- `field_form::write_fields` is no longer public (Rust API of `specforge-graph`).

## What would reopen it

- A command or a pass that must read a method typed.
- A reader that needs a member's position.
