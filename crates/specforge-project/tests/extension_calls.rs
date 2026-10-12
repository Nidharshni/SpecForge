//! What the host sends compiler passes and custom validators, compared
//! with the wire goldens in `crates/specforge-wasm/tests/wire/`, how it
//! reads their answers, and what a failure becomes. These were plan 04's
//! T1 characterization pins; T6 and T7 flipped the ones that pinned a bug.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use specforge_common::{Diagnostic, Severity};
use specforge_extension_sdk::prelude::*;
use specforge_project::CompiledProject;
use specforge_project::coverage::TestReport;
use specforge_project::passes::{AnalysisContext, pass_input, run_extension_passes};
use specforge_wasm::testing::InProcessRuntime;
use specforge_wasm::{WasmCallResult, WasmTrapInfo};
use std::sync::Arc;
use tempfile::TempDir;

const EXT: &str = "@pin/passes";

fn wire_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../specforge-wasm/tests/wire")
}

fn sorted(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            Value::Object(
                keys.into_iter()
                    .map(|k| (k.clone(), sorted(&map[k])))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
        other => other.clone(),
    }
}

fn golden(name: &str, actual: &Value) {
    let path = wire_dir().join(name);
    if std::env::var_os("SPECFORGE_BLESS").is_some() {
        let mut text = serde_json::to_string_pretty(&sorted(actual)).unwrap();
        text.push('\n');
        fs::write(&path, text).unwrap();
    }
    let expected: Value = serde_json::from_str(
        &fs::read_to_string(&path).unwrap_or_else(|e| panic!("golden {}: {e}", path.display())),
    )
    .unwrap();
    assert_eq!(actual, &expected, "golden {name}");
}

/// `gadget`s (testable, verify `unit`, a `needs` reference, a `status`
/// lifecycle field, an `abstract` flag exempting them), a W004-style rule
/// obligating them, a custom rule `validate__shape`, a check pass `audit`
/// and an analyze pass `report`, each declared with its handler.
fn extension() -> ContributionsBuilder {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new(EXT, "1.0.0"));
    c.kind("gadget", |k| {
        k.keyword("gadget")
            .testable(true)
            .supports_verify(true)
            .verify_kinds(&["unit"])
            .lifecycle_field("status");
        k.field("needs", |f| {
            f.field_type(FieldType::Reference)
                .edge("needs")
                .target_kind("gadget");
        });
        k.field("status", |f| {
            f.field_type(FieldType::String);
        });
        k.field("abstract", |f| {
            f.field_type(FieldType::Bool).exempts_obligations();
        });
    });
    c.rule("W990", |r| {
        r.check(CheckKind::NoVerifyStatements)
            .target_kind("gadget")
            .message_template("gadget '{id}' declares no obligations");
    });
    // The validator passes everything and both passes report nothing; the
    // tests answer otherwise through `answer_raw`.
    c.rule("E991", |r| {
        r.check(CheckKind::Custom)
            .target_kind("gadget")
            .wasm_function("validate__shape")
            .severity(ValidationSeverity::Error)
            .message_template("gadget '{id}' has a bad {field}")
            .validate(|_| ValidatorVerdict::Pass);
    });
    c.pass("audit", |p| {
        p.phase("check").run(no_findings);
    });
    c.pass("report", |p| {
        p.run(no_findings);
    });
    c
}

fn no_findings(_: &PassInput) -> Vec<PassDiagnostic> {
    Vec::new()
}

fn runtime() -> InProcessRuntime {
    InProcessRuntime::new().with(extension)
}

const SPEC: &str = "gadget a \"A\" {\n  needs b\n  status \"active\"\n  verify unit \"a works\"\n}\n\n\
gadget b \"B\" {\n  status \"retired\"\n}\n\ngadget c \"C\" {\n  abstract true\n}\n";

fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    let config = json!({ "name": "p", "version": "0.1.0", "extensions": [EXT] });
    fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    specforge_installed::testing::install_configured(dir.path(), &specforge_project::builtins());
    fs::write(dir.path().join("a.spec"), SPEC).unwrap();
    dir
}

