//! What the host sends the command, collector, scanner and migration-hook
//! exports, compared with the wire goldens in
//! `crates/specforge-wasm/tests/wire/`, and what a collector's failure
//! becomes. These were plan 04's T1 characterization pins; T5 and T7
//! flipped the ones that pinned a bug.

use std::fs;
use std::path::PathBuf;

use serde_json::{Value, json};
use specforge_ops::collect::{Collector, dispatch};
use specforge_ops::migrate::{MigrationInput, invoke_hooks};
use specforge_protocol_types::{CollectReportFile, ExtensionDeclaration, HandshakeResponse};
use specforge_test_macros::test as specforge_test;
use specforge_wasm::testing::InProcessRuntime;
use specforge_wasm::{CallFailure, WasmCallResult, WasmTrapInfo};

const EXT: &str = "@pin/ext";

fn wire_dir() -> PathBuf {
    specforge_repo_root::repo_root().join("crates/specforge-wasm/tests/wire")
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

fn trap(export: &str) -> WasmCallResult {
    WasmCallResult::Trap(WasmTrapInfo {
        kind: "k".to_string(),
        message: "m".to_string(),
        export_name: export.to_string(),
    })
}

fn answering(export: &str, result: WasmCallResult) -> InProcessRuntime {
    InProcessRuntime::new().answer_raw(EXT, export, result)
}

// ── C5 · collector ──

fn collector() -> Collector {
    Collector {
        extension: EXT.to_string(),
        name: "x".to_string(),
        export: "collect__x".to_string(),
        detect: Vec::new(),
        run: Vec::new(),
        report: "report.json".to_string(),
        capture: None,
    }
}

fn reports() -> Vec<CollectReportFile> {
    vec![CollectReportFile {
        path: "target/report.json".to_string(),
        content: "{}".to_string(),
    }]
}

#[specforge_test(
    behavior = "dispatch_collector",
    verify = "the collector receives a CollectInput and answers a CollectOutput, and an answer that is not one is an error naming the collector"
)]
fn a_collector_receives_a_collect_input_and_answers_a_collect_output() {
    let answer = json!({"entity_results": [{"entity_id": "a", "test_results": [
        {"name": "t", "status": "passed", "verify": "v", "duration_ms": 2.0}
    ]}], "unlinked": [{"name": "m::t", "path": ["m", "t"], "status": "failed"}]});
    let runtime = answering(
        "collect__x",
        WasmCallResult::Ok(answer.to_string().into_bytes()),
    );
    let read = dispatch(Some(&runtime), &collector(), &reports(), None).unwrap();
    // flipped in T7: `stdout` is absent when nothing was captured, not null
    golden("collect.input.json", &runtime.calls()[0].input);
    assert_eq!(read.entity_results[0].entity_id, "a");
    let test = &read.entity_results[0].test_results[0];
    assert_eq!(
        (test.name.as_str(), test.verify.as_deref(), test.duration_ms),
        ("t", Some("v"), Some(2.0))
    );
    assert_eq!(read.unlinked[0].path, ["m", "t"]);
    dispatch(Some(&runtime), &collector(), &reports(), Some("out")).unwrap();
    assert_eq!(runtime.calls()[1].input["stdout"], "out");

    // flipped in T7: an answer that is not a CollectOutput (an empty array
    // or object, a test without a name, not JSON) is an error naming the
    // collector, never empty results; a trap is too.
    let missing_name = json!({"entity_results": [{"entity_id": "a", "test_results": [
        {"status": "passed"}
    ]}]});
    let unknown_status = json!({"entity_results": [{"entity_id": "a", "test_results": [
        {"name": "t", "status": "ok"}
    ]}]});
    for raw in [
        b"[]".to_vec(),
        b"{}".to_vec(),
        missing_name.to_string().into_bytes(),
        unknown_status.to_string().into_bytes(),
        b"not json".to_vec(),
    ] {
        let runtime = answering("collect__x", WasmCallResult::Ok(raw.clone()));
        let err = dispatch(Some(&runtime), &collector(), &reports(), None).unwrap_err();
        let shown = String::from_utf8_lossy(&raw);
        assert!(
            matches!(
                err.failure,
                CallFailure::Malformed {
                    expected: "CollectOutput",
                    ..
                }
            ),
            "{shown}: {err}"
        );
        assert!(
            err.to_string().starts_with(&format!(
                "collector collect__x() of '{EXT}' answered output that is not a CollectOutput: "
            )),
            "{err}"
        );
    }
    let runtime = answering("collect__x", trap("collect__x"));
    assert_eq!(
        dispatch(Some(&runtime), &collector(), &reports(), None)
            .unwrap_err()
            .to_string(),
        format!("collector collect__x() of '{EXT}' trapped: k: m")
    );
}

