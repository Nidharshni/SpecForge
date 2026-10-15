//! Check-phase extension passes: a pass declared with `phase: "check"`
//! runs inside every compile (one-shot and session), in its declared
//! order, and what it reports joins the compile's diagnostics; the build
//! cache is handed to it as `previous`.

use std::fs;
use std::sync::Arc;

use serde_json::{Value, json};
use specforge_common::{Diagnostic, Severity};
use specforge_extension_sdk::prelude::*;
use specforge_project::Buffer;
use specforge_project::testing::TestProject;
use specforge_test::prelude::*;
use specforge_wasm::testing::InProcessRuntime;

const EXT: &str = "@test/passes";

/// An extension, in process, that declares the `gadget` kind and five
/// passes, deliberately out of order:
/// - `second` (check, after `first`) and `first` (check) report nothing;
/// - `audit` (check) reports E951 for every gadget whose id starts with
///   `bad`, naming the entity and giving no span, W952 with an explicit
///   span-less, entity-less message, and W953 naming `ghost_gadget`, an
///   entity the graph does not hold;
/// - `boom` (check) panics, a trap;
/// - `report` (no phase) reports I950 for every gadget.
///
/// The runtime records every call with its input, oldest first.
pub(crate) fn passes_extension() -> InProcessRuntime {
    InProcessRuntime::new().with(|| {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new(EXT, "1.0.0"));
        // `docs` names files (a file reference field): the checks report
        // a missing one (E016).
        c.kind("gadget", |k| {
            k.keyword("gadget");
            k.field("docs", |f| {
                f.field_type(FieldType::StringList).file_reference();
            });
        });
        c.pass("second", |p| {
            p.after("first").phase("check").run(nothing);
        });
        c.pass("audit", |p| {
            p.after("resolve").phase("check").run(audit);
        });
        c.pass("report", |p| {
            p.after("resolve").run(report);
        });
        c.pass("boom", |p| {
            p.phase("check")
                .run(|_: &PassInput| -> Vec<PassDiagnostic> { panic!("the pass broke") });
        });
        c.pass("first", |p| {
            p.phase("check").run(nothing);
        });
        c
    })
}

fn nothing(_: &PassInput) -> Vec<PassDiagnostic> {
    Vec::new()
}

fn audit(input: &PassInput) -> PassOutput {
    let mut diagnostics: Vec<PassDiagnostic> = input
        .entities
        .iter()
        .filter(|e| e.kind == "gadget" && e.id.starts_with("bad"))
        .map(|e| {
            PassDiagnostic::new(
                "E951",
                PassSeverity::Error,
                format!("gadget '{}' fails the audit", e.id),
            )
            .with_entity(&e.id)
        })
        .collect();
    diagnostics.push(PassDiagnostic::warning("W952", "the audit ran"));
    // About an entity the graph does not hold.
    diagnostics
        .push(PassDiagnostic::warning("W953", "a gadget went missing").with_entity("ghost_gadget"));
    let mut summary = serde_json::Map::new();
    summary.insert("audited".to_string(), true.into());
    PassOutput {
        diagnostics,
        summary,
    }
}

fn report(input: &PassInput) -> Vec<PassDiagnostic> {
    input
        .entities
        .iter()
        .filter(|e| e.kind == "gadget")
        .map(|e| {
            PassDiagnostic::new(
                "I950",
                PassSeverity::Info,
                format!("gadget '{}' reported", e.id),
            )
        })
        .collect()
}

/// The pass exports called so far, oldest first.
fn called(ext: &InProcessRuntime) -> Vec<String> {
    ext.calls()
        .into_iter()
        .filter(|c| c.export.starts_with("__pass_"))
        .map(|c| c.export)
        .collect()
}

/// The input the last call to `export` received.
fn last_input(ext: &InProcessRuntime, export: &str) -> Value {
    ext.calls()
        .into_iter()
        .rev()
        .find(|c| c.export == export)
        .map(|c| c.input)
        .unwrap_or_else(|| panic!("{export} was never called"))
}

