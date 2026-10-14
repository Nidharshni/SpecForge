use specforge_common::{
    Diagnostic, DiagnosticData, Severity, SourceSpan, Sym, diagnostic_summary, render_diagnostics,
    render_plain,
};
use specforge_test::prelude::*;

fn diag_with_span(
    code: &str,
    severity: Severity,
    msg: &str,
    file: &str,
    line: usize,
    col: usize,
) -> Diagnostic {
    Diagnostic::untyped(code, severity, msg).with_span(SourceSpan {
        file: Sym::new(file),
        start_line: line,
        start_col: col,
        end_line: line,
        end_col: col + 5,
    })
}

fn diag_with_suggestion(code: &str, severity: Severity, msg: &str, suggestion: &str) -> Diagnostic {
    Diagnostic::untyped(code, severity, msg)
        .with_span(SourceSpan {
            file: Sym::new("test.spec"),
            start_line: 10,
            start_col: 4,
            end_line: 10,
            end_col: 20,
        })
        .with_suggestion(suggestion)
}

// B:print_diagnostics_structured — verify unit "error diagnostic is formatted with file:line:col"
#[specforge_test(
    behavior = "print_diagnostics_structured",
    verify = "error diagnostic is formatted with file:line:col"
)]
fn error_diagnostic_formatted_with_file_line_col() {
    let diag = diag_with_span(
        "E001",
        Severity::Error,
        "unresolved entity 'foo'",
        "src/auth.spec",
        42,
        8,
    );
    let formatted = specforge_common::format_diagnostic(&diag);
    assert!(
        formatted.contains("src/auth.spec"),
        "should include file path"
    );
    assert!(formatted.contains("42"), "should include line number");
    assert!(formatted.contains("8"), "should include column number");
    assert!(formatted.contains("E001"), "should include diagnostic code");
    assert!(formatted.contains("error"), "should include severity label");
}

// B:print_diagnostics_structured — verify unit "suggestion is displayed when available"
#[specforge_test(
    behavior = "print_diagnostics_structured",
    verify = "suggestion is displayed when available"
)]
fn suggestion_displayed_when_available() {
    let diag = diag_with_suggestion(
        "E001",
        Severity::Error,
        "unresolved entity 'behavor'",
        "did you mean 'behavior'?",
    );
    let formatted = specforge_common::format_diagnostic(&diag);
    assert!(
        formatted.contains("did you mean 'behavior'?"),
        "should display suggestion"
    );
}

// Not linked to the Print Diagnostics Structured contract: format_diagnostic
// is a plain one-line format with no colour, so it cannot prove
// color_coding_applied. print_diagnostics_contract_consistency in
// specforge-cli's tests proves the contract on `specforge check`'s output.
#[test]
fn print_diagnostics_contract() {
    // Requires: diagnostics collected (validation_complete)
    // Ensures: formatted with file path, line, column, severity
    let diag = diag_with_span(
        "E001",
        Severity::Error,
        "unresolved entity 'foo'",
        "src/core.spec",
        15,
        4,
    );
    let formatted = specforge_common::format_diagnostic(&diag);

    assert!(
        formatted.contains("src/core.spec"),
        "must include file path"
    );
    assert!(formatted.contains("15"), "must include line");
    assert!(formatted.contains("4"), "must include column");
    assert!(formatted.contains("error"), "must include severity label");
    assert!(formatted.contains("E001"), "must include diagnostic code");

    // With suggestion
    let diag2 = diag_with_suggestion("E001", Severity::Error, "unresolved", "did you mean 'bar'?");
    let formatted2 = specforge_common::format_diagnostic(&diag2);
    assert!(
        formatted2.contains("did you mean 'bar'?"),
        "must include suggestion"
    );
}

// B:present_diagnostics_as_json — verify unit "diagnostics are presented as one JSON array"
#[specforge_test(
    behavior = "present_diagnostics_as_json",
    verify = "diagnostics are presented as one JSON array"
)]
fn diagnostics_serialized_as_json_array() {
    let diags = vec![
        diag_with_span(
            "E001",
            Severity::Error,
            "unresolved entity 'foo'",
            "test.spec",
            10,
            4,
        ),
        diag_with_span(
            "W002",
            Severity::Warning,
            "unused entity 'bar'",
            "test.spec",
            20,
            0,
        ),
    ];
    let json = specforge_common::serialize_diagnostics(&diags);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    let arr = parsed.as_array().expect("should be JSON array");
    assert_eq!(arr.len(), 2);
}

