// Where an unclosed body ends (ADR 0038 D7).
//
// The scanner produces `_form_start`: zero-width, after the token before a
// line that starts a top-level form. The line test is the
// lexer's form-start rule, `specforge_parser::lex::starts_top_level_form`,
// read here over the line's characters: the line is indented by at most the
// indentation of the form being read, and its lexemes read `use` then a
// path, `{` or `*`; `pub use` and the same; `spec "Title" {`;
// `ref scheme.kind:id "Title"`; `kind name ["Title"] {`; or `kind name =`.
// A title is a `"…"` closed on that line.
//
// Between top-level forms the token is valid (and optional): the scanner
// records the form's indentation in its state. Inside a top-level form's
// body it is not valid; the grammar makes `_in_body` valid there only so
// that the scanner runs, and tree-sitter, finding `_form_start` unexpected,
// recovers by inserting the body's MISSING "}" before it. The tree keeps
// the error, and every form after the body.
//
// Tree-sitter inserts one MISSING token per error, so a nested block, list
// or expression group left open inside that body is ended first by a
// second token, `_close`, zero-width too and valid there as the
// alternative to its closing bracket; the body's MISSING "}" then reports
// the error for them all.

#include "tree_sitter/alloc.h"
#include "tree_sitter/parser.h"

#include <stdbool.h>
#include <stdint.h>
#include <string.h>

enum TokenType {
  FORM_START,
  IN_BODY,
  CLOSE,
};

// The indentation, in bytes, of the line the current top-level form started
// on.
typedef struct {
  uint32_t indent;
} Scanner;

void *tree_sitter_specforge_external_scanner_create(void) {
  Scanner *scanner = ts_calloc(1, sizeof(Scanner));
  return scanner;
}

void tree_sitter_specforge_external_scanner_destroy(void *payload) {
  ts_free(payload);
}

unsigned tree_sitter_specforge_external_scanner_serialize(void *payload, char *buffer) {
  memcpy(buffer, payload, sizeof(Scanner));
  return sizeof(Scanner);
}

void tree_sitter_specforge_external_scanner_deserialize(void *payload, const char *buffer,
                                                         unsigned length) {
  Scanner *scanner = payload;
  if (length == sizeof(Scanner)) {
    memcpy(scanner, buffer, sizeof(Scanner));
  } else {
    scanner->indent = 0;
  }
}

// Whitespace inside a line: Rust's `is_ascii_whitespace` without `\n`.
static bool is_line_space(int32_t c) {
  return c == ' ' || c == '\t' || c == '\f' || c == '\r';
}

// A character that starts an identifier, as the lexer reads one: an ASCII
// letter, `_`, or any non-ASCII character.
static bool starts_ident(int32_t c) {
  return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || c == '_' || c >= 0x80;
}

static bool is_word(int32_t c) {
  return starts_ident(c) || (c >= '0' && c <= '9');
}

static bool starts_ascii_ident(int32_t c) {
  return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || c == '_';
}

static bool is_ascii_word(int32_t c) {
  return starts_ascii_ident(c) || (c >= '0' && c <= '9');
}

// A character of a scheme ref ID's last part: the grammar's
// `/[^\s"{}()\[\],]+/`.
static bool is_ref_id_char(int32_t c) {
  return !(c == ' ' || c == '\t' || c == '\n' || c == '\f' || c == '\r' || c == '"' ||
           c == '{' || c == '}' || c == '(' || c == ')' || c == '[' || c == ']' || c == ',');
}

static void advance(TSLexer *lexer) { lexer->advance(lexer, false); }

static void skip_line_space(TSLexer *lexer) {
  while (!lexer->eof(lexer) && is_line_space(lexer->lookahead)) {
    advance(lexer);
  }
}

#define WORD_MAX 8

// Read an identifier lexeme into `word` (its first WORD_MAX - 1 characters,
// enough to tell the keywords apart). False when none starts here, or when
// a `.` follows it: the lexer then reads a scheme ref ID or a `.`, and
// neither starts a form in a word's place.
static bool read_word(TSLexer *lexer, char word[WORD_MAX]) {
  if (lexer->eof(lexer) || !starts_ident(lexer->lookahead)) {
    return false;
  }
  unsigned length = 0;
  while (!lexer->eof(lexer) && is_word(lexer->lookahead)) {
    if (length < WORD_MAX - 1) {
      // Non-ASCII characters never match a keyword.
      word[length++] = lexer->lookahead < 0x80 ? (char)lexer->lookahead : '\x01';
    } else {
      word[WORD_MAX - 2] = '\x01';
    }
    advance(lexer);
  }
  word[length] = '\0';
  return lexer->eof(lexer) || lexer->lookahead != '.';
}

