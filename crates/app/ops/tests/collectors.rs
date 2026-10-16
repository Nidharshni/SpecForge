//! The runner extensions' collectors, read through the protocol from
//! their real Wasm blobs and dispatched by the host's collect flow.
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

use specforge_ops::analysis::collect::{collectors, dispatch};
use specforge_protocol_types::ExtensionDeclaration;
use specforge_protocol_types::{CollectReportFile, TestStatus};
use specforge_wasm::CallFailure;
use specforge_wasm::protocol::load_declaration;

/// A Wasm runtime for a temp project enabling `ext_names`.
fn wasm_runtime_for(ext_names: &[&str]) -> specforge_component::ComponentRuntime {
    let runtime = specforge_component::ComponentRuntime::new();
    let names: Vec<String> = ext_names.iter().map(|name| name.to_string()).collect();
    specforge_component::builtins::load_builtins_for(&runtime, &names).unwrap();
    runtime
}

/// An extension's declaration, loaded through the protocol.
fn load_via_protocol(ext_name: &str) -> ExtensionDeclaration {
    let runtime = wasm_runtime_for(&[ext_name]);
    load_declaration(&runtime, ext_name).unwrap().declaration
}

/// A temp project at `dir` holding `source`, with the real blobs of `@specforge/testing` and `ext`
/// loaded (the root holds a project: collect refuses a directory that holds none).
fn project_with(
    ext: &str,
    source: &str,
) -> (tempfile::TempDir, specforge_project::testing::Assembled) {
    let runtime: specforge_project::SharedRuntime =
        std::sync::Arc::new(wasm_runtime_for(&["@specforge/testing", ext]));
    let dir = tempfile::TempDir::new().unwrap();
    let config = r#"{"name": "shop"}"#;
    std::fs::write(dir.path().join("specforge.json"), config).unwrap();
    let project = specforge_project::testing::Assembled::assembling_at(dir.path(), |root| {
        specforge_project::Environment::assemble(
            specforge_common::parse_project_config(&root.join("specforge.json"), config),
            specforge_installed::Installed::unread(root),
            specforge_project::ExtensionLoad::In {
                runtime,
                loaded: specforge_project::Loaded {
                    enabled: Vec::new(),
                    declarations: vec![load_via_protocol(ext)],
                },
            },
        )
    })
    .source(source);
    (dir, project)
}

/// Collect `runner` of `project`, running through `machine`.
fn run_collect(
    project: &specforge_project::testing::Assembled,
    runner: &str,
    machine: &specforge_ops::testing::ScriptedMachine,
) -> specforge_ops::analysis::collect::Outcome {
    use specforge_ops::analysis::collect::{Mode, Request, RunnerOutput, collect};
    use specforge_ops::testing::Viewed;
    collect(
        &project.view(),
        Request {
            runner: Some(runner),
            mode: Mode::Run(RunnerOutput::Discard),
            announce: &mut |_, _| {},
            caller: specforge_ops::testing::caller(),
        },
        machine,
    )
    .unwrap()
}

#[specforge_test_macros::test(
    behavior = "vt_declare_vitest_collector",
    verify = "vitest declares its collector"
)]
fn vitest_declares_its_collector() {
    let manifest = load_via_protocol("@specforge/vitest");
    assert!(
        manifest
            .peers()
            .iter()
            .any(|p| p.name == "@specforge/testing" && !p.optional),
        "testing is a required peer"
    );
    let collectors = collectors(std::slice::from_ref(&manifest));
    assert_eq!(collectors.len(), 1);
    let c = &collectors[0];
    assert_eq!(c.name, "vitest");
    assert_eq!(c.export, "collect__vitest");
    assert_eq!(c.detect, ["vitest.config.*", "vitest.workspace.*"]);
    assert_eq!(&c.run[..3], ["npx", "--no", "vitest"]);
    assert!(c.run.iter().any(|a| a == "--outputFile.json={report}"));
    assert_eq!(c.report, ".specforge/reports/vitest.json");
}

#[specforge_test_macros::test(
    behavior = "ct_declare_cargo_collector",
    verify = "cargo-test declares its collector"
)]
fn cargo_test_declares_its_collector() {
    let manifest = load_via_protocol("@specforge/cargo-test");
    assert!(
        manifest
            .peers()
            .iter()
            .any(|p| p.name == "@specforge/testing" && !p.optional),
        "testing is a required peer"
    );
    let collectors = collectors(std::slice::from_ref(&manifest));
    assert_eq!(collectors.len(), 1);
    let c = &collectors[0];
    assert_eq!(c.name, "cargo-test");
    assert_eq!(c.export, "collect__cargo_test");
    assert_eq!(c.detect, ["Cargo.toml"]);
    assert_eq!(c.run, ["cargo", "test", "--workspace", "--no-fail-fast"]);
    assert_eq!(c.report, "target/specforge");
    assert_eq!(
        c.capture.as_deref(),
        Some("stdout"),
        "plain tests only appear in libtest's output"
    );
}