// B:present_diagnostics_as_json — verify unit "each diagnostic carries code, severity, message, file, line and column"
#[specforge_test(
    behavior = "present_diagnostics_as_json",
    verify = "each diagnostic carries code, severity, message, file, line and column"
)]
fn each_diagnostic_includes_code_severity_message_file_line_column() {
    let diags = vec![diag_with_span(
        "E001",
        Severity::Error,
        "unresolved entity 'foo'",
        "src/auth.spec",
        42,
        8,
    )];
    let json = specforge_common::serialize_diagnostics(&diags);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let entry = &parsed[0];

    assert_eq!(entry["code"].as_str().unwrap(), "E001");
    assert_eq!(entry["severity"].as_str().unwrap(), "Error");
    assert_eq!(
        entry["message"].as_str().unwrap(),
        "unresolved entity 'foo'"
    );
    assert_eq!(entry["file"].as_str().unwrap(), "src/auth.spec");
    assert_eq!(entry["line"].as_u64().unwrap(), 42);
    assert_eq!(entry["column"].as_u64().unwrap(), 8);
}

// B:present_diagnostics_as_json — verify unit "suggestion is included when available"
#[specforge_test(
    behavior = "present_diagnostics_as_json",
    verify = "suggestion is included when available"
)]
fn suggestion_field_included_in_json_when_available() {
    let diags = vec![diag_with_suggestion(
        "E001",
        Severity::Error,
        "unresolved entity 'behavor'",
        "did you mean 'behavior'?",
    )];
    let json = specforge_common::serialize_diagnostics(&diags);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed[0]["suggestion"].as_str().unwrap(),
        "did you mean 'behavior'?"
    );
}

// B:present_diagnostics_as_json — verify unit "the presented JSON is valid and parseable"
#[specforge_test(
    behavior = "present_diagnostics_as_json",
    verify = "the presented JSON is valid and parseable"
)]
fn json_diagnostics_output_is_valid_json() {
    let diags = vec![
        diag_with_span("E001", Severity::Error, "bad ref", "test.spec", 1, 0),
        diag_with_suggestion("W002", Severity::Warning, "unused", "remove it"),
    ];
    let json = specforge_common::serialize_diagnostics(&diags);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("must be valid JSON");
    assert!(parsed.is_array());
}

// B:exit_code_reflects_diagnostic_severity — verify unit "exit 0 with no errors"
#[specforge_test(
    behavior = "exit_code_reflects_diagnostic_severity",
    verify = "exit 0 with no errors"
)]
fn exit_code_zero_no_errors() {
    let diags = vec![
        Diagnostic::untyped("W002", Severity::Warning, "unused entity".to_string()),
        Diagnostic::new(specforge_common::codes::I003, "note".to_string()),
    ];
    assert_eq!(specforge_common::compute_exit_code(&diags), 0);
    assert_eq!(specforge_common::compute_exit_code(&[]), 0);
}

// B:exit_code_reflects_diagnostic_severity — verify unit "exit 1 with errors"
#[specforge_test(
    behavior = "exit_code_reflects_diagnostic_severity",
    verify = "exit 1 with errors"
)]
fn exit_code_one_with_errors() {
    let diags = vec![Diagnostic::new(
        specforge_common::codes::E001,
        "unresolved".to_string(),
    )];
    assert_eq!(specforge_common::compute_exit_code(&diags), 1);
}

// B:exit_code_reflects_diagnostic_severity — verify contract "requires/ensures consistency for exit code severity mapping"
#[specforge_test(
    behavior = "exit_code_reflects_diagnostic_severity",
    verify = "Exit Code Reflects Diagnostic Severity: exit code severity mapping holds — validation_complete_fired, exit_zero_on_clean, exit_one_on_errors, strict_mode_enforced"
)]
fn exit_code_contract() {
    // Requires: diagnostics collected (validation_complete)
    // Ensures: exit 0 when no errors, exit 1 when errors present
    let no_errors = vec![Diagnostic::untyped("W001", Severity::Warning, "w")];
    let with_errors = vec![
        Diagnostic::new(specforge_common::codes::E001, "e"),
        Diagnostic::untyped("W001", Severity::Warning, "w"),
    ];
    assert_eq!(specforge_common::compute_exit_code(&no_errors), 0);
    assert_eq!(specforge_common::compute_exit_code(&with_errors), 1);
}

#[test]
fn diagnostic_includes_context_snippet() {
    // The formatted diagnostic includes file:line:col as the context locator.
    // This provides the context snippet reference for agents/tools to look up the source.
    let diag = diag_with_span(
        "E001",
        Severity::Error,
        "unresolved entity 'foo'",
        "src/auth.spec",
        42,
        8,
    );
    let formatted = specforge_common::format_diagnostic(&diag);

    // Context snippet is represented as file:line:col location reference
    assert!(
        formatted.contains("src/auth.spec:42:8"),
        "must include file:line:col context reference"
    );
    assert!(
        formatted.contains("unresolved entity 'foo'"),
        "must include the diagnostic message"
    );
}