fn gadgets(input: &Value) -> Vec<String> {
    input["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "gadget")
        .map(|e| e["id"].as_str().unwrap().to_string())
        .collect()
}

pub(crate) fn project(spec: &str) -> TestProject {
    TestProject::new()
        .config(|c| c["name"] = "p".into())
        .enabling(&[EXT])
        .installing_enabled()
        .file("a.spec", spec)
}

pub(crate) const SPEC: &str =
    "gadget good \"Good\" {\n  status active\n}\n\ngadget bad_one \"Bad\" {\n  status retired\n}\n";

fn with_code<'a>(diagnostics: &'a [Diagnostic], code: &str) -> Vec<&'a Diagnostic> {
    diagnostics.iter().filter(|d| d.code == code).collect()
}

#[specforge_test(
    behavior = "run_check_phase_passes",
    verify = "a pass declared for the check phase runs on every compile"
)]
fn a_check_pass_runs_on_every_compile() {
    let dir = project(SPEC);
    let ext = Arc::new(passes_extension());

    let compiled = dir.compile_in(ext.clone());

    let diagnostics = compiled.diagnostics();
    let audit = with_code(&diagnostics, "E951");
    assert_eq!(audit.len(), 1, "{diagnostics:?}");
    assert_eq!(audit[0].severity, Severity::Error);
    assert_eq!(audit[0].message, "gadget 'bad_one' fails the audit");
    let ran = with_code(&diagnostics, "W952");
    assert_eq!(ran.len(), 1, "{diagnostics:?}");
    assert_eq!(ran[0].severity, Severity::Warning);
    assert_eq!(
        gadgets(&last_input(&ext, "__pass_audit")),
        ["bad_one", "good"],
        "the pass sees the compiled entities"
    );

    // A second compile runs it again.
    ext.clear_calls();
    let again = dir.compile_in(ext.clone()).diagnostics();
    assert_eq!(with_code(&again, "E951").len(), 1, "{again:?}");
    assert!(called(&ext).contains(&"__pass_audit".to_string()));
}

/// A pass that names the entity a diagnostic is about: the name survives
/// as `Subject` data (attribution reads it, never the message), the span
/// is the entity's when the graph holds it, and none when it does not.
#[test]
fn a_pass_diagnostic_naming_an_entity_carries_it_as_data() {
    let dir = project(SPEC);
    let ext = Arc::new(passes_extension());
    let diagnostics = dir.compile_in(ext.clone()).diagnostics();

    let audit = with_code(&diagnostics, "E951")[0];
    assert_eq!(
        audit.data.as_deref(),
        Some(&specforge_common::DiagnosticData::Subject {
            entity: "bad_one".into()
        })
    );
    assert_eq!(audit.span.as_ref().map(|s| s.start_line), Some(5));
    let ghost = with_code(&diagnostics, "W953")[0];
    assert!(ghost.span.is_none(), "{ghost:?}");
    assert_eq!(
        ghost.data.as_deref().map(|d| d.entities()),
        Some(vec!["ghost_gadget"])
    );
    let ran = with_code(&diagnostics, "W952")[0];
    assert!(
        ran.data.is_none(),
        "a diagnostic naming nothing carries nothing"
    );
}

#[specforge_test(
    behavior = "run_check_phase_passes",
    verify = "a session reports a check pass's diagnostics after an update"
)]
fn a_session_reports_check_pass_diagnostics_after_an_update() {
    let dir = project("gadget good \"Good\" {\n}\n");
    let ext = Arc::new(passes_extension());
    let mut session = dir.session_in(Arc::clone(&ext) as specforge_project::SharedRuntime);
    assert!(with_code(&session.project().diagnostics(), "E951").is_empty());

    let update = session.hold(&[Buffer::new(
        dir.root().join("a.spec"),
        "gadget good \"Good\" {\n}\n\ngadget bad_two \"Bad\" {\n}\n",
    )]);

    let audit = with_code(&update.diagnostics, "E951");
    assert_eq!(audit.len(), 1, "{:?}", update.diagnostics);
    assert_eq!(audit[0].message, "gadget 'bad_two' fails the audit");
    // The session reports what a fresh compile of the same sources does.
    fs::write(
        dir.root().join("a.spec"),
        "gadget good \"Good\" {\n}\n\ngadget bad_two \"Bad\" {\n}\n",
    )
    .unwrap();
    let fresh = dir.compile_in(ext.clone()).diagnostics();
    assert_eq!(as_set(&update.diagnostics), as_set(&fresh));
}