#[specforge_test(
    invariant = "collector_output_conformance",
    verify = "a test status outside passed, failed and skipped is an error naming the collector, never a skipped test"
)]
fn a_status_outside_the_vocabulary_is_not_a_collect_output() {
    let answers = [
        json!({"entity_results": [{"entity_id": "a", "test_results": [
            {"name": "t", "status": "ok"}]}]}),
        json!({"entity_results": [{"entity_id": "a", "test_results": [
            {"name": "t", "status": "Passed"}]}]}),
        json!({"entity_results": [], "unlinked": [
            {"name": "m::t", "path": ["m", "t"], "status": "ignored"}]}),
    ];
    for answer in answers {
        let runtime = answering(
            "collect__x",
            WasmCallResult::Ok(answer.to_string().into_bytes()),
        );
        let err = dispatch(Some(&runtime), &collector(), &reports(), None).unwrap_err();
        assert!(
            matches!(
                err.failure,
                CallFailure::Malformed {
                    expected: "CollectOutput",
                    ..
                }
            ),
            "{answer}: {err}"
        );
        let shown = err.to_string();
        assert!(
            shown.starts_with(&format!(
                "collector collect__x() of '{EXT}' answered output that is not a CollectOutput: "
            )),
            "{shown}"
        );
        assert!(shown.contains("unknown variant"), "{shown}");
    }
}

// ── C8 · migration hook ──

fn hooked() -> specforge_registry::RegistryBuild {
    specforge_registry::build_registries(vec![ExtensionDeclaration {
        handshake: HandshakeResponse {
            name: EXT.into(),
            version: "1.0.0".into(),
            migration_hook: Some("migrate__x".into()),
            ..HandshakeResponse::default()
        },
        ..ExtensionDeclaration::default()
    }])
}

fn hook_input() -> MigrationInput {
    MigrationInput {
        from: "0.9".into(),
        to: "1.0".into(),
        files: vec!["old.spec".into()],
    }
}

#[specforge_test(
    behavior = "call_extension_exports",
    verify = "every extension call encodes its input as the protocol type the SDK decodes"
)]
fn the_migration_hook_input_and_any_answer() {
    for answer in [&b"garbage"[..], b"", b"{}"] {
        let runtime = answering("migrate__x", WasmCallResult::Ok(answer.to_vec()));
        let (invoked, failures) = invoke_hooks(&hooked(), &runtime, &hook_input());
        assert_eq!(invoked, [format!("{EXT}:migrate__x")]);
        assert!(failures.is_empty());
        golden("migrate.input.json", &runtime.calls()[0].input);
    }
    let runtime = answering("migrate__x", trap("migrate__x"));
    let (invoked, failures) = invoke_hooks(&hooked(), &runtime, &hook_input());
    assert!(invoked.is_empty());
    assert_eq!(
        failures,
        [format!(
            "migration hook migrate__x() of '{EXT}' trapped: k: m"
        )]
    );
}

mod evidence {
    use crate::view_support::{Project, registries};
    use serde_json::Map;
    use specforge_ops::command::{CommandFormat, ExtensionCommand, run};
    use specforge_protocol_types::{
        CommandDescriptor, CommandEvidence, CommandOutput, EntityEvidence,
    };
    use specforge_test_macros::test as specforge_test;
    use specforge_wasm::WasmCallResult;
    use specforge_wasm::testing::InProcessRuntime;

