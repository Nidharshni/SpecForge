use specforge_parser::{Expr, FieldValue, parse};

fn metric_field(source: &str) -> FieldValue {
    let result = parse(source, "test.spec");
    assert!(
        result.errors.is_empty(),
        "unexpected parse errors: {:?}",
        result.errors
    );
    let entity = result.entities.first().expect("entity");
    entity.fields.get("metric").expect("metric field").clone()
}

#[test]
fn expr_group_parses_to_typed_ast() {
    let source = r#"
constraint latency_budget "Latency Budget" {
    description "Budget"
    metric expr {
        file_change_to_diagnostics < 100ms
        peak_memory + cache_size <= 64MB
        not (retries == 0 or timeout > 30s)
    }
}
"#;
    let FieldValue::Expression(conjuncts) = metric_field(source) else {
        panic!("expected FieldValue::Expression");
    };
    assert_eq!(conjuncts.len(), 3);

    // line 1: file_change_to_diagnostics < 100ms
    let Expr::Cmp(op, lhs, rhs) = &conjuncts[0].expr else {
        panic!("expected comparison");
    };
    assert_eq!(*op, specforge_parser::CmpOp::Lt);
    assert_eq!(
        lhs.expr,
        Expr::Var("file_change_to_diagnostics".to_string())
    );
    assert_eq!(rhs.expr, Expr::Num(100.0, "ms".to_string()));

    // line 2: peak_memory + cache_size <= 64MB
    let Expr::Cmp(op, lhs, _) = &conjuncts[1].expr else {
        panic!("expected comparison");
    };
    assert_eq!(*op, specforge_parser::CmpOp::Le);
    assert!(matches!(lhs.expr, Expr::Add(_, _)));

    // line 3: not (retries == 0 or timeout > 30s)
    let Expr::Not(inner) = &conjuncts[2].expr else {
        panic!("expected negation");
    };
    assert!(matches!(inner.expr, Expr::Or(_, _)));
}

#[test]
fn expr_group_spans_are_absolute_file_positions() {
    let source = r#"
constraint c "C" {
    metric expr {
        latency < 100ms
    }
}
"#;
    let FieldValue::Expression(conjuncts) = metric_field(source) else {
        panic!("expected FieldValue::Expression");
    };
    let span = &conjuncts[0].span;
    // `latency` starts on source line 4, col 9 (1-based)
    assert_eq!(span.start_line, 4);
    assert_eq!(span.start_col, 9);
    assert_eq!(span.end_line, 4);
    assert_eq!(span.end_col, 24);
}

#[test]
fn string_metric_form_still_yields_string() {
    let source = r#"
constraint c "C" {
    metric """
        latency < 100ms
    """
}
"#;
    assert!(matches!(metric_field(source), FieldValue::String(_)));
}

#[test]
fn malformed_expr_group_produces_parse_error() {
    let result = parse(
        r#"
constraint c "C" {
    metric expr {
        latency <
    }
}
"#,
        "bad.spec",
    );
    assert!(
        !result.errors.is_empty(),
        "missing operand must surface a parse error"
    );
}

#[test]
fn corpus_parses_clean_end_to_end() {
    // Grammar regression net: EVERY corpus file must parse with zero
    // errors. The historical baselines (port `method` signatures, union
    // types with `|`) are fully supported since the method and type_union
    // grammar slices - the tolerated-error era is over.
    let root = specforge_repo_root::repo_root().join("spec");

    let mut files = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read_dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().map(|e| e == "spec").unwrap_or(false) {
                files.push(path);
            }
        }
    }
    assert!(
        files.len() > 150,
        "corpus unexpectedly small: {}",
        files.len()
    );

    let mut unexpected = Vec::new();
    for path in &files {
        let source = std::fs::read_to_string(path).expect("read spec");
        let result = parse(&source, &path.to_string_lossy());
        if result.errors.is_empty() {
            continue;
        }
        if !result.errors.is_empty() {
            let name = path.file_name().unwrap().to_string_lossy();
            unexpected.push(format!("{}: {:?}", name, result.errors.first()));
        }
    }
    assert!(
        unexpected.is_empty(),
        "new parse errors outside the documented baseline: {unexpected:?}"
    );
}
