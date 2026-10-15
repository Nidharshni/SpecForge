# A field's declared type decides what its value references

**Status:** accepted (2026-10-15, architecture round 10, plan 02). Extends ADR 0064 D1 from a field's storage and
wire form to what its value references. Amends ADR 0032 (the graph inputs: `single_reference_fields` is gone,
`FieldForms` carries the member kinds) and ADR 0019 (the field text of an open-fields kind's list).

Whether a written value named an entity was decided by how it was written, not by what its field declares. The
parser turned any unquoted list (numbers and booleans included) into a reference list, a list under a field named
`values` into a variant list, and a mix of quoted and bare items into a parse error; eight readers (the linker and
its debug assertion, the entity snapshot behind E022, navigation, the contracts pass, the LSP cursor and completion,
the derived-reference walk) then re-derived "is this a reference" from those variants.

Reproduced at `fb4c5957`: `aliases [1, 2]` on a `string_list` was E003; `aliases [t1]` made a false edge;
`persona ghost` and `persona "dev"` were silently nothing; a single reference to the wrong kind skipped E022;
`see_also "t1"` made no edge; a mixed list was E001 and E061; only the key `values` escaped E003 on an open kind;
`failure_mode.invariant inv` counted as no contract obligation (A010); the editor offered a definition for a string
list's bare word; and this repository's spec carried a dangling single reference nobody reported.

## Decision

- **D1. The parser reads syntax only.** One `FieldValue::List(Vec<ListItem>)`; a `ListItem` keeps its text, its token
  span (quotes included) and how it was written (`Written::{Bare, SchemeRef, Quoted, Literal}`). No field-name rule,
  no mixed-list error (E001 is for syntax).
- **D2. The field form reads what a value references.** `FieldForms::reading(kind, field)` gives a `Reading`
  (`Declared`, `Member` of an open-fields kind, or `Undeclared`); `Reading::references(entry)` is the one rule:
  a declared `reference` references its value, bare or quoted; a `reference_list` every item; a `string_list` only
  scheme ref IDs; other declared types and members nothing; an undeclared field its bare words and scheme ref IDs.
  The linker (E003, I004), E022, navigation, the contracts pass and the derived walk ask it; the editor asks
  `Reading::may_name`.
- **D3. Quoted ids count (02-U2).** A quoted id on a reference field is a reference like the bare word.
- **D4. A scheme ref ID in a `string_list` references its `ref` (02-U1).** No builtin change, no re-vendor.
- **D5. Undeclared fields keep the syntax reading, minus its bugs.** Literals reference nothing; a mixed list's bare
  items do.
- **D6. An open-fields member references nothing.** The derived walk reads its bare list items as type names, as it
  read `values`; its list's field text joins with `, `.
- **D7. Link order.** W061 runs over every edge but a declared single reference's, which links after it
  (`single_references_are_not_in_cycles`).
- **D8. A list is written as its items' text.** An undeclared `[1, true]` exports `["1","true"]`.
- **D9. E022 stays at the entity**, though references now carry spans.
- **D10. The dangling `invariant wasm_extension_runtime_integrity`** in this repository's spec is deleted (02-U3).

## Consequences

- Bare words on a `string_list` stop linking: false edges disappear and "unreferenced" infos may appear elsewhere.
- Single references are E003, E022 and contract obligations; quoted ids link or are E003.
- `GraphInputs.single_reference_fields`, `FieldValue::{ReferenceList, StringList, MixedList}`, `SpannedRef` and
  `ValueShape::{Strings, References, Mixed}` are deleted. Pre-release: no shim.

## Rejected

- Declare every `refs` field a `reference_list` with a `ref` target: a builtin change and a re-vendor, and `ref` is
  structural, so unresolved refs would be I004 naming a kind no extension provides.
- A `string_list` references nothing: `refs [gh.issue:42]` would stop linking and W012 would fire.
- Keeping `ReferenceList`/`StringList`/`MixedList` and coercing between them: `StringList` needs spans and lexemes
  anyway, so it converges on one list with three names.
- Including single references in W061 cycles: a behavior change nobody asked for.

## What would reopen it

A field type whose references depend on more than its declared type and the written lexeme.