    /// A runtime answering the probe command.
    fn probe_runtime() -> std::sync::Arc<InProcessRuntime> {
        std::sync::Arc::new(
            InProcessRuntime::new().answer_raw(
                "@pin/ext",
                "cmd__probe",
                WasmCallResult::Ok(
                    CommandOutput {
                        exit_code: 0,
                        stdout: "{}".into(),
                        stderr: String::new(),
                    }
                    .to_bytes(),
                ),
            ),
        )
    }

    /// The evidence `project`'s command run sends the export: the `evidence`
    /// of the input it received (`None`: the input carries none).
    fn sent(project: &Project, runtime: &InProcessRuntime) -> CommandEvidence {
        let command = ExtensionCommand::new(
            "@pin/ext",
            "pin",
            &CommandDescriptor {
                id: "probe".into(),
                title: "Probe".into(),
                description: String::new(),
                category: None,
                export: "cmd__probe".into(),
                args: Vec::new(),
            },
        );
        run(&project.view(), &command, &Map::new(), CommandFormat::Json)
            .expect("the export answers");
        let input = runtime
            .calls()
            .last()
            .expect("the export was called")
            .input
            .clone();
        match input.get("evidence") {
            // Absent, not null (ADR 0013 D7).
            None => CommandEvidence::None,
            Some(evidence) => serde_json::from_value(evidence.clone()).unwrap(),
        }
    }

    const SOURCE: &str = "behavior proven \"Proven\" {\n  verify unit \"a\"\n  verify unit \"b\"\n}\n\
                          behavior half \"Half\" {\n  verify unit \"a\"\n  verify unit \"b\"\n}\n";

    #[specforge_test(
        behavior = "dispatch_surface_command",
        verify = "the CommandInput carries what the recorded tests prove, per entity that counts toward coverage"
    )]
    fn the_input_carries_each_counted_entitys_proof() {
        let runtime = probe_runtime();
        let project = Project::in_runtime(SOURCE, registries(&["behavior"], &[]), runtime.clone());
        let pass = |verify: &str| serde_json::json!({"name": verify, "status": "passed", "verify": verify});
        std::fs::write(
            project.dir.path().join("specforge-report.json"),
            serde_json::json!({"runner": "fixture", "results": {
                "proven": {"tests": [pass("a"), pass("b")]},
                "half": {"tests": [pass("a")]},
            }})
            .to_string(),
        )
        .unwrap();
        let CommandEvidence::Recorded { entities } = sent(&project, &runtime) else {
            panic!("a recorded report is evidence");
        };
        assert_eq!(
            entities.get("proven"),
            Some(&EntityEvidence {
                obligations: 2,
                proven: 2,
                failing: 0
            })
        );
        assert!(entities["proven"].is_proven());
        assert_eq!(
            entities.get("half"),
            Some(&EntityEvidence {
                obligations: 2,
                proven: 1,
                failing: 0
            })
        );
        assert!(!entities["half"].is_proven());
        assert_eq!(entities.len(), 2, "only what counts toward coverage");
    }

    #[specforge_test(
        behavior = "dispatch_surface_command",
        verify = "a command's input says when the recorded test report cannot be read, and carries no evidence without one"
    )]
    fn no_report_is_no_evidence_and_a_broken_one_says_why() {
        let runtime = probe_runtime();
        let project = Project::in_runtime(SOURCE, registries(&["behavior"], &[]), runtime.clone());
        assert_eq!(sent(&project, &runtime), CommandEvidence::None);
        std::fs::write(
            project.dir.path().join("specforge-report.json"),
            "{not json",
        )
        .unwrap();
        let CommandEvidence::Unreadable { reason } = sent(&project, &runtime) else {
            panic!("a broken report is unreadable evidence");
        };
        assert!(reason.contains("invalid test results"), "{reason}");
        let wire = serde_json::to_value(CommandEvidence::Unreadable { reason }).unwrap();
        assert_eq!(wire["state"], "unreadable");
    }
}

/// An operation over a view calls extensions in the view's runtime.
mod the_views_runtime {
    use std::sync::Arc;

