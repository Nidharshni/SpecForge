# A field written twice is decided by the graph build

**Status:** accepted (2026-10-22, architecture round 24, plan 03). Amends [ADR 0019](0019-one-entity-snapshot-per-compile.md) ("a name written twice has its last value") and extends [ADR 0182](0182-the-graph-answers-what-a-blocks-field-holds.md) D2 (a block's own entry is its first; the build keeps no other). Mirrors [ADR 0149](0149-a-file-states-a-kinds-shared-fields-once.md) D5 (E078, a `defaults` block).

## Context

- A block may write one field twice (`direction inbound`, then `direction outbound`) and `check` was clean. Each reader of the
  node then picked its own entry: the graph JSON, the registry's rules and the compiler passes read the last; the typed edits,
  the headline, the lifecycle cache and the exempting flag read the first; the linker, the Markdown export, the hover and a
  custom validator's field list read every entry. Eighteen sites decided it, three ways.
- `specforge set port_one direction bidirectional` rewrote the first line, said `wrote 1 file(s)`, and `query` still said
  `outbound`; `unset beh ports` left the later list live; one inspect document said `contract: "first contract"` beside
  `fields.contract: "second contract"`.
- The house answer elsewhere is "the first, and say so": E078 (a key twice in a `defaults` block) and E002 (a duplicate ID).
  ADR 0019 was the only written decision, and it said "last".

## Decisions

- **D1. E081, an error, at each later entry** (U2 a). A key written twice in one block, at any depth (an entry of a block
  field, a member of a type), in every entity block but `define` and `defaults` (E078 holds those), a duplicate-ID loser
  included. It is reported at the later entry's value and names the first's line. It never blocks the build.
- **D2. The node holds the first entry of each key** (U1 a), `FieldMap::first_of_each_key`, placed by the build
  (`FileDefaults::resolve`, `node_from_entity`). Every reader of a node reads the one entry. The parser keeps every entry:
  a typed edit needs the later entries' spans and the finder their names.
- **D3. The finder keeps every written name.** A later entry's names are tokens held by its block with no edge, as an E002
  loser's are; a rename edits them.
- **D4. A typed edit that changes a field removes its later entries first** (U3 a), once, in `edit::defaults::change`
  (`Block::drop_repeats`): `set` leaves one line holding the new value, `unset` none, `link` and `unlink` change the list the
  graph holds.
- **D5. `FieldMap::entry` is the parse-level rule** ("the first"), read by `get`, `held`, `Block::entry` and the path's nested
  entry; `repeats` and `first_of_each_key` are the build's.

## Rejected

- The last entry (U1 b): it flips `get` (about a hundred callers), `held`, the typed edits and the cache, and contradicts E078
  and E002; the message would read as "the later line wins".
- Every entry (U1 c): a list is `[a, b]`; a scalar cannot hold two values.
- A warning, W163 (U2 b): a silent drop that passes `check`; W163 stays released.
- A typed edit that refuses (U3 b), or changes the first only (U3 c): the first leaves the later entry live after `unset`.
- The parser dropping entries, or the formatter merging them.

## Consequences

- A file that writes a key twice fails `check` (E081) and its outputs read the first entry. No tracked spec, fixture or test
  text wrote one except two tests, rewritten.
- The formal pass's W039 (a `requires` naming a condition twice) is not reached from a spec any more: the repeat is E081. The
  pass stays in the extension until its blob is rebuilt.
- `specforge-parser`: `FieldMap::{entry, first_of_each_key, repeats}`, `Repeat`. `EntityRecord::field` reads one entry; the
  graph delta keeps one value per key. `specforge-protocol-types` doc comments state the rule (a blob input).

## What would reopen it

A field kind that legitimately repeats (a multi-valued scalar), declared by an extension.
