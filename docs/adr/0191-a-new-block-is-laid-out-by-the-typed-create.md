# A new block is laid out by the typed create, the editor's snippet included

**Status:** accepted (2026-10-21, architecture round 20, plan 08). Amends [ADR 0023](0023-the-lsp-reads-a-document-through-one-module.md) D6 (a kind's and a field's snippet is the create's skeleton), [ADR 0151](0151-a-spec-file-is-changed-by-a-typed-edit.md) D2 (the create's layout has a second reader) and [ADR 0153](0153-a-value-is-spelled-by-one-module.md) D1 (the LSP no longer quotes a value).

## Context

The language server laid out "a new block of kind K" twice. Its stub code action goes through the typed create
([ADR 0161](0161-a-stub-fix-is-a-typed-edit.md)): the kind's declaration order, the file's indent, aligned keys, each
value in its field's form. Its keyword completion built the block by hand: required fields by name, a literal
two-space lead, no alignment, a string list's item bare; its field completion hard-coded a block field's braces at 4
and 2 spaces and set no `insertTextMode`. A snippet filled in was rewritten by the next `format`, under the defaults
and under `indent_width = 4`. None of the thirteen commits that taught the create its layout reached the snippet.

## Decision

- **D1** A new block of a kind, or a new member, is laid out by the typed create: `edit::Skeletons` answers its
  skeleton, the create's header and fields (`create::header`, `create::fill`) fed one marker per slot, laid out with
  the place's format configuration ([ADR 0169](0169-the-format-configuration-is-read-through-the-callers-disk.md)) and
  spelled by the writer (ADR 0153). A skeleton's slots are the id, the title, each required field
  (`FieldInfo::required`, which the create stub reads too) and the final cursor.
- **D2** A skeleton holds no value: it is not planned, compiled or judged (no E080), and writes nothing. ADR 0161's
  rejected "draft plan without E080" is a plan that writes; a skeleton writes nothing.
- **D3** The LSP's keyword and field snippets are skeletons: the slots become tabstops in text order, the final cursor
  is `$0`, a slot's placeholder text is its default; a field item says `insertTextMode: adjustIndentation`. The LSP
  decides no order, lead, alignment or form.

## Rejected

- The LSP spelling the forms (ADR 0153 D1: no other code quotes a `.spec` value).
- Formatting the snippet text: snippet syntax does not parse.
- The stub's `TODO` plan: E080 refuses an enum's or a reference's `TODO`.
- Every declared field in the snippet: a 20-line snippet for a `failure_mode`.

## Consequences

The snippet changes: declaration order, the file's indent, aligned keys, a string list's item quoted, tabstops numbered
in text order, an integer or bool slot `${n:0}` / `${n:false}`. `specforge-lsp` loses `field_snippet` and
`keyword_snippet`; `completion::items` takes the document's `Place`.

## Reopen when

A completion must realign its neighbours (`additionalTextEdits`), or a client population ignores `insertTextMode`.
