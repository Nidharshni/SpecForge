//! What a command writes on stderr beside its document (ADR 0120): under `--format json` a fact the document
//! carries is said once, in the document; a notice no document carries stays on stderr.

use crate::support::{Cli, TestProject};

/// A project whose registries are `registries`.
fn with_registries(registries: serde_json::Value) -> TestProject {
    TestProject::new().config(|c| {
        c["name"] = "demo".into();
        c["registries"] = registries;
    })
}

/// A project with the software extension and one behavior.
fn software() -> TestProject {
    TestProject::new()
        .raw_config(
            r#"{"name":"t","version":"0.1.0","spec_root":"spec","extensions":["@specforge/software"]}"#,
        )
        .file(
            "spec/main.spec",
            "behavior parse_input \"Parse\" {\n  contract \"x\"\n}\n",
        )
}

/// A project with no extension and a `spec/main.spec` of `source`.
fn bare(source: &str) -> TestProject {
    TestProject::new()
        .raw_config(r#"{"name":"t","version":"0.1.0","spec_root":"spec","extensions":[]}"#)
        .file("spec/main.spec", source)
}

#[test]
#[ignore = "flipped by T5"]
fn analyze_writes_a_stray_record_only_in_its_document() {
    let p = TestProject::new()
        .raw_config(
            r#"{"name":"cov","spec_root":"src","extensions":["@specforge/software","@specforge/testing"]}"#,
        )
        .file(
            "src/a.spec",
            "type widget \"Widget\" {\n  id string @unique\n  verify unit \"widget valid\"\n}\n",
        )
        .file(
            "specforge-report.json",
            r#"{"runner":"r","results":{"widget":{"tests":[{"name":"w","status":"passed"}]},"wodget":{"tests":[{"name":"x","status":"passed"}]}}}"#,
        );

    let doc = Cli::on(&p).document(&["analyze", "coverage"]);

    assert_eq!(
        doc.json["stray_records"],
        serde_json::json!([{"entity_id": "wodget", "near": "widget"}]),
        "{}",
        doc.json
    );
    assert_eq!(doc.code, 0);
    assert_eq!(doc.stderr, "");
}

#[test]
#[ignore = "flipped by T6"]
fn check_writes_no_note_beside_its_document() {
    let failing = bare("behavior parse_input \"A\" {\n}\nbehavior parse_input \"B\" {\n}\n");
    let run = Cli::on(&failing).run(&["check", "--cache", "--format", "json"]);
    assert_eq!(run.code, 1, "{}", run.stderr);
    assert_eq!(run.stderr, "");
    assert!(!failing.root().join("specforge-cache.json").exists());

    let passing = bare("behavior parse_input \"A\" {\n}\n");
    let run = Cli::on(&passing).run(&["check", "--lint", "pedantic", "--format", "json"]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert_eq!(run.stderr, "");
}

#[test]
#[ignore = "flipped by T7"]
fn search_reports_a_failed_registry_once() {
    let p = with_registries(serde_json::json!([
        {"alias": "down", "url": "http://127.0.0.1:9", "default_registry": true}
    ]));

    let run = Cli::on(&p).run(&["search", "x", "--format", "json"]);

    assert_eq!(run.code, 1, "{}", run.stderr);
    let json = run.json();
    let failures = json["diagnostics"].as_array().unwrap();
    assert_eq!(failures.len(), 1, "{json}");
    assert_eq!(failures[0]["code"], "R005", "{json}");
    let both = format!("{}{}", run.stdout, run.stderr);
    assert_eq!(
        both.matches("search failed on registry 'down'").count(),
        1,
        "{both}"
    );
}

#[test]
fn search_keeps_the_registry_configuration_beside_its_document() {
    let p = with_registries(serde_json::json!([
        {"alias": "main", "url": "http://127.0.0.1:9", "default_registry": true},
        {"alias": "main", "url": "http://127.0.0.1:9"}
    ]));

    let run = Cli::on(&p).run(&["search", "x", "--format", "json"]);

    assert!(
        run.stderr
            .contains("warning[W140]: Duplicate registry alias \"main\""),
        "{}",
        run.stderr
    );
    assert!(!run.stdout.contains("W140"), "{}", run.stdout);
}

#[test]
#[ignore = "flipped by T8"]
fn explore_writes_its_notices_only_in_its_document() {
    let p = software();

    let doc = Cli::on(&p).document(&["explore", "--kind", "behaviour"]);

    assert_eq!(doc.json["notices"][0]["code"], "I020", "{}", doc.json);
    assert_eq!(doc.stderr, "");
}

#[test]
fn a_notice_no_document_carries_stays_beside_it() {
    let p = software();

    let run = Cli::on(&p).run(&["model", "--kinds", "behaviour", "--format", "json"]);

    assert!(
        run.stderr
            .contains("info[I020]: unknown entity kind 'behaviour'"),
        "{}",
        run.stderr
    );
    assert!(run.json().get("notices").is_none(), "{}", run.stdout);
}