#[test]
fn exit_code_unaffected_by_format_flag() {
    // The exit code is computed from diagnostics alone, independent of output format.
    // Whether diagnostics are serialized as JSON or formatted as text, exit code is the same.
    let diags_with_errors = vec![diag_with_span(
        "E001",
        Severity::Error,
        "bad ref",
        "test.spec",
        1,
        0,
    )];
    let diags_no_errors = vec![diag_with_span(
        "W002",
        Severity::Warning,
        "unused",
        "test.spec",
        1,
        0,
    )];

    // Serialize as JSON (simulating --format=json) — exit code unchanged
    let _json = specforge_common::serialize_diagnostics(&diags_with_errors);
    assert_eq!(specforge_common::compute_exit_code(&diags_with_errors), 1);

    let _json = specforge_common::serialize_diagnostics(&diags_no_errors);
    assert_eq!(specforge_common::compute_exit_code(&diags_no_errors), 0);

    // Format as text (default format) — exit code unchanged
    let _text = specforge_common::format_diagnostic(&diags_with_errors[0]);
    assert_eq!(specforge_common::compute_exit_code(&diags_with_errors), 1);

    let _text = specforge_common::format_diagnostic(&diags_no_errors[0]);
    assert_eq!(specforge_common::compute_exit_code(&diags_no_errors), 0);
}

// "exit 1 with warnings in strict mode" is proven by
// specforge-project's tests/policy.rs: strict is DiagnosticPolicy's
// promotion, then compute_exit_code.

// B:present_diagnostics_as_json — suggestion is null, not a string, when
// there is none (the key stays, as `check` always printed it).
#[specforge_test(behavior = "present_diagnostics_as_json")]
fn suggestion_field_null_in_json_when_none() {
    let diags = vec![diag_with_span(
        "W002",
        Severity::Warning,
        "unused",
        "test.spec",
        1,
        0,
    )];
    let json = specforge_common::serialize_diagnostics(&diags);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed[0]["suggestion"].is_null(), "{parsed}");
}

// B:present_diagnostics_as_json — verify unit "the span is nested beside the flat location, with its end positions"
#[specforge_test(
    behavior = "present_diagnostics_as_json",
    verify = "the span is nested beside the flat location, with its end positions"
)]
fn span_is_nested_beside_the_flat_location() {
    let located =
        Diagnostic::new(specforge_common::codes::E003, "unresolved").with_span(SourceSpan {
            file: "auth.spec".into(),
            start_line: 4,
            start_col: 3,
            end_line: 6,
            end_col: 2,
        });
    let unlocated = Diagnostic::new(
        specforge_common::codes::W113,
        "circular import detected: a.spec -> b.spec",
    );
    let json = specforge_common::serialize_diagnostics(&[located, unlocated]);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    // Nested, as `check` has always printed it, end positions included...
    assert_eq!(
        parsed[0]["span"],
        serde_json::json!({
            "file": "auth.spec", "start_line": 4, "start_col": 3, "end_line": 6, "end_col": 2,
        })
    );
    // ...and flat, as MCP has always printed it.
    assert_eq!(parsed[0]["file"], "auth.spec");
    assert_eq!(parsed[0]["line"], 4);
    assert_eq!(parsed[0]["column"], 3);

    // Without a location every location key is there, null.
    for key in ["span", "file", "line", "column"] {
        assert!(parsed[1][key].is_null(), "{key}: {}", parsed[1]);
    }
}

// === diagnostic truncation ===
// Not linked to export_diagnostics_as_json: `check --format json` never
// truncates. truncate_diagnostics caps `analyze`'s human output.

#[test]
fn truncate_diagnostics_limits_output() {
    let mut diags: Vec<Diagnostic> = (0..150)
        .map(|i| Diagnostic::new(specforge_common::codes::E001, format!("error {}", i)))
        .collect();

    specforge_common::truncate_diagnostics(&mut diags);

    assert_eq!(diags.len(), 101, "should be 100 + 1 summary");
    assert_eq!(diags.last().unwrap().code, "I999");
    assert!(diags.last().unwrap().message.contains("150"));
}

#[test]
fn truncate_diagnostics_no_op_under_limit() {
    let mut diags: Vec<Diagnostic> = (0..50)
        .map(|i| Diagnostic::new(specforge_common::codes::E001, format!("error {}", i)))
        .collect();

    specforge_common::truncate_diagnostics(&mut diags);

    assert_eq!(diags.len(), 50, "should not truncate under limit");
}

// === DiagnosticsExt trait ===

#[test]
fn diagnostics_ext_has_errors() {
    use specforge_common::DiagnosticsExt;

    let no_errors = vec![
        Diagnostic::untyped("W001", Severity::Warning, "warn"),
        Diagnostic::untyped("I001", Severity::Info, "info"),
    ];
    assert!(!no_errors.has_errors());
    assert_eq!(no_errors.error_count(), 0);

    let with_errors = vec![
        Diagnostic::untyped("W001", Severity::Warning, "warn"),
        Diagnostic::new(specforge_common::codes::E001, "error"),
    ];
    assert!(with_errors.has_errors());
    assert_eq!(with_errors.error_count(), 1);
}

