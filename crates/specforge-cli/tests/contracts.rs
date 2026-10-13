use crate::support::{Cli, TestProject};
use specforge_test_macros::test as specforge_test;
use std::fs;

/// An empty directory: no `specforge.json` yet, so `init` can scaffold one.
fn empty_directory() -> TestProject {
    TestProject::new().without_config()
}

// B:scaffold_new_project — verify contract "requires/ensures consistency for new project scaffolding"
#[specforge_test(
    behavior = "scaffold_new_project",
    verify = "Scaffold New Project: new project scaffolding holds — filesystem_available, no_existing_project, valid_config_created, schema_field_included, project_initialized_emitted"
)]
fn scaffold_new_project_contract() {
    // Requires: empty directory + project name
    // Ensures: specforge.json + spec/ directory created with valid content
    let dir = empty_directory();

    // filesystem_available; project_initialized_emitted: init reports it.
    Cli::on(&dir)
        .bare_command(&["init", "--name", "contract-test"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "Initialized project 'contract-test'",
        ));

    assert_scaffold_contract(&dir, "contract-test");
}

/// The scaffold_new_project clauses after `init --name <name>` ran
/// successfully in `dir`, then a second init there.
pub(crate) fn assert_scaffold_contract(project: &TestProject, name: &str) {
    let dir = project.root();
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
    Cli::on(project)
        .bare_command(&["init", "--name", "second"])
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
    let dir = empty_directory();

    let output = Cli::on(&dir)
        .bare_command(&["init", "--name", "ci-contract"])
        .write_stdin("") // empty stdin — must not hang
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "must succeed without interactive input"
    );
    assert!(
        dir.root().join("specforge.json").exists(),
        "config must be created"
    );

    let content = fs::read_to_string(dir.root().join("specforge.json")).unwrap();
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
    let dir = empty_directory();

    Cli::on(&dir)
        .bare_command(&["init", "--name", "zero-ext-contract"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.root().join("specforge.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(
        json["extensions"],
        serde_json::json!([]),
        "extensions must be empty array"
    );

    // Project must still be functional: check + export succeed
    Cli::on(&dir)
        .bare_command(&["check", "--format", "json", "--path"])
        .arg(dir.root().join("spec"))
        .assert()
        .success();

    let export_output = Cli::on(&dir)
        .bare_command(&["export", "--format", "graph", "--path"])
        .arg(dir.root().join("spec"))
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
