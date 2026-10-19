# A relation is written on one side

**Status:** accepted (2026-10-18, syntax round R4). Amends [ADR 0060](0060-edge-types-resolved-once.md) (D6: the inverse
reading of `inverse_of`) and [ADR 0032](0032-one-graph-build.md) (the inverse pairs of the graph inputs). Consequences amended by [ADR 0157](0157-a-typed-edit-names-a-relations-owning-side.md) (a typed edit refuses the other side, naming the owning field).

The builtin extensions declare every relation from one side: 53 reference fields, 53 relations, none declared from both
ends. What authors and agents read said otherwise. `docs/quick-reference.md` made `behaviors` a required field of
`feature`, gave `invariant` an `enforced_by` and `event` a `trigger` and `consumers`, and 19 of its 20 edge rows named
no declared edge type. The authoring skills, three entity pages, the spec-writing guide ("Back-link") and 106 test
fixtures wrote those reverse fields too. Writing one was only W020, a warning: the graph kept an untyped reverse edge,
W001/W003/W007/W008 then said the real relation was missing, and the software extension's rule E051 validated
`triggers`, a field no extension declares. Underneath, two of the four builtin `inverse_of` declarations
(`milestone.behaviors`, `constraint.enforced_by`) named a field of the target kind that is a different relation, which
gave the graph two bogus bidirectional pairs and trace a "declared from the other side" rule nothing could satisfy.

## Decision

- **D1.** The owning field of a relation is the one reference field an extension declares for it; the docs follow the
  declarations. No builtin relation changes direction.
- **D2.** Writing the other side is **E079**, an error naming the owning field and the exact edit. It is recognised by
  name (an `inverse_of` naming the field, so a documented reverse is refused even with dangling ids) and by values (the
  ids of one other kind that owns a relation into this kind). A self-relation's other side is not recognised: it stays
  W020. Kinds with `open_fields` stay unchecked.
- **D3.** E079 does not change the graph build: the unknown field still writes its untyped edge, as an unknown field
  does; `check` fails.
- **D4.** An extension that declares a relation from both ends is **W162**: an `inverse_of` naming a field of its
  target kind, or two kinds linking each other both ways. A load warning, never a failed compile.
- **D5.** The two `inverse_of` that name another relation are dropped; the two that name what the target kind reads the
  relation by (`behavior.features` as `behaviors`, `behavior.invariants` as `enforced_by`) stay.
- **D6.** E051 is retired into E079.
- **D7.** `query` and `trace` keep their output. The other side is the incoming edges (`trace` upstream, the neighborhood
  of `query`). `ExpectedEdge::inverse_labels` goes.
- **D8.** The both-sides graph machinery is deleted: `FieldRegistry::inverse_fields`, `GraphInputs::bidirectional_pairs`,
  `Graph::with_bidirectional_pairs` and the complementary-cycle suppression. They suppressed no cycle a builtin can form.
- **D9.** No `.spec` migration tool: the corpus has 0 reverse writes. The 106 Rust fixtures are rewritten by hand.
- **D10.** The quick reference's entities and relations are generated from the pinned builtin declarations and held by a
  test, so the docs cannot drift again. Entity pages keep their prose; a reference row must be a registered field.
- **U1-U4.** Settled with each recommendation: E079 is an error; `inverse_of` stays on the wire as the name the target
  kind reads the relation by; the quick reference is generated; all 106 fixtures are rewritten.

## Rejected

- **Both sides with a consistency check.** Two writers of one fact is the drift this removes.
- **Deleting `inverse_of` from the protocol.** A protocol bump and a re-vendor of every blob for a name the generated
  docs use ("read here as `behaviors`").
- **A migration tool for reverse writes.** Nothing to migrate in the corpus (D9).
- **E079 as a warning.** The relation written there is lost silently; a rename of W020 keeps the loss.

## Consequences

- A project written to the old docs fails `check` until edited; the suggestion carries the edit. Tracked projects: 0.
- Extension authors get W162 for a relation declared from both ends.
- A typed edit on the other side is refused naming the owning field ([ADR 0157](0157-a-typed-edit-names-a-relations-owning-side.md)).

## What would reopen it

A relation whose two ends belong to two independently versioned extensions that both must be able to write it.