// ============================================================================
// Spanless diagnostic fallback (L9)
// ============================================================================

// L9: spanless diagnostic uses code as fallback location
#[specforge_test(
    behavior = "print_diagnostics_structured",
    verify = "spanless diagnostic uses code as fallback location"
)]
fn spanless_diagnostic_uses_code_as_fallback() {
    let diag = Diagnostic::new(
        specforge_common::codes::W061,
        "some warning without location".to_string(),
    );
    let formatted = specforge_common::format_diagnostic(&diag);
    // Should include the diagnostic code in the location fallback
    assert!(
        formatted.contains("<W061>"),
        "spanless diagnostic should use code as fallback location, got: {}",
        formatted,
    );
    // Should NOT use the generic "<unknown>"
    assert!(
        !formatted.contains("<unknown>"),
        "should not use generic '<unknown>' fallback, got: {}",
        formatted,
    );
}

#[specforge_test(
    behavior = "print_diagnostics_structured",
    verify = "spanless error diagnostic also uses code"
)]
fn spanless_error_diagnostic_uses_code() {
    let diag = Diagnostic::new(specforge_common::codes::E001, "unresolved".to_string());
    let formatted = specforge_common::format_diagnostic(&diag);
    assert!(
        formatted.contains("<E001>"),
        "expected code-based fallback, got: {}",
        formatted,
    );
}

#[specforge_test(
    behavior = "print_diagnostics_structured",
    verify = "error diagnostic is formatted with file:line:col"
)]
fn spanned_diagnostic_ignores_code_fallback() {
    let diag = diag_with_span("E001", Severity::Error, "bad ref", "src/test.spec", 10, 5);
    let formatted = specforge_common::format_diagnostic(&diag);
    // Should use the real span, not the code fallback
    assert!(formatted.contains("src/test.spec:10:5"));
    assert!(!formatted.contains("<E001>"));
}

// B:present_diagnostics_as_json — verify unit "a typed payload is presented under data, and its absence adds no key"
#[specforge_test(
    behavior = "present_diagnostics_as_json",
    verify = "a typed payload is presented under data, and its absence adds no key"
)]
fn a_typed_payload_is_presented_under_data_and_its_absence_adds_no_key() {
    let plain = diag_with_suggestion(
        "E003",
        Severity::Error,
        "unresolved reference 'tokn' in entity 'login'",
        "did you mean 'token'?",
    );
    let typed = plain
        .clone()
        .with_data(DiagnosticData::UnresolvedReference {
            target: "tokn".into(),
            entity: "login".into(),
            field: "invariants".into(),
            did_you_mean: Some("token".into()),
        });

    // Without data the entry is byte for byte what it was before data existed.
    assert_eq!(
        specforge_common::serialize_diagnostics(std::slice::from_ref(&plain)),
        concat!(
            r#"[{"code":"E003","title":"Unresolved reference","severity":"Error","#,
            r#""message":"unresolved reference 'tokn' in entity 'login'","#,
            r#""span":{"file":"test.spec","start_line":10,"start_col":4,"end_line":10,"end_col":20},"#,
            r#""suggestion":"did you mean 'token'?","file":"test.spec","line":10,"column":4}]"#,
        )
    );

    // With it, the same entry plus `data`, tagged by kind.
    let json = specforge_common::serialize_diagnostics(&[typed]);
    let mut parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed[0]["data"],
        serde_json::json!({
            "kind": "unresolved_reference",
            "target": "tokn",
            "entity": "login",
            "field": "invariants",
            "did_you_mean": "token",
        })
    );
    parsed[0].as_object_mut().unwrap().remove("data");
    let before: serde_json::Value =
        serde_json::from_str(&specforge_common::serialize_diagnostics(&[plain])).unwrap();
    assert_eq!(parsed, before, "data is the only key it adds");
}

// The Diagnostic itself round-trips its payload, and one serialized
// before `data` existed still reads.
#[test]
fn a_diagnostic_round_trips_its_payload_and_reads_without_one() {
    let typed = Diagnostic::new(
        specforge_common::codes::E025,
        "import target not found: ./autth.spec",
    )
    .with_data(DiagnosticData::UnresolvedImport {
        path: "./autth.spec".into(),
        did_you_mean: None,
    });
    let json = serde_json::to_value(&typed).unwrap();
    assert_eq!(
        json["data"],
        serde_json::json!({"kind": "unresolved_import", "path": "./autth.spec"})
    );
    assert_eq!(serde_json::from_value::<Diagnostic>(json).unwrap(), typed);

    let old = serde_json::json!({
        "code": "W113", "severity": "Warning", "message": "m", "span": null, "suggestion": null,
    });
    let read: Diagnostic = serde_json::from_value(old.clone()).unwrap();
    assert_eq!(read.data, None);
    assert_eq!(serde_json::to_value(&read).unwrap(), old);
}

