# A reference is decided in the linker

**Status:** accepted (2026-10-16, architecture round 12, plan 06). Reopens [ADR 0093](0093-a-fields-declared-type-decides-what-its-value-references.md)
D9 (E022 at the entity) and [ADR 0031](0031-the-registry-build-checks-the-entities.md) D1's placement of E022. Amends
[ADR 0032](0032-one-graph-build.md) (the graph inputs: `absent_reference_targets` is gone; `FieldForms` answers each
reference field's target, `target_owners` names the owner of each absent target kind) and
[ADR 0019](0019-one-entity-snapshot-per-compile.md) (the record holds no references).

## Context

Whether a written reference is an edge, E003, I004 or E022 was decided in three modules, each reading the field's
target kind its own way: the linker (E003 and I004, learning a target kind only for kinds no extension declares), a
registry check (E022, over a list of references the snapshot recomputed) and the derived walk (its own kind filter).

When the target kind is not loaded they disagreed. A missing id was an info at the reference with a placeholder
`specforge add @specforge/<name>` (the spec requires the extension's name; E024 names `@specforge/product` for the same
kind). An existing id of the wrong kind was an error on the whole entity block with no help. A wrong-kind reference was
still an edge, so trace said "resolved".

## Decisions

**D1. A field form carries its declared target kind;** `FieldForms::target(kind, field)` answers a `Target`
(`Loaded` or `Absent`: whether a loaded extension declares it). `GraphInputs.target_owners` names, for each absent
kind, the extension that owns it, read by `RegistryBuild::graph_inputs()` from the lookup E024 reads (the candidate
vocabulary's `kind_owner`, ADR 0122), so I004, E022 and E024 name the same extension. "Loaded" is not in the form: a
registry entry's form is built before the kinds are final and is shared with E061 and the writers.

**D2. The linker takes one step per written reference** (`Graph::resolve` gives a `Resolution`: `Linked`,
`Mistyped`, `Unresolved`, `Unavailable`). The step yields the edge and at most one of E003, I004 and E022.

**D3. One span policy:** the reference's own span, else the entity's.

**D4. One suggestion source:** the graph's ids for "did you mean", `target_owners` for "enable".

**D5. A wrong-kind reference is E022 and an edge, loaded kind or not.** The linker's invariant, "every reference to
an existing entity has its edge", keeps editor navigation, occurrences and rename working over it.

**D6. E022 is a graph-build diagnostic,** reported with E003 and I004 in link order, before the registry's checks.
ADR 0031 D1's list loses E022.

**D7. The derived walk asks `Target::admits`,** not a copy of the target kind.

**D8. The snapshot keeps references only for the validator context,** whose wire shape is a protocol type.

## Rejected

- "Loaded" stamped in each form: the registry entry's form and the build's would differ for one field.
- The graph reading the index or the vocabulary: the graph sits below the registry and names no registry type.
- I004 for a wrong-kind id: enabling the extension does not fix it.
- Dropping the edge of a wrong-kind reference: rename would leave the reference behind and go-to-definition would stop.
- `DiagnosticData` for E022: no consumer acts on it.

## Consequences

E022 moves to the reference token, gains a suggestion and is reported before W012. I004 names the extension. Nothing
else changes: severities, messages and edges are as before.

## What would reopen this

A consumer that acts on E022 (it then gets data), or a reference whose target is more than one kind.
