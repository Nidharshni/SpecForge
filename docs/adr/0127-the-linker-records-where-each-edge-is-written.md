# The linker records where each edge is written

**Status:** accepted (2026-10-16, architecture round 13, plan 05). Refines [ADR 0016](0016-navigation-is-one-module.md) D2 (the
token of a reference is the graph's, decided once) and drops its block scan; agrees with [ADR 0117](0117-a-reference-is-decided-in-the-linker.md)
(D3's span is the one recorded) and [ADR 0116](0116-navigation-reads-the-text-the-project-was-compiled-from.md) (positions are in the compiled text). D5 amended by [ADR 0163](0163-a-written-token-is-found-by-one-finder.md): navigation's tokens ask the graph's finder over the compiled parses; its holders still read `Graph::written`.

## Context

- The linker computes where each reference is written (`Reference.span`, which E003, I004 and E022 point at), then makes an
  `Edge` of the id alone: the span is dropped. The derived walk never computes a position at all; it splits type strings.
- Navigation decides each occurrence again, with three strategies: it re-reads the holder's references, re-reads the field's
  `derived_from` and takes every name token after a method's first `(`, and, for an edge nothing else explains, scans the
  holder's whole block. The scan answers 38 production edges in this repository's spec (union bodies have no value span).
- The strategies disagree with the derived walk and with the spec, live on the shared binary:
  - a signature type written beside a listed item is missed, so `rename` leaves it dangling (E004 after the rename);
  - a parameter's name is found as a reference, so `rename` rewrites it;
  - a quoted union member is a derived edge with no token, so `rename` refuses the whole project.
- Three of the six commits that touched navigation's occurrences in one month changed the linker's or the walk's rule and
  then had to copy the change into navigation.
- The parser kept no position for a union member, a union body's value, a parameter's type or a return type.

## Decisions

**D1. The graph keeps, parallel to its edges, where each is written.** `Graph::written(&Edge) -> &[SourceSpan]`, from a
record index-aligned with the edges. Never a field of `Edge`: it stays a `Copy` key, and none of its 67 constructors or 38
readers change.

**D2. The linker records each reference's span** (the span E003, I004 and E022 already point at).

**D3. The derived walk records each name's span.** The parser keeps type-expression positions (`TypeMember`,
`Parameter::ty_span`, `MethodDecl::returns_span`, a union body's value span, none of them serialized); a name is a lexer
identifier (ADR 0023) at the expression's position plus its offset. A parameter's name is never an expression.

**D4. A quoted union member is never a type name** (`Written::Quoted`, not the first character of a string the parser had
already unquoted).

**D5. Navigation reads `Graph::written` and decides nothing.** An edge with no written place (a graph built by hand), or one
the text no longer spells, is one `Entity` occurrence at the holder's block; `rename` refuses it, as it refuses a stale text.

**D6. Positions do not leave the process.** `JsonEdge` and `PassEdge` keep three fields: no Graph Protocol schema change,
no pass-wire change, no re-vendor.

**D7. Debug builds assert that every edge the linker made is written somewhere** (`assert_linked`).

## Rejected

- Spans on `Edge`: loses `Copy`, adds 67 construction edits and 24 bytes per element to every graph walk.
- `ListItem` for union members: its text is unquoted, a union field's members keep their quotes in three renderers.
- A parallel span vector in `FieldEntry`: drifts under coercion and says nothing about quoting.
- Keeping the block scan: ready to hide the next gap, as it hid the union-body one.
- Exporting spans (a schema bump, or a protocol-types change and a re-vendor), for no current reader.

## Consequences

`specforge-parser`: `FieldValue::{TypeUnion, VariantList}` hold `Vec<TypeMember>`; `Parameter::ty_span` and
`MethodDecl::returns_span` are new. `specforge-graph`: `Graph::written` is new, `type_expressions` is gone. `find_references`,
`rename` and the LSP's references, prepareRename and go-to-definition answer by the walk's rule (no shape change). `check` may
report W002 for a type referenced only by a quoted union-body member, as the spec already says.

## What would reopen it

A consumer outside the process that needs a reference's position (then D6 is revisited with its schema bump), or a reference
shape with no token (it would surface as an `Entity` occurrence and a debug assertion).