/// The payloads navigation attributes a diagnostic from: a reference
/// cycle's path and a pass diagnostic's subject are presented under
/// `data`, tagged by kind, like the others.
#[test]
fn the_entity_payloads_are_presented_under_data() {
    let cycle = Diagnostic::new(
        specforge_common::codes::W061,
        "reference cycle detected: a -> b -> a",
    )
    .with_data(DiagnosticData::ReferenceCycle {
        path: vec!["a".into(), "b".into(), "a".into()],
    });
    let subject = Diagnostic::untyped("E951", Severity::Error, "gadget fails the audit").with_data(
        DiagnosticData::Subject {
            entity: "bad_one".into(),
        },
    );
    let json = specforge_common::serialize_diagnostics(&[cycle.clone(), subject.clone()]);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed[0]["data"],
        serde_json::json!({"kind": "reference_cycle", "path": ["a", "b", "a"]})
    );
    assert_eq!(
        parsed[1]["data"],
        serde_json::json!({"kind": "subject", "entity": "bad_one"})
    );
    for diagnostic in [cycle, subject] {
        let back: Diagnostic =
            serde_json::from_str(&serde_json::to_string(&diagnostic).unwrap()).unwrap();
        assert_eq!(back, diagnostic, "the payload round-trips");
    }
}

#[specforge_test(
    type = "ReferenceCycleData",
    verify = "ReferenceCycleData schema is valid"
)]
fn reference_cycle_data_is_tagged_and_names_its_path() {
    let data = DiagnosticData::ReferenceCycle {
        path: vec!["x".into(), "y".into(), "x".into()],
    };
    assert_eq!(
        serde_json::to_value(&data).unwrap(),
        serde_json::json!({"kind": "reference_cycle", "path": ["x", "y", "x"]})
    );
    assert_eq!(data.entities(), ["x", "y"]);
}

#[specforge_test(type = "SubjectData", verify = "SubjectData schema is valid")]
fn subject_data_is_tagged_and_names_its_entity() {
    let data = DiagnosticData::Subject {
        entity: "bad_one".into(),
    };
    assert_eq!(
        serde_json::to_value(&data).unwrap(),
        serde_json::json!({"kind": "subject", "entity": "bad_one"})
    );
    assert_eq!(data.entities(), ["bad_one"]);
    let unresolved = DiagnosticData::UnresolvedReference {
        target: "t".into(),
        entity: "holder".into(),
        field: "f".into(),
        did_you_mean: None,
    };
    assert_eq!(unresolved.entities(), ["holder"]);
    let import = DiagnosticData::UnresolvedImport {
        path: "p".into(),
        did_you_mean: None,
    };
    assert!(import.entities().is_empty());
}

/// A diagnostic an extension reported names it, and the catalogue describes
/// the diagnostic only for the code's owner: a squatted core code is not
/// titled as core's, an owner's own code is, and the host's carries no
/// `origin` key.
#[specforge_test(
    behavior = "call_extension_exports",
    verify = "a diagnostic an extension reported names its extension, and a code it may not use is not described as its owner's"
)]
fn a_squatted_code_is_not_described_as_core() {
    let squatted = Diagnostic::from_extension("@acme/squat", "E001", Severity::Info, "found");
    let own = Diagnostic::from_extension("@specforge/testing", "W004", Severity::Warning, "w");
    let host = Diagnostic::new(specforge_common::codes::E003, "unresolved");
    assert_eq!(squatted.origin(), Some("@acme/squat"));
    assert_eq!(host.origin(), None);

    let json =
        serde_json::to_value(specforge_common::diagnostics_json(&[squatted, own, host])).unwrap();
    assert_eq!(json[0]["code"], "E001");
    assert_eq!(json[0]["title"], serde_json::Value::Null);
    assert_eq!(json[0]["origin"], "@acme/squat");
    assert_eq!(json[1]["title"], "Untested testable entity");
    assert_eq!(json[1]["origin"], "@specforge/testing");
    assert_eq!(json[2]["title"], "Unresolved reference");
    assert!(
        json[2].as_object().unwrap().get("origin").is_none(),
        "a host diagnostic has no origin key: {}",
        json[2]
    );
}

// === format_diagnostics_with_source_context (moved from the validator) ===

/// An unresolved reference `nonexistent` in column 39 of line 2, as the
/// graph's E003 spans it.
fn unresolved_reference_at_39() -> (
    Diagnostic,
    std::collections::HashMap<String, String>,
    &'static str,
) {
    let source = "behavior alpha \"A\" { contract \"first\" }\nfeature gamma \"G\" { behaviors [alpha, nonexistent] }\n";
    let diagnostic = Diagnostic::new(
        specforge_common::codes::E003,
        "unresolved reference 'nonexistent'",
    )
    .with_span(SourceSpan {
        file: Sym::new("main.spec"),
        start_line: 2,
        start_col: 39,
        end_line: 2,
        end_col: 50,
    });
    let sources = std::collections::HashMap::from([("main.spec".to_string(), source.to_string())]);
    (diagnostic, sources, source)
}