#[specforge_test_macros::test(
    behavior = "ct_report_unlinked_tests",
    verify = "tests the attribute did not record are reported as unlinked"
)]
fn cargo_test_reports_plain_tests_as_unlinked() {
    let runtime = wasm_runtime_for(&["@specforge/cargo-test"]);
    let manifest = load_via_protocol("@specforge/cargo-test");
    let collector = &collectors(std::slice::from_ref(&manifest))[0];
    let stdout =
        include_str!("../../../../extensions/cargo-test/tests/fixtures/libtest-stdout.txt");
    let report = CollectReportFile {
        path: "target/specforge/shop_lib.json".into(),
        content: r#"{"entries":[{"entity_id":"cart","test_name":"panics",
            "module_path":"shop_lib::tests","file":"src/lib.rs","status":"passed"}]}"#
            .into(),
    };
    let out = dispatch(Some(&runtime), collector, &[report], Some(stdout)).unwrap();
    assert_eq!(out.entity_results.len(), 1, "the attribute's own result");
    let unlinked: Vec<(&str, TestStatus)> = out
        .unlinked
        .iter()
        .map(|t| (t.name.as_str(), t.status))
        .collect();
    assert_eq!(
        unlinked,
        [
            ("tests::plain_ignored", TestStatus::Skipped),
            ("tests::slow_one", TestStatus::Skipped),
            (
                "tests::add_item::rejects_a_duplicate_item",
                TestStatus::Passed
            ),
            ("tests::add_item__rejects_an_empty_name", TestStatus::Passed),
            ("tests::fails", TestStatus::Failed),
            ("cart__is_empty_at_first", TestStatus::Passed),
        ],
        "doc tests, the attribute's test and replayed failure output are left out"
    );
    assert_eq!(
        out.unlinked[2].path,
        ["tests", "add_item", "rejects_a_duplicate_item"]
    );
}

#[specforge_test_macros::test(
    behavior = "management_operations_over_the_project_view",
    verify = "collect maps test results to the entities of its view"
)]
fn collect_maps_results_to_the_views_entities() {
    use specforge_ops::analysis::collect::{Mode, Request, collect};
    use specforge_ops::view::ProjectView;

    let (dir, project) = project_with("@specforge/cargo-test", "behavior a \"A\" {\n}\n");
    let report = dir.path().join("target/specforge/shop.json");
    std::fs::create_dir_all(report.parent().unwrap()).unwrap();
    std::fs::write(
        &report,
        r#"{"entries":[
            {"entity_id":"a","test_name":"proves_a","module_path":"shop::tests","file":"src/lib.rs","status":"passed"},
            {"entity_id":"zz","test_name":"proves_zz","module_path":"shop::tests","file":"src/lib.rs","status":"passed"}
        ]}"#,
    )
    .unwrap();
    let view = ProjectView::of(project.compiled());

    let reports = [report];
    let outcome = collect(
        &view,
        Request {
            runner: Some("cargo-test"),
            mode: Mode::Reports(&reports),
            announce: &mut |_, _| panic!("nothing runs when reading reports"),
            caller: specforge_ops::testing::caller(),
        },
        &specforge_ops::testing::ScriptedMachine::new(),
    )
    .unwrap();

    // Written at the view's root, with the view's entity only.
    assert_eq!(outcome.report, dir.path().join("specforge-report.json"));
    let written: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&outcome.report).unwrap()).unwrap();
    let results: Vec<&str> = written["results"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(results, ["a"], "{written}");
    let w115: Vec<&str> = outcome
        .diagnostics
        .iter()
        .filter(|d| d.code == "W115")
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(w115.len(), 1, "{:?}", outcome.diagnostics);
    assert!(w115[0].contains("'zz'"), "{}", w115[0]);
}

#[specforge_test_macros::test(
    behavior = "ct_map_binary_reports",
    verify = "a report entry whose status is not passed, failed or skipped is the collector's error"
)]
fn cargo_test_refuses_an_entry_with_an_unknown_status() {
    let runtime = wasm_runtime_for(&["@specforge/cargo-test"]);
    let manifest = load_via_protocol("@specforge/cargo-test");
    let collector = &collectors(std::slice::from_ref(&manifest))[0];
    let report = CollectReportFile {
        path: "target/specforge/shop_lib.json".into(),
        content: r#"{"entries":[{"entity_id":"cart","test_name":"flaky","status":"flaky"}]}"#
            .into(),
    };
    let err = dispatch(Some(&runtime), collector, &[report], None).unwrap_err();
    assert!(matches!(err.failure, CallFailure::Trapped { .. }), "{err}");
    assert!(
        err.to_string().contains("target/specforge/shop_lib.json"),
        "{err}"
    );
}

