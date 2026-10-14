use crate::support::{Cli, TestProject};
use specforge_project::testing::specs;
use specforge_test_macros::test as specforge_test;
use std::fs;

// === check_mode_for_ci (self-check) ===

#[specforge_test(
    behavior = "check_mode_for_ci",
    verify = "check mode works in CI environment"
)]
fn self_check_runs_without_crashing() {
    // Find the project's spec/ directory relative to the crate manifest
    let spec_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap() // crates/
        .parent()
        .unwrap() // project root
        .join("spec");

    if !spec_dir.exists() {
        panic!("spec/ directory not found at {:?}", spec_dir);
    }

    // As a CI runner invokes it: CI set, no terminal, empty stdin.
    let host = TestProject::new().without_config();
    let ci_check = |args: &[&str], path: &std::path::Path| {
        let mut argv = vec!["check"];
        argv.extend(args);
        argv.extend(["--path", path.to_str().unwrap()]);
        Cli::on(&host)
            .env("CI", "true")
            .env("TERM", "dumb")
            .stdin("")
            .bare_command(&argv)
            .output()
            .unwrap()
    };

    // The repository's own specs, which CI checks: exit 0, the summary on
    // stderr and nothing on stdout. `spec/` is in the repository's project,
    // so it is that project that is checked, extensions and all.
    let output = ci_check(&[], &spec_dir);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stderr}");
    assert!(output.stdout.is_empty());
    assert!(stderr.contains("0 errors, "), "{stderr}");
    assert!(!stderr.contains('\x1b'), "no terminal escapes: {stderr:?}");

    let output = ci_check(&["--format=json"], &spec_dir);
    assert_eq!(output.status.code(), Some(0));
    let diagnostics: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let codes: Vec<&str> = diagnostics
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["code"].as_str().unwrap())
        .collect();
    assert!(!codes.contains(&"I002"), "the project's extensions loaded");
    assert!(
        diagnostics
            .as_array()
            .unwrap()
            .iter()
            .all(|d| d["severity"] != "Error"),
        "{diagnostics}"
    );

    // A broken project fails the CI step with exit 1 and says why on stderr.
    let dir = TestProject::new().without_config().files(&[(
        "main.spec",
        "\nbehavior alpha \"A\" { contract \"first\" }\nfeature gamma \"G\" { behaviors [alpha, nonexistent] }\n",
    )]);
    let output = ci_check(&[], dir.root());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(output.stdout.is_empty());
    assert!(
        stderr.contains("[E003] Error: unresolved reference 'nonexistent' in entity 'gamma'"),
        "{stderr}"
    );
}

// === graceful_degradation_without_extensions ===

#[specforge_test(
    behavior = "graceful_degradation_without_extensions",
    verify = "specforge check with zero extensions exits cleanly with I002"
)]
fn check_without_extensions_exits_cleanly_with_i002() {
    let dir = TestProject::new().without_config().files(&[
        (
            "specforge.json",
            r#"{ "name": "p", "version": "0.1.0", "extensions": [] }"#,
        ),
        (
            "main.spec",
            "thing alpha \"A\" {\n  refs [beta]\n}\n\nthing beta \"B\" {\n}\n",
        ),
    ]);

    let output = Cli::on(&dir)
        .command(&["check", "--format=json"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let diagnostics: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let diagnostics = diagnostics.as_array().unwrap();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0]["code"], "I002");
    assert_eq!(diagnostics[0]["severity"], "Info");
}

#[specforge_test(
    behavior = "handle_all_extensions_failed_to_load",
    verify = "specforge check with all extensions unavailable exits cleanly"
)]
fn check_with_every_extension_unavailable_exits_cleanly() {
    let dir = TestProject::new().without_config().files(&[
        (
            "specforge.json",
            r#"{ "name": "p", "version": "0.1.0", "extensions": ["@specforge/no-such-a", "@specforge/no-such-b"] }"#,
        ),
        ("main.spec", "thing alpha \"A\" {\n}\n"),
    ]);

    let output = Cli::on(&dir)
        .command(&["check", "--format=json"])
        .output()
        .unwrap();

    // The load errors fail the check (exit 1, not a crash), every
    // diagnostic is reported, and the compile went on structurally.
    assert_eq!(output.status.code(), Some(1));
    let diagnostics: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let codes: Vec<&str> = diagnostics
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["code"].as_str().unwrap())
        .collect();
    assert_eq!(codes, ["E028", "E028", "I002"]);
}

// === registry_build_peer_dependencies ===

