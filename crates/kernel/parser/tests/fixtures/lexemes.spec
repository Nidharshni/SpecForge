// Forms of the language the repository's spec does not write, read by
// lexer_agrees_with_the_grammar. It must parse without an error.
use "./other.spec"

behavior multi "Strings spanning lines" {
  contract "a regular string
    may run on across lines, as the grammar reads it"
  description "an escaped \" quote, and a \\ backslash"
  notes """a triple-quoted string with "one" and ""two"" quotes"""
  refs [gh.issue:42, web.page:https://x.io/a//b?q=1]
}

ref web.page:https://example.org/a//b "A page"

type state = "open" | "closed" | -1 | 2 | ready
