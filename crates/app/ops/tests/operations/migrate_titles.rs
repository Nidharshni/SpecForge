//! `specforge migrate` adds the titles blocks lack and keeps the graph comparison (ADR 0148).

use specforge_test_macros::test as specforge_test;

use crate::migrate::{project, project_with_extension, request, run};
use crate::migrate_judged::judge_planned;
use specforge_migrate::structural_differences;

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
    verify = "a planned text that also changes a field refuses the titles"
)]
fn a_planned_text_that_also_changes_a_field_refuses_the_titles() {
    let dir = project_with_extension();
    let file = dir.path().join("old.spec");
    let original = "// Alpha does a thing.\nbehavior alpha {\n  note \"a\"\n}\n";
    std::fs::write(&file, original).unwrap();
    // The titled text, as the titles step plans it, with a field changed too.
    let planned = "// specforge-format: 1.0\nbehavior alpha \"Alpha\" {\n  note \"b\"\n}\n";

    let verdict = judge_planned(dir.path(), None, planned);

    let messages: Vec<String> = structural_differences(&verdict.graph_delta)
        .into_iter()
        .map(|d| d.message)
        .collect();
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert!(messages[0].contains("note"), "{messages:?}");
    assert!(verdict.refusal().is_some(), "{verdict:?}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), original);
}

#[ignore = "flipped by T4"]
#[specforge_test(
    behavior = "add_missing_titles",
    verify = "a migration leaves no E076 it names migrate for"
)]
fn a_migration_leaves_no_e076_it_names_migrate_for() {
    let dir = project();
    let file = dir.path().join("old.spec");
    std::fs::write(
        &file,
        "// Alpha does a thing.\nbehavior alpha \"\" {\n}\n\nbehavior beta \"  \" {\n}\n\nbehavior gamma {\n}\n",
    )
    .unwrap();

    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        None,
    );

    assert!(outcome.ok() && outcome.migrated(), "{outcome:?}");
    assert!(outcome.structural_differences().is_empty());
    assert!(
        outcome
            .verdict
            .post_errors()
            .all(|d| !d.is(specforge_common::codes::E076)),
        "{:?}",
        outcome.verdict
    );
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "behavior alpha \"Alpha does a thing.\" {\n}\n\nbehavior beta \"Beta\" {\n}\n\nbehavior gamma \"Gamma\" {\n}\n"
    );
}
