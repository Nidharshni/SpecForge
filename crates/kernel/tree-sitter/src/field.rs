//! The grammar's field names (`grammar.js`; generated into
//! `src/node-types.json`): what `tree_sitter::Node::child_by_field_name`
//! takes. The parser's AST walk and the formatter name fields through these
//! and through no literal of their own; `tests/vocabulary.rs` holds this list
//! to the compiled grammar both ways (ADR 0038).

/// `import_binding`, `namespace_import`: the name an import is bound to.
pub const ALIAS: &str = "alias";
/// `type_generic`: the generic's base type.
pub const BASE: &str = "base";
/// `use_import`, `pub_use_import`: the `{ A, B }` bindings.
pub const BINDINGS: &str = "bindings";
/// `verify_statement`: the description string.
pub const DESCRIPTION: &str = "description";
/// `array_type`: the element type.
pub const ELEMENT: &str = "element";
/// `ref_inline`, `ref_full`: the `scheme.kind:id` token.
pub const ID: &str = "id";
/// `field`: the key.
pub const KEY: &str = "key";
/// `entity_block`, `union_block`, `verify_statement`: the keyword or verify kind.
pub const KIND: &str = "kind";
/// `expr_or`, `expr_and`, `expr_cmp`, `expr_add`: the left operand.
pub const LHS: &str = "lhs";
/// `entity_block`, `spec_block`, `define_block`, `union_block`,
/// `method_statement`, `parameter`: the name.
pub const NAME: &str = "name";
/// `use_import`, `pub_use_import`: the `* as namespace` import.
pub const NAMESPACE: &str = "namespace";
/// `expr_cmp`, `expr_add`: the operator.
pub const OP: &str = "op";
/// `parameter`: the `?` of an optional parameter.
pub const OPTIONAL: &str = "optional";
/// `use_import`, `pub_use_import`: the path string.
pub const PATH: &str = "path";
/// `method_statement`, `function_type`: the return type.
pub const RETURNS: &str = "returns";
/// `expr_or`, `expr_and`, `expr_cmp`, `expr_add`: the right operand.
pub const RHS: &str = "rhs";
/// `entity_block`, `ref_inline`, `ref_full`: the title string.
pub const TITLE: &str = "title";
/// `parameter`: the parameter's type.
pub const TYPE: &str = "type";
/// `field`: the value.
pub const VALUE: &str = "value";
/// `union_block`: the `a | b | …` variants.
pub const VARIANTS: &str = "variants";

/// Every constant above.
pub const ALL: &[&str] = &[
    ALIAS,
    BASE,
    BINDINGS,
    DESCRIPTION,
    ELEMENT,
    ID,
    KEY,
    KIND,
    LHS,
    NAME,
    NAMESPACE,
    OP,
    OPTIONAL,
    PATH,
    RETURNS,
    RHS,
    TITLE,
    TYPE,
    VALUE,
    VARIANTS,
];
