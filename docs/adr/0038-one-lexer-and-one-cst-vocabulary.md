# One reader of the language's lexemes and one CST vocabulary

**Status:** accepted (2026-10-07)

ADR 0023 made `specforge_parser::lex` the one lexer of the `.spec` language beside tree-sitter's
grammar, and cleanup round B kept the expression tokenizer apart on purpose. Four scanners still copied
parts of the grammar's lexical rules. The parser's recovery from unclosed strings had its own string,
comment, identifier and scheme-ref-ID readers, plus a hand copy of the top-level block shapes; the LSP
cursor re-derived whether a string was closed and re-implemented the ref-ID byte rule; the formatter
joined text onto one line with a string scanner that did not know comments. They disagreed, visibly: a
union block after an unclosed string was dropped from the graph; a `\` before a line break lost every
entity of the file; a valid string spanning lines was highlighted as fields, and hover and
go-to-definition answered from words inside it (the lexer ended every regular string at its line's end,
ADR 0023 D1); the formatter deleted comments written between a statement's tokens and turned a
multi-line import into a comment. Separately, the parser's AST walk and the formatter named tree-sitter
node kinds and fields with about 110 string literals and no shared definition.

## Decision

**D1. Strings are read as the grammar reads them.** `lex` reads `"…"` and `"""…"""` across lines.
`LexemeKind::Str { closed }` marks one the grammar would not close. For a `"…"` that is no closing quote,
a `\` before a line break (no escape, as the grammar's `seq("\\", /./)`), or, for one spanning lines, a
closing quote that runs straight into text, the pairing an unclosed quote shifts (`lex::runs_into_text`:
anything but whitespace, a bracket, `,`, `|`, `/` or the text's end). Such a string ends at its first
line's end, so an unclosed quote still never swallows a document being typed. A `"""…"""` is closed by
the first `"""`; with none, it runs to the end of the text (amended by D7: it ends before the next form
line). A test checks the lexer against the grammar
over the repository's spec, its integrations and examples, and a fixture of the forms they do not write.

**D2. The parser's recovery reads through the lexer.** The culprit is the first string the lexer marks
unclosed, or a multi-line `"""…"""` whose closing quotes run into text. It is ended before the next line
whose lexemes start a top-level form of the grammar: `use`, `pub use`, `spec "T" {`, `ref id "T"`,
`kind name ["T"] {` or `kind name =`. A test reads the grammar's own top-level nodes in the corpora and
requires each to start a resume line.

**D3. The LSP asks the lexer.** Whether a string is closed is the lexeme's, and whether a word is a
scheme ref ID being typed is `lex::is_ref_id_prefix`. The scheme-ref-ID byte rule exists once.

**D4. One CST vocabulary, in the grammar crate.** `tree_sitter_specforge::kind` (the 42 named node
kinds) and `tree_sitter_specforge::field` (the 20 field names) are the only spellings the parser and
the formatter use; a test checks them against the compiled grammar both ways. Anonymous nodes are their
own text; an error node is `Node::is_error()`.

**D5. A statement holding a comment is kept as written.** An import, an inline ref, a union block, a
field, a verify statement or a method that holds a comment between its tokens is not rebuilt on one
line: its first line takes the body's indentation and the rest stays as written. Other one-line
rebuilds join the lexer's lexemes with single spaces.

## Consequences

- A valid string spanning lines is one string in semantic tokens, and words inside it name no entity.
- A half-typed quote reads as before, except when the next quote in the text is followed by whitespace,
  a bracket, `,`, `|`, `/` or the end of the text (a following string starting with a space or a
  bracket): it then runs on to that quote, as the grammar reads it.
- Recovery keeps a union block after an unclosed string, and keeps the blocks after a string broken by
  `\` before a line break.
- A grammar change that adds, removes or renames a rule or field fails the vocabulary test, and one that
  adds a top-level form fails the recovery test, until the code follows.
- The formatter no longer drops or moves comments inside statements; such statements are not
  normalized.

## Rejected

- **A lexer mode per reader** (grammar strings for recovery, line-ended strings for the editor): two
  readings again, and the editor's misreading stays.
- **The grammar's strings with no fallback**: an unclosed quote being typed swallows the document.
- **Resume lines found by tree-sitter**: a header line alone never parses; ADR 0023 rejects reading
  half-typed structure through tree-sitter's error recovery.
- **A `build.rs` generating the vocabulary from `node-types.json`**, or **an enum of node kinds**: no
  more complete than constants checked against the compiled grammar, and heavier.
- **The lexer in the grammar crate**: saves one dependency edge (formatter → parser) at the cost of every
  reader's import path.

## What would reopen it

A spec in the repository that writes a string spanning lines whose closing quote is followed by `@` or
another character outside the follow set: the follow set grows, or the grammar gives strings a follow
rule. A grammar that makes a top-level form span lines before its first token that decides it: the
resume rule reads more than one line.

## Amendment: one form-start rule (2026-10-11, architecture plan 03)

D2 stated where a top-level form starts as recovery's own rule (`recovery::is_block_start`, private). The
LSP's document kept a second copy (`Walk::starts_a_block` in `specforge-lsp/src/document/syntax.rs`, written
with ADR 0023 and documented as "the parser's recovery rule"), which the inventory of scanners above missed.
The two disagreed. The LSP's copy had no union form, wanted a `{` after `ref scheme.kind:id "T"`, took a bare
`use` or `pub`, took any line with a `{` after a name, and looked at column 1 only. So inside a body left
unclosed (a block being typed), a union or an inline ref written after it was read as fields of that body:
highlighted as fields, with the body's fields offered as completions after it, and the unclosed entity named
by the cursor.

**D6. One form-start rule, the lexer module's.** `specforge_parser::lex::starts_top_level_form(text, lexemes,
max_indent)` says whether the first of `lexemes` is the first lexeme of a line, indented by no more than
`max_indent` bytes (`lex::line_indent`), whose lexemes read one of D2's shapes; a title is a `"…"` closed on
that line. The parser's recovery passes each line it scans, lexed alone. The LSP's document passes its own
lexemes from the line's first one, with the indentation of the last line it began with nothing open, and
closes every open body, block and list there. One corpus test holds the rule to the grammar's top-level
nodes both ways: each starts a form line, read alone and read in its document, and no other line inside one
does. A second corpus test, in the LSP, leaves every body of the repository's spec unclosed and requires the
document to resume at each form.

D3 now reads: the LSP asks the lexer whether a string is closed, whether a word is a scheme ref ID being
typed, and where a top-level form starts.

### Consequences

- In the editor, a union, an inline `ref` or a `define` written after a body left unclosed is read as its
  own form: highlighted as one, with keywords completed after it.
- A bare `use` or `pub` at a line's start inside a body left unclosed no longer ends it; `use "`, `use {`
  and `use *` do, as the grammar reads an import.
- A document whose top-level forms are indented resumes at a form indented like the one before it, not only
  at column 1.
- A grammar change that adds a top-level form fails one test, and both readers follow when the rule does.
- Unchanged: the LSP reads an unclosed `"""` to the end of the text (D1), where recovery ends it before the
  next form line, so the editor shows the rest of such a document as a string while the graph keeps its
  blocks. (Superseded by D7 below: the lexer ends it there too.)

### Rejected

- **Keep the LSP's copy, pinned equal to recovery's by a test**: two rules kept equal by a test, the
  arrangement D2 removed from recovery.
- **A lenient rule for the editor** (a bare `use` or `pub` ends a body): a rule with a mode per reader, which
  D1's Rejected list rules out; in recovery it would also end a string at a prose line starting with `use`.
- **The rule in a module of its own**: one function and a helper; `lex` is the module both readers import.
- **The rule over a line's text, lexing inside**: the LSP holds its document's lexemes and would lex every
  line of an open body twice.

### What would reopen it

D2's trigger: a grammar whose top-level form spans lines before the token that decides it (a title
spanning lines is already no form start).

## Amendment: an unclosed `"""` ends where recovery resumes (2026-10-11, round 6 follow-ups)

D1 let an unclosed `"""…"""` run to the end of the text, and D6's consequences kept that for the editor:
recovery ended such a string before the next form line while the LSP's document read the rest of the text
as one string, greying out every block the graph kept. Recovery also blamed a second shape the lexer read as
closed: a `"""…"""` spanning lines whose closing quotes run straight into text, the pairing an unclosed `"""`
shifts.

**D7. The lexer ends an unclosed `"""` where recovery resumes.** A `"""…"""` the grammar would not close (no
closing `"""`, or, spanning lines, closing quotes that run straight into text, `lex::runs_into_text`) is
`Str { closed: false }` and ends at the end of the line before the next line that starts a top-level form
(`lex::next_form_line`: D6's rule over each line lexed alone, indented no deeper than the last line begun
outside every brace), else at the text's end. Recovery ends every unclosed string at that same line
(`lex::next_form_line`), and its culprit is simply the first string the lexer marks unclosed. D1's sentence
"with none, it runs to the end of the text" now holds only when no form line follows.

### Consequences

- In the editor, the blocks after an unclosed `"""` are highlighted and completed as blocks, as the parser
  reads them; the string ends at the line before the first of them.
- A `"""` left open in the last block of a document still reads to the end of the text.
- Recovery no longer detects the shifted `"""` pairing itself; the lexer's `closed` says it.

### Rejected

- **The LSP finds the form line itself after an unclosed string lexeme**: a second reader of where a string
  ends, which D1 and D3 rule out.
- **End the unclosed `"""` at its first line's end, like a `"…"`**: the lines after `"""` are the string's
  text being typed, and would be read as fields.

## Amendment: the grammar ends an unclosed body at a form start (2026-10-11, round 6 follow-ups)

The parser has no recovery of its own for an unclosed body: tree-sitter's error recovery decided, and it read
every line after the body as its fields. A file with one body left open lost every block after it from the
graph (a union after it became a field, W020), while the LSP's document (D6) resumed at each form.

**D8. The grammar's external scanner reads D6's rule.** `crates/tree-sitter-specforge/src/scanner.c`
produces a zero-width `_form_start` after the token before a line that starts a top-level form: first on its
line, indented no deeper than the line the current top-level form started on, and reading one of D2's shapes
(`lex::starts_top_level_form`, over the line's characters). Between top-level forms the token is optional and
records that indentation. Inside a top-level form's body it is not valid (a never-produced `_in_body` makes
the scanner run there), and tree-sitter recovers by inserting the body's MISSING `}` before it, so the tree
keeps the error and every form after it, and the parser reports `missing '}'` at the body's last member. A
nested block, list or expression group left open in that body is ended first by a zero-width `_close`, valid
as the alternative to its closing bracket (tree-sitter inserts one MISSING token per error; the body's reports
it). The tokens are hidden: no node kind or field changes.

The rule is now written twice, in Rust and in C. Tree-sitter's scanner cannot call the lexer: the grammar
crate sits below the parser, and every other host of the grammar (an editor, the tree-sitter CLI) compiles
only its C. So a test in the parser
(`the_grammars_scanner_reads_form_starts_as_the_lexer_does`) holds the two equal on every line of the
repository's spec, its integrations and examples, and on the shapes they do not write; a corpus test in the
grammar crate leaves each top-level body of those files unclosed and requires every top-level form to stay.

### Consequences

- The graph keeps the blocks after a body left open; the file still has its syntax error.
- A valid file is read as before unless a line inside a body, indented no deeper than its form, reads as a
  form start (`guard expr { … }` at column 0 inside a block at column 0): that body now ends there, as the
  LSP's document already read it. No file of the repository's spec does; the corpus test of D6 checks it.
- Unclosed strings are still ended by D2's recovery.

### Rejected

- **The scanner in Rust, linked into the grammar crate**: it could not call the parser's lexer either (a
  dependency cycle), and the grammar would then work in no other tree-sitter host.
- **A zero-width token that silently closes every body**: the tree would hold no error, and an unclosed body
  would compile clean.
- **A recovery pass in the parser** (end the body before the next form line and reparse the pieces, as D2 does
  for strings): the formatter and every other reader of the CST would still see tree-sitter's reading.

## Amendment: one reading of a broken document (2026-10-12, architecture round 7, plan 14)

D2's recovery now cuts the source before every form start the grammar's tree missed, not only after an
unclosed string, and the parser and the formatter read its pieces (`specforge_parser::cst`). D6's rule is
asked in one walk of a document (`lex::form_starts`) by recovery and the LSP's document; D8's scanner also
ends a field left without its value, `)` and `>`. See ADR 0063.
