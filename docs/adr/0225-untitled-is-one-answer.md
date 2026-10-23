# Untitled is one answer

**Status:** accepted (2026-10-22, architecture round 25, plan 05). Extends [ADR 0148](0148-every-block-has-a-title.md) D2 and D4 (a blank title is no title for every reader of the rule), [ADR 0186](0186-a-migration-is-judged-before-it-is-written.md) D7 (the titles step's plan asks the rule its judge applies) and the titles-step amendment of [ADR 0187](0187-a-declarations-name-is-where-the-parse-read-it.md).

## Context

- E076 reports a block that becomes a node and has no title, or a blank one, and tells the user to run `specforge migrate`. The titles step asked only whether the parse read a title token, so `type session "" {` was skipped: `migrate` reported success and the error stayed (also for blank unions, refs, `"\t"` and a string holding a newline).
- E076 on a `spec ""` block sent the user to migrate too; a spec block's title is its name, which no step writes and a retitle refuses.
- A create refused a blank title up front (`invalid_title`); a retitle asked nothing: it wrote the blank title when the block had none (E080 pairs the new E076 with the old one, ADR 0211 D1) and was refused only by the compile, as E080, when it had one.
- Six sites spelled the rule (two "which blocks take a title", four "is this text no title"), with two answers.

```text
BEFORE  E076 none-or-blank  step is_some  judge none-or-blank  create trim()  retitle (nothing)
AFTER   graph::title::{is_untitled, owes_title}  <- E076, step, judge, create, retitle
```

## Decisions

- **D1.** The graph owns "this block owes a title and has none": `specforge_graph::title::{is_untitled, owes_title}`. Untitled is absent or only whitespace; a block owes a title when it becomes a node (not `define`, not `defaults`). E076 asks it.
- **D2.** The titles step asks it (ADR 0186 D7) and writes every title E076 reports but a spec block's: a missing one after the name, a blank one where the parse read it. Its judge asks the same rule.
- **D3.** E076's help names `migrate` exactly where the step writes; a spec block's help says to write its name.
- **D4.** A create and a retitle refuse a blank title before anything is planned (`invalid_title`, `EditField::Title`).
- **D5.** A blank title with no doc comment above takes the id in words, as a missing one does.

## Rejected

- Normalising `""` to `None` in the parser or the node: the kernel would decide a graph rule, the node would lose what was written, and the step and the retitle still need `title_span`.
- Leaving the retitle to E080: ADR 0211's key pairs the new E076 with the old, so a blank retitle of an untitled block is written.
- A `writes_title` predicate: one caller; the spec exclusion is one `structural::SPEC` compare.

## Consequences

- Breaking: `migrate` rewrites blank titles (was skipped); `set --title ""` and MCP `edit` `retitle` with a blank title are refused `invalid_title` (were written, or E080); E076's help and `explain E076` text change.
- Readers still show a blank title as written (`""`); E076 is live while it stands.
- No new code, no re-vendor.

## What would reopen it

- A writer that can write a spec block's name as a title without renaming it, or a kind whose blocks become nodes with no title (an extension-declared untitled kind).
