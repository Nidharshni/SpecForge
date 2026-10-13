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

// ── tokenizer ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64, String),
    Var(String),
    And,
    Or,
    Not,
    Le,
    Ge,
    Lt,
    Gt,
    Eq,
    Ne,
    Plus,
    Minus,
    LParen,
    RParen,
}

#[derive(Debug, Clone)]
struct SpannedTok {
    tok: Tok,
    start_line: usize,
    start_col: usize,
    end_line: usize,
    end_col: usize,
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_lowercase() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit()
}

fn tokenize(src: &str) -> Result<Vec<SpannedTok>, ExprError> {
    let mut toks = Vec::new();
    let mut line = 1usize;
    let mut col = 1usize;
    let mut chars = src.chars().peekable();

    while let Some(&c) = chars.peek() {
        let (start_line, start_col) = (line, col);
        match c {
            '\n' => {
                line += 1;
                col = 1;
                chars.next();
                continue;
            }
            ' ' | '\t' | '\r' => {
                col += 1;
                chars.next();
                continue;
            }
            '(' => {
                chars.next();
                col += 1;
                toks.push(SpannedTok {
                    tok: Tok::LParen,
                    start_line,
                    start_col,
                    end_line: line,
                    end_col: col,
                });
            }
            ')' => {
                chars.next();
                col += 1;
                toks.push(SpannedTok {
                    tok: Tok::RParen,
                    start_line,
                    start_col,
                    end_line: line,
                    end_col: col,
                });
            }
            '+' => {
                chars.next();
                col += 1;
                toks.push(SpannedTok {
                    tok: Tok::Plus,
                    start_line,
                    start_col,
                    end_line: line,
                    end_col: col,
                });
            }
            '-' => {
                chars.next();
                col += 1;
                toks.push(SpannedTok {
                    tok: Tok::Minus,
                    start_line,
                    start_col,
                    end_line: line,
                    end_col: col,
                });
            }
            '<' => {
                chars.next();
                col += 1;
                let op = if chars.peek() == Some(&'=') {
                    chars.next();
                    col += 1;
                    Tok::Le
                } else {
                    Tok::Lt
                };
                toks.push(SpannedTok {
                    tok: op,
                    start_line,
                    start_col,
                    end_line: line,
                    end_col: col,
                });
            }
            '>' => {
                chars.next();
                col += 1;
                let op = if chars.peek() == Some(&'=') {
                    chars.next();
                    col += 1;
                    Tok::Ge
                } else {
                    Tok::Gt
                };
                toks.push(SpannedTok {
                    tok: op,
                    start_line,
                    start_col,
                    end_line: line,
                    end_col: col,
                });
            }
            '=' => {
                chars.next();
                col += 1;
                if chars.peek() == Some(&'=') {
                    chars.next();
                    col += 1;
                }
                toks.push(SpannedTok {
                    tok: Tok::Eq,
                    start_line,
                    start_col,
                    end_line: line,
                    end_col: col,
                });
            }
            '!' => {
                chars.next();
                col += 1;
                if chars.peek() == Some(&'=') {
                    chars.next();
                    col += 1;
                    toks.push(SpannedTok {
                        tok: Tok::Ne,
                        start_line,
                        start_col,
                        end_line: line,
                        end_col: col,
                    });
                } else {
                    return Err(ExprError {
                        message: "expected '=' after '!' (use '!=' for inequality)".to_string(),
                        line: start_line,
                        col: start_col,
                    });
                }
            }
            _ if c.is_ascii_digit() => {
                let mut text = String::new();
                let mut seen_dot = false;
                while let Some(&d) = chars.peek() {
                    if d.is_ascii_digit() || (d == '.' && !seen_dot) {
                        if d == '.' {
                            seen_dot = true;
                        }
                        text.push(d);
                        chars.next();
                        col += 1;
                    } else {
                        break;
                    }
                }
                let mut unit = String::new();
                while let Some(&u) = chars.peek() {
                    if u.is_ascii_alphabetic() {
                        unit.push(u);
                        chars.next();
                        col += 1;
                    } else {
                        break;
                    }
                }
                let value: f64 = text.parse().map_err(|_| ExprError {
                    message: format!("invalid number '{text}'"),
                    line: start_line,
                    col: start_col,
                })?;
                toks.push(SpannedTok {
                    tok: Tok::Num(value, unit),
                    start_line,
                    start_col,
                    end_line: line,
                    end_col: col,
                });
            }
            _ if is_ident_start(c) => {
                let mut name = String::new();
                while let Some(&w) = chars.peek() {
                    if is_ident_continue(w) {
                        name.push(w);
                        chars.next();
                        col += 1;
                    } else {
                        break;
                    }
                }
                let tok = match name.as_str() {
                    "and" => Tok::And,
                    "or" => Tok::Or,
                    "not" => Tok::Not,
                    _ => Tok::Var(name.clone()),
                };
                toks.push(SpannedTok {
                    tok,
                    start_line,
                    start_col,
                    end_line: line,
                    end_col: col,
                });
            }
            _ => {
                return Err(ExprError {
                    message: format!("unexpected character '{c}'"),
                    line: start_line,
                    col: start_col,
                });
            }
        }
    }
    Ok(toks)
}

