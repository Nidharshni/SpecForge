use super::ExprError;

// ── tokenizer ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Tok {
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
pub(super) struct SpannedTok {
    pub(super) tok: Tok,
    pub(super) start_line: usize,
    pub(super) start_col: usize,
    pub(super) end_line: usize,
    pub(super) end_col: usize,
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_lowercase() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit()
}

pub(super) fn tokenize(src: &str) -> Result<Vec<SpannedTok>, ExprError> {
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