/// The input the last call to `export` received.
fn last_input(runtime: &InProcessRuntime, export: &str) -> Value {
    runtime
        .calls()
        .into_iter()
        .rev()
        .find(|c| c.export == export)
        .unwrap_or_else(|| panic!("{export} was never called"))
        .input
}

// ── C4 · compiler pass ──

#[specforge_test_macros::test(
    behavior = "call_extension_exports",
    verify = "every extension call encodes its input as the protocol type the SDK decodes"
)]
fn c4_the_pass_input_of_an_analysis() {
    let dir = project();
    let runtime = Arc::new(runtime());
    let compiled = CompiledProject::compile(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Some(runtime.clone()),
    );
    let report: TestReport = serde_json::from_value(json!({
        "runner": "cargo-test",
        "results": {"a": {"file": "a.rs", "tests": [
            {"name": "a_works", "status": "pass", "verify": "a works", "duration_ms": 1.5,
             "runner": "cargo-test"}
        ]}}
    }))
    .unwrap();
    let proved: HashSet<String> = ["b".to_string(), "a".to_string()].into();
    let registries = compiled.environment().registries();
    let input = pass_input(&AnalysisContext {
        graph: compiled.graph(),
        kind_registry: registries.kinds(),
        field_registry: registries.fields(),
        entities: compiled.entities(),
        test_results: Some(&report),
        proved_claims: Some(&proved),
    });
    // flipped in T6: the report's own keys (a result's `file`, a test's
    // `duration_ms` and `runner`) are not the protocol's, and not sent
    golden("pass.input.json", &serde_json::to_value(&input).unwrap());
}

#[specforge_test_macros::test(
    behavior = "call_extension_exports",
    verify = "every extension call encodes its input as the protocol type the SDK decodes"
)]
fn c4_the_pass_input_of_a_compile_carries_previous() {
    let dir = project();
    fs::write(
        dir.path().join("specforge-cache.json"),
        json!({"format": 1, "statuses": {"a": {"kind": "gadget", "status": "draft"}}}).to_string(),
    )
    .unwrap();
    let runtime = Arc::new(runtime());
    CompiledProject::compile(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Some(runtime.clone()),
    );
    // flipped in T6: `test_results` and `proved_claims` are absent, not null
    golden(
        "pass.check.input.json",
        &last_input(&runtime, "__pass_audit"),
    );
}

fn span(file: &str, line: usize) -> Value {
    json!({"file": file, "start_line": line, "start_col": 1, "end_line": line, "end_col": 2})
}

/// A check pass answering `answer` (raw bytes or a trap).
fn compile_answering(answer: WasmCallResult) -> Vec<Diagnostic> {
    let dir = project();
    let runtime = Arc::new(runtime().answer_raw(EXT, "__pass_audit", answer));
    CompiledProject::compile(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Some(runtime.clone()),
    )
    .diagnostics()
}

fn pass_findings(diagnostics: &[Diagnostic]) -> Vec<&Diagnostic> {
    diagnostics
        .iter()
        .filter(|d| d.code.starts_with('X') || d.code == "E028")
        .collect()
}

