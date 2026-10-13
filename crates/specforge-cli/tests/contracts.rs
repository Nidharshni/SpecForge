use assert_cmd::Command;
use specforge_test_macros::test as specforge_test;
use std::fs;
use tempfile::TempDir;

fn specforge_cmd() -> Command {
    assert_cmd::cargo_bin_cmd!("specforge")
}

// B:scaffold_new_project — verify contract "requires/ensures consistency for new project scaffolding"
#[specforge_test(
    behavior = "scaffold_new_project",
    verify = "Scaffold New Project: new project scaffolding holds — filesystem_available, no_existing_project, valid_config_created, schema_field_included, project_initialized_emitted"
)]
fn scaffold_new_project_contract() {
    // Requires: empty directory + project name
    // Ensures: specforge.json + spec/ directory created with valid content
    let dir = TempDir::new().unwrap();

    // filesystem_available; project_initialized_emitted: init reports it.
    specforge_cmd()
        .args(["init", "--name", "contract-test"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "Initialized project 'contract-test'",
        ));

    assert_scaffold_contract(dir.path(), "contract-test");
}

/// The scaffold_new_project clauses after `init --name <name>` ran
/// successfully in `dir`, then a second init there.
pub(crate) fn assert_scaffold_contract(dir: &std::path::Path, name: &str) {
    // valid_config_created
    let config_path = dir.join("specforge.json");
    let content = fs::read_to_string(&config_path).unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&content).expect("specforge.json must be valid JSON");
    assert_eq!(json["name"], name, "name must match input");
    assert_eq!(json["version"], "0.1.0");
    assert_eq!(json["extensions"], serde_json::json!([]));
    assert_eq!(json["spec_root"], "spec");
    // schema_field_included
    assert_eq!(
        json["$schema"],
        "https://specforge.dev/schema/specforge.json"
    );
    // The spec root it names exists with the starter file.
    assert!(dir.join("spec").join("hello.spec").is_file());

    // no_existing_project: a second init in the same directory fails and
    // leaves the existing configuration alone.
    let edited = content.replace("0.1.0", "9.9.9");
    fs::write(&config_path, &edited).unwrap();
    specforge_cmd()
        .args(["init", "--name", "second"])
        .current_dir(dir)
        .assert()
        .failure()
        .stderr(predicates::str::contains("already"));
    assert_eq!(fs::read_to_string(&config_path).unwrap(), edited);
}

// B:non_interactive_init — verify contract "requires/ensures consistency for non-interactive init"
#[specforge_test(
    behavior = "non_interactive_init",
    verify = "Non-Interactive Init: non-interactive init holds — name_flag_provided, filesystem_available, no_existing_project, config_identical_to_interactive, all_prompts_skipped, json_output_supported, project_initialized_emitted"
)]
fn non_interactive_init_contract() {
    // Requires: --name flag provided (no interactive prompts)
    // Ensures: project created without requiring stdin input
    let dir = TempDir::new().unwrap();

    let output = specforge_cmd()
        .args(["init", "--name", "ci-contract"])
        .current_dir(dir.path())
        .write_stdin("") // empty stdin — must not hang
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "must succeed without interactive input"
    );
    assert!(
        dir.path().join("specforge.json").exists(),
        "config must be created"
    );

    let content = fs::read_to_string(dir.path().join("specforge.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(json["name"], "ci-contract");
}

// B:graceful_zero_extension_init — verify contract "requires/ensures consistency for zero-extension init"
#[specforge_test(
    behavior = "graceful_zero_extension_init",
    verify = "Graceful Zero-Extension Init: zero-extension init holds — zero_extensions_selected, filesystem_available, empty_extensions_list, structural_starter_valid, valid_graph_exportable, project_initialized_emitted"
)]
fn graceful_zero_extension_init_contract() {
    // Requires: init with no --extensions flag
    // Ensures: specforge.json has empty extensions array, project still functional
    let dir = TempDir::new().unwrap();

    specforge_cmd()
        .args(["init", "--name", "zero-ext-contract"])
        .current_dir(dir.path())
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join("specforge.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(
        json["extensions"],
        serde_json::json!([]),
        "extensions must be empty array"
    );

    // Project must still be functional: check + export succeed
    specforge_cmd()
        .args(["check", "--format", "json"])
        .arg("--path")
        .arg(dir.path().join("spec"))
        .assert()
        .success();

    let export_output = specforge_cmd()
        .args(["export", "--format", "graph"])
        .arg("--path")
        .arg(dir.path().join("spec"))
        .output()
        .unwrap();
    assert!(
        export_output.status.success(),
        "export must succeed with zero extensions"
    );
    let stdout = String::from_utf8_lossy(&export_output.stdout);
    let graph: serde_json::Value =
        serde_json::from_str(&stdout).expect("export must produce valid JSON");
    assert!(graph["nodes"].is_array(), "graph must have nodes array");
}
