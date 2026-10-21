use super::*;
use crate::terminal::Facts;
use clap::Parser;
use specforge_extension_sdk::{ContributionsBuilder, ExtensionMeta};
use specforge_ops::authoring::migrate::MigrationInput;
use specforge_wasm::testing::InProcessRuntime;
use std::sync::Arc;

#[derive(Parser)]
struct Line {
    #[command(flatten)]
    args: Args,
}

/// A project enabling `@acme/x` with one file at the old format version, and `@acme/x` served in process with a
/// migration hook that renames the entity: the graph loses `alpha`, so the migration is rolled back.
fn rolled_back_project() -> (tempfile::TempDir, Arc<InProcessRuntime>) {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name": "p", "version": "0.1.0", "extensions": ["@acme/x"]}"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("old.spec"),
        "// specforge-format: 0.9\nbehavior alpha \"Alpha\" {\n}\n",
    )
    .unwrap();
    specforge_installed::testing::install(dir.path(), &["@acme/x"]);
    let runtime = InProcessRuntime::new().with(|| {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
        c.migration_hook_handler("migrate_acme", |input: &MigrationInput| {
            let file = &input.files[0];
            let text = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
            std::fs::write(file, text.replace("alpha", "beta")).map_err(|e| e.to_string())
        });
        c
    });
    (dir, Arc::new(runtime))
}

fn migrate(format: &str) -> (Exit, String, String) {
    let (dir, runtime) = rolled_back_project();
    let line = Line::try_parse_from([
        "migrate",
        "--path",
        dir.path().to_str().unwrap(),
        "--format",
        format,
    ])
    .expect("the flags parse");
    let (terminal, capture) = Terminal::captured("", Facts::PIPED);
    let exit = line.args.run_in(runtime, &terminal);
    (exit, capture.stdout(), capture.stderr())
}

#[specforge_test_macros::test(
    behavior = "rollback_failed_migration",
    verify = "under --format json a rolled-back migration writes nothing beside its document"
)]
fn a_rolled_back_migration_writes_nothing_beside_its_json_document() {
    let (exit, stdout, stderr) = migrate("json");

    assert_eq!(exit, Exit::Failed);
    let document: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(document["rolled_back"], true, "{document}");
    assert_eq!(
        document["structural_differences"][0]["code"], "W054",
        "{document}"
    );
    assert_eq!(
        document["hooks_invoked"],
        serde_json::json!(["@acme/x:migrate_acme"]),
        "{document}"
    );
    assert_eq!(stderr, "");
}

#[test]
fn a_rolled_back_migration_tells_a_person_on_stderr() {
    let (exit, _, stderr) = migrate("human");

    assert_eq!(exit, Exit::Failed);
    assert!(stderr.contains("W054"), "{stderr}");
    assert!(stderr.contains("files restored from backups"), "{stderr}");
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

#[ignore = "flipped by T4"]
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