#[specforge_test(
    behavior = "registry_build_peer_dependencies",
    verify = "specforge check reports a missing required peer dependency"
)]
fn check_reports_a_missing_required_peer() {
    // @specforge/cargo-test requires @specforge/testing.
    let dir = TestProject::new().without_config().files(&[
        (
            "specforge.json",
            r#"{ "name": "p", "version": "0.1.0", "extensions": ["@specforge/cargo-test"] }"#,
        ),
        ("main.spec", "spec \"p\" {\n}\n"),
    ]);

    let output = Cli::on(&dir)
        .command(&["check", "--format=json"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let diagnostics: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let e027: Vec<&serde_json::Value> = diagnostics
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["code"] == "E027")
        .collect();
    assert_eq!(e027.len(), 1, "{diagnostics}");
    assert_eq!(e027[0]["severity"], "Error");
    assert_eq!(
        e027[0]["suggestion"],
        "install it with: specforge add @specforge/testing"
    );
}

// === check_mode_for_ci ===

#[specforge_test(
    behavior = "exit_code_reflects_diagnostic_severity",
    verify = "exit 0 with no errors"
)]
fn check_clean_project_exits_zero() {
    let dir = TestProject::new()
        .without_config()
        .files(&[("main.spec", r#"behavior alpha "A" { contract "first" }"#)]);

    Cli::on(&dir).command(&["check"]).assert().success();
}

// === exit_code_reflects_diagnostic_severity ===

#[specforge_test(
    behavior = "exit_code_reflects_diagnostic_severity",
    verify = "exit 1 with errors"
)]
fn check_project_with_errors_exits_one() {
    let dir = TestProject::new().without_config().files(&[(
        "main.spec",
        r#"
behavior alpha "A" { contract "first" }
feature gamma "G" { behaviors [alpha, nonexistent] }
"#,
    )]);

    Cli::on(&dir).command(&["check"]).assert().code(1);
}

#[specforge_test(
    behavior = "exit_code_reflects_diagnostic_severity",
    verify = "exit 1 with warnings in strict mode"
)]
fn strict_mode_promotes_warnings_to_errors() {
    // Orphan ref produces W012 (warning) — with --strict it becomes an error → exit 1
    let dir = TestProject::new().without_config().files(&[(
        "main.spec",
        r#"
behavior alpha "A" { contract "first" }
ref gh.issue:42 "Orphan ref"
"#,
    )]);

    // Without --strict: exit 0 (only warnings)
    Cli::on(&dir).command(&["check"]).assert().success();

    // With --strict: exit 1 (warnings promoted to errors)
    Cli::on(&dir)
        .command(&["check", "--strict"])
        .assert()
        .code(1);
}

// === print_diagnostics_structured ===

#[specforge_test(
    behavior = "export_diagnostics_as_json",
    verify = "JSON output is valid and parseable"
)]
fn json_format_outputs_valid_json() {
    let dir = TestProject::new().without_config().files(&[(
        "main.spec",
        r#"
behavior alpha "A" { contract "first" }
feature gamma "G" { behaviors [alpha, nonexistent] }
"#,
    )]);

    let output = Cli::on(&dir)
        .command(&["check", "--format=json"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("invalid JSON: {}\noutput: {}", e, stdout));

    assert!(
        parsed.is_array(),
        "JSON output should be an array of diagnostics"
    );
    let arr = parsed.as_array().unwrap();
    assert!(!arr.is_empty(), "should have at least one diagnostic");

    let first = &arr[0];
    assert!(
        first.get("code").is_some(),
        "diagnostic should have 'code' field"
    );
    assert!(
        first.get("severity").is_some(),
        "diagnostic should have 'severity' field"
    );
    assert!(
        first.get("message").is_some(),
        "diagnostic should have 'message' field"
    );
}

// === print_diagnostics_structured ===

#[specforge_test(
    behavior = "print_diagnostics_structured",
    verify = "error diagnostic is formatted with file:line:col"
)]
fn structured_output_includes_file_line_col() {
    let dir = TestProject::new().without_config().files(&[(
        "main.spec",
        r#"
behavior alpha "A" { contract "first" }
feature gamma "G" { behaviors [alpha, nonexistent] }
"#,
    )]);

    let output = Cli::on(&dir).command(&["check"]).output().unwrap();

    // `nonexistent` starts at line 3, column 39.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("main.spec:3:39"),
        "should contain file:line:col in stderr: {}",
        stderr
    );
}