#[specforge_test(
    behavior = "format_diagnostics_with_source_context",
    verify = "diagnostic shows file:line:col"
)]
fn diagnostic_shows_file_line_col() {
    let (diagnostic, sources, _) = unresolved_reference_at_39();
    let output = render_diagnostics(&[diagnostic], &sources, false);

    // `nonexistent` starts in column 39 of line 2.
    assert!(output.contains("[E003]"), "{output}");
    assert!(output.contains("╭─[ main.spec:2:39 ]"), "{output}");
}

#[specforge_test(
    behavior = "format_diagnostics_with_source_context",
    verify = "context snippet highlights offending token"
)]
fn diagnostic_shows_source_context() {
    let (diagnostic, sources, _) = unresolved_reference_at_39();
    let output = render_diagnostics(&[diagnostic], &sources, false);

    // The offending line is shown, and the row under it underlines exactly
    // the columns of `nonexistent`: 39 through 49.
    let lines: Vec<&str> = output.lines().collect();
    let row = lines
        .iter()
        .position(|l| l.ends_with("feature gamma \"G\" { behaviors [alpha, nonexistent] }"))
        .unwrap_or_else(|| panic!("source line missing:\n{output}"));
    // Character columns: the margin's box-drawing characters are multi-byte.
    let code_start = lines[row][..lines[row].find("feature").unwrap()]
        .chars()
        .count();
    let token_start = code_start + "feature gamma \"G\" { behaviors [alpha, ".len();
    let underline: Vec<(usize, char)> = lines[row + 1]
        .chars()
        .enumerate()
        .filter(|(i, c)| *i >= code_start && !c.is_whitespace())
        .collect();
    let marked: Vec<usize> = underline.iter().map(|(i, _)| *i).collect();
    assert_eq!(
        marked,
        (token_start..token_start + "nonexistent".len()).collect::<Vec<_>>(),
        "{output}"
    );
    assert!(
        underline.iter().all(|(_, c)| matches!(c, '─' | '┬')),
        "{output}"
    );
}

#[specforge_test(
    behavior = "format_diagnostics_with_source_context",
    verify = "multi-line span shows full range"
)]
fn diagnostic_renders_multiline_span() {
    use std::collections::HashMap;

    let diag = Diagnostic::untyped("E099", Severity::Error, "test multi-line error".to_string())
        .with_span(SourceSpan {
            file: Sym::new("test.spec"),
            start_line: 2,
            start_col: 1,
            end_line: 4,
            end_col: 2,
        });

    let source = "line 1\nbehavior alpha \"A\" {\n  contract \"first\"\n}\nline 5\n";
    let sources: HashMap<String, String> = vec![("test.spec".to_string(), source.to_string())]
        .into_iter()
        .collect();
    let output = render_diagnostics(&[diag], &sources, false);

    // The range opens on line 2, runs through line 3 and closes on line 4's
    // brace; the lines outside it are not shown.
    assert!(output.contains("╭─[ test.spec:2:1 ]"), "{output}");
    let body: Vec<&str> = output
        .lines()
        .skip_while(|l| !l.contains("╭─▶"))
        .take_while(|l| !l.contains("├─▶"))
        .collect();
    assert!(
        body.first()
            .is_some_and(|l| l.ends_with(" 2 │ ╭─▶ behavior alpha \"A\" {")),
        "{output}"
    );
    assert!(
        body[1..].iter().all(|l| l.contains('┆')),
        "line 3 sits inside the range: {output}"
    );
    assert!(output.contains(" 4 │ ├─▶ }"), "{output}");
    assert!(!output.contains("line 1"), "{output}");
    assert!(!output.contains("line 5"), "{output}");
}

/// Render one warning spanning `start..end` (line, col) of `source` in `t.spec`.
fn render_one(
    source: &str,
    (start_line, start_col): (usize, usize),
    (end_line, end_col): (usize, usize),
) -> String {
    let diag = Diagnostic::untyped(
        "A015",
        Severity::Warning,
        "entity 'b' has unproven obligations".to_string(),
    )
    .with_span(SourceSpan {
        file: Sym::new("t.spec"),
        start_line,
        start_col,
        end_line,
        end_col,
    })
    .with_suggestion("link a test".to_string());
    let sources = std::collections::HashMap::from([("t.spec".to_string(), source.to_string())]);
    render_diagnostics(&[diag], &sources, false)
}

#[specforge_test(
    behavior = "format_diagnostics_with_source_context",
    verify = "a span after a multi-byte character keeps its position"
)]
fn diagnostic_after_multibyte_character_keeps_its_position() {
    // `…` and `é` are one character but three and two bytes.
    let source = "a \"x … é\" {\n}\n\nb \"y\" {\n  z\n}\n\nc\n";
    let output = render_one(source, (4, 1), (6, 2));
    assert!(output.contains("t.spec:4:1 ]"), "{output}");
    assert!(
        output.contains("6 │ ├─▶ }"),
        "the range ends on b's brace: {output}"
    );
    assert!(!output.contains(" 7 │"), "{output}");
}

