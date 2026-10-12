# A broken document reads the same in the graph, the editor and the formatter

**Status:** accepted (2026-10-12, architecture round 7, plan 14). Points at ADR 0038 D2 and ADR 0021 D5; amends
neither.

ADR 0038 gave the language one lexer and one form-start rule, and its D8 taught the grammar's scanner to end an
unclosed body at a form start. Three readers still disagreed on a document being typed:

- The scanner runs only where a body member may start or close. A field name typed without its value, an open
  `(` or `<`, or a method name with no `(` left tree-sitter waiting for a token, and it took the next form's
  header as that token. The graph lost every form after it, while the LSP's document (D6) showed them:
  highlighted as declarations, with hover and go-to-definition finding nothing.
- The formatter parsed raw tree-sitter: after a string left unclosed it kept the whole file, while the
  parser's recovery (D2) read every form.
- The graph build hid every E001 starting inside a `port` or `type` entity (the body-parser kinds), so an
  unclosed body or string there compiled without a syntax error, and, before D8, silently lost the forms after
  it. The same exemption hid real grammar errors in a `type` body (`@min(0)`, an inline object).
- The LSP, recovery and the lexer each measured the top-level indentation in their own walk.

## Decision

**D1. One reading, `specforge_parser::cst`.** `Cst::read(source, old_tree)` takes the grammar's tree of the
whole text when it parsed, or when it starts a top-level node at every form start of the text and no string was
left unclosed. Otherwise it cuts the text before each form start the tree missed and before the first form
start after each unclosed string, and parses each piece alone; a piece holding an unclosed string is parsed
with the string's delimiter appended (ADR 0038 D2). The parser's walk to the AST and the formatter read the
same pieces; the whole text's tree is kept for incremental reuse.

**D2. The scanner reaches the commonest half-typed members.** `_close` is also the alternative to a field's
value, to `)` (parameters, function types, parenthesized expressions) and to `>` (generic arguments), so
tree-sitter's own tree, which every tree-sitter host reads, ends the body there. Each `_close` is followed by
the body's MISSING `}`: none is silent. What the scanner does not reach (a method name with no `(`, an operand
being typed) is D1's to cut.

**D3. One walk of form starts.** `lex::form_starts(text, lexemes)` is the lexemes that start a top-level form
as a document reads it: first on their line, reading D6's shapes at most as deep as the last line begun with no
bracket open; such a line closes every bracket open before it. `Cst` cuts at the ones the tree missed; the
LSP's document closes its frames at each. The lexer keeps its own lines-alone reading only to end an unclosed
`"""` (ADR 0038 D7), where the text after the string cannot be lexed whole.

**D4. An unclosed body is reported at its brace, and no parse error is hidden.** An unclosed top-level body is
an E001 at its `{`: "syntax error: unclosed block - missing closing '}'; it was ended before the block on line
N" (no suffix at the end of the text), as an unclosed string is reported at its opening quote. A body whose `}`
an unclosed string took is reported by the string alone. The graph build's exemption for body-parser kinds is
deleted outright, with `GraphInputs.body_parser_kinds` and the `extension_owned_body_syntax` behavior: every
parse error is a diagnostic. The `has_body_parser` flag stays in the protocol and now changes nothing in the
compiler; dropping it is a protocol change for the next bump.

**D5. The formatter formats what the parser reads.** Every top-level form of every piece that reads whole is
formatted; a form holding an ERROR or MISSING node, or the piece's unclosed string, is kept as written from its
first line to its last source line (blank lines after it excluded), with one W142 spanning those lines (ADR 0021
D5, which the code now meets); adjacent kept forms share one.

## Consequences

- A body being typed never takes the forms after it, in the graph, the editor or the formatter, whatever member
  is half-typed; `check` reports one E001 for it, at its `{`.
- An unclosed body or string in a `port` or `type` entity is reported, and so is syntax the grammar does not
  read inside a `type` body.
- `specforge format` after an unclosed string formats the rest of the file; W142 names the broken form's lines.
- A form whose own header the grammar cannot read (an unclosed title, a method header cut by D1 inside a
  `port`) is not in the graph, though the editor highlights its header: its E001 says so. No plan owns this.
- `recovery.rs` is gone; `cst.rs` holds the recovery and the clean case alike.

## Rejected

- **The scanner alone**: every half-typed shape needs an alternative in another grammar rule, and the method
  header, parameter and operand cases would each regenerate the parser.
- **Recovery in Rust alone, deleting the scanner**: external tree-sitter hosts would read tree-sitter's default
  recovery again, reverting ADR 0038 D8.
- **Reparsing with the string closed in place** (one tree, no pieces): the rest of the broken form would be
  read as code, so a prose line typed into the string would cascade errors where the string's one E001 stands
  today.
- **A new diagnostic code for an unclosed body**: nothing reads the difference; E001 and the message serve.
- **A `ParseErrorKind` on every parse error**: it existed only to keep the exemption for grammar errors; with
  the exemption gone nothing reads it.

## What would reopen it

A top-level form whose first line does not decide it (ADR 0038 D2's trigger).
