# A typed edit changes a block's field through one module, own or inherited

**Status:** accepted (2026-10-19, architecture round 18, plan 05). Amends [ADR 0149](0149-a-file-states-a-kinds-shared-fields-once.md) D3 (`{}` says none for a block field, as `[]` does for a list), [ADR 0151](0151-a-spec-file-is-changed-by-a-typed-edit.md) D5 and its round-16 amendment (an entry the block inherits is copied before `set` or `unset` changes it; `unset` leaves the field holding none) and [ADR 0154](0154-a-typed-edit-asks-its-files-defaults-over-the-text-it-plans.md) D3 (every typed edit of a field, entries included, ends with the restatement check). Builds on [ADR 0087](0087-a-file-over-the-cap-becomes-a-directory-module-along-its-own-seams.md) (the split of `edit/block.rs`). Amended by round 19 plan 02 (2026-10-21): D6, D7. D1 and D4 amended by [ADR 0182](0182-the-graph-answers-what-a-blocks-field-holds.md): a verb is handed `Held` (what the field holds; where a value goes stays D6's `Block`); `unlink` of an inherited single reference is refused as `unset` is. D5 holds for a default in any form, and so does D7's create.

## Context

Whether a block's field is its own or its file's default (ADR 0149) was answered by four entry points in
`edit/defaults.rs` (`inherited`, `without_repeat`, `write_list`, `unset_inherited`), and each verb picked which to call.
The `field.entry` paths round 16 added never called any of them. Reproduced on `main` @ `6ba2b433`, with
`defaults behavior { requires { graph_ready "Graph built" } }` and a block that writes nothing:

- `set b requires.disk_ready ...` writes a `requires` holding only the new entry: the inherited entry is dropped;
- `unset b requires.graph_ready` answers "nothing to write", and `unset b requires` is refused, though the block holds
  the entry and `requires {}` is a valid "none";
- `set` of an entry to the default's value, and an `unset` that leaves the own field equal to the default, write a field
  that restates the default (W161), which the invariant `typed_edit_canonical_form` forbids;
- `unset` over a default alternates across runs: own, default, `[]`, default.

Round 15 fixed link and create's reading of the default; round 16 added the entry paths beside them without it. Each fix
touched the verb it found; none moved the rule out of the verbs.

## Decision

- **D1.** One module, `edit::defaults::change`, reads and writes a block's field, own or inherited. Every verb that
  changes a field (`set`, `unset`, `link`, `unlink`, `create`, and the `field.entry` paths) reads its path first, then
  hands its change to it; no verb asks `FileDefaults` itself.
- **D2.** Copy on write covers block fields: an inherited block field is copied as its defaults block writes it, an
  inherited list as the graph stores it (ADR 0154 D4), before the verb changes it. A field of one value is not copied: a
  verb writes it whole.
- **D3.** After every change, an own field that restates its default (W161's `FileDefaults::restates`) is removed, so no
  typed edit writes a restating W161.
- **D4.** A field left empty over a default holds none: `[]` for a list, `{}` for a block field; a field of one value
  cannot say none and is refused `not_editable`. With no default, the line goes.
- **D5.** `unset` means the block holds none of the field (or entry): it is idempotent. Going back to the default is
  `set` to the default's value (D3 then removes the own field).
- **D6.** The block places a value: a verb names the field (or `field.entry`) and the input; `Block::put`,
  `Block::put_entry`, `Block::place` and `Block::put_verify` work out the column the writer lays the value out at and
  the line it goes on. No verb computes a lead or a line. A new entry goes inside its block field, which is first
  opened when the block writes it as none (`{}`) or not at all; annotations given go on the value placed, new or
  existing.
- **D7.** A create is its header line, one `set` per field (in key order) and one add-verify per obligation; it has no
  placement rule of its own, and an equal obligation given twice is written once.

## Rejected

- Keeping four entry points and adding the entry arms' calls: the next path added beside them forgets again, as round
  16's did after round 15's fixes.
- A verb handing a finished value: the entry verbs would have to read the inherited entries themselves, so the module
  would need a second "what does it hold now" call.
- Copying a block field by value: it reorders the entries and can only spell strings.
- Merging an own block field with its default (ADR 0149 Rejected: a content decision).
- Peel semantics for `unset` (the own field goes, the default shows through): a toggle.

## Consequences

`set` and `unset` of an entry over a default copy it; `unset` of an inherited block field writes `{}` (was refused);
`unset` of an own field over a default writes none (was: back to the default); `unset` of an own scalar over a default is
refused. `edit/block.rs` is split in three along its seams (ADR 0087). One more format pass when a copy is made.
`set` of an entry into a block field written `{}` writes it inside (was: an undeclared member above it, W020); `set` of
a new entry writes its annotations (was: dropped); `new --verify x --verify x` writes one line.

## What would reopen it

A second defaults scope (project-wide), merging (ADR 0149's reopen), or a verb that must say "inherit" without knowing
the default's value.