#[specforge_test(
    behavior = "print_diagnostics_structured",
    verify = "diagnostic includes context snippet"
)]
fn structured_output_includes_context_snippet() {
    let dir = TestProject::new().without_config().files(&[(
        "main.spec",
        r#"
behavior alpha "A" { contract "first" }
feature gamma "G" { behaviors [alpha, nonexistent] }
"#,
    )]);

    let output = Cli::on(&dir).command(&["check"]).output().unwrap();

    // The offending source line, numbered, with the reference underlined.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(" 3 │ feature gamma \"G\" { behaviors [alpha, nonexistent] }\n"),
        "should contain the source line in stderr: {}",
        stderr
    );
    let lines: Vec<&str> = stderr.lines().collect();
    let at = lines
        .iter()
        .position(|l| l.contains("feature gamma"))
        .unwrap();
    let column_of = |line: &str, byte: usize| line[..byte].chars().count();
    let source = lines[at];
    let underline = lines[at + 1];
    assert_eq!(
        column_of(underline, underline.find('─').unwrap()),
        column_of(source, source.find("nonexistent").unwrap()),
        "the underline starts under `nonexistent`: {stderr}"
    );
    assert_eq!(
        underline.trim_start_matches([' ', '│']),
        "─────┬─────",
        "and spans its 11 characters: {stderr}"
    );
}

#[specforge_test(
    behavior = "print_diagnostics_structured",
    verify = "suggestion is displayed when available"
)]
fn structured_output_includes_suggestion() {
    let dir = TestProject::new().without_config().files(&[(
        "main.spec",
        r#"
behavior alpha_parser "A" { contract "first" }
feature gamma "G" { behaviors [alpha_parsr] }
"#,
    )]);

    let output = Cli::on(&dir).command(&["check"]).output().unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("did you mean") && stderr.contains("alpha_parser"),
        "should display did-you-mean suggestion in stderr: {}",
        stderr
    );
}

// === check_mode_for_ci ===

#[specforge_test(
    behavior = "check_mode_for_ci",
    verify = "check mode produces no output files"
)]
fn check_mode_produces_no_output_files() {
    let dir = TestProject::new()
        .without_config()
        .files(&[("main.spec", r#"behavior alpha "A" { contract "first" }"#)]);

    let before: Vec<_> = fs::read_dir(dir.root())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name())
        .collect();

    Cli::on(&dir).command(&["check"]).assert().success();

    let after: Vec<_> = fs::read_dir(dir.root())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name())
        .collect();

    assert_eq!(before, after, "check mode should not produce any new files");
}

#[specforge_test(
    behavior = "check_mode_for_ci",
    verify = "check mode prints diagnostics to stderr"
)]
fn check_mode_prints_to_stderr() {
    let dir = TestProject::new().without_config().files(&[(
        "main.spec",
        r#"
behavior alpha "A" { contract "first" }
feature gamma "G" { behaviors [alpha, nonexistent] }
"#,
    )]);

    let output = Cli::on(&dir).command(&["check"]).output().unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("E001") || stderr.contains("error"),
        "diagnostics should be printed to stderr: {}",
        stderr
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.is_empty(),
        "human-format check should not produce stdout output, got: {}",
        stdout
    );
}

// === export_diagnostics_as_json ===

