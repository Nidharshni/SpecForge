//! The `.spec` language read from text: the one lexer (`lex`), the recovered syntax (`cst`), the AST the parser
//! walks it into (`ast`, `parse`), the expression grammar (`expr`) and the format version header
//! (`format_version`). Every reader of `.spec` text goes through it (ADR 0038, ADR 0063).

pub mod ast;
pub mod cst;
pub mod expr;
pub mod format_version;
pub mod lex;
mod parse;

pub use ast::{
    Annotation, Entity, EntityId, EntityKind, FieldEntry, FieldMap, FieldValue, ImportBinding,
    ImportDeclaration, ImportKind, MethodDecl, Parameter, ParseError, REF_SCHEME_FIELD, SpannedRef,
    SpecFile, UNION_VARIANTS_FIELD, VerifyStatement,
};
pub use expr::{CmpOp, Expr, ExprError, ExprSpan, SpannedExpr, parse_expression};
pub use format_version::{
    CURRENT_FORMAT_VERSION, FORMAT_HEADER_PREFIX, FormatVersion, MAX_SUPPORTED_VERSION,
    MIN_SUPPORTED_VERSION, detect_format_version,
};
pub use parse::{parse, parse_incremental};
