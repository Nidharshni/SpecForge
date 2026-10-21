//! A migration is judged before it is written (ADR 0186): its preview carries the run's verdict, a refused run
//! writes nothing, and its cause counts what it adds.

use specforge_ops::authoring::migrate::Failure;
use specforge_test_macros::test as specforge_test;

use crate::migrate::{OLD, hooked, project_with_extension, request, run};
use specforge_ops::Wrote;
use specforge_ops::authoring::migrate::{MigrationInput, Verdict, judge};
use specforge_project::SharedRuntime;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

/// The text `old.spec` (`OLD`) becomes under the core migration: the header at the current format version.
pub(crate) fn migrated() -> String {
    OLD.replace("0.9", "1.0")
}

/// Judge `planned` as the text of the project's `old.spec`: the verdict a run would reach had its steps and hooks
/// produced that text. A production hook cannot write a file, so a test that needs a migration to change the graph or
/// break the compile plans the text itself.
pub(crate) fn judge_planned(
    dir: &Path,
    runtime: Option<SharedRuntime>,
    planned: impl Into<String>,
) -> Verdict {
    let pre = specforge_project::testing::compile_at(dir, runtime);
    let planned = BTreeMap::from([("old.spec".to_string(), planned.into())]);
    let input = MigrationInput {
        from: "0.9".into(),
        to: "1.0".into(),
        files: vec![dir.join("old.spec").display().to_string()],
    };
    judge(&pre, &planned, &input)
}

/// Five `behavior` blocks writing `owner`, no field of their kind: the defaults hoist is E077 and reaches no block,
/// so the migration changes the graph (and the project already reports errors of its own).
pub(crate) fn refused_project() -> (tempfile::TempDir, String) {
    let dir = crate::migrate::project();
    let block = |id: &str| format!("behavior {id} \"T\" {{\n  owner [x]\n}}\n");
    let original = ["a", "b", "c", "d", "e"].map(block).join("\n");
    std::fs::write(dir.path().join("old.spec"), &original).unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name": "p", "version": "0.1.0", "extensions": ["@specforge/software"]}"#,
    )
    .unwrap();
    specforge_installed::testing::install_configured(dir.path(), &specforge_project::builtins());
    (dir, original)
}

#[specforge_test(
    behavior = "validate_post_migration_integrity",
    verify = "a refused migration's cause counts only the errors it would add"
)]
fn a_refused_migrations_cause_counts_only_the_errors_it_adds() {
    let (dir, original) = refused_project();

    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        Some(specforge_project::testing::component_runtime()),
    );

    assert!(
        matches!(
            outcome.failure(),
            Some(Failure::DoesNotCompile { errors: 1 })
        ),
        "{:?}",
        outcome.failure()
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("old.spec")).unwrap(),
        original
    );
}

#[specforge_test(
    behavior = "generate_migration_diff",
    verify = "a dry run reports the verdict the run reaches: its hooks, its W054 and its errors"
)]
fn a_dry_run_reports_the_verdict_the_run_reaches() {
    let (dir, original) = refused_project();
    let root = specforge_common::ProjectRoot::of(dir.path()).unwrap();
    let mut request = request(&root);
    request.dry_run = true;

    let outcome = run(
        &request,
        Some(specforge_project::testing::component_runtime()),
    );

    assert!(!outcome.ok(), "{outcome:?}");
    assert!(outcome.failure().is_some(), "{outcome:?}");
    assert!(outcome.verdict.compared, "{outcome:?}");
    assert!(!outcome.structural_differences().is_empty(), "{outcome:?}");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("old.spec")).unwrap(),
        original
    );
    assert!(!dir.path().join("old.spec.bak").exists());
    assert!(matches!(outcome.wrote(), Wrote::Previewed), "{outcome:?}");
}

#[cfg(unix)]
#[specforge_test(
    behavior = "validate_post_migration_integrity",
    verify = "a refused migration writes nothing, not even a backup"
)]
fn a_refused_migration_writes_nothing() {
    use std::os::unix::fs::MetadataExt as _;
    let (dir, original) = refused_project();
    let file = dir.path().join("old.spec");
    let before = std::fs::metadata(&file).unwrap();

    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        Some(specforge_project::testing::component_runtime()),
    );

    let after = std::fs::metadata(&file).unwrap();
    assert!(outcome.failure().is_some(), "{outcome:?}");
    assert_eq!(after.ino(), before.ino(), "the source was replaced");
    assert_eq!(after.modified().unwrap(), before.modified().unwrap());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), original);
    assert!(outcome.writes.is_empty(), "{:?}", outcome.writes);
    assert!(!dir.path().join("old.spec.bak").exists());
    assert!(!dir.path().join(".specforge").exists());
}

#[specforge_test(
    behavior = "invoke_extension_migration_hooks",
    verify = "the hooks run before any file is written"
)]
fn the_hooks_run_before_any_file_is_written() {
    let dir = project_with_extension();
    // The hook sees the file as it is on disk: still the old text.
    let runtime = Arc::new(hooked(|input| {
        let text = std::fs::read_to_string(&input.files[0]).map_err(|e| e.to_string())?;
        if text == OLD {
            Ok(())
        } else {
            Err(format!("a file was written before the hooks ran: {text}"))
        }
    }));

    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        Some(runtime),
    );

    assert!(
        outcome.verdict.hook_failures.is_empty(),
        "{:?}",
        outcome.verdict.hook_failures
    );
    assert!(outcome.ok(), "{outcome:?}");
}