#[specforge_test(
    behavior = "export_diagnostics_as_json",
    verify = "each diagnostic includes code, severity, message, file, line, column"
)]
fn json_diagnostics_have_complete_fields() {
    let dir = TestProject::new().without_config().files(&[(
        "main.spec",
        r#"
behavior alpha "A" { contract "first" }
feature gamma "G" { behaviors [alpha, nonexistent] }
"#,
    )]);

    let output = Cli::on(&dir)
        .command(&["check", "--format=json"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let diagnostics: Vec<serde_json::Value> = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("invalid JSON: {}\noutput: {}", e, stdout));

    for diag in &diagnostics {
        assert!(diag.get("code").is_some(), "missing 'code': {:?}", diag);
        assert!(
            diag.get("severity").is_some(),
            "missing 'severity': {:?}",
            diag
        );
        assert!(
            diag.get("message").is_some(),
            "missing 'message': {:?}",
            diag
        );
        // A project-wide diagnostic (I002 here: no extension) has no span.
        if let Some(span) = diag.get("span").filter(|s| !s.is_null()) {
            assert!(
                span.get("file").is_some(),
                "span missing 'file': {:?}",
                span
            );
            assert!(
                span.get("start_line").is_some(),
                "span missing 'start_line': {:?}",
                span
            );
            assert!(
                span.get("start_col").is_some(),
                "span missing 'start_col': {:?}",
                span
            );
        }
    }
}

// === contract tests ===

/// A project whose check reports exactly one error (E003), one warning
/// (W012) and one info (I004: `features` targets a kind only the absent
/// product extension provides).
fn one_of_each_severity() -> TestProject {
    TestProject::new().without_config().files(&[
        (
            "specforge.json",
            r#"{"name":"t","version":"0.1.0","spec_root":"spec","extensions":["@specforge/software"]}"#,
        ),
        (
            "spec/main.spec",
            "behavior alpha \"A\" {\n  category command\n  contract \"first\"\n  features [user_authentication]\n  invariants [nonexistent]\n}\nref gh.issue:42 \"Orphan ref\"\n",
        ),
    ])
}

/// `specforge check` on `dir` with the colour variables set as given (the
/// others removed); stderr is a pipe, never a terminal.
fn check_stderr(dir: &TestProject, no_color: Option<&str>, force: Option<&str>) -> String {
    let mut cli = Cli::on(dir);
    if let Some(v) = no_color {
        cli = cli.env("NO_COLOR", v);
    }
    if let Some(v) = force {
        cli = cli.env("CLICOLOR_FORCE", v);
    }
    let output = cli.command(&["check"]).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    String::from_utf8(output.stderr).unwrap()
}

/// `text` without its SGR escapes (`ESC [ ... m`).
fn strip_sgr(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("\x1b[") {
        out.push_str(&rest[..at]);
        let end = rest[at..].find('m').expect("unterminated SGR");
        rest = &rest[at + end + 1..];
    }
    out.push_str(rest);
    out
}

#[specforge_test(
    behavior = "print_diagnostics_structured",
    verify = "Print Diagnostics Structured: structured diagnostic printing holds — validation_complete_fired, structured_format_enforced, color_coding_applied"
)]
fn print_diagnostics_contract_consistency() {
    let dir = one_of_each_severity();
    // CLICOLOR_FORCE stands in for a terminal, which a test cannot have.
    let stderr = check_stderr(&dir, None, Some("1"));
    let text = strip_sgr(&stderr);

    // validation_complete_fired: every collected diagnostic is printed, and
    // the summary counts them.
    assert!(
        stderr.contains("\x1b[1;31m1 error, 1 warning, 1 info\x1b[0m\n"),
        "{stderr:?}"
    );

    // structured_format_enforced: colour adds escapes and nothing else, so
    // the coloured text is the plain text; each diagnostic carries its code,
    // severity, message, file:line:col and source line.
    assert_eq!(text, check_stderr(&dir, None, None));
    for (heading, location, source) in [
        (
            "[E003] Error: unresolved reference 'nonexistent' in entity 'alpha'\n",
            "─[ main.spec:5:15 ]\n",
            " 5 │   invariants [nonexistent]\n",
        ),
        (
            "[W012] Warning: unreferenced ref 'gh.issue:42' has no incoming edges\n",
            "─[ main.spec:7:1 ]\n",
            " 7 │ ref gh.issue:42 \"Orphan ref\"\n",
        ),
        (
            "[I004] Advice: reference 'user_authentication' in field 'features' of 'alpha' targets kind 'feature', which no enabled extension provides\n",
            "─[ main.spec:4:13 ]\n",
            " 4 │   features [user_authentication]\n",
        ),
    ] {
        let at = text
            .find(heading)
            .unwrap_or_else(|| panic!("{heading}: {text}"));
        let report = &text[at..];
        assert!(report.find(location) < report.find(source), "{text}");
        assert!(report.contains(source), "{text}");
    }

    // color_coding_applied: the severity heading is wrapped in SGR red for
    // the error, yellow for the warning and blue for the info.
    assert!(
        stderr.contains(
            "\x1b[31m[E003] Error:\x1b[0m unresolved reference 'nonexistent' in entity 'alpha'\n"
        ),
        "error heading is red: {stderr:?}"
    );
    assert!(
        stderr.contains(
            "\x1b[33m[W012] Warning:\x1b[0m unreferenced ref 'gh.issue:42' has no incoming edges\n"
        ),
        "warning heading is yellow: {stderr:?}"
    );
    assert!(
        stderr.contains("\x1b[34m[I004] Advice:\x1b[0m reference 'user_authentication'"),
        "info heading is blue: {stderr:?}"
    );
}