/// Diagnostics as their sorted full JSON: order-independent.
fn as_set(diagnostics: &[Diagnostic]) -> Vec<String> {
    let mut set: Vec<String> = diagnostics
        .iter()
        .map(|d| serde_json::to_string(d).unwrap())
        .collect();
    set.sort();
    set
}

#[specforge_test(
    behavior = "run_check_phase_passes",
    verify = "a pass without the check phase runs only under analyze"
)]
fn a_pass_without_the_check_phase_runs_only_under_analyze() {
    let dir = project(SPEC);
    let ext = Arc::new(passes_extension());

    let compiled = dir.compile_in(ext.clone());
    assert!(
        !called(&ext).contains(&"__pass_report".to_string()),
        "{:?}",
        called(&ext)
    );
    assert!(with_code(&compiled.diagnostics(), "I950").is_empty());

    ext.clear_calls();
    let registries = compiled.environment().registries();
    let ctx = specforge_project::passes::AnalysisContext {
        graph: compiled.graph(),
        kind_registry: registries.kinds(),
        field_registry: registries.fields(),
        entities: compiled.entities(),
        test_results: None,
        proved_claims: None,
    };
    let reports = specforge_project::passes::run_extension_passes(
        registries.analyze_passes(),
        &ctx,
        ext.as_ref(),
    );
    let names: Vec<&str> = reports.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["@test/passes:report"], "analyze skips check passes");
    assert_eq!(reports[0].findings.len(), 2);
    assert_eq!(called(&ext), ["__pass_report"]);
}

#[specforge_test(
    behavior = "run_check_phase_passes",
    verify = "check passes run in their declared after/before order"
)]
fn check_passes_run_in_their_declared_order() {
    let dir = project(SPEC);
    let ext = Arc::new(passes_extension());

    dir.compile_in(ext.clone());

    // `second` is declared first but runs after `first`; the others keep
    // their declaration order.
    assert_eq!(
        called(&ext),
        [
            "__pass_audit",
            "__pass_boom",
            "__pass_first",
            "__pass_second"
        ]
    );
}

#[specforge_test(
    behavior = "run_check_phase_passes",
    verify = "a trapping check pass is a diagnostic, not a crash"
)]
fn a_trapping_check_pass_is_a_diagnostic() {
    let dir = project(SPEC);
    let ext = Arc::new(passes_extension());

    let diagnostics = dir.compile_in(ext.clone()).diagnostics();

    let failures: Vec<&Diagnostic> = with_code(&diagnostics, "E028")
        .into_iter()
        .filter(|d| {
            d.message
                .starts_with("compiler pass __pass_boom() of '@test/passes' trapped: ")
        })
        .collect();
    assert_eq!(failures.len(), 1, "{diagnostics:?}");
    assert_eq!(failures[0].severity, Severity::Error);
    assert!(
        failures[0].message.contains("unreachable"),
        "{}",
        failures[0].message
    );
    // The passes after it still ran.
    assert!(called(&ext).contains(&"__pass_second".to_string()));
}

#[specforge_test(
    behavior = "run_check_phase_passes",
    verify = "a pass diagnostic naming an entity gets that entity's span"
)]
fn a_pass_diagnostic_naming_an_entity_gets_its_span() {
    let dir = project(SPEC);
    let ext = Arc::new(passes_extension());

    let compiled = dir.compile_in(ext.clone());

    let diagnostics = compiled.diagnostics();
    let audit = with_code(&diagnostics, "E951");
    let entity_span = compiled
        .graph()
        .node("bad_one")
        .unwrap()
        .source_span
        .clone();
    assert_eq!(audit[0].span.as_ref(), Some(&entity_span));
    assert_eq!(entity_span.file.as_str(), "a.spec");
    // A diagnostic that names no entity keeps no span.
    assert_eq!(with_code(&diagnostics, "W952")[0].span, None);
}