#[specforge_test(
    behavior = "format_diagnostics_with_source_context",
    verify = "the message appears once, in the heading"
)]
fn diagnostic_message_appears_once() {
    let source = "a {\n}\n\nb \"y\"\n";
    for (start, end) in [((4, 1), (4, 6)), ((1, 1), (2, 2))] {
        let output = render_one(source, start, end);
        assert_eq!(
            output.matches("has unproven obligations").count(),
            1,
            "{output}"
        );
        assert!(
            output.starts_with("[A015] Warning: entity 'b' has unproven obligations\n"),
            "{output}"
        );
    }
}

#[specforge_test(
    behavior = "format_diagnostics_with_source_context",
    verify = "no rendered line ends in whitespace"
)]
fn diagnostic_lines_have_no_trailing_whitespace() {
    let source = "a {\n}\n\nb \"y\"\n";
    for (start, end) in [((4, 1), (4, 6)), ((1, 1), (2, 2))] {
        let output = render_one(source, start, end);
        for line in output.lines() {
            assert_eq!(line, line.trim_end(), "trailing whitespace in:\n{output}");
        }
    }
}

#[specforge_test(
    behavior = "format_diagnostics_with_source_context",
    verify = "consecutive diagnostics are separated by a blank line"
)]
fn diagnostics_are_separated_by_a_blank_line() {
    let one = render_one("a\n", (1, 1), (1, 2));
    let diag = |code: &str| {
        Diagnostic::untyped(code, Severity::Warning, "m").with_span(SourceSpan {
            file: Sym::new("t.spec"),
            start_line: 1,
            start_col: 1,
            end_line: 1,
            end_col: 2,
        })
    };
    let sources = std::collections::HashMap::from([("t.spec".to_string(), "a\n".to_string())]);
    let output = render_diagnostics(&[diag("A001"), diag("A002")], &sources, false);
    assert!(output.contains("╯\n\n[A002]"), "{output}");
    assert!(
        output.ends_with("╯\n"),
        "no blank line after the last: {output}"
    );
    assert!(one.ends_with("╯\n"), "{one}");
}

#[specforge_test(
    behavior = "format_diagnostics_with_source_context",
    verify = "Format Diagnostics with Source Context: diagnostic source context formatting holds — valid_source_span, header_present, context_snippet_present, caret_marker_present"
)]
fn diagnostic_format_contract_consistency() {
    use std::collections::HashMap;

    // Requires: valid SourceSpan referencing accessible source
    // Ensures: output includes file:line:col header, context snippet, caret marker
    let diag = Diagnostic::new(
        specforge_common::codes::E001,
        "unresolved reference".to_string(),
    )
    .with_span(SourceSpan {
        file: Sym::new("test.spec"),
        start_line: 2,
        start_col: 20,
        end_line: 2,
        end_col: 31,
    });

    let source = "line 1\nfeature gamma \"G\" { behaviors [nonexistent] }\nline 3\n";
    let sources: HashMap<String, String> = vec![("test.spec".to_string(), source.to_string())]
        .into_iter()
        .collect();
    let output = render_diagnostics(&[diag], &sources, false);
    let lines: Vec<&str> = output.lines().collect();

    // header_present: the code, then file:line:col.
    assert_eq!(lines[0], "[E001] Error: unresolved reference", "{output}");
    assert_eq!(lines[1].trim(), "╭─[ test.spec:2:20 ]", "{output}");

    // context_snippet_present: the offending line, and only it.
    let row = lines
        .iter()
        .position(|l| l.ends_with(" 2 │ feature gamma \"G\" { behaviors [nonexistent] }"))
        .unwrap_or_else(|| panic!("{output}"));
    assert!(
        !output.contains("line 1") && !output.contains("line 3"),
        "{output}"
    );

    // caret_marker_present: the row below marks columns 20 through 30.
    let code_start = lines[row][..lines[row].find("feature").unwrap()]
        .chars()
        .count();
    let marked: Vec<usize> = lines[row + 1]
        .chars()
        .enumerate()
        .filter(|(i, c)| *i >= code_start && !c.is_whitespace())
        .map(|(i, _)| i - code_start + 1)
        .collect();
    assert_eq!(marked, (20..31).collect::<Vec<_>>(), "{output}");
}

// === aggregate_diagnostic_summary (moved from the validator) ===

