# A delete finds references by the one reference rule

**Status:** accepted (2026-10-19, architecture round 15, plan 04). Extends [ADR 0093](0093-a-fields-declared-type-decides-what-its-value-references.md)
D2 (a typed edit's referrer scan asks the rule), [ADR 0149](0149-a-file-states-a-kinds-shared-fields-once.md) D11
(where "once per token" is answered) and [ADR 0151](0151-a-spec-file-is-changed-by-a-typed-edit.md) D5 (what a delete
does about a reference it may not remove). D1 and D2 extended by [ADR 0163](0163-a-written-token-is-found-by-one-finder.md): the referrer scan is the graph's finder (this ADR's reopen case); a derived type name refuses the delete too.

`delete --unlink-references` found the references it must remove with its own copy of "this field names X", which
drifted from ADR 0093's rule four ways. It missed a scheme ref in a `string_list`, a `reference_list` written as one
value, a reference in an undeclared field and a reference written in a `defaults` block. Each of those deletes ended
in a generic E080 naming the symptom (an E003 per block) rather than the cause. `link` and `unlink` refused the
one-value list as "not a list", though the graph build stores it as a one-item list.

Separately, rename and the LSP each stripped the duplicates navigation returns for an inherited reference, each
relying on the ordering of a module they do not own. Both were correct; the fact lived in the wrong place.

## Decision

- **D1. A delete finds references by the one rule.** `delete` with `unlink_references` asks `Reading::references` of
  every entry of the texts the plan holds, as the graph build stores it (coerced), a `defaults` block's entries read
  as the kind it names. A typed edit reads a list written as one value as its one item.
- **D2. A reference no typed edit writes refuses the delete.** One written in a `defaults` block (ADR 0151 D5), or in
  a field its kind does not declare, refuses as `not_editable`, naming each `file:line`, before any text changes. A
  default for an undeclared field (E077) names nothing. A plain delete is unchanged: E080.
- **D3. Navigation answers each written token once.** `Navigator::tokens` is `references` with one occurrence per
  span; rename and the editor ask it; `find_references` keeps a holder per block.

## Rejected

- **The navigator for delete**: the plan holds texts no compile has seen, and unlinking per inheriting block leaves a
  dead default (W161).
- **Unlinking in every inheriting block.**
- **Removing the id from the default** (amends ADR 0151 D5): better decided once for all typed edits.
- **A `ReferenceQuery` flag for "once per token"**, a new code, a pre-check for a plain delete (ADR 0151 rejected
  validation by hand).

## Consequences

- The E080 for the four cases becomes an unlink (scheme ref, one-value list) or `not_editable` (default, undeclared).
- `link` and `unlink` accept a one-value list; `rename.rs` and `answers.rs` lose their dedup.

## Reopen when

A typed edit writes `defaults` blocks (then D2 removes the token instead), or a reader of parsed entries besides
delete appears (then the coerce-then-ask pair belongs in the graph).
