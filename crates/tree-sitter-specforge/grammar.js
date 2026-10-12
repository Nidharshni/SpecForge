/// <reference types="tree-sitter-cli/dsl" />
// Tree-sitter grammar for the SpecForge .spec DSL
//
// Design principle: ZERO domain knowledge in core.
// Only 4 constructs have dedicated grammar rules:
//   - use (import syntax)
//   - spec (singleton project root)
//   - ref (scheme.kind:identifier compound ID)
//   - define (user-defined entity types)
//
// Everything else is parsed by the generic entity_block rule.
// The parser accepts ANY keyword — validation happens in the semantic phase.

module.exports = grammar({
  name: "specforge",

  extras: ($) => [/\s/, $.comment],

  // src/scanner.c: where an unclosed body ends (ADR 0038 D7). All three
  // tokens are zero-width and hidden.
  //   _form_start  before a line that starts a top-level form, by the
  //                lexer's form-start rule (`lex::starts_top_level_form`).
  //                Valid only between top-level forms; inside a top-level
  //                form's body it is an unexpected token, and tree-sitter
  //                recovers by inserting the body's MISSING "}" before it.
  //   _in_body     never produced: valid wherever such a body may hold a
  //                member or close, so the scanner runs there.
  //   _close       ends whatever a body member left open (a bracket, or a
  //                field's value) at the same line (tree-sitter inserts one
  //                MISSING token per error; the body's reports it).
  externals: ($) => [$._form_start, $._in_body, $._close],

  word: ($) => $.identifier,

  rules: {
    source_file: ($) =>
      repeat(
        seq(
          optional($._form_start),
          choice($.use_import, $.pub_use_import, $._block),
        ),
      ),

    // --- Comments ----------------------------------------------------
    comment: (_) => token(seq("//", /.*/)),

    // --- Use imports -------------------------------------------------
    import_binding: ($) =>
      seq($.identifier, optional(seq("as", field("alias", $.identifier)))),

    import_bindings: ($) =>
      seq("{", commaSep1($.import_binding), "}"),

    namespace_import: ($) =>
      seq("*", "as", field("alias", $.identifier)),

    use_import: ($) =>
      seq(
        "use",
        choice(
          // Full: use "path"
          field("path", $.string),
          // Selective: use { A, B } from "path"
          seq(field("bindings", $.import_bindings), "from", field("path", $.string)),
          // Namespace: use * as alias from "path"
          seq(field("namespace", $.namespace_import), "from", field("path", $.string)),
        ),
      ),

    pub_use_import: ($) =>
      seq(
        "pub",
        "use",
        choice(
          // Full: pub use "path"
          field("path", $.string),
          // Selective: pub use { A, B } from "path"
          seq(field("bindings", $.import_bindings), "from", field("path", $.string)),
          // Namespace: pub use * as alias from "path"
          seq(field("namespace", $.namespace_import), "from", field("path", $.string)),
        ),
      ),

    // --- Top-level blocks --------------------------------------------
    _block: ($) =>
      choice(
        $.spec_block,
        $.ref_block,
        $.define_block,
        $.union_block,
        $.entity_block,
      ),

    // --- Generic entity block (ANY keyword) --------------------------
    // This is the core of zero-domain-knowledge parsing.
    // Parses: keyword name ["title"] { fields }
    entity_block: ($) =>
      seq(
        field("kind", $.identifier),
        field("name", $.identifier),
        optional(field("title", $.string)),
        "{",
        repeat(choice($.field, $.verify_statement, $.method_statement, $._in_body)),
        "}",
      ),

    // --- spec (singleton, dedicated rule) ----------------------------
    spec_block: ($) =>
      seq(
        "spec",
        field("name", $.string),
        "{",
        repeat(choice($.field, $.verify_statement, $.method_statement, $._in_body)),
        "}",
      ),

    // --- ref (dedicated rule — compound ID syntax) -------------------
    ref_block: ($) =>
      choice($.ref_inline, $.ref_full),

    ref_inline: ($) =>
      seq(
        "ref",
        field("id", $.scheme_ref_id),
        field("title", $.string),
      ),

    ref_full: ($) =>
      seq(
        "ref",
        field("id", $.scheme_ref_id),
        field("title", $.string),
        "{",
        repeat(choice($.field, $._in_body)),
        "}",
      ),

    scheme_ref_id: (_) =>
      token(seq(
        /[a-zA-Z_][a-zA-Z0-9_]*/,
        ".",
        /[a-zA-Z_][a-zA-Z0-9_]*/,
        ":",
        /[^\s"{}()\[\],]+/,
      )),

    // --- define (dedicated rule) -------------------------------------
    define_block: ($) =>
      seq(
        "define",
        field("name", $.identifier),
        "{",
        repeat(choice($.field, $.verify_statement, $.method_statement, $._in_body)),
        "}",
      ),

    // --- Union type block: keyword name = variant | variant | ... ---
    union_block: ($) =>
      seq(
        field("kind", $.identifier),
        field("name", $.identifier),
        "=",
        field("variants", $.union_variants),
      ),

    union_variants: ($) =>
      seq($._union_variant, repeat(seq("|", $._union_variant))),

    _union_variant: ($) =>
      choice($.string, $.negative_integer, $.integer, $.identifier),

    negative_integer: (_) => token(seq("-", /[0-9]+/)),

    // --- Shared rules ------------------------------------------------

    // Field: key value [annotations...]
    // `verify` is word-reserved for verify statements, but a type may still
    // declare a field named verify (`verify string @optional`): with no
    // string after it, it is a field key.
    field: ($) =>
      seq(
        field("key", choice($.identifier, alias("verify", $.identifier))),
        // A name typed before a form line has no value yet: `_close` ends
        // the field there, and the body misses its "}" (ADR 0063 D2).
        field("value", choice($._value, $._close)),
        repeat($.annotation),
      ),

    annotation: ($) =>
      seq(token(seq("@", /[a-zA-Z_][a-zA-Z0-9_]*/)), optional($.string)),

    _value: ($) =>
      choice(
        $.triple_quoted_string,
        $.string,
        $.date_literal,
        $.negative_integer,
        $.integer,
        $.boolean,
        $.list,
        $.nested_block,
        $.array_type,
        $.identifier,
        $.type_union,
        $.expr_group,
      ),
    // Type[] — array type suffix (e.g., ImportDeclaration[]); nests
    // (float[][]) and applies to generics (Result<T, E>[]).
    array_type: ($) =>
      seq(
        field("element", choice($.identifier, $.type_generic, $.array_type)),
        token.immediate("[]"),
      ),

    // method name(param: Type, ...) -> ReturnType
    // A generic block member (ports define their interfaces this way);
    // `method` is word-reserved like `verify`. Types nest: generics and
    // array suffixes compose (Result<string[], EmitterError>).
    method_statement: ($) =>
      seq(
        "method",
        field("name", $.identifier),
        "(",
        optional(commaSep1($.parameter)),
        choice(")", $._close),
        optional(seq("->", field("returns", $._type_ref))),
      ),

    // name[?]: Type [annotations...] — `?` marks an optional parameter.
    parameter: ($) =>
      seq(
        field("name", $.identifier),
        optional(field("optional", "?")),
        ":",
        field("type", $._type_ref),
        repeat($.annotation),
      ),

    _type_ref: ($) =>
      choice(
        $.type_generic,
        $.array_type,
        $.identifier,
        $.unit_type,
        $.function_type,
      ),

    // () — the unit type (Result<(), E>).
    unit_type: (_) => seq("(", ")"),

    // fn(A, B) -> R — a function type.
    function_type: ($) =>
      seq(
        "fn",
        "(",
        optional(commaSep1($._type_ref)),
        choice(")", $._close),
        optional(seq("->", field("returns", $._type_ref))),
      ),

    type_generic: ($) =>
      seq(
        field("base", $.identifier),
        "<",
        commaSep1($._type_ref),
        choice(">", $._close),
      ),

    // Union-typed field declaration: query_scope string | string[]
    // Engages only when `|` follows a type; bare identifiers stay plain
    // values. (RES-20 direction: declared field types.)
    // Members may also be string literals: status "ok" | "error".
    type_union: ($) =>
      prec.right(
        seq(
          choice($._type_ref, $.string),
          repeat1(seq("|", choice($._type_ref, $.string))),
        ),
      ),

    // verify [kind] "description"
    // Wins over a field keyed `verify` whenever a string follows.
    verify_statement: ($) =>
      prec(
        1,
        seq(
          "verify",
          optional(field("kind", $.identifier)),
          field("description", $.string),
        ),
      ),

    // --- Lists -------------------------------------------------------
    list: ($) =>
      seq(
        "[",
        optional(commaSep1($._list_item)),
        optional(","),
        choice("]", $._close),
      ),

    _list_item: ($) =>
      choice($.string, $.scheme_ref_id, $.integer, $.identifier),

    // Nested block: { key value ... }
    nested_block: ($) =>
      seq("{", repeat($.field), choice("}", $._close)),
    // Formal expression group: expr { a < 10ms and b > 5 }
    // A generic value form — no domain knowledge; validation is semantic.
    expr_group: ($) =>
      seq(
        "expr",
        "{",
        $._expression,
        repeat(seq(optional(","), $._expression)),
        optional(","),
        choice("}", $._close),
      ),

    _expression: ($) => $.expr_or,

    // Precedence (loosest to tightest): or, and, comparison, additive
    expr_or: ($) =>
      prec.left(
        1,
        seq(field("lhs", $.expr_and), repeat(seq("or", field("rhs", $.expr_and)))),
      ),

    expr_and: ($) =>
      prec.left(
        2,
        seq(field("lhs", $.expr_cmp), repeat(seq("and", field("rhs", $.expr_cmp)))),
      ),

    expr_cmp: ($) =>
      seq(
        field("lhs", $.expr_add),
        optional(
          seq(
            field("op", choice("<", "<=", ">", ">=", "==", "!=")),
            field("rhs", $.expr_add),
          ),
        ),
      ),

    expr_add: ($) =>
      prec.left(
        3,
        seq(
          field("lhs", $.expr_atom),
          repeat(seq(field("op", choice("+", "-")), field("rhs", $.expr_atom))),
        ),
      ),

    expr_atom: ($) =>
      choice(
        $.number_with_unit,
        $.identifier,
        seq("(", $._expression, choice(")", $._close)),
        prec(4, seq("-", $.expr_atom)),
        prec(4, seq("not", $.expr_atom)),
      ),

    identifier: (_) => /[a-zA-Z_][a-zA-Z0-9_]*/,

    string: (_) => token(seq('"', repeat(choice(/[^"\\]/, seq("\\", /./))), '"')),

    triple_quoted_string: (_) => {
      return token(seq(
        '"""',
        repeat(choice(
          /[^"]/,
          seq('"', /[^"]/),
          seq('""', /[^"]/),
        )),
        '"""',
      ));
    },

    date_literal: (_) => token(seq(/[0-9]{4}/, "-", /[0-9]{2}/, "-", /[0-9]{2}/)),

    integer: (_) => /[0-9]+/,
    boolean: (_) => choice("true", "false"),

    // Numeric literal with optional unit suffix: 100, 100ms, 1.5s
    number_with_unit: (_) =>
      token(seq(/[0-9]+/, optional(seq(".", /[0-9]+/)), /[a-zA-Z]*/)),
  },
});

function commaSep1(rule) {
  return seq(rule, repeat(seq(",", rule)));
}
