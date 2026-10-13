# A reply's output schema is one document: named shapes are defined once

**Status:** accepted (2026-10-11). Amends ADR 0048 D1 ("inline"), D3 (the keywords the checker
reads), "Rejected: schemars" and "What would reopen it".

ADR 0048 derived each core tool's outputSchema from its reply type, every subschema inline. Three
things followed:

- **A root that is no object.** The derive called an untagged union of newtypes "not an object"
  from its syntax. Trace's and explain's outputSchema roots were `{"oneOf": […]}` with no
  `"type": "object"`, which MCP 2025-06-18 and 2025-11-25 require. The TypeScript SDK (1.32.1,
  2.3.1) and the Python SDK (`mcp` 2.3.0) refused the whole `tools/list`; a client built on them
  saw no SpecForge tool.
- **One type written many times in one document.** Migrate's schema held the diagnostic schema
  three times (6.9 of its 10.2 KB), query's the Graph Protocol schema three times, outline the
  source span four times.
- **A checker that could not follow a reference, and identity that compared nothing.** ADR 0048 D8
  said the domain types a reply embeds are "compared by identity"; with no name, the reply/spec test
  skipped them. Each `tools/call` also built its tool's schema twice, once to check the reply and
  once to learn that the tool had a schema.

Measured on the round-6 binary: `tools/list` is 84.7 KB at 2025-06-18, of which output schemas
are 61.0 KB; nine inline diagnostic schemas are 20.7 KB of it.

## Decision

- **D1. A shape can be named.** `#[shape(define)]` (or `define = "Name"`) gives a type a name
  (`Shape::NAME`). A document defines a named shape once under its root's `$defs` and refers to it
  as `{"$ref": "#/$defs/<name>"}` wherever it appears, even when it appears once. Every other
  subschema stays inline. A name belongs to one type, and a type with type parameters cannot be
  named. A named shape may be recursive. Outside MCP, the published Graph Protocol schema (ADR 0064) is held to
  the same subset: `shape::unchecked_keywords` lists any keyword `violations` would ignore.
- **D2. Which types are named.** A domain type another spec file declares that a reply embeds
  (`Diagnostic`, `MigrationResult`, `MigrationDiff`, `RollbackSummary`), and any type whose object
  schema a core tool's document would otherwise hold more than once (`SourceSpan`, `FormatVersion`,
  `TraceLink`, `GraphProtocolSchema`, `SchemaRefBlock`, `Edge`, `TokenBudget`). A test refuses a
  document that repeats an object schema.
- **D3. A document's root is its type's own body, never a reference, and an `Object`'s root states
  `"type": "object"`.** A union states it when every branch is an object (a reference: when the
  body it names is one), decided from the branches, not the derive's syntax.
- **D4. One document per type, built once per process** (`shape::document`, keyed by type). The
  tool lists it as its outputSchema and checks every reply against it. Input schemas stay inline
  (ADR 0033).
- **D5. The checker resolves `$ref` within its document.** `shape::violations` follows a
  same-document JSON Pointer (`#/$defs/…`, `#/definitions/…`, `#`) and applies the reference's
  siblings. It never fetches any other reference: that reference, one naming nothing, or a chain
  of references that never ends is a violation. Extension tools' declared schemas are checked the
  same way.
- **D6. `flatten` over a union keeps writing the outer fields into each branch.** A branch that is
  a reference is merged as the body it names. A closed union cannot share its branches' bodies
  without `unevaluatedProperties`, which draft-07 validators ignore. The copies cost 1.1 KB across
  the listing.
- **D7. No `$schema`.** The dialect is MCP's default, JSON Schema 2020-12. A reference has no
  sibling keys, so draft-07 readers agree.

## Consequences

- User-visible over MCP:
  - trace's and explain's outputSchema roots gain `"type": "object"`, and SDK clients list the
    server's tools again;
  - thirteen core outputSchemas carry `$defs` and refer to `Diagnostic`, `SourceSpan` and the other
    named shapes with `$ref`;
  - core `tools/list` shrinks from 84.7 KB to about 74.4 KB;
  - an extension tool whose declared input or output schema uses a same-document `$ref` is checked
    below the reference, and one using any other `$ref` refuses every call (`invalid_input`) or every
    output (`schema_mismatch`).
- Each tool's schema is one value for the life of the server; a tool call no longer builds it.
- The reply/spec test checks that a shared domain type is a reference to the definition of that
  name, where it used to skip it.
- Each tool's schema is its own document, so a type is defined once per tool, not once per listing.

## Rejected

- **One `$defs` for the whole listing.** MCP lists each tool's schemas as independent documents. A
  client resolves a reference within one tool's schema only (the Python SDK validates each with an
  empty registry; SEP-2106 forbids fetching references). There is no shared root.
- **Inlining a named shape used once.** A type's spelling would depend on its siblings, and the
  reply/spec identity check would accept two forms, for 0.8 KB.
- **An open `data: {}` in reply diagnostics.** It would save 12.6 KB by dropping the typed payload
  that lets a client validate a diagnostic.
- **`allOf` with `unevaluatedProperties: false` for a flattened union.** The TypeScript SDK 1.x
  validator (Ajv, draft-07 class) ignores the keyword and accepts any extra key.
- **A JSON Schema validator crate as a test oracle.** The crate releases weekly; the clients that
  matter run in JavaScript and Python and are scripted for a manual check. The published Graph Protocol schema is validated by the in-repo checker for the
  same reason (ADR 0064).
- **schemars** stays rejected (ADR 0048). Its output would need a transform for every schema (root
  `title`, `$schema`, `format`). Its references are now SpecForge's own, so they are no longer a
  reason.

## What would reopen it

A client that refuses a same-document `$ref` in an outputSchema. The named shapes then go back to
inline by removing their `define` attributes; the document builder and the checker stay. Or an MCP
revision that lets a listing share definitions across tools.