#[specforge_test(
    behavior = "aggregate_diagnostic_summary",
    verify = "summary shows correct counts"
)]
fn summary_shows_correct_counts() {
    let diagnostics = vec![
        Diagnostic::new(specforge_common::codes::E001, "error 1".to_string()),
        Diagnostic::new(specforge_common::codes::W012, "warning 1".to_string()),
        Diagnostic::new(specforge_common::codes::E002, "error 2".to_string()),
        Diagnostic::new(specforge_common::codes::I004, "info 1".to_string()),
    ];

    let summary = diagnostic_summary(&diagnostics, true);

    assert!(
        summary.contains("2 error"),
        "should show 2 errors: got '{}'",
        summary
    );
    assert!(
        summary.contains("1 warning"),
        "should show 1 warning: got '{}'",
        summary
    );
    assert!(
        summary.contains("1 info"),
        "should show 1 info: got '{}'",
        summary
    );
}

#[specforge_test(
    behavior = "aggregate_diagnostic_summary",
    verify = "summary matches actual diagnostics"
)]
fn summary_clean_project() {
    // A clean project reports zero of everything, uncoloured, with no
    // codes listed.
    assert_eq!(
        diagnostic_summary(&[], true),
        "0 errors, 0 warnings, 0 infos"
    );

    // Planted: two unresolved references and one orphan ref.
    let diagnostics = vec![
        Diagnostic::new(specforge_common::codes::E003, "ghost".to_string()),
        Diagnostic::new(specforge_common::codes::E003, "phantom".to_string()),
        Diagnostic::new(specforge_common::codes::W012, "orphan".to_string()),
    ];
    assert_eq!(
        diagnostic_summary(&diagnostics, true)
            .lines()
            .next()
            .unwrap(),
        "\x1b[1;31m2 errors, 1 warning, 0 infos\x1b[0m"
    );
    assert_eq!(
        diagnostic_summary(&diagnostics, false)
            .lines()
            .next()
            .unwrap(),
        "2 errors, 1 warning, 0 infos"
    );
}

#[specforge_test(
    behavior = "aggregate_diagnostic_summary",
    verify = "summary is red when errors exist"
)]
fn summary_red_when_errors_exist() {
    let diagnostics = vec![Diagnostic::new(
        specforge_common::codes::E001,
        "test error".to_string(),
    )];
    let summary = diagnostic_summary(&diagnostics, true);

    // ANSI red escape: \x1b[31m
    assert!(
        summary.contains("\x1b[31m") || summary.contains("\x1b[1;31m"),
        "summary with errors should contain red ANSI escape, got: {:?}",
        summary
    );
}

#[specforge_test(
    behavior = "aggregate_diagnostic_summary",
    verify = "Aggregate Diagnostic Summary: diagnostic summary aggregation holds — validation_executed, counts_match"
)]
fn summary_contract_consistency() {
    // Requires: validation has completed
    // Ensures: counts match actual diagnostics exactly
    let diagnostics = vec![
        Diagnostic::new(specforge_common::codes::E001, "e".to_string()),
        Diagnostic::new(specforge_common::codes::E002, "e".to_string()),
        Diagnostic::new(specforge_common::codes::E003, "e".to_string()),
        Diagnostic::new(specforge_common::codes::W012, "w".to_string()),
    ];
    let summary = diagnostic_summary(&diagnostics, true);

    assert!(
        summary.contains("3 error"),
        "must report exact error count: got '{}'",
        summary
    );
    assert!(
        summary.contains("1 warning"),
        "must report exact warning count: got '{}'",
        summary
    );
    assert!(
        summary.contains("0 info"),
        "must report exact info count: got '{}'",
        summary
    );
}

#[specforge_test(
    behavior = "format_diagnostics_with_source_context",
    verify = "Format Diagnostics with Source Context: diagnostic source context formatting holds — valid_source_span, header_present, context_snippet_present, caret_marker_present"
)]
fn only_a_diagnostic_with_a_span_gets_a_snippet() {
    let source = "line 1\nfeature gamma \"G\" {}\n";
    let sources = std::collections::HashMap::from([("t.spec".to_string(), source.to_string())]);
    let spanned =
        Diagnostic::untyped("E003", Severity::Error, "unresolved").with_span(SourceSpan {
            file: Sym::new("t.spec"),
            start_line: 2,
            start_col: 1,
            end_line: 2,
            end_col: 8,
        });
    let spanless =
        Diagnostic::untyped("W061", Severity::Warning, "a cycle").with_suggestion("break it");

    let out = render_diagnostics(&[spanned, spanless.clone()], &sources, false);

    // The spanned one is a snippet; the spanless one is its plain lines,
    // after a blank line, naming no file.
    let (snippet, plain) = out
        .split_once("\n\n")
        .unwrap_or_else(|| panic!("a blank line between: {out}"));
    assert!(snippet.contains("[E003] Error: unresolved"), "{out}");
    assert!(snippet.contains("╭─[ t.spec:2:1 ]"), "{out}");
    assert_eq!(plain, "warning[W061]: a cycle\n  = help: break it\n");
    assert_eq!(
        render_plain(&spanless),
        "warning[W061]: a cycle\n  = help: break it"
    );
}