#[specforge_test_macros::test(
    behavior = "call_extension_exports",
    verify = "a pass answer may be bare diagnostics or diagnostics with a summary, and its diagnostics come back in canonical order with an entity's span attached"
)]
fn c4_a_pass_answer_is_bare_or_with_a_summary_and_comes_back_sorted() {
    let diagnostics = json!([
        {"code": "X2", "severity": "Warning", "message": "b", "span": span("z.spec", 3)},
        {"code": "X1", "severity": "Info", "message": "names a", "entity": "a"},
        {"code": "X2", "severity": "Warning", "message": "a", "span": span("a.spec", 9)},
        {"code": "X2", "severity": "Error", "message": "c", "span": span("a.spec", 9),
         "data": {"kind": "shadowed_keyword", "keyword": "b"}}
    ]);
    for answer in [
        diagnostics.clone(),
        json!({"diagnostics": diagnostics, "summary": {"n": 4}}),
    ] {
        let all = compile_answering(WasmCallResult::Ok(answer.to_string().into_bytes()));
        let found = pass_findings(&all);
        let shown: Vec<(&str, &str, Option<String>)> = found
            .iter()
            .map(|d| {
                (
                    d.code.as_str(),
                    d.message.as_str(),
                    d.span
                        .as_ref()
                        .map(|s| format!("{}:{}", s.file.as_str(), s.start_line)),
                )
            })
            .collect();
        assert_eq!(
            shown,
            [
                ("X1", "names a", Some("a.spec:1".to_string())),
                ("X2", "a", Some("a.spec:9".to_string())),
                ("X2", "c", Some("a.spec:9".to_string())),
                ("X2", "b", Some("z.spec:3".to_string())),
            ],
            "{all:?}"
        );
        // flipped in T6: a guest cannot set the host's diagnostic data (D9)
        assert!(found[2].data.is_none(), "{:?}", found[2]);
        // The entity a diagnostic names is the host's subject data (ADR 0016).
        assert_eq!(
            found[0].data.as_deref(),
            Some(&specforge_common::DiagnosticData::Subject { entity: "a".into() }),
            "{:?}",
            found[0]
        );
    }
}

#[specforge_test_macros::test(
    behavior = "run_check_phase_passes",
    verify = "a trapping check pass is a diagnostic, not a crash"
)]
fn c4_a_failing_check_pass_is_e028() {
    let trapped = compile_answering(WasmCallResult::Trap(WasmTrapInfo {
        kind: "k".into(),
        message: "m".into(),
        export_name: "__pass_audit".into(),
    }));
    let e028: Vec<&Diagnostic> = trapped.iter().filter(|d| d.code == "E028").collect();
    assert_eq!(e028.len(), 1, "{trapped:?}");
    assert_eq!(e028[0].severity, Severity::Error);
    assert_eq!(
        e028[0].message,
        format!("compiler pass __pass_audit() of '{EXT}' trapped: k: m")
    );
    assert_eq!(
        e028[0].suggestion.as_deref(),
        Some(
            format!("report the failure to the author of '{EXT}', or check it is installed and up to date")
                .as_str()
        )
    );
    let malformed = compile_answering(WasmCallResult::Ok(b"{\"nope\":1}".to_vec()));
    let e028: Vec<&Diagnostic> = malformed.iter().filter(|d| d.code == "E028").collect();
    assert_eq!(e028.len(), 1, "{malformed:?}");
    assert!(
        e028[0].message.starts_with(&format!(
            "compiler pass __pass_audit() of '{EXT}' answered output that is not a PassAnswer: "
        )),
        "{}",
        e028[0].message
    );
}

#[specforge_test_macros::test(
    behavior = "call_extension_exports",
    verify = "an analyze pass that traps is reported as an E028 finding of that pass"
)]
fn c4_a_failing_analyze_pass_is_an_e028_finding_of_its_report() {
    // flipped in T6: was no report at all, a stderr line
    let dir = project();
    let runtime = Arc::new(runtime().answer_raw(
        EXT,
        "__pass_report",
        WasmCallResult::Trap(WasmTrapInfo {
            kind: "k".into(),
            message: "m".into(),
            export_name: "__pass_report".into(),
        }),
    ));
    let compiled = CompiledProject::compile(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Some(runtime.clone()),
    );
    let registries = compiled.environment().registries();
    let reports = run_extension_passes(
        registries.analyze_passes(),
        &AnalysisContext {
            graph: compiled.graph(),
            kind_registry: registries.kinds(),
            field_registry: registries.fields(),
            entities: compiled.entities(),
            test_results: None,
            proved_claims: None,
        },
        runtime.as_ref(),
    );
    assert_eq!(reports.len(), 1);
    let report = &reports[0];
    assert_eq!(report.name, format!("{EXT}:report"));
    assert_eq!(report.findings.len(), 1, "{:?}", report.findings);
    assert_eq!(report.findings[0].code, "E028");
    assert_eq!(report.findings[0].severity, Severity::Error);
    assert_eq!(
        report.findings[0].message,
        format!("compiler pass __pass_report() of '{EXT}' trapped: k: m")
    );
    assert_eq!(
        report.summary,
        json!({"extension": EXT, "pass": "report", "entities_analyzed": 3, "failed": true})
    );
}

