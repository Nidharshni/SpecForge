use super::tokenizer::{SpannedTok, Tok, tokenize};
use super::{CmpOp, Expr, ExprError, ExprSpan, SpannedExpr};

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