static bool read_ascii_ident(TSLexer *lexer) {
  if (lexer->eof(lexer) || !starts_ascii_ident(lexer->lookahead)) {
    return false;
  }
  while (!lexer->eof(lexer) && is_ascii_word(lexer->lookahead)) {
    advance(lexer);
  }
  return true;
}

// A scheme ref ID: `scheme.kind:id`.
static bool read_ref_id(TSLexer *lexer) {
  if (!read_ascii_ident(lexer) || lexer->lookahead != '.') {
    return false;
  }
  advance(lexer);
  if (!read_ascii_ident(lexer) || lexer->lookahead != ':') {
    return false;
  }
  advance(lexer);
  if (lexer->eof(lexer) || !is_ref_id_char(lexer->lookahead)) {
    return false;
  }
  while (!lexer->eof(lexer) && is_ref_id_char(lexer->lookahead)) {
    advance(lexer);
  }
  return true;
}

// A title: a `"…"` closed on this line (`\` escapes any character but a
// line break), not a `"""`.
static bool read_title(TSLexer *lexer) {
  if (lexer->lookahead != '"') {
    return false;
  }
  advance(lexer);
  if (lexer->lookahead == '"') {
    advance(lexer);
    return lexer->eof(lexer) || lexer->lookahead != '"';
  }
  for (;;) {
    if (lexer->eof(lexer) || lexer->lookahead == '\n') {
      return false;
    }
    if (lexer->lookahead == '\\') {
      advance(lexer);
      if (lexer->eof(lexer) || lexer->lookahead == '\n') {
        return false;
      }
    } else if (lexer->lookahead == '"') {
      advance(lexer);
      return true;
    }
    advance(lexer);
  }
}

// An import's target: a path, bindings or `* as`.
static bool at_import_target(TSLexer *lexer) {
  return !lexer->eof(lexer) &&
         (lexer->lookahead == '"' || lexer->lookahead == '{' || lexer->lookahead == '*');
}

// Whether the line from here reads a top-level form's first lexemes.
static bool line_starts_form(TSLexer *lexer) {
  char word[WORD_MAX];
  if (!read_word(lexer, word)) {
    return false;
  }
  skip_line_space(lexer);
  if (strcmp(word, "use") == 0) {
    return at_import_target(lexer);
  }
  if (strcmp(word, "pub") == 0) {
    char next[WORD_MAX];
    if (!read_word(lexer, next) || strcmp(next, "use") != 0) {
      return false;
    }
    skip_line_space(lexer);
    return at_import_target(lexer);
  }
  if (strcmp(word, "spec") == 0) {
    if (!read_title(lexer)) {
      return false;
    }
    skip_line_space(lexer);
    return lexer->lookahead == '{';
  }
  if (strcmp(word, "ref") == 0) {
    if (!read_ref_id(lexer)) {
      return false;
    }
    skip_line_space(lexer);
    return read_title(lexer);
  }
  char name[WORD_MAX];
  if (!read_word(lexer, name)) {
    return false;
  }
  skip_line_space(lexer);
  if (lexer->lookahead == '=') {
    return true;
  }
  if (lexer->lookahead == '"') {
    if (!read_title(lexer)) {
      return false;
    }
    skip_line_space(lexer);
  }
  return lexer->lookahead == '{';
}

bool tree_sitter_specforge_external_scanner_scan(void *payload, TSLexer *lexer,
                                                  const bool *valid_symbols) {
  Scanner *scanner = payload;
  bool top_level = valid_symbols[FORM_START];
  bool in_body = valid_symbols[IN_BODY];
  bool nested = valid_symbols[CLOSE];
  // More than one valid: tree-sitter is recovering from an error and offers
  // every token. Zero-width tokens are dropped there anyway.
  if (top_level + in_body + nested != 1) {
    return false;
  }
  // The token is zero-width at the end of the previous one: the whitespace
  // after it stays the next token's, so a body ended here ends at its last
  // member, and the next scan reads the same line break again.
  lexer->mark_end(lexer);
  bool new_line = false;
  uint32_t indent = 0;
  while (!lexer->eof(lexer) && (is_line_space(lexer->lookahead) || lexer->lookahead == '\n')) {
    if (lexer->lookahead == '\n') {
      new_line = true;
      indent = 0;
    } else {
      indent++;
    }
    advance(lexer);
  }
  if (lexer->eof(lexer)) {
    return false;
  }
  // The form's first lexeme is the first of its line; with no line break
  // before it, only at the text's start, before any body.
  if (!new_line && (!top_level || lexer->get_column(lexer) != indent)) {
    return false;
  }
  if (!top_level && indent > scanner->indent) {
    return false;
  }
  if (!line_starts_form(lexer)) {
    return false;
  }
  if (nested) {
    lexer->result_symbol = CLOSE;
  } else {
    scanner->indent = indent;
    lexer->result_symbol = FORM_START;
  }
  return true;
}