// ── C6 · custom validator ──

#[specforge_test_macros::test(
    behavior = "call_extension_exports",
    verify = "every extension call encodes its input as the protocol type the SDK decodes"
)]
fn c6_the_validator_context() {
    let dir = project();
    let runtime = Arc::new(runtime());
    CompiledProject::compile(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Some(runtime.clone()),
    );
    let contexts: Vec<Value> = runtime
        .calls()
        .into_iter()
        .filter(|c| c.export == "validate__shape")
        .map(|c| c.input)
        .collect();
    // The probe on load, then one call per gadget.
    golden("validate.input.json", &Value::Array(contexts));
}

#[specforge_test_macros::test(
    behavior = "register_custom_validation_patterns",
    verify = "a custom validator's verdict is read as the protocol's ValidatorVerdict, and a failure is reported once as W112"
)]
fn c6_a_verdict_is_read_and_a_failure_is_w112_on_load() {
    let dir = project();
    let runtime = Arc::new(runtime().answer_raw(
        EXT,
        "validate__shape",
        WasmCallResult::Ok(br#"{"verdict":"fail","field":"needs","value":"b"}"#.to_vec()),
    ));
    let failed = CompiledProject::compile(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Some(runtime.clone()),
    )
    .diagnostics();
    let e991: Vec<&Diagnostic> = failed.iter().filter(|d| d.code == "E991").collect();
    assert_eq!(e991.len(), 3, "{failed:?}");
    assert_eq!(e991[0].message, "gadget 'a' has a bad needs");

    let runtime = Arc::new(runtime_with_trap());
    let trapped = CompiledProject::compile(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Some(runtime.clone()),
    )
    .diagnostics();
    let w112: Vec<&Diagnostic> = trapped.iter().filter(|d| d.code == "W112").collect();
    assert_eq!(w112.len(), 1, "{trapped:?}");
    assert_eq!(
        w112[0].message,
        format!(
            "extension '{EXT}': rule 'E991': wasm_function 'validate__shape' could not be resolved \
             (custom validator validate__shape() of '{EXT}' trapped: k: m) — the rule will not fire"
        )
    );
    assert!(trapped.iter().all(|d| d.code != "E991"), "{trapped:?}");

    // An answer that is not a ValidatorVerdict is a failure too: W112 once,
    // and the rule never fires on a default verdict.
    let answering_garbage = self::runtime().answer_raw(
        EXT,
        "validate__shape",
        WasmCallResult::Ok(br#"{"field":"needs"}"#.to_vec()),
    );
    let malformed = CompiledProject::compile(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Some(Arc::new(answering_garbage)),
    )
    .diagnostics();
    let w112: Vec<&Diagnostic> = malformed.iter().filter(|d| d.code == "W112").collect();
    assert_eq!(w112.len(), 1, "{malformed:?}");
    assert!(
        w112[0].message.contains(&format!(
            "(custom validator validate__shape() of '{EXT}' answered output that is not a ValidatorVerdict: "
        )),
        "{}",
        w112[0].message
    );
    assert!(malformed.iter().all(|d| d.code != "E991"), "{malformed:?}");
}

fn runtime_with_trap() -> InProcessRuntime {
    runtime().answer_raw(
        EXT,
        "validate__shape",
        WasmCallResult::Trap(WasmTrapInfo {
            kind: "k".into(),
            message: "m".into(),
            export_name: "validate__shape".into(),
        }),
    )
}