#[specforge_test(
    behavior = "print_diagnostics_structured",
    verify = "diagnostics are plain when output is not a terminal"
)]
fn diagnostics_plain_when_not_a_terminal() {
    let dir = one_of_each_severity();
    // stderr is a pipe and nothing forces colour: not one escape byte, the
    // summary line included.
    let stderr = check_stderr(&dir, None, None);
    assert!(!stderr.contains('\x1b'), "{stderr:?}");
    assert!(
        stderr.contains("[E003] Error: unresolved reference 'nonexistent' in entity 'alpha'\n"),
        "{stderr:?}"
    );
    assert!(
        stderr.contains("\n1 error, 1 warning, 1 info\n"),
        "{stderr:?}"
    );
    // CLICOLOR_FORCE set to 0 or empty forces nothing.
    assert_eq!(check_stderr(&dir, None, Some("0")), stderr);
    assert_eq!(check_stderr(&dir, None, Some("")), stderr);

    // JSON output is never coloured, even when forced.
    let output = Cli::on(&dir)
        .command(&["check", "--format=json"])
        .env_remove("NO_COLOR")
        .env("CLICOLOR_FORCE", "1")
        .output()
        .unwrap();
    assert!(!output.stdout.contains(&0x1b), "{output:?}");
    assert!(!output.stderr.contains(&0x1b), "{output:?}");
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(parsed.as_array().unwrap().len(), 3);
}

#[specforge_test(
    behavior = "print_diagnostics_structured",
    verify = "NO_COLOR disables diagnostic colour"
)]
fn no_color_disables_diagnostic_colour() {
    let dir = one_of_each_severity();
    let plain = check_stderr(&dir, None, None);
    // NO_COLOR wins over CLICOLOR_FORCE.
    let stderr = check_stderr(&dir, Some("1"), Some("1"));
    assert!(!stderr.contains('\x1b'), "{stderr:?}");
    assert_eq!(stderr, plain);
    // An empty NO_COLOR counts as unset (no-color.org): forcing still works.
    let stderr = check_stderr(&dir, Some(""), Some("1"));
    assert!(
        stderr.contains("\x1b[31m[E003] Error:\x1b[0m"),
        "{stderr:?}"
    );
}

