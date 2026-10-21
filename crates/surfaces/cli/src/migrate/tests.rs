use super::*;
use crate::terminal::Facts;
use clap::Parser;

#[derive(Parser)]
struct Line {
    #[command(flatten)]
    args: Args,
}

/// The fixture of the ops tests' `refused_project`: five `owner` blocks, no field of their kind, so the defaults
/// hoist is E077 and the migration changes the graph while the project already has errors of its own.
fn refused_project() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name": "p", "version": "0.1.0", "extensions": ["@specforge/software"]}"#,
    )
    .unwrap();
    let block = |id: &str| format!("behavior {id} \"T\" {{\n  owner [x]\n}}\n");
    let text = ["a", "b", "c", "d", "e"].map(block).join("\n");
    std::fs::write(dir.path().join("old.spec"), text).unwrap();
    specforge_installed::testing::install_configured(dir.path(), &specforge_project::builtins());
    dir
}

fn migrate_refused(args: &[&str]) -> (Exit, String, String) {
    let dir = refused_project();
    let path = dir.path().to_str().unwrap();
    let line = Line::try_parse_from(
        ["migrate", "--path", path]
            .into_iter()
            .chain(args.iter().copied()),
    )
    .expect("the flags parse");
    let (terminal, capture) = Terminal::captured("", Facts::PIPED);
    let exit = line
        .args
        .run_in(specforge_project::testing::component_runtime(), &terminal);
    (exit, capture.stdout(), capture.stderr())
}

#[specforge_test_macros::test(
    behavior = "validate_post_migration_integrity",
    verify = "under --format json a refused migration writes nothing beside its document"
)]
fn a_refused_migration_writes_nothing_beside_its_json_document() {
    let (exit, stdout, stderr) = migrate_refused(&["--format", "json"]);

    assert_eq!(exit, Exit::Failed);
    let document: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(document["ok"], false, "{document}");
    assert_eq!(
        document["structural_differences"][0]["code"], "W054",
        "{document}"
    );
    assert_eq!(
        document["hooks_invoked"],
        serde_json::json!([]),
        "{document}"
    );
    assert_eq!(stderr, "");
}

#[test]
fn a_refused_migration_tells_a_person_on_stderr() {
    let (exit, _, stderr) = migrate_refused(&["--format", "human"]);

    assert_eq!(exit, Exit::Failed);
    assert!(stderr.contains("W054"), "{stderr}");
    assert!(stderr.contains("does not compile"), "{stderr}");
    assert!(stderr.contains("nothing was written"), "{stderr}");
}

#[specforge_test_macros::test(
    behavior = "validate_post_migration_integrity",
    verify = "a failed migration is told on stderr by its failure's message, naming no backup it did not make"
)]
fn a_failed_migration_is_told_by_its_failures_message() {
    let (exit, _, stderr) = migrate_refused(&["--no-backup", "--format", "human"]);

    assert_eq!(exit, Exit::Failed);
    assert!(stderr.contains("W054"), "{stderr}");
    assert!(stderr.contains("does not compile"), "{stderr}");
    assert!(!stderr.contains("backups"), "{stderr}");
}

#[specforge_test_macros::test(
    behavior = "generate_migration_diff",
    verify = "a dry run the run would refuse exits 1"
)]
fn a_dry_run_the_run_would_refuse_exits_failed() {
    let (exit, stdout, _) = migrate_refused(&["--dry-run", "--format", "json"]);

    let document: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(exit, Exit::Failed, "{document}");
    assert_eq!(document["ok"], false, "{document}");
}
