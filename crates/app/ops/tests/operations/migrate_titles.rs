//! `specforge migrate` adds the titles blocks lack and keeps the graph comparison (ADR 0148).

use std::sync::Arc;

use specforge_test_macros::test as specforge_test;

use crate::migrate::{hooked, project, project_with_extension, request, run};

const UNTITLED: &str = "// Alpha does a thing.\nbehavior alpha {\n}\n";
const TITLED: &str = "behavior alpha \"Alpha does a thing.\" {\n}\n";

#[specforge_test(
    behavior = "add_missing_titles",
    verify = "a migration that adds titles is kept, with no W054"
)]
fn a_migration_that_adds_titles_is_kept() {
    let dir = project();
    let file = dir.path().join("old.spec");
    std::fs::write(&file, UNTITLED).unwrap();

    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        None,
    );

    assert!(outcome.ok() && outcome.migrated(), "{outcome:?}");
    assert!(outcome.structural_differences().is_empty());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), TITLED);
}

#[specforge_test(
    behavior = "add_missing_titles",
    verify = "a dry run shows the titles and writes nothing"
)]
fn a_dry_run_shows_the_titles_and_writes_nothing() {
    let dir = project();
    let file = dir.path().join("old.spec");
    std::fs::write(&file, UNTITLED).unwrap();
    let root = specforge_common::ProjectRoot::of(dir.path()).unwrap();
    let mut dry = request(&root);
    dry.dry_run = true;

    let outcome = run(&dry, None);

    assert!(outcome.pending, "{outcome:?}");
    assert!(
        outcome.summary.diffs[0]
            .unified_text
            .contains("+behavior alpha \"Alpha does a thing.\" {"),
        "{:?}",
        outcome.summary.diffs
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), UNTITLED);
}

#[specforge_test(
    behavior = "add_missing_titles",
    verify = "a project already titled has nothing pending"
)]
fn a_project_already_titled_has_nothing_pending() {
    let dir = project();
    std::fs::write(dir.path().join("old.spec"), TITLED).unwrap();

    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        None,
    );

    assert!(!outcome.pending, "{outcome:?}");
}

#[specforge_test(
    behavior = "add_missing_titles",
    verify = "a hook that also changes a field rolls the titles back"
)]
fn a_hook_that_also_changes_a_field_rolls_the_titles_back() {
    let dir = project_with_extension();
    let file = dir.path().join("old.spec");
    let original = "// Alpha does a thing.\nbehavior alpha {\n  note \"a\"\n}\n";
    std::fs::write(&file, original).unwrap();
    let runtime = Arc::new(hooked(|input| {
        let path = &input.files[0];
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        std::fs::write(path, text.replace("\"a\"", "\"b\"")).map_err(|e| e.to_string())
    }));

    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        Some(runtime),
    );

    let messages: Vec<String> = outcome
        .structural_differences()
        .into_iter()
        .map(|d| d.message)
        .collect();
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert!(messages[0].contains("note"), "{messages:?}");
    assert!(outcome.rollback.is_some() && !outcome.ok(), "{outcome:?}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), original);
}