#[specforge_test(
    behavior = "exit_code_reflects_diagnostic_severity",
    verify = "Exit Code Reflects Diagnostic Severity: exit code severity mapping holds — validation_complete_fired, exit_zero_on_clean, exit_one_on_errors, strict_mode_enforced"
)]
fn exit_code_contract_consistency() {
    // Requires: validation_complete fired
    // Ensures: exit 0 on clean, exit 1 on errors, strict promotes warnings
    let clean_dir = TestProject::new()
        .without_config()
        .files(&[("main.spec", r#"behavior alpha "A" { contract "first" }"#)]);
    Cli::on(&clean_dir).command(&["check"]).assert().success();

    let error_dir = TestProject::new()
        .without_config()
        .files(&[("main.spec", r#"feature g "G" { behaviors [nonexistent] }"#)]);
    Cli::on(&error_dir).command(&["check"]).assert().code(1);
}

#[specforge_test(
    behavior = "check_mode_for_ci",
    verify = "Check Mode for CI: CI check mode holds — validation_complete_fired, no_output_files_produced, diagnostics_to_stderr, appropriate_exit_code"
)]
fn check_mode_contract_consistency() {
    // Requires: validation_complete fired
    // Ensures: no output files, diagnostics to stderr, appropriate exit code
    let dir = TestProject::new().without_config().files(&[(
        "main.spec",
        r#"
behavior alpha "A" { contract "first" }
feature gamma "G" { behaviors [alpha, nonexistent] }
"#,
    )]);

    let before_count = fs::read_dir(dir.root()).unwrap().count();

    let output = Cli::on(&dir).command(&["check"]).output().unwrap();

    let after_count = fs::read_dir(dir.root()).unwrap().count();
    assert_eq!(
        before_count, after_count,
        "check must not produce output files"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty(), "diagnostics must go to stderr");

    assert_eq!(output.status.code(), Some(1), "errors must cause exit 1");
}

#[specforge_test(
    behavior = "export_diagnostics_as_json",
    verify = "diagnostics serialized as JSON array to stdout"
)]
fn json_format_outputs_to_stdout() {
    let dir = TestProject::new().without_config().files(&[(
        "main.spec",
        r#"
behavior alpha "A" { contract "first" }
feature gamma "G" { behaviors [alpha, nonexistent] }
"#,
    )]);

    let output = Cli::on(&dir)
        .command(&["check", "--format=json"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.is_empty(), "JSON output should go to stdout");
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");
    assert!(parsed.is_array(), "should be a JSON array");
}

#[specforge_test(
    behavior = "export_diagnostics_as_json",
    verify = "exit code unaffected by format flag"
)]
fn json_format_exit_code_matches_human_format() {
    // With errors: both human and json format should exit 1
    let dir = TestProject::new().without_config().files(&[(
        "main.spec",
        r#"
behavior alpha "A" { contract "first" }
feature gamma "G" { behaviors [alpha, nonexistent] }
"#,
    )]);

    let human_output = Cli::on(&dir)
        .command(&["check", "--format=human"])
        .output()
        .unwrap();

    let json_output = Cli::on(&dir)
        .command(&["check", "--format=json"])
        .output()
        .unwrap();

    assert_eq!(
        human_output.status.code(),
        json_output.status.code(),
        "exit code must be same regardless of format flag"
    );
}

#[specforge_test(
    behavior = "export_diagnostics_as_json",
    verify = "Export Diagnostics as JSON: JSON diagnostic export holds — validation_complete_fired, json_array_produced, diagnostic_fields_complete, exit_code_unaffected"
)]
fn json_diagnostics_contract_consistency() {
    // Requires: validation_complete (diagnostics collected)
    // Ensures: JSON array to stdout, complete fields, exit code unaffected
    let dir = TestProject::new().without_config().files(&[(
        "main.spec",
        r#"
behavior alpha "A" { contract "first" }
feature gamma "G" { behaviors [alpha, nonexistent] }
"#,
    )]);

    let output = Cli::on(&dir)
        .command(&["check", "--format=json"])
        .output()
        .unwrap();

    // JSON array to stdout
    let stdout = String::from_utf8_lossy(&output.stdout);
    let diags: Vec<serde_json::Value> =
        serde_json::from_str(&stdout).expect("must produce valid JSON array");
    assert!(!diags.is_empty(), "must contain diagnostics");

    // Complete fields
    for d in &diags {
        assert!(d.get("code").is_some());
        assert!(d.get("severity").is_some());
        assert!(d.get("message").is_some());
    }

    // Exit code reflects severity (errors present → exit 1)
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn json_diagnostics_include_suggestion_when_available() {
    let dir = TestProject::new().without_config().files(&[(
        "main.spec",
        r#"
behavior alpha_parser "A" { contract "first" }
feature gamma "G" { behaviors [alpha_parsr] }
"#,
    )]);

    let output = Cli::on(&dir)
        .command(&["check", "--format=json"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let diags: Vec<serde_json::Value> =
        serde_json::from_str(&stdout).expect("must produce valid JSON");

    let has_suggestion = diags.iter().any(|d| d.get("suggestion").is_some());
    assert!(
        has_suggestion,
        "at least one diagnostic should have a suggestion: {:?}",
        diags
    );
}

// === typed format flags (C14-10) ===

/// A project whose check reports two errors (E007, a module cycle), one
/// warning (W061, the reference cycle) and two infos (I067, modules
/// without features).
fn project_with_every_severity() -> TestProject {
    TestProject::new().without_config().files(&[
        (
            "specforge.json",
            r#"{"name":"p","version":"0.1.0","extensions":["@specforge/software","@specforge/product"]}"#,
        ),
        (
            "main.spec",
            "module core_mod \"Core\" {\n  description \"The core\"\n  depends_on  [edge_mod]\n}\n\nmodule edge_mod \"Edge\" {\n  description \"The edge\"\n  depends_on  [core_mod]\n}\n",
        ),
    ])
}

/// `specforge check --format json <dir> <args>`: exit code, the codes
/// printed, and stderr.
fn check_json(dir: &TestProject, args: &[&str]) -> (Option<i32>, Vec<String>, String) {
    let mut argv = vec!["check", "--format", "json"];
    argv.extend(args);
    let output = Cli::on(dir).command(&argv).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let codes = serde_json::from_str::<serde_json::Value>(&stdout)
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .map(|d| d["code"].as_str().unwrap_or("?").to_string())
        .collect();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.code(), codes, stderr)
}

#[specforge_test(
    behavior = "exit_code_reflects_diagnostic_severity",
    verify = "an unknown --lint profile fails with a clap error (exit 2), before anything is compiled"
)]
fn unknown_lint_profile_is_rejected_by_clap() {
    let dir = project_with_every_severity();
    let (code, codes, stderr) = check_json(&dir, &["--lint", "nonsense", "--cache"]);
    assert_eq!(code, Some(2), "{stderr}");
    assert!(
        stderr.contains("invalid value 'nonsense'") && stderr.contains("inferred, pedantic"),
        "expected a clap parse error naming the profiles, got: {stderr}"
    );
    // Nothing ran: no diagnostics printed, no cache note, no cache file.
    assert!(codes.is_empty(), "{codes:?}");
    assert!(!stderr.contains("specforge-cache.json"), "{stderr}");
    assert!(!dir.root().join("specforge-cache.json").exists());

    // An unknown --severity is refused the same way.
    let (code, _, stderr) = check_json(&dir, &["--severity", "errors"]);
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("invalid value 'errors'"), "{stderr}");
}

