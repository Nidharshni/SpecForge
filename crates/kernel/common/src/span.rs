use crate::Sym;

#[derive(
    Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, crate::shape::Shape,
)]
#[shape(define)]
pub struct SourceSpan {
    pub file: Sym,
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}
