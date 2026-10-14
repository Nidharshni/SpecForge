//! Formal expression language for `.spec` metric blocks (RES-25 rung 2).
//!
//! A small, typed expression AST shared by the prove pass, the validator,
//! and authoring tools. The surface language:
//!
//! ```text
//! latency < 100ms
//! latency > 500ms and latency < 1s
//! peak_memory + cache_size <= 64MB
//! not (retries == 0 or timeout > 30s)
//! ```
//!
//! Precedence (loosest to tightest): `or`, `and`, comparisons, `+`/`-`,
//! unary `-`, primaries (numbers with optional unit suffixes, identifiers,
//! parenthesized groups). Spans are 1-based line/column pairs relative to
//! the parsed input, matching tree-sitter byte-column conventions.

use serde::Serialize;

use specforge_common::{SourceSpan, Sym};

// ── AST ─────────────────────────────────────────────────────────────────────

/// A 1-based line/column source range relative to the parsed input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExprSpan {
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Le,
    Ge,
    Lt,
    Gt,
    Eq,
    Ne,
}

impl CmpOp {
    pub fn as_smt(self) -> &'static str {
        match self {
            Self::Le => "<=",
            Self::Ge => ">=",
            Self::Lt => "<",
            Self::Gt => ">",
            Self::Eq => "=",
            Self::Ne => "distinct",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Le => "<=",
            Self::Ge => ">=",
            Self::Lt => "<",
            Self::Gt => ">",
            Self::Eq => "==",
            Self::Ne => "!=",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Numeric literal with an optional unit suffix (`100`, `100ms`).
    /// Units are carried, not interpreted — unit normalization is future work.
    Num(f64, String),
    Var(String),
    Cmp(CmpOp, Box<SpannedExpr>, Box<SpannedExpr>),
    And(Box<SpannedExpr>, Box<SpannedExpr>),
    Or(Box<SpannedExpr>, Box<SpannedExpr>),
    Add(Box<SpannedExpr>, Box<SpannedExpr>),
    Sub(Box<SpannedExpr>, Box<SpannedExpr>),
    Neg(Box<SpannedExpr>),
    Not(Box<SpannedExpr>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpannedExpr {
    pub expr: Expr,
    pub span: ExprSpan,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExprError {
    pub message: String,
    pub line: usize,
    pub col: usize,
}

impl std::fmt::Display for ExprError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (line {}, col {})", self.message, self.line, self.col)
    }
}

impl std::error::Error for ExprError {}

impl SpannedExpr {
    /// Binding strength for parenthesization when rendering.
    fn precedence(&self) -> u8 {
        match &self.expr {
            Expr::Or(_, _) => 1,
            Expr::And(_, _) => 2,
            Expr::Cmp(_, _, _) => 3,
            Expr::Add(_, _) | Expr::Sub(_, _) => 4,
            Expr::Neg(_) | Expr::Not(_) => 5,
            Expr::Num(_, _) | Expr::Var(_) => 6,
        }
    }

    fn render(&self, parent: u8, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.precedence() < parent {
            write!(f, "(")?;
            self.render_inner(f)?;
            write!(f, ")")
        } else {
            self.render_inner(f)
        }
    }

    fn render_inner(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.expr {
            Expr::Num(v, unit) => {
                if v.fract() == 0.0 {
                    write!(f, "{}", *v as i64)?;
                } else {
                    write!(f, "{v}")?;
                }
                write!(f, "{unit}")
            }
            Expr::Var(name) => write!(f, "{name}"),
            Expr::Cmp(op, l, r) => {
                l.render(4, f)?;
                write!(f, " {} ", op.as_str())?;
                r.render(4, f)
            }
            Expr::And(l, r) => {
                l.render(2, f)?;
                write!(f, " and ")?;
                r.render(3, f)
            }
            Expr::Or(l, r) => {
                l.render(1, f)?;
                write!(f, " or ")?;
                r.render(2, f)
            }
            Expr::Add(l, r) => {
                l.render(4, f)?;
                write!(f, " + ")?;
                r.render(5, f)
            }
            Expr::Sub(l, r) => {
                l.render(4, f)?;
                write!(f, " - ")?;
                r.render(5, f)
            }
            Expr::Neg(e) => {
                write!(f, "-")?;
                e.render(5, f)
            }
            Expr::Not(e) => {
                write!(f, "not ")?;
                e.render(5, f)
            }
        }
    }
}

impl std::fmt::Display for SpannedExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.render(0, f)
    }
}

impl SpannedExpr {
    /// Best-effort mapping to a file span, given the origin (1-based line,
    /// 1-based column) of this expression's first character in the file.
    /// Relative lines shift the origin line; columns are exact only on the
    /// origin line (dedented block content loses column fidelity).
    pub fn absolute_span(&self, file: Sym, origin_line: usize, origin_col: usize) -> SourceSpan {
        SourceSpan {
            file,
            start_line: origin_line + self.span.start_line - 1,
            start_col: if self.span.start_line == 1 {
                origin_col + self.span.start_col - 1
            } else {
                1
            },
            end_line: origin_line + self.span.end_line - 1,
            end_col: if self.span.end_line == 1 {
                origin_col + self.span.end_col - 1
            } else {
                1
            },
        }
    }
}

mod parser;
mod tokenizer;

pub use parser::parse_expression;

#[cfg(test)]
mod tests;