// ── the build cache, read ──────────────────────────────────────────────────

pub(crate) fn write_cache(dir: &TestProject, text: &str) {
    dir.write(specforge_project::BUILD_CACHE_FILE, text);
}

#[specforge_test(
    behavior = "read_build_cache",
    verify = "check passes receive the cached statuses as previous"
)]
fn check_passes_receive_the_cached_statuses() {
    let dir = project(SPEC);
    write_cache(
        &dir,
        r#"{"format": 1, "statuses": {
            "good": {"kind": "gadget", "status": "proposed"},
            "gone": {"kind": "gadget", "status": "done"}
        }}"#,
    );
    let ext = Arc::new(passes_extension());

    let diagnostics = dir.compile_in(ext.clone()).diagnostics();

    let expected = json!({
        "statuses": {
            "gone": {"kind": "gadget", "status": "done"},
            "good": {"kind": "gadget", "status": "proposed"}
        }
    });
    for pass in ["__pass_audit", "__pass_first", "__pass_second"] {
        assert_eq!(last_input(&ext, pass)["previous"], expected, "{pass}");
    }
    assert!(
        with_code(&diagnostics, "W144").is_empty(),
        "{diagnostics:?}"
    );

    // A session reads it too, and sees a cache written after it opened.
    let ext = Arc::new(passes_extension());
    let mut session = dir.session_in(Arc::clone(&ext) as specforge_project::SharedRuntime);
    assert_eq!(last_input(&ext, "__pass_audit")["previous"], expected);
    write_cache(
        &dir,
        r#"{"format": 1, "statuses": {"good": {"kind": "gadget", "status": "active"}}}"#,
    );
    let cache = dir.root().join(specforge_project::BUILD_CACHE_FILE);
    session
        .apply(&session.changes([cache.as_path()]))
        .expect("the cache is a check input");
    assert_eq!(
        last_input(&ext, "__pass_audit")["previous"]["statuses"]["good"]["status"],
        "active"
    );
}

#[specforge_test(
    behavior = "read_build_cache",
    verify = "without a cache file previous is absent"
)]
fn without_a_cache_previous_is_absent() {
    let dir = project(SPEC);
    let ext = Arc::new(passes_extension());

    let diagnostics = dir.compile_in(ext.clone()).diagnostics();

    let input = last_input(&ext, "__pass_audit");
    assert!(input.get("previous").is_none(), "{input}");
    assert!(
        with_code(&diagnostics, "W144").is_empty(),
        "{diagnostics:?}"
    );
}