#[specforge_test(
    behavior = "filter_reported_diagnostics",
    verify = "check --severity prints only that severity and never changes the exit code"
)]
fn check_severity_shows_one_severity_and_keeps_the_exit_code() {
    let dir = project_with_every_severity();
    let (code, all, _) = check_json(&dir, &[]);
    assert_eq!(code, Some(1));
    assert_eq!(all, ["W061", "E007", "E007", "I067", "I067"]);

    for (severity, expected) in [
        ("info", vec!["I067", "I067"]),
        ("Info", vec!["I067", "I067"]),
        ("error", vec!["E007", "E007"]),
        ("WARNING", vec!["W061"]),
    ] {
        let (code, codes, stderr) = check_json(&dir, &["--severity", severity]);
        assert_eq!(codes, expected, "--severity {severity}");
        assert_eq!(code, Some(1), "the errors still fail the check: {stderr}");
    }

    // Strict promotes first: the warning is shown as an error.
    let (code, codes, _) = check_json(&dir, &["--strict", "--severity", "error"]);
    assert_eq!(codes, ["W061", "E007", "E007"]);
    assert_eq!(code, Some(1));
    let (_, codes, _) = check_json(&dir, &["--strict", "--severity", "warning"]);
    assert!(codes.is_empty(), "{codes:?}");

    // Human output: only the infos are rendered, the summary counts
    // everything and says what it shows.
    let output = Cli::on(&dir)
        .command(&["check", "--severity", "info"])
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("[I067]"), "{stderr}");
    assert!(!stderr.contains("[E007]"), "{stderr}");
    assert!(!stderr.contains("[W061]"), "{stderr}");
    assert!(
        stderr.contains("2 errors, 1 warning, 2 infos (showing info only)"),
        "{stderr}"
    );

    // The filter does not decide the cache either: the check failed.
    let (_, _, stderr) = check_json(&dir, &["--severity", "info", "--cache"]);
    assert!(stderr.contains("not written: the check failed"), "{stderr}");
    assert!(!dir.root().join("specforge-cache.json").exists());
}

#[specforge_test(
    behavior = "check_diagnostic_policy",
    verify = "--lint pedantic is accepted and changes nothing"
)]
fn lint_pedantic_is_accepted_and_changes_nothing() {
    let dir = project_with_every_severity();
    let plain = check_json(&dir, &[]);
    let (code, codes, stderr) = check_json(&dir, &["--lint", "pedantic"]);
    assert_eq!((code, &codes), (plain.0, &plain.1));
    assert_eq!(
        stderr
            .matches("note: --lint pedantic is the default: info diagnostics are always reported")
            .count(),
        1,
        "{stderr}"
    );
    // With another profile in the same flag, pedantic is still a no-op.
    let (code, codes, _) = check_json(&dir, &["--lint=pedantic,inferred"]);
    assert_eq!((code, &codes), (plain.0, &plain.1));
}

#[specforge_test(
    behavior = "exit_code_reflects_diagnostic_severity",
    verify = "a typo'd --format fails with a clap error (exit 2), not a bespoke runtime error"
)]
fn unknown_format_value_is_rejected_by_clap() {
    let dir = TestProject::new()
        .without_config()
        .files(&[("main.spec", r#"behavior alpha "A" { contract "first" }"#)]);

    let output = Cli::on(&dir)
        .command(&["stats", "--format=yaml"])
        .env("COLUMNS", "100")
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(2),
        "clap rejects unknown values with exit code 2"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid value"),
        "expected a clap parse error, got: {stderr}"
    );
}

// === provide_infer_status_cli: an unusable manifest ===

#[specforge_test(
    behavior = "provide_infer_status_cli",
    verify = "an unusable manifest is refused with E071"
)]
fn infer_status_refuses_an_unusable_manifest_with_e071() {
    let dir = TestProject::new()
        .without_config()
        .files(&[("specforge-infer.json", "{ nope")]);

    let output = Cli::on(&dir).command(&["infer-status"]).output().unwrap();

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("error[E071]: failed to parse specforge-infer.json"),
        "{stderr}"
    );

    // `check --lint inferred` fails on it too, instead of passing over it.
    let output = Cli::on(&dir)
        .command(&["check", "--lint", "inferred"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E071"), "{stderr}");
}

#[specforge_test(
    behavior = "provide_infer_status_cli",
    verify = "prints the sessions the manifest records, with their timestamps"
)]
fn infer_status_prints_the_session_history() {
    let dir = TestProject::new().without_config().files(&[(
        "specforge-infer.json",
        r#"{"version": 1, "source_roots": [], "sessions": [
            {"session_id": "s-1", "agent": "claude", "status": "paused",
             "started_at": "2026-10-01T00:00:00Z", "ended_at": "2026-10-01T01:00:00Z"},
            {"session_id": "s-2", "agent": "codex", "status": "active",
             "started_at": "2026-10-02T00:00:00Z"}]}"#,
    )]);

    let output = Cli::on(&dir).command(&["infer-status"]).output().unwrap();

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(
            "Sessions:\n  s-1  claude  paused  2026-10-01T00:00:00Z \u{2192} 2026-10-01T01:00:00Z\n  s-2  codex  active  2026-10-02T00:00:00Z \u{2192} \u{2026}\n"
        ),
        "{stdout}"
    );
    assert!(!stdout.contains("No specforge-infer.json yet"), "{stdout}");
}

