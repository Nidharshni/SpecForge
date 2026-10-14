pub mod ast;
pub mod cst;
pub mod expr;
pub mod format_version;
pub mod lex;
mod parse;
#[cfg(feature = "testing")]
pub mod testing;

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