#[specforge_test_macros::test(
    behavior = "vt_map_vitest_report",
    verify = "a linked test whose status vitest's JSON reporter does not write is the collector's error"
)]
fn vitest_refuses_a_linked_test_with_an_unknown_status() {
    let runtime = wasm_runtime_for(&["@specforge/vitest"]);
    let manifest = load_via_protocol("@specforge/vitest");
    let collector = &collectors(std::slice::from_ref(&manifest))[0];
    let report = CollectReportFile {
        path: ".specforge/reports/vitest.json".into(),
        content: r#"{"testResults":[{"assertionResults":[{"fullName":"t","status":"flaky",
            "meta":{"specforge":{"behavior":"a"}}}]}]}"#
            .into(),
    };
    let err = dispatch(Some(&runtime), collector, &[report], None).unwrap_err();
    assert!(matches!(err.failure, CallFailure::Trapped { .. }), "{err}");
    let shown = err.to_string();
    assert!(
        shown.contains("test 't'") && shown.contains("'flaky'"),
        "{shown}"
    );
}

const LOGIN: &str = "behavior login \"Login\" {\n  verify unit \"accepts valid credentials\"\n}\n";

#[specforge_test_macros::test(
    behavior = "resolve_test_conventions",
    verify = "collect links plain tests by naming convention"
)]
fn collect_links_plain_tests_by_naming_convention() {
    use specforge_ops::testing::{ScriptedMachine, ScriptedRun};
    let (dir, project) = project_with("@specforge/cargo-test", LOGIN);
    // Plain `#[test]` functions print libtest's lines and write no report.
    let machine = ScriptedMachine::new()
        .approving_every_command()
        .answering(ScriptedRun {
            stdout: Some(
                "running 3 tests\n\
                 test tests::login__accepts_valid_credentials ... ok\n\
                 test login::rejects_a_locked_account ... FAILED\n\
                 test tests::unrelated ... ok\n"
                    .to_string(),
            ),
            exit_code: Some(101),
            ..ScriptedRun::default()
        });

    let outcome = run_collect(&project, "cargo-test", &machine);

    let runner = &outcome.runners[0];
    assert_eq!(
        (
            runner.stats.passed,
            runner.stats.failed,
            runner.stats.skipped
        ),
        (1, 1, 0)
    );
    assert_eq!((runner.files, runner.by_convention), (0, 2));
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("specforge-report.json")).unwrap())
            .unwrap();
    let tests = &report["results"]["login"]["tests"];
    assert_eq!(tests[0]["name"], "tests::login__accepts_valid_credentials");
    assert_eq!(tests[0]["verify"], "accepts valid credentials");
    assert_eq!(tests[0]["status"], "passed");
    assert_eq!(tests[1]["name"], "login::rejects_a_locked_account");
    assert!(tests[1]["verify"].is_null(), "not an obligation of login");
    assert_eq!(tests[1]["status"], "failed");
    assert!(
        dir.path()
            .join("target/specforge/cargo-test.stdout.txt")
            .exists()
    );
}

#[specforge_test_macros::test(
    behavior = "vt_declare_vitest_collector",
    verify = "collect runs vitest with the report path and maps linked tests"
)]
fn vitest_runs_with_the_report_path_and_maps_linked_tests() {
    use specforge_ops::testing::{ScriptedMachine, ScriptedRun};
    let (dir, project) = project_with("@specforge/vitest", LOGIN);
    let machine = ScriptedMachine::new()
        .approving_every_command()
        .answering(ScriptedRun {
            files: vec![(
                "".into(),
                r#"{"testResults":[{"assertionResults":[
                    {"fullName":"login accepts valid credentials","status":"passed","duration":3,
                     "meta":{"specforge":{"behavior":"login","verify":"accepts valid credentials"}}},
                    {"fullName":"unlinked","status":"passed","meta":{}}]}]}"#
                    .to_string(),
            )],
            exit_code: Some(0),
            ..ScriptedRun::default()
        });

    let outcome = run_collect(&project, "vitest", &machine);

    let argv = &machine.runs()[0].argv;
    assert_eq!(
        argv[..4],
        ["npx", "--no", "vitest", "run"],
        "never downloads vitest: {argv:?}"
    );
    let report_at = dir.path().join(".specforge/reports/vitest.json");
    assert!(
        argv.contains(&format!("--outputFile.json={}", report_at.display())),
        "the report path is expanded: {argv:?}"
    );
    assert_eq!(outcome.runners[0].stats.passed, 1);
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("specforge-report.json")).unwrap())
            .unwrap();
    let test = &report["results"]["login"]["tests"][0];
    assert_eq!(test["verify"], "accepts valid credentials");
    assert_eq!(test["runner"], "vitest");
}
