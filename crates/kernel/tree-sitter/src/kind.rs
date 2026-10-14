//! The kinds of the grammar's named nodes (`grammar.js`; generated into
//! `src/node-types.json`): what `tree_sitter::Node::kind()` returns. The
//! parser's AST walk and the formatter name node kinds through these and
//! through no literal of their own; `tests/vocabulary.rs` holds this list to
//! the compiled grammar both ways, so a grammar change that adds, removes or
//! renames a rule fails until the list follows it (ADR 0038).
//!
//! Anonymous nodes (`{`, `|`, `->`, `not`, keywords) are their own text and
//! have no constant; an `ERROR` node is `Node::is_error()`.

/// The whole file: imports and top-level blocks.
pub const SOURCE_FILE: &str = "source_file";
/// `// …` to the end of its line (an extra: it may sit between any tokens).
pub const COMMENT: &str = "comment";
/// `use "path"`, `use { A } from "path"`, `use * as a from "path"`.
pub const USE_IMPORT: &str = "use_import";
/// `pub use …`, the re-exporting import.
pub const PUB_USE_IMPORT: &str = "pub_use_import";
/// `{ A, B as C }` of an import.
pub const IMPORT_BINDINGS: &str = "import_bindings";
/// One `A` or `A as B` of an import's bindings.
pub const IMPORT_BINDING: &str = "import_binding";
/// `* as alias` of an import.
pub const NAMESPACE_IMPORT: &str = "namespace_import";
/// `kind name ["Title"] { … }`: any keyword's block.
pub const ENTITY_BLOCK: &str = "entity_block";
/// `spec "Title" { … }`, the project root.
pub const SPEC_BLOCK: &str = "spec_block";
/// A `ref` block: a `REF_INLINE` or a `REF_FULL`.
pub const REF_BLOCK: &str = "ref_block";
/// `ref scheme.kind:id "Title"`.
pub const REF_INLINE: &str = "ref_inline";
/// `ref scheme.kind:id "Title" { … }`.
pub const REF_FULL: &str = "ref_full";
/// `scheme.kind:id`, one token.
pub const SCHEME_REF_ID: &str = "scheme_ref_id";
/// `define name { … }` (ADR 0005: parsed, registers nothing).
pub const DEFINE_BLOCK: &str = "define_block";
/// `kind name = a | b | …`.
pub const UNION_BLOCK: &str = "union_block";
/// A union block's `a | b | …`.
pub const UNION_VARIANTS: &str = "union_variants";
/// `key value [@annotation …]` in a body.
pub const FIELD: &str = "field";
/// `@name ["arg"]`.
pub const ANNOTATION: &str = "annotation";
/// `verify [kind] "description"`.
pub const VERIFY_STATEMENT: &str = "verify_statement";
/// `method name(params) [-> Type]`.
pub const METHOD_STATEMENT: &str = "method_statement";
/// `name[?]: Type [@annotation …]` of a method.
pub const PARAMETER: &str = "parameter";
/// `Type[]`.
pub const ARRAY_TYPE: &str = "array_type";
/// `Base<T, …>`.
pub const TYPE_GENERIC: &str = "type_generic";
/// `()`.
pub const UNIT_TYPE: &str = "unit_type";
/// `fn(A, …) [-> R]`.
pub const FUNCTION_TYPE: &str = "function_type";
/// `T | U | "lit"` as a field's value.
pub const TYPE_UNION: &str = "type_union";
/// `[a, b, …]`.
pub const LIST: &str = "list";
/// `{ key value … }` as a field's value.
pub const NESTED_BLOCK: &str = "nested_block";
/// `expr { … }`.
pub const EXPR_GROUP: &str = "expr_group";
/// `a or b`.
pub const EXPR_OR: &str = "expr_or";
/// `a and b`.
pub const EXPR_AND: &str = "expr_and";
/// `a < b` (or a lone operand).
pub const EXPR_CMP: &str = "expr_cmp";
/// `a + b - c`.
pub const EXPR_ADD: &str = "expr_add";
/// A number, identifier, `( … )`, `-x` or `not x`.
pub const EXPR_ATOM: &str = "expr_atom";
/// `100`, `100ms`, `1.5s` in an expression.
pub const NUMBER_WITH_UNIT: &str = "number_with_unit";
/// `[a-zA-Z_][a-zA-Z0-9_]*`.
pub const IDENTIFIER: &str = "identifier";
/// `"…"`.
pub const STRING: &str = "string";
/// `"""…"""`.
pub const TRIPLE_QUOTED_STRING: &str = "triple_quoted_string";
/// `2026-10-07`.
pub const DATE_LITERAL: &str = "date_literal";
/// `42`.
pub const INTEGER: &str = "integer";
/// `-42`.
pub const NEGATIVE_INTEGER: &str = "negative_integer";
/// `true`, `false`.
pub const BOOLEAN: &str = "boolean";

/// Every constant above.
pub const ALL: &[&str] = &[
    SOURCE_FILE,
    COMMENT,
    USE_IMPORT,
    PUB_USE_IMPORT,
    IMPORT_BINDINGS,
    IMPORT_BINDING,
    NAMESPACE_IMPORT,
    ENTITY_BLOCK,
    SPEC_BLOCK,
    REF_BLOCK,
    REF_INLINE,
    REF_FULL,
    SCHEME_REF_ID,
    DEFINE_BLOCK,
    UNION_BLOCK,
    UNION_VARIANTS,
    FIELD,
    ANNOTATION,
    VERIFY_STATEMENT,
    METHOD_STATEMENT,
    PARAMETER,
    ARRAY_TYPE,
    TYPE_GENERIC,
    UNIT_TYPE,
    FUNCTION_TYPE,
    TYPE_UNION,
    LIST,
    NESTED_BLOCK,
    EXPR_GROUP,
    EXPR_OR,
    EXPR_AND,
    EXPR_CMP,
    EXPR_ADD,
    EXPR_ATOM,
    NUMBER_WITH_UNIT,
    IDENTIFIER,
    STRING,
    TRIPLE_QUOTED_STRING,
    DATE_LITERAL,
    INTEGER,
    NEGATIVE_INTEGER,
    BOOLEAN,
];
