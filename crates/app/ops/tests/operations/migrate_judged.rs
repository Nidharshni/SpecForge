//! A migration is judged before it is written (ADR 0186): its preview carries the run's verdict, a refused run
//! writes nothing, and its cause counts what it adds.

use specforge_ops::authoring::migrate::Failure;
use specforge_test_macros::test as specforge_test;

use crate::migrate::{request, run};

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

#[ignore = "flipped by T6"]
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
