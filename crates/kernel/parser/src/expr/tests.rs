use super::tokenizer::tokenize;
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
    let root = specforge_repo_root::repo_root();
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
                            let read =
                                parse_expression(source).unwrap_or_else(|e| panic!("{at}: {e}"));
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