/// A reader that closes stdout early (`specforge stats | head`) ends the
/// output quietly: no panic, nothing on stderr, and the command's own exit.
#[cfg(unix)]
#[specforge_test(
    behavior = "report_command_outcome",
    verify = "a closed stdout ends the output quietly and keeps the command's exit"
)]
fn a_closed_stdout_ends_the_output_quietly_in_the_binary() {
    use std::io::Read;

    let dir = TestProject::new().without_config().files(&[
        (
            "specforge.json",
            r#"{"name":"t","version":"0.1.0","extensions":[]}"#,
        ),
        (
            "main.spec",
            "behavior parse_input \"Parse\" {\n  contract \"x\"\n}\n",
        ),
    ]);
    let mut child = Cli::on(&dir).start(&["stats"]);
    // The child compiles before it writes, so the read end is closed first.
    drop(child.stdout.take());
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(0));
    assert_eq!(stderr, "");
}

#[specforge_test(
    behavior = "export_diagnostics_as_json",
    verify = "diagnostics serialized as JSON array to stdout"
)]
fn check_json_shows_cross_extension_errors() {
    let dir = TestProject::new().file(
        "main.spec",
        r#"
feature gamma "G" { behaviors [nonexistent] }
decision use_rust "D" {
    status accepted
    context "need performance"
    decision_text "use Rust"
    consequences "fast"
}
"#,
    );

    let parsed = Cli::on(&dir).document(&["check"]).json;
    let diagnostics = parsed.as_array().unwrap();
    assert!(
        diagnostics.iter().any(|d| d["code"] == "E003"),
        "should produce E003 for nonexistent ref in cross-kind context"
    );
}

#[specforge_test(
    behavior = "check_mode_for_ci",
    verify = "Check Mode for CI: CI check mode holds — validation_complete_fired, no_output_files_produced, diagnostics_to_stderr, appropriate_exit_code"
)]
fn check_full_multi_extension_project_exits_zero() {
    let dir = TestProject::new().file("main.spec", specs::MULTI_EXTENSION);
    let tree = |root: &std::path::Path| {
        let mut entries: Vec<_> = walk(root)
            .into_iter()
            .map(|p| (p.clone(), std::fs::read(&p).unwrap_or_default()))
            .collect();
        entries.sort();
        entries
    };
    let before = tree(dir.root());

    // validation_complete_fired, appropriate_exit_code: a clean project
    // validates and exits 0.
    let output = Cli::on(&dir).run(&["check"]);
    let stderr = &output.stderr;
    assert_eq!(output.code, 0, "{stderr}");

    // diagnostics_to_stderr: the report goes to stderr, nothing to stdout.
    assert!(output.stdout.is_empty());
    // No specforge.json: checked structurally, which I002 says.
    assert!(stderr.contains("0 errors, 0 warnings, 1 info"), "{stderr}");

    // no_output_files_produced
    assert_eq!(tree(dir.root()), before, "check must not write files");

    // An error in the same project flips the exit code, still on stderr.
    let broken = format!(
        "{}\nfeature broken_ref \"B\" {{ behaviors [missing_one] }}\n",
        specs::MULTI_EXTENSION
    );
    let dir = TestProject::new().file("main.spec", &broken);
    let output = Cli::on(&dir).run(&["check"]);
    let stderr = &output.stderr;
    assert_eq!(output.code, 1, "{stderr}");
    assert!(output.stdout.is_empty());
    assert!(stderr.contains("missing_one"), "{stderr}");
}

/// Every file under `dir`, recursively.
fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            files.extend(walk(&path));
        } else {
            files.push(path);
        }
    }
    files
}
