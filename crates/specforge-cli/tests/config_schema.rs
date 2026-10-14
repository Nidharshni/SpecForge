//! C6-13 acceptance: the shipped config JSON schema covers every user-facing
//! config key, and the two shipped copies never drift.

use crate::support::{Cli, TestProject};
use specforge_repo_root::repo_root;

fn schema_path() -> std::path::PathBuf {
    repo_root().join("schema/specforge.schema.json")
}

#[test]
fn shipped_schema_copies_are_identical() {
    let main = std::fs::read_to_string(schema_path()).unwrap();
    let vscode = std::fs::read_to_string(
        repo_root().join("integrations/vscode/schemas/specforge.schema.json"),
    )
    .unwrap();
    assert_eq!(
        main, vscode,
        "schema/specforge.schema.json and integrations/vscode/schemas/specforge.schema.json drifted; edit both together"
    );
}

#[test]
fn schema_covers_project_config_keys() {
    let schema = schema_path().to_string_lossy().into_owned();
    let doc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&schema).unwrap())
        .expect("config schema is valid JSON");
    let props = doc["properties"].as_object().expect("properties object");

    // Typed on ProjectConfig and user-facing (C6-13: these were formally
    // invalid under additionalProperties:false before the fix).
    for key in ["inference", "registries", "exclude"] {
        assert!(props.contains_key(key), "config schema is missing '{key}'");
    }
    let inference = &props["inference"];
    assert!(
        inference["properties"]["global"].is_object()
            && inference["properties"]["kinds"].is_object()
            && inference["properties"]["density_threshold"].is_object(),
        "inference def must mirror InferenceConfig (global, kinds, density_threshold)"
    );

    // The registry entry shape mirrors RegistryConfig.
    let registry = &doc["$defs"]["registry"];
    for field in ["alias", "url", "scope_filter", "default_registry"] {
        assert!(
            registry["properties"].get(field).is_some(),
            "registry def is missing RegistryConfig field '{field}'"
        );
    }
}

#[test]
fn specforge_json_with_inference_and_registries_loads() {
    // End-to-end: the compiler consumes both keys without treating them as
    // unknown config (they parse into ProjectConfig / registry client).
    let project = TestProject::new()
        .raw_config(
            r#"{
            "name": "parity",
            "spec_root": "src",
            "extensions": [],
            "inference": { "density_threshold": 0.5, "kinds": { "widget": "src/" } },
            "registries": [ { "alias": "local", "url": "http://127.0.0.1:1", "default_registry": true } ]
        }"#,
        )
        .file("src/a.spec", "type Widget {\n  id string @unique\n}\n");

    let src = project.root().join("src");
    let out = Cli::on(&project)
        .bare_command(&["check", "--path", src.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "config with inference + registries must compile: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Nothing reads a `coverage` key, in specforge.json or in the `spec`
/// block: the proof gate is `specforge analyze coverage --min N`, and the
/// report path is the project's specforge-report.json (or
/// `--test-results`). The schema and the docs must not offer one.
#[test]
fn no_unread_coverage_config_is_documented() {
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(schema_path()).unwrap()).unwrap();
    assert!(
        doc["properties"].get("coverage").is_none(),
        "properties.coverage"
    );
    assert!(doc["$defs"].get("coverage").is_none(), "$defs.coverage");
    assert!(
        !doc["$comment"]
            .as_str()
            .unwrap_or_default()
            .contains("coverage"),
        "$comment lists coverage as consumed"
    );

    for doc in ["docs/entities/spec.md", "docs/quick-reference.md"] {
        let text = std::fs::read_to_string(repo_root().join(doc)).unwrap();
        for key in [
            "coverage {",
            "require_violation_tests",
            "fail_on_unknown_ids",
            "| `coverage` |",
            "surface, coverage",
        ] {
            assert!(!text.contains(key), "{doc} documents `{key}`");
        }
    }
}

/// Nothing reads a `test_dirs` key: each collector extension knows where
/// its runner's tests are. The schema and the docs must not offer one.
#[test]
fn no_unread_test_dirs_config_is_documented() {
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(schema_path()).unwrap()).unwrap();
    assert!(
        doc["properties"].get("test_dirs").is_none(),
        "properties.test_dirs"
    );
    assert!(
        !doc["$comment"]
            .as_str()
            .unwrap_or_default()
            .contains("test_dirs"),
        "$comment lists test_dirs as consumed"
    );
    for doc in [
        "docs/entities/spec.md",
        "docs/quick-reference.md",
        "spec/types/config.spec",
    ] {
        let text = std::fs::read_to_string(repo_root().join(doc)).unwrap();
        assert!(!text.contains("test_dirs"), "{doc} documents `test_dirs`");
    }
}