// ── parser ──────────────────────────────────────────────────────────────────

struct Parser {
    toks: Vec<SpannedTok>,
    pos: usize,
}

impl Parser {
    fn new(toks: Vec<SpannedTok>) -> Self {
        Self { toks, pos: 0 }
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|t| &t.tok)
    }

    fn bump(&mut self) -> Option<SpannedTok> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn err_at_last(&self, message: impl Into<String>) -> ExprError {
        let at = self
            .toks
            .get(self.pos)
            .or_else(|| self.toks.last())
            .map(|t| (t.start_line, t.start_col))
            .unwrap_or((1, 1));
        ExprError {
            message: message.into(),
            line: at.0,
            col: at.1,
        }
    }

    /// or (loosest) → and → comparison → additive → primary
    fn parse_or(&mut self) -> Result<SpannedExpr, ExprError> {
        let mut lhs = self.parse_and()?;
        while self.peek() == Some(&Tok::Or) {
            self.bump();
            let rhs = self.parse_and()?;
            let span = ExprSpan {
                start_line: lhs.span.start_line,
                start_col: lhs.span.start_col,
                end_line: rhs.span.end_line,
                end_col: rhs.span.end_col,
            };
            lhs = SpannedExpr {
                expr: Expr::Or(Box::new(lhs), Box::new(rhs)),
                span,
            };
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> Result<SpannedExpr, ExprError> {
        let mut lhs = self.parse_cmp()?;
        while self.peek() == Some(&Tok::And) {
            self.bump();
            let rhs = self.parse_cmp()?;
            let span = ExprSpan {
                start_line: lhs.span.start_line,
                start_col: lhs.span.start_col,
                end_line: rhs.span.end_line,
                end_col: rhs.span.end_col,
            };
            lhs = SpannedExpr {
                expr: Expr::And(Box::new(lhs), Box::new(rhs)),
                span,
            };
        }
        Ok(lhs)
    }

    fn parse_cmp(&mut self) -> Result<SpannedExpr, ExprError> {
        let lhs = self.parse_add()?;
        let op = match self.peek() {
            Some(Tok::Le) => CmpOp::Le,
            Some(Tok::Ge) => CmpOp::Ge,
            Some(Tok::Lt) => CmpOp::Lt,
            Some(Tok::Gt) => CmpOp::Gt,
            Some(Tok::Eq) => CmpOp::Eq,
            Some(Tok::Ne) => CmpOp::Ne,
            _ => return Ok(lhs),
        };
        self.bump();
        let rhs = self.parse_add()?;
        let span = ExprSpan {
            start_line: lhs.span.start_line,
            start_col: lhs.span.start_col,
            end_line: rhs.span.end_line,
            end_col: rhs.span.end_col,
        };
        Ok(SpannedExpr {
            expr: Expr::Cmp(op, Box::new(lhs), Box::new(rhs)),
            span,
        })
    }

    fn parse_add(&mut self) -> Result<SpannedExpr, ExprError> {
        let mut lhs = self.parse_primary()?;
        while matches!(self.peek(), Some(Tok::Plus) | Some(Tok::Minus)) {
            let minus = self.peek() == Some(&Tok::Minus);
            self.bump();
            let rhs = self.parse_primary()?;
            let span = ExprSpan {
                start_line: lhs.span.start_line,
                start_col: lhs.span.start_col,
                end_line: rhs.span.end_line,
                end_col: rhs.span.end_col,
            };
            let expr = if minus {
                Expr::Sub(Box::new(lhs), Box::new(rhs))
            } else {
                Expr::Add(Box::new(lhs), Box::new(rhs))
            };
            lhs = SpannedExpr { expr, span };
        }
        Ok(lhs)
    }

    fn parse_primary(&mut self) -> Result<SpannedExpr, ExprError> {
        match self.bump() {
            Some(SpannedTok {
                tok: Tok::Num(v, unit),
                start_line,
                start_col,
                end_line,
                end_col,
            }) => Ok(SpannedExpr {
                expr: Expr::Num(v, unit),
                span: ExprSpan {
                    start_line,
                    start_col,
                    end_line,
                    end_col,
                },
            }),
            Some(SpannedTok {
                tok: Tok::Var(name),
                start_line,
                start_col,
                end_line,
                end_col,
            }) => Ok(SpannedExpr {
                expr: Expr::Var(name),
                span: ExprSpan {
                    start_line,
                    start_col,
                    end_line,
                    end_col,
                },
            }),
            Some(SpannedTok {
                tok: Tok::Minus,
                start_line,
                start_col,
                ..
            }) => {
                let inner = self.parse_primary()?;
                let span = ExprSpan {
                    start_line,
                    start_col,
                    end_line: inner.span.end_line,
                    end_col: inner.span.end_col,
                };
                Ok(SpannedExpr {
                    expr: Expr::Neg(Box::new(inner)),
                    span,
                })
            }
            Some(SpannedTok {
                tok: Tok::Not,
                start_line,
                start_col,
                ..
            }) => {
                let inner = self.parse_primary()?;
                let span = ExprSpan {
                    start_line,
                    start_col,
                    end_line: inner.span.end_line,
                    end_col: inner.span.end_col,
                };
                Ok(SpannedExpr {
                    expr: Expr::Not(Box::new(inner)),
                    span,
                })
            }
            Some(SpannedTok {
                tok: Tok::LParen,
                start_line,
                start_col,
                ..
            }) => {
                let inner = self.parse_or()?;
                match self.bump() {
                    Some(SpannedTok {
                        tok: Tok::RParen,
                        end_line,
                        end_col,
                        ..
                    }) => {
                        let span = ExprSpan {
                            start_line,
                            start_col,
                            end_line,
                            end_col,
                        };
                        Ok(SpannedExpr {
                            expr: inner.expr,
                            span,
                        })
                    }
                    _ => Err(self.err_at_last("expected ')'")),
                }
            }
            Some(SpannedTok {
                tok,
                start_line,
                start_col,
                ..
            }) => Err(ExprError {
                message: format!("unexpected token {tok:?}"),
                line: start_line,
                col: start_col,
            }),
            None => {
                let (line, col) = self
                    .toks
                    .last()
                    .map(|t| (t.end_line, t.end_col))
                    .unwrap_or((1, 1));
                Err(ExprError {
                    message: "unexpected end of expression".to_string(),
                    line,
                    col,
                })
            }
        }
    }
}

/// Parse one complete expression. The whole input must be a single
/// expression — trailing tokens are an error (callers split prose lines).
pub fn parse_expression(src: &str) -> Result<SpannedExpr, ExprError> {
    let mut parser = Parser::new(tokenize(src)?);
    let expr = parser.parse_or()?;
    if parser.pos != parser.toks.len() {
        return Err(parser.err_at_last("trailing tokens after expression"));
    }
    Ok(expr)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(src: &str) -> SpannedExpr {
        parse_expression(src).unwrap_or_else(|e| panic!("parse '{src}' failed: {e}"))
    }

    #[test]
    fn parses_comparison_with_unit() {
        let e = parse_ok("latency < 100ms");
        let Expr::Cmp(CmpOp::Lt, lhs, rhs) = &e.expr else {
            panic!("expected comparison")
        };
        assert_eq!(lhs.expr, Expr::Var("latency".to_string()));
        assert_eq!(rhs.expr, Expr::Num(100.0, "ms".to_string()));
        assert_eq!(
            rhs.span,
            ExprSpan {
                start_line: 1,
                start_col: 11,
                end_line: 1,
                end_col: 16
            }
        );
        assert_eq!(
            e.span,
            ExprSpan {
                start_line: 1,
                start_col: 1,
                end_line: 1,
                end_col: 16
            }
        );
    }

    #[test]
    fn and_binds_tighter_than_or() {
        let e = parse_ok("a or b and c");
        let Expr::Or(lhs, rhs) = &e.expr else {
            panic!("expected Or")
        };
        assert_eq!(lhs.expr, parse_ok("a").expr);
        assert!(matches!(rhs.expr, Expr::And(_, _)));
    }

    #[test]
    fn comparison_binds_tighter_than_and() {
        let e = parse_ok("a < 10 and b > 5");
        assert!(matches!(e.expr, Expr::And(_, _)));
        let Expr::And(lhs, _) = &e.expr else { panic!() };
        assert!(matches!(lhs.expr, Expr::Cmp(CmpOp::Lt, _, _)));
    }

    #[test]
    fn additive_inside_comparison() {
        let e = parse_ok("peak + cache <= 64MB");
        let Expr::Cmp(CmpOp::Le, lhs, _) = &e.expr else {
            panic!("expected <=")
        };
        assert!(matches!(lhs.expr, Expr::Add(_, _)));
    }

    #[test]
    fn parens_override_precedence() {
        let e = parse_ok("(a or b) and c");
        let Expr::And(lhs, _) = &e.expr else { panic!() };
        assert!(matches!(lhs.expr, Expr::Or(_, _)));
    }

    #[test]
    fn equality_forms() {
        assert!(matches!(
            parse_ok("retries == 0").expr,
            Expr::Cmp(CmpOp::Eq, _, _)
        ));
        assert!(matches!(
            parse_ok("retries = 0").expr,
            Expr::Cmp(CmpOp::Eq, _, _)
        ));
        assert!(matches!(
            parse_ok("retries != 0").expr,
            Expr::Cmp(CmpOp::Ne, _, _)
        ));
    }

    #[test]
    fn negative_literal() {
        let e = parse_ok("temp > -5");
        let Expr::Cmp(_, _, rhs) = &e.expr else {
            panic!()
        };
        assert!(matches!(rhs.expr, Expr::Neg(_)));
    }

    #[test]
    fn multiline_expression_tracks_lines() {
        let e = parse_ok("a < 10 and\nb > 5");
        assert_eq!(
            e.span,
            ExprSpan {
                start_line: 1,
                start_col: 1,
                end_line: 2,
                end_col: 6
            }
        );
    }

    #[test]
    fn errors_carry_positions() {
        let err = parse_expression("a < ").unwrap_err();
        assert_eq!(err.line, 1);
        assert_eq!(err.col, 4);

        let err = parse_expression("a ! b").unwrap_err();
        assert_eq!(
            err.message,
            "expected '=' after '!' (use '!=' for inequality)"
        );

        let err = parse_expression("a < 10 extra").unwrap_err();
        assert_eq!(err.message, "trailing tokens after expression");

        let err = parse_expression("a < 10.5.5").unwrap_err();
        assert!(err.message.starts_with("unexpected character"));
    }

    #[test]
    fn not_takes_primary() {
        let e = parse_ok("not (a or b)");
        assert!(matches!(e.expr, Expr::Not(_)));
    }

    #[test]
    fn display_renders_minimal_parens() {
        assert_eq!(parse_ok("a < 10 and b > 5").to_string(), "a < 10 and b > 5");
        assert_eq!(parse_ok("(a or b) and c").to_string(), "(a or b) and c");
        assert_eq!(parse_ok("a or b and c").to_string(), "a or b and c");
        assert_eq!(parse_ok("not (a or b)").to_string(), "not (a or b)");
        assert_eq!(parse_ok("100ms").to_string(), "100ms");
        assert_eq!(
            parse_ok("peak + cache <= 64MB").to_string(),
            "peak + cache <= 64MB"
        );
    }

    #[test]
    fn unit_suffixes_are_carried() {
        let e = parse_ok("100ms");
        assert_eq!(e.expr, Expr::Num(100.0, "ms".to_string()));
        let e = parse_ok("50");
        assert_eq!(e.expr, Expr::Num(50.0, String::new()));
        let e = parse_ok("1.5s");
        assert_eq!(e.expr, Expr::Num(1.5, "s".to_string()));
    }

    // ── agreement with the language's other readers ────────────────────────
    //
    // `.spec` text is read three ways: tree-sitter's grammar (an `expr { }`
    // group becomes a `SpannedExpr` in parse.rs), `crate::lex` (the editor's
    // lexer, ADR 0023) and `tokenize` (the prove pass's reader of expressions
    // in prose). `tokenize` is not built on `lex`: see the module doc of
    // `crate::lex`. This pins the three on the repository's own spec.

    use crate::lex::{LexemeKind, lex};
    use crate::{FieldValue, parse};

    /// The `.spec` files under `dir`, recursively.
    fn spec_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                spec_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "spec") {
                out.push(path);
            }
        }
    }

    /// The byte offset of 1-based (`line`, `col`) in `text`.
    fn offset(text: &str, line: usize, col: usize) -> usize {
        let start: usize = text
            .split_inclusive('\n')
            .take(line - 1)
            .map(str::len)
            .sum();
        start + col - 1
    }

    /// Byte ranges, start to end.
    type Ranges = Vec<(usize, usize)>;

    /// The byte ranges of `tokenize`'s tokens of `text`, and of `lex`'s
    /// lexemes with the two-character operators joined: the expression
    /// language's tokens are `lex`'s, but for those operators.
    fn token_ranges(text: &str) -> (Ranges, Ranges) {
        let tokens: Vec<(usize, usize)> = tokenize(text)
            .unwrap_or_else(|e| panic!("tokenize '{text}': {e}"))
            .iter()
            .map(|t| {
                (
                    offset(text, t.start_line, t.start_col),
                    offset(text, t.end_line, t.end_col),
                )
            })
            .collect();
        let mut lexemes: Vec<(usize, usize)> = Vec::new();
        let mut previous: Option<(usize, usize, LexemeKind)> = None;
        for lexeme in lex(text) {
            let joins = |first: char| {
                previous.is_some_and(|(_, end, kind)| {
                    end == lexeme.start
                        && kind == LexemeKind::Punct(first)
                        && lexeme.kind == LexemeKind::Punct('=')
                })
            };
            if ['<', '>', '=', '!'].into_iter().any(joins) {
                let last = lexemes.last_mut().expect("the operator's first character");
                last.1 = lexeme.end;
                previous = None;
            } else {
                lexemes.push((lexeme.start, lexeme.end));
                previous = Some((lexeme.start, lexeme.end, lexeme.kind));
            }
        }
        (tokens, lexemes)
    }

    #[specforge_test_macros::test(
        behavior = "lex_spec_text",
        verify = "the expression tokenizer, the lexer and the grammar agree on every expression of the repository's spec"
    )]
    fn the_tokenizer_agrees_with_the_lexer_and_the_grammar_on_the_corpus() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut files = Vec::new();
        for dir in ["spec", "extensions", "examples", "integrations/rust/spec"] {
            spec_files(&root.join(dir), &mut files);
        }
        assert!(files.len() > 150, "found only {} spec files", files.len());
        // The forms of the language, with the corpus's few groups to spare.
        for source in [
            "a < 10 and b > 5",
            "peak_memory + cache_size <= 64MB",
            "not (retries == 0 or timeout > 30s)",
            "retries != 0 or retries = 1",
            "latency >= 1.5s",
            "temp > -5",
        ] {
            let (tokens, lexemes) = token_ranges(source);
            assert_eq!(tokens, lexemes, "'{source}'");
        }
        let (mut groups, mut lines) = (0, 0);
        for path in &files {
            let text = std::fs::read_to_string(path).unwrap();
            let file = parse(&text, &path.to_string_lossy());
            for entity in &file.entities {
                for entry in entity.fields.entries() {
                    match &entry.value {
                        // `expr { … }`: what the grammar read is what
                        // `parse_expression` reads from the same text, and
                        // `tokenize` and `lex` cut that text alike.
                        FieldValue::Expression(conjuncts) => {
                            for conjunct in conjuncts {
                                let span = conjunct.span;
                                let from = offset(&text, span.start_line, span.start_col);
                                let to = offset(&text, span.end_line, span.end_col);
                                let source = &text[from..to];
                                let at = format!("{}: '{source}'", path.display());
                                let (tokens, lexemes) = token_ranges(source);
                                assert_eq!(tokens, lexemes, "{at}");
                                let read = parse_expression(source)
                                    .unwrap_or_else(|e| panic!("{at}: {e}"));
                                // Spans are relative to the text read, so the
                                // expressions are compared as written back.
                                assert_eq!(read.to_string(), conjunct.to_string(), "{at}");
                                groups += 1;
                            }
                        }
                        // A prose field the prove pass reads line by line:
                        // every line it accepts is cut as `lex` cuts it.
                        FieldValue::String(prose) => {
                            for line in prose.lines().filter(|l| parse_expression(l).is_ok()) {
                                let (tokens, lexemes) = token_ranges(line);
                                assert_eq!(tokens, lexemes, "{}: '{line}'", path.display());
                                lines += 1;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        assert!(groups > 0, "no expression group in the corpus");
        eprintln!("checked {groups} expression conjuncts and {lines} prose lines");
    }
}
