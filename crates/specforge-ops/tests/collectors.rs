//! The runner extensions' collectors, read through the protocol from
//! their real Wasm blobs and dispatched by the host's collect flow.

use specforge_ops::collect::{collectors, dispatch};
use specforge_protocol_types::CollectReportFile;
use specforge_protocol_types::ExtensionDeclaration;
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
    let stdout = include_str!("../../../extensions/cargo-test/tests/fixtures/libtest-stdout.txt");
    let report = CollectReportFile {
        path: "target/specforge/shop_lib.json".into(),
        content: r#"{"entries":[{"entity_id":"cart","test_name":"panics",
            "module_path":"shop_lib::tests","file":"src/lib.rs","status":"pass"}]}"#
            .into(),
    };
    let out = dispatch(Some(&runtime), collector, &[report], Some(stdout)).unwrap();
    assert_eq!(out.entity_results.len(), 1, "the attribute's own result");
    let unlinked: Vec<(&str, &str)> = out
        .unlinked
        .iter()
        .map(|t| (t.name.as_str(), t.status.as_str()))
        .collect();
    assert_eq!(
        unlinked,
        [
            ("tests::plain_ignored", "skipped"),
            ("tests::slow_one", "skipped"),
            ("tests::add_item::rejects_a_duplicate_item", "passed"),
            ("tests::add_item__rejects_an_empty_name", "passed"),
            ("tests::fails", "failed"),
            ("cart__is_empty_at_first", "passed"),
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
    use specforge_ops::collect::{Consent, Mode, Request, collect};
    use specforge_ops::view::ProjectView;

    let runtime: specforge_project::SharedRuntime = std::sync::Arc::new(wasm_runtime_for(&[
        "@specforge/testing",
        "@specforge/cargo-test",
    ]));
    let env = specforge_project::Environment::with_registries(
        specforge_registry::build_registries(vec![load_via_protocol("@specforge/cargo-test")]),
        Some(runtime),
    );
    let (graph, _) = specforge_graph::build_graph(&[specforge_parser::parse(
        "behavior a \"A\" {\n}\n",
        "main.spec",
    )]);
    let dir = tempfile::TempDir::new().unwrap();
    // The root holds a project: collect refuses a directory that holds none.
    std::fs::write(dir.path().join("specforge.json"), r#"{"name": "shop"}"#).unwrap();
    let report = dir.path().join("target/specforge/shop.json");
    std::fs::create_dir_all(report.parent().unwrap()).unwrap();
    std::fs::write(
        &report,
        r#"{"entries":[
            {"entity_id":"a","test_name":"proves_a","module_path":"shop::tests","file":"src/lib.rs","status":"pass"},
            {"entity_id":"zz","test_name":"proves_zz","module_path":"shop::tests","file":"src/lib.rs","status":"pass"}
        ]}"#,
    )
    .unwrap();
    let recorded = specforge_project::coverage::RecordedCoverage::over(&graph, &env);
    let view = ProjectView::new(&graph, &env, Some(dir.path()), &recorded);

    let reports = [report];
    let outcome = collect(
        &view,
        Request {
            runner: Some("cargo-test"),
            mode: Mode::Reports(&reports),
            consent: Consent::Approved,
            announce: &mut |_, _| panic!("nothing runs when reading reports"),
        },
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