    use serde_json::Map;
    use specforge_extension_sdk::prelude::*;
    use specforge_ops::analyze::{AnalyzeOptions, analyze};
    use specforge_ops::command::{CommandFormat, ExtensionCommands, RunError, run};
    use specforge_ops::view::ProjectView;
    use specforge_project::SharedRuntime;
    use specforge_project::testing::compile_at;
    use specforge_test_macros::test as specforge_test;
    use specforge_wasm::CallFailure;
    use specforge_wasm::testing::InProcessRuntime;
    use tempfile::TempDir;

    /// `@t/x`: an analyze pass `scan` and a command `hello`.
    fn extension() -> InProcessRuntime {
        InProcessRuntime::new().with(|| {
            let mut c = ContributionsBuilder::new(ExtensionMeta::new("@t/x", "1.0.0"));
            c.pass("scan", |p| {
                p.run(|_: &PassInput| Vec::<PassDiagnostic>::new());
            });
            c.command("hello", |cmd| {
                cmd.title("Hello").handler(|_| CommandOutput {
                    exit_code: 0,
                    stdout: "{}\n".into(),
                    stderr: String::new(),
                });
            });
            c
        })
    }

    /// A project enabling `@t/x`, with one `main.spec`.
    fn project() -> TempDir {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("specforge.json"),
            r#"{"name":"p","version":"0.1.0","extensions":["@t/x"]}"#,
        )
        .unwrap();
        specforge_installed::testing::install(dir.path(), &["@t/x"]);
        std::fs::write(dir.path().join("main.spec"), "behavior a \"A\" {\n}\n").unwrap();
        dir
    }

    #[specforge_test(
        invariant = "extensions_run_in_their_loading_runtime",
        verify = "an operation over a project view calls its extensions in the runtime the view's environment loaded them in"
    )]
    fn operations_over_a_view_call_extensions_in_its_runtime() {
        let dir = project();
        let rt = Arc::new(extension());
        let shared: SharedRuntime = rt.clone();
        let compiled = compile_at(dir.path(), Some(shared));
        let view = ProjectView::of(&compiled);

        let outcome = analyze(&view, &AnalyzeOptions::default()).unwrap();
        assert!(
            outcome.passes.iter().any(|p| p.name == "@t/x:scan"),
            "{:?}",
            outcome.passes.iter().map(|p| &p.name).collect::<Vec<_>>()
        );
        let commands = ExtensionCommands::build(view.registries());
        let hello = commands
            .all()
            .iter()
            .find(|c| c.id() == "hello")
            .expect("the extension declares `hello`");
        run(&view, hello, &Map::new(), CommandFormat::Json).expect("the export answers");

        let exports: Vec<String> = rt.calls().into_iter().map(|c| c.export).collect();
        assert!(exports.contains(&"__pass_scan".to_string()), "{exports:?}");
        assert!(exports.contains(&"cmd__hello".to_string()), "{exports:?}");
    }

    #[test]
    fn without_a_runtime_an_extension_call_is_not_loaded() {
        let dir = project();
        // Compiled with a runtime only to learn the command the project's
        // extension declares; the environment under test has none.
        let declared = compile_at(dir.path(), Some(Arc::new(extension())));
        let commands = ExtensionCommands::build(ProjectView::of(&declared).registries());
        let hello = commands.all().iter().find(|c| c.id() == "hello").unwrap();

        let bare = compile_at(dir.path(), None);
        let view = ProjectView::of(&bare);
        let RunError::Call(error) =
            run(&view, hello, &Map::new(), CommandFormat::Json).unwrap_err()
        else {
            panic!("a command over an environment with no runtime is a failed call");
        };
        assert_eq!(error.failure, CallFailure::NotLoaded);
        assert_eq!(error.diagnostic().code, "E028");

        let outcome = analyze(&view, &AnalyzeOptions::default()).unwrap();
        assert!(
            !outcome.passes.iter().any(|p| p.name.starts_with("@t/x:")),
            "no extension pass runs without a runtime"
        );
    }
}
