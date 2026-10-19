//! What `--format json` prints for a preview and for a run that changed nothing (ADR 0162): a preview lists no
//! `files_written`; a run lists every file it wrote, `[]` when none.

use crate::support::{Cli, TestProject};
use serde_json::{Value, json};

fn json_of(project: &TestProject, args: &[&str]) -> Value {
    let run = Cli::on(project).run(args);
    assert_eq!(run.code, 0, "{args:?}\n{}", run.stdout);
    serde_json::from_str(&run.stdout).unwrap()
}

#[test]
#[ignore = "flipped by T6"]
fn a_run_that_changes_nothing_lists_no_file() {
    let project = TestProject::copy_of(specforge_repo_root::repo_root().join("examples/todo-app"));
    let link = [
        "link",
        "complete_task",
        "invariants",
        "task_id_uniqueness",
        "--format",
        "json",
    ];
    json_of(&project, &link);
    let again = json_of(&project, &link);
    assert_eq!(again["entities"], json!([]), "{again}");
    assert_eq!(again["files_written"], json!([]), "{again}");
}

#[test]
#[ignore = "flipped by T6"]
fn an_add_dry_run_of_a_present_extension_lists_no_file() {
    let project = TestProject::new().without_config();
    project.write(
        "specforge.json",
        r#"{"name": "demo", "version": "0.1.0", "extensions": ["@specforge/software"]}"#,
    );
    let dry = json_of(
        &project,
        &[
            "add",
            "@specforge/software",
            "--dry-run",
            "--format",
            "json",
        ],
    );
    let run = json_of(
        &project,
        &["add", "@specforge/software", "--format", "json"],
    );
    assert!(dry.get("files_written").is_none(), "{dry}");
    assert_eq!(run["files_written"], json!([]), "{run}");
    assert_ne!(dry, run);
}
