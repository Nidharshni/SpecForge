use super::*;

impl<'a> ParseContext<'a> {
    pub(super) fn parse_expr_group(&self, node: Node<'a>) -> Vec<SpannedExpr> {
        let mut cursor = node.walk();
        node.children(&mut cursor)
            .filter(|c| c.kind() == kind::EXPR_OR)
            .map(|c| self.convert_expr(c))
            .collect()
    }

    fn expr_span(&self, node: Node<'a>) -> ExprSpan {
        let span = self.span(node);
        ExprSpan {
            start_line: span.start_line,
            start_col: span.start_col,
            end_line: span.end_line,
            end_col: span.end_col,
        }
    }

    /// Map an expression CST node onto the typed [`SpannedExpr`] AST.
    /// The grammar guarantees node shapes, so degenerate arms only occur
    /// under ERROR recovery and yield neutral placeholders.
    fn convert_expr(&self, node: Node<'a>) -> SpannedExpr {
        let span = self.expr_span(node);
        match node.kind() {
            kind::EXPR_OR | kind::EXPR_AND => {
                let is_or = node.kind() == kind::EXPR_OR;
                let mut cursor = node.walk();
                let mut parts = node
                    .children(&mut cursor)
                    .filter(|c| c.is_named())
                    .map(|c| self.convert_expr(c));
                let mut acc = parts.next().unwrap_or_else(|| fallback_var(span));
                for rhs in parts {
                    let joined = join_span(&acc.span, &rhs.span);
                    let expr = if is_or {
                        Expr::Or(Box::new(acc), Box::new(rhs))
                    } else {
                        Expr::And(Box::new(acc), Box::new(rhs))
                    };
                    acc = SpannedExpr { expr, span: joined };
                }
                acc
            }
            kind::EXPR_CMP => {
                let mut lhs: Option<SpannedExpr> = None;
                let mut op: Option<CmpOp> = None;
                let mut rhs: Option<SpannedExpr> = None;
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    match child.kind() {
                        kind::EXPR_ADD => {
                            if lhs.is_none() {
                                lhs = Some(self.convert_expr(child));
                            } else {
                                rhs = Some(self.convert_expr(child));
                            }
                        }
                        "<" => op = Some(CmpOp::Lt),
                        "<=" => op = Some(CmpOp::Le),
                        ">" => op = Some(CmpOp::Gt),
                        ">=" => op = Some(CmpOp::Ge),
                        "==" => op = Some(CmpOp::Eq),
                        "!=" => op = Some(CmpOp::Ne),
                        _ => {}
                    }
                }
                match (lhs, op, rhs) {
                    (Some(l), Some(op), Some(r)) => SpannedExpr {
                        expr: Expr::Cmp(op, Box::new(l), Box::new(r)),
                        span,
                    },
                    (Some(l), _, _) => l,
                    _ => fallback_var(span),
                }
            }
            kind::EXPR_ADD => {
                let mut acc: Option<SpannedExpr> = None;
                let mut pending_sub = false;
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    match child.kind() {
                        kind::EXPR_ATOM => {
                            let operand = self.convert_expr(child);
                            match acc.take() {
                                None => acc = Some(operand),
                                Some(lhs) => {
                                    let joined = join_span(&lhs.span, &operand.span);
                                    let expr = if pending_sub {
                                        Expr::Sub(Box::new(lhs), Box::new(operand))
                                    } else {
                                        Expr::Add(Box::new(lhs), Box::new(operand))
                                    };
                                    acc = Some(SpannedExpr { expr, span: joined });
                                }
                            }
                        }
                        "-" => pending_sub = true,
                        "+" => pending_sub = false,
                        _ => {}
                    }
                }
                acc.unwrap_or_else(|| fallback_var(span))
            }
            kind::EXPR_ATOM => {
                let mut cursor = node.walk();
                let children: Vec<Node<'a>> = node.children(&mut cursor).collect();
                let prefix = children.iter().find_map(|c| match c.kind() {
                    "-" | "not" => Some(c.kind()),
                    _ => None,
                });
                let operand = children.iter().find(|c| {
                    matches!(
                        c.kind(),
                        kind::NUMBER_WITH_UNIT | kind::IDENTIFIER | kind::EXPR_OR | kind::EXPR_ATOM
                    )
                });
                let inner = match operand {
                    Some(c) if c.kind() == kind::NUMBER_WITH_UNIT => {
                        let text = self.text(*c);
                        let unit_start = text.trim_end_matches(char::is_alphabetic).len();
                        let value = text[..unit_start].parse::<f64>().unwrap_or(0.0);
                        SpannedExpr {
                            expr: Expr::Num(value, text[unit_start..].to_string()),
                            span,
                        }
                    }
                    Some(c) if c.kind() == kind::IDENTIFIER => SpannedExpr {
                        expr: Expr::Var(self.text(*c).to_string()),
                        span,
                    },
                    // parenthesized group: keep the group's value, span covers parens
                    Some(c) => {
                        let converted = self.convert_expr(*c);
                        SpannedExpr {
                            expr: converted.expr,
                            span,
                        }
                    }
                    None => fallback_var(span),
                };
                match prefix {
                    Some("not") => SpannedExpr {
                        expr: Expr::Not(Box::new(inner)),
                        span,
                    },
                    Some(_) => SpannedExpr {
                        expr: Expr::Neg(Box::new(inner)),
                        span,
                    },
                    None => inner,
                }
            }
            _ => fallback_var(span),
        }
    }
}

fn join_span(a: &ExprSpan, b: &ExprSpan) -> ExprSpan {
    ExprSpan {
        start_line: a.start_line.min(b.start_line),
        start_col: a.start_col.min(b.start_col),
        end_line: a.end_line.max(b.end_line),
        end_col: a.end_col.max(b.end_col),
    }
}

fn fallback_var(span: ExprSpan) -> SpannedExpr {
    SpannedExpr {
        expr: Expr::Var(String::new()),
        span,
    }
}
