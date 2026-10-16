# The paths a field names are read once; E016 is a host rule

**Status:** accepted (2026-10-16, architecture round 12, plan 07). Reopens [ADR 0031](0031-the-registry-build-checks-the-entities.md)
D1's order and its rejected option "the structural checks as host rules", for E016 alone. Amends
[ADR 0020](0020-rules-are-a-typed-set.md) D1's order and [ADR 0107](0107-the-checks-read-through-the-disk-they-are-given.md)
D5(2).

## Context

"Which paths does this field name" had two readers, and they disagreed.

- E016 (`checks/files.rs`) read a list's items or a non-empty quoted string. A bare word on a `string` file-reference
  field, which the graph stores as an identifier, named nothing: `guide README` got no E016.
- The `file_exists` rule (`rules/check.rs`) read a list's items, else the field's text, whatever the shape. It reported
  `guide README`, read a block's keys as paths, and asked the disk about the spec root for an empty value. It never
  suggested a close sibling; only E016 did.

The two readers changed together six times in ten days, each change touching both, and ADR 0107's two drifts were
about them.

## Decisions

**D1. One module reads the paths a field names** (`specforge_registry::rules::files`). A scalar's text (a quoted
string, a bare word, a date, an integer, a boolean) is one path and each item of a list is one, when not empty; a
block, an expression group, a variant list and a type union name none (E061 reports such a value when the field's
declared type refuses it). The field's last occurrence is read. A relative path is the spec root's; an absolute one
is checked as written. Per named path: one `exists`; for a missing one, one `list` of its directory.

**D2. E016 is the host's `file_exists` rule for every `file_reference` field,** built like E006 (ADR 0020 D1), one
per (kind, field). The rule set's order is the declared rules by code, then E006 by (kind, field), then E016 by
(kind, field). E016 is reported after the rules, no longer second.

**D3. Every `file_exists` rule's missing path carries the close sibling,** read by listing its directory.

**D4. What changes for a user.** A bare word on a file-reference `string` field is E016. An extension's `file_exists`
rule stops reading a block's keys, an expression's display, variant and union members and empty values, and gains the
`did you mean` help. A file created beside a missing named file re-runs the checks.

## Rejected

- E016 kept as a structural check reading through the shared module: two runners for one check.
- A `suggest` flag on `Check::FileExists`: two behaviours behind one check kind, and extension authors never get the
  help.
- E016 kept second by splitting the rule run: it splits the rule set's one fixed order to keep a position nobody reads.
- The rule's any-shape reading: it keeps reading a block's keys as paths.

## Consequences

ADR 0031's rejection stood for "the structural checks as a group" because host rules would carry messages, data and a
gate with no target kind. None holds for E016: it has a target kind, carries no data and does not depend on the gate
(no kinds, no `file_reference` field). It stands for W012, E024, E013, E014, W020, W158, E022 and E061. ADR 0031 D4
("E016 follows the registry") and ADR 0107 D1-D2 (reads through the `Disk`) stand. D5(2) of ADR 0107 is amended in its
outcome, not its reason: the listing it removed was predicted and never read; this one is a real read the checks
make.

## What would reopen this

A file check that needs more than a field's paths (a glob, metadata), or a structural check other than E016 that fits
a host rule.
