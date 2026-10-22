use super::*;
use crate::terminal::Facts;
use clap::Parser;

#[derive(Parser)]
struct Line {
    #[command(flatten)]
    args: Args,
}

/// A project enabling `@acme/x`, whose one file is at an old format version: a migration over it has work to do.
fn refused_project() -> tempfile::TempDir {
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
    specforge_installed::testing::install(dir.path(), &hooked_runtime(), &["@acme/x"]);
    dir
}

/// `@acme/x`, in process, whose migration hook always fails: the run is refused.
fn hooked_runtime() -> specforge_wasm::testing::InProcessRuntime {
    use specforge_extension_sdk::prelude::*;
    specforge_wasm::testing::InProcessRuntime::new().with(|| {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
        c.migration_hook_handler("migrate_acme", |_| Err("refused".into()));
        c
    })
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
        .run_in(std::sync::Arc::new(hooked_runtime()), &terminal);
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
    assert!(
        document["hook_failures"][0]
            .as_str()
            .is_some_and(|f| !f.is_empty()),
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
    assert!(stderr.contains("a migration hook failed"), "{stderr}");
    assert!(stderr.contains("nothing was written"), "{stderr}");
}

#[specforge_test_macros::test(
    behavior = "validate_post_migration_integrity",
    verify = "a failed migration is told on stderr by its failure's message, naming no backup it did not make"
)]
fn a_failed_migration_is_told_by_its_failures_message() {
    let (exit, _, stderr) = migrate_refused(&["--no-backup", "--format", "human"]);

    assert_eq!(exit, Exit::Failed);
    assert!(stderr.contains("a migration hook failed"), "{stderr}");
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
