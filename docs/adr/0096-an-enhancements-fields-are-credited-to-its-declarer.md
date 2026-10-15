# An enhancement's fields are credited to the extension that declares them

**Status:** accepted (2026-10-15, architecture round 10, plan 04).

An entity enhancement (`c.enhance(kind, ..)`) carried one guest-written string, `source_extension`. The SDK and the
docs called it the kind's *owner*. The registry build read it as the owner to decide whether an unknown target kind is
skipped silently, then credited the enhancement's fields to that same string. Every reader of that credit (trace,
schema, model `contributed_by`, LSP hover, edge writers, W019/W021) took it to mean the *contributor*. The builtins
disagreed on what to write: software wrote `@specforge/product`, formal wrote itself.

Reproduced at `fb4c5957`:

- `specforge trace tree_sitter_specforge` lists software's opt-in `ports` / `ports_defined` as missing, because they are
  credited to product and trace judges them as product's own fields.
- `specforge model` has no `contributed_by` for `module.ports`, `module.ports_defined`, `milestone.behaviors`.
- `specforge schema` credits product with those fields, while their edge types are software's.
- `specforge outline` names an owner that is not loaded (`@specforge/governance` for testing's enhancements) because it
  falls back to the wire string.

## Decision

- **D1. The guest names the kind only.** `EntityEnhancementDescriptor { target_kind, fields, verify_kinds }`;
  the SDK's `enhance(target_kind, f)`.
- **D2. The host credits the declaring extension.** `FieldRegistryEntry.source_extension` of an enhancement field is
  the extension that answered the declaration (`ExtensionDeclaration::name()`), as for a kind's own and a shared field.
- **D3. The owner is the kind registry's.** Nothing reads the owner from the wire.
- **D4. An unknown target kind is silent behind an unloaded optional peer, else I004.** When no loaded extension
  declares the target, the enhancement is skipped without a diagnostic if its extension declares an optional peer the
  project does not load; otherwise it is I004. Every silent skip the builtins rely on sits behind such a peer.
- **D5. Protocol 1.5.0.** Dropping a wire field is a minor step (ADR 0094 already moved 1.4.0 to 1.5.0 in this round).
  A 1.4 guest's `source_extension` key is W138 and ignored.
- **D6. The outline's owner is optional.** `OutlineEnhancement.owner` is `None` (JSON `null`) when no declaration
  declares the kind; markdown says so, mermaid and dot draw no edge.

## Consequences

- Trace, model, schema and hover credit software, not product, for `module.ports`, `module.ports_defined` and
  `milestone.behaviors`; a project that exports sees a patch step of its recorded schema version (ADR 0067).
- The SDK signature `enhance(kind, owner, f)` breaks; the three builtins and every blob re-vendor.
- An extension with an absent optional peer gets no I004 for a mistyped target kind. Never worse than before.

## Rejected

- Credit = the kind's owner (what `registries/field.rs` and two tests said): every reader wants the contributor, and
  `contributed_by`, the schema doc and trace's opt-in rule would each need a second fact.
- A narrower optional wire field `conditional_on`: a guest-supplied fact that must agree with the peers, the same
  class of bug.
- The bundled keyword index: works for builtins only.
- Renaming `source_extension` to `declared_by` across the registries: about 40 test sites and a published schema key
  for nothing; the name stays, its doc says "the extension that declared it".

## What would reopen it

An extension that must enhance a kind owned by a hard peer conditionally, where an optional peer is the wrong model.