#[specforge_test(
    behavior = "read_build_cache",
    verify = "an invalid cache file is W144 and previous is absent"
)]
fn an_invalid_cache_is_w144() {
    for (text, problem) in [
        ("{ not json", "does not parse"),
        (r#"{"statuses": {}}"#, "does not parse"),
        (r#"{"format": 2, "statuses": {}}"#, "declares format 2"),
    ] {
        let dir = project(SPEC);
        write_cache(&dir, text);
        let ext = Arc::new(passes_extension());

        let diagnostics = dir.compile_in(ext.clone()).diagnostics();

        let warnings = with_code(&diagnostics, "W144");
        assert_eq!(warnings.len(), 1, "{text}: {diagnostics:?}");
        assert_eq!(warnings[0].severity, Severity::Warning);
        assert!(
            warnings[0].message.contains(problem),
            "{}",
            warnings[0].message
        );
        assert!(last_input(&ext, "__pass_audit").get("previous").is_none());
        // The passes still ran.
        assert_eq!(with_code(&diagnostics, "E951").len(), 1);
    }

    // With no check pass loaded, the file is not read.
    let dir = TestProject::new().without_config().file("a.spec", SPEC);
    write_cache(&dir, "{ not json");
    let diagnostics = dir.compile_bare().diagnostics();
    assert!(
        with_code(&diagnostics, "W144").is_empty(),
        "{diagnostics:?}"
    );
}

/// What the coverage rule needs to read from each entity, read the way a
/// guest built with the SDK reads it: through the SDK's own `PassEntity`.
#[specforge_test(
    behavior = "run_check_phase_passes",
    verify = "the pass input carries each entity's exemption, which the SDK's PassEntity reads"
)]
fn the_pass_input_carries_each_entitys_exemption() {
    use specforge_extension_sdk::prelude::*;
    use specforge_wasm::testing::InProcessRuntime;
    use std::cell::RefCell;

    thread_local! {
        static SEEN: RefCell<Vec<(String, bool, bool)>> = const { RefCell::new(Vec::new()) };
    }
    fn extension() -> ContributionsBuilder {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new("@test/exempt", "1.0.0"));
        c.kind("gadget", |k| {
            k.keyword("gadget").testable(true).supports_verify(true);
            k.field("abstract", |f| {
                f.field_type(FieldType::Bool).exempts_obligations();
            });
        });
        // Only gadgets owe obligations: a widget is exempt by its kind.
        c.kind("widget", |k| {
            k.keyword("widget").testable(true);
        });
        c.rule("W990", |r| {
            r.check(CheckKind::NoVerifyStatements)
                .target_kind("gadget")
                .message_template("gadget '{id}' declares no obligations");
        });
        // The pass reads the SDK's PassInput: what it sees is recorded.
        c.pass("exempt", |p| {
            p.phase("check").run(|input: &PassInput| {
                SEEN.with(|seen| {
                    seen.borrow_mut().extend(
                        input
                            .entities
                            .iter()
                            .map(|e| (e.id.clone(), e.testable, e.owes_none)),
                    )
                });
                Vec::<PassDiagnostic>::new()
            });
        });
        c
    }

    let dir = TestProject::new()
        .config(|c| c["name"] = "p".into())
        .enabling(&["@test/exempt"])
        .installing_enabled()
        .file(
            "a.spec",
            "gadget owes \"Owes\" {\n  verify unit \"it works\"\n}\n\n\
             gadget free \"Free\" {\n  abstract true\n}\n\nwidget w \"W\" {\n}\n",
        );
    let runtime = InProcessRuntime::new().with(extension);

    dir.compile_in(Arc::new(runtime));

    let mut seen = SEEN.with(|seen| seen.borrow().clone());
    seen.sort();
    assert_eq!(
        seen,
        [
            ("free".to_string(), true, true),
            ("owes".to_string(), true, false),
            ("w".to_string(), true, true),
        ]
    );
}

/// A pass finding whose code the extension may not use is kept as given,
/// and one W150 is added per distinct (code, severity).
#[specforge_test(
    behavior = "call_extension_exports",
    verify = "a pass diagnostic whose code the extension may not use is reported (W150) and kept"
)]
fn a_pass_finding_with_a_code_it_may_not_use_is_reported() {
    let output = PassOutput {
        diagnostics: vec![
            PassDiagnostic::new("E001", PassSeverity::Info, "x"),
            PassDiagnostic::new("E001", PassSeverity::Info, "y"),
            // Its own range, at the level its prefix states: not reported.
            PassDiagnostic::new("W950", PassSeverity::Warning, "fine"),
        ],
        summary: Default::default(),
    };

    let findings = specforge_project::passes::pass_findings(
        "@acme/squat",
        "audit",
        output,
        &specforge_project::snapshot::EntitySnapshot::default(),
    );

    let codes: Vec<_> = findings.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["E001", "E001", "W950", "W150"]);
    assert!(findings[..3].iter().all(|d| d.severity != Severity::Error));
    let w150 = &findings[3];
    assert_eq!(w150.severity, Severity::Warning);
    assert!(w150.message.contains("'@acme/squat'"), "{}", w150.message);
    assert!(w150.message.contains("pass 'audit'"), "{}", w150.message);
    assert!(w150.message.contains("'E001'"), "{}", w150.message);
    assert!(w150.message.contains("core owns"), "{}", w150.message);
}
