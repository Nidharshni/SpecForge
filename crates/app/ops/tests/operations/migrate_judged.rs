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

/// Five `behavior` blocks writing `owner [x]`: the project already reports errors of its own (E014 `x`). The
/// verdict tests plan its hoisted text themselves.
pub(crate) fn erring_project() -> (tempfile::TempDir, String) {
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

/// A project whose one migration hook fails, so a run over it is refused.
fn hook_refused() -> (tempfile::TempDir, SharedRuntime) {
    (
        project_with_extension(),
        Arc::new(hooked(|_| Err("refused".into()))),
    )
}

#[specforge_test(
    behavior = "validate_post_migration_integrity",
    verify = "a refused migration's cause counts only the errors it would add"
)]
fn a_refused_migrations_cause_counts_only_the_errors_it_adds() {
    let (dir, original) = erring_project();
    let owner_free = ["a", "b", "c", "d", "e"]
        .map(|id| format!("behavior {id} \"T\" {{\n}}\n"))
        .join("\n");
    let planned = format!("defaults behavior {{\n  owner [x]\n}}\n\n{owner_free}");

    let verdict = judge_planned(
        dir.path(),
        Some(specforge_project::testing::component_runtime()),
        planned,
    );

    assert!(
        matches!(
            verdict.refusal(),
            Some(Failure::DoesNotCompile { errors: 1 })
        ),
        "{:?}",
        verdict.refusal()
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
    let kept = || {
        (
            project_with_extension(),
            Arc::new(hooked(|_| Ok(()))) as SharedRuntime,
        )
    };
    for fixture in [kept, hook_refused] {
        let outcomes = [true, false].map(|dry_run| {
            let (dir, runtime) = fixture();
            let root = specforge_common::ProjectRoot::of(dir.path()).unwrap();
            let mut request = request(&root);
            request.dry_run = dry_run;
            let outcome = run(&request, Some(runtime));
            if dry_run {
                assert!(matches!(outcome.wrote(), Wrote::Previewed), "{outcome:?}");
                assert_eq!(
                    std::fs::read_to_string(dir.path().join("old.spec")).unwrap(),
                    OLD
                );
                assert!(!dir.path().join("old.spec.bak").exists());
            }
            outcome
        });
        let [dry, wet] = &outcomes;
        assert_eq!(dry.ok(), wet.ok(), "{dry:?} {wet:?}");
        assert_eq!(
            format!("{:?}", dry.failure()),
            format!("{:?}", wet.failure())
        );
        assert_eq!(dry.verdict.hooks_invoked, wet.verdict.hooks_invoked);
        assert_eq!(dry.verdict.hook_failures, wet.verdict.hook_failures);
        assert_eq!(dry.verdict.compared, wet.verdict.compared);
        assert_eq!(
            dry.structural_differences().len(),
            wet.structural_differences().len()
        );
        assert_eq!(
            dry.verdict.post_errors().count(),
            wet.verdict.post_errors().count()
        );
    }
}

#[cfg(unix)]
#[specforge_test(
    behavior = "validate_post_migration_integrity",
    verify = "a refused migration writes nothing, not even a backup"
)]
fn a_refused_migration_writes_nothing() {
    use std::os::unix::fs::MetadataExt as _;
    let (dir, runtime) = hook_refused();
    let file = dir.path().join("old.spec");
    let before = std::fs::metadata(&file).unwrap();
    let state = crate::writes::files_under(&dir.path().join(".specforge"));

    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        Some(runtime),
    );

    let after = std::fs::metadata(&file).unwrap();
    assert!(outcome.failure().is_some(), "{outcome:?}");
    assert_eq!(after.ino(), before.ino(), "the source was replaced");
    assert_eq!(after.modified().unwrap(), before.modified().unwrap());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), OLD);
    assert!(outcome.writes.is_empty(), "{:?}", outcome.writes);
    assert!(!dir.path().join("old.spec.bak").exists());
    assert_eq!(
        crate::writes::files_under(&dir.path().join(".specforge")),
        state
    );
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

#[specforge_test(
    behavior = "validate_post_migration_integrity",
    verify = "a planned text that adds an error is refused even when the graph is unchanged"
)]
fn a_planned_text_that_adds_an_error_and_keeps_the_graph_is_refused() {
    let dir = project_with_extension();
    // A second `alpha`: E002, and the graph is the one the project has.
    let planned = format!("{}behavior alpha \"Alpha\" {{\n}}\n", migrated());

    let verdict = judge_planned(dir.path(), None, planned);

    assert!(verdict.graph_delta.is_empty(), "{:?}", verdict.graph_delta);
    assert_eq!(verdict.added_errors, 1);
    assert!(
        matches!(
            verdict.refusal(),
            Some(Failure::DoesNotCompile { errors: 1 })
        ),
        "{:?}",
        verdict.refusal()
    );
}

#[specforge_test(
    behavior = "validate_post_migration_integrity",
    verify = "a migration that only moves lines is kept in a file that already reports a duplicate ID"
)]
fn a_migration_that_only_moves_a_duplicate_ids_position_is_kept() {
    let dir = project_with_extension();
    let duplicate = "behavior alpha \"Alpha\" {\n}\n";
    std::fs::write(dir.path().join("old.spec"), format!("{OLD}{duplicate}")).unwrap();
    // The header at 1.0 and one blank line after it: the E002 now names another line.
    let planned = format!("{}\n{duplicate}", migrated().replacen('\n', "\n\n", 1));

    let verdict = judge_planned(dir.path(), None, planned);

    assert!(verdict.graph_delta.is_empty(), "{:?}", verdict.graph_delta);
    assert!(verdict.refusal().is_none(), "{:?}", verdict.refusal());
}
