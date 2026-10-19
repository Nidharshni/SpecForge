//! What a mutation's outcome says about a preview (ADR 0162): a preview answers `Previewed` and its document says
//! `dry_run: true`; a run that found nothing to change is no preview.

use serde_json::json;
use specforge_ops::authoring::edit;
use specforge_ops::authoring::format::{self, Mode};
use specforge_ops::authoring::migrate;
use specforge_ops::extension::{self, AddRequest};
use specforge_ops::view::ProjectView;
use specforge_ops::{Writes, Wrote};

use crate::writes::{
    OLD, TIDY, candidates, compiled_with_builtins, migration, project, remove_over,
};

fn add_over(root: &std::path::Path, specifier: &str, dry_run: bool) -> extension::Added {
    extension::add(
        &AddRequest {
            root: &specforge_common::ProjectRoot::of(root).unwrap(),
            extensions: vec![specifier.to_string()],
            base: None,
            allow_unsigned: false,
            dry_run,
            caller: specforge_ops::testing::caller(),
        },
        &specforge_ops::registry::Unconfigured("test"),
        &candidates(),
    )
    .unwrap()
}

fn format_over(root: &std::path::Path, mode: Mode) -> format::Outcome {
    format::run(&format::Request {
        root: &specforge_common::ProjectRoot::of(root).unwrap(),
        paths: &[],
        mode,
        caller: specforge_ops::testing::caller(),
        runtime: None,
    })
    .unwrap()
}

fn migrate_over(root: &std::path::Path, dry_run: bool) -> migrate::Outcome {
    migrate::run(
        &migration(&specforge_common::ProjectRoot::of(root).unwrap(), dry_run),
        None,
    )
    .unwrap()
}

/// The `dry_run` of each document, in the order: add, remove, migrate, format check, format diff/preview.
fn marks(dry_run: bool) -> Vec<serde_json::Value> {
    let doc = |value: &serde_json::Value| value["dry_run"].clone();
    let on_product = project(&["@specforge/product"], &[]);
    let add = add_over(on_product.path(), "@specforge/product", dry_run);
    let remove = remove_over(
        &compiled_with_builtins(on_product.path()),
        "@specforge/product",
        dry_run,
    );
    let old = project(&[], &[("old.spec", OLD)]);
    let nothing = project(&[], &[("tidy.spec", TIDY)]);
    let migrate = migrate_over(if dry_run { old.path() } else { nothing.path() }, dry_run);
    let modes: &[Mode] = if dry_run {
        &[Mode::Check, Mode::Preview]
    } else {
        &[Mode::Write]
    };
    let mut marks = vec![
        doc(&serde_json::to_value(add.document()).unwrap()),
        doc(&serde_json::to_value(remove.document()).unwrap()),
        doc(&serde_json::to_value(migrate.document()).unwrap()),
    ];
    for mode in modes {
        let formatted = format_over(nothing.path(), *mode);
        marks.push(doc(
            &serde_json::to_value(formatted.document(false)).unwrap()
        ));
    }
    marks
}

#[test]
#[ignore = "flipped by T8"]
fn every_preview_document_says_dry_run() {
    let marks = marks(true);
    assert_eq!(marks, vec![json!(true); marks.len()], "{marks:?}");
}

#[test]
#[ignore = "flipped by T8"]
fn every_run_document_says_dry_run_false() {
    let marks = marks(false);
    assert_eq!(marks, vec![json!(false); marks.len()], "{marks:?}");
}

#[test]
fn each_preview_answers_previewed() {
    let on_product = project(&["@specforge/product"], &[]);
    assert_eq!(
        add_over(on_product.path(), "@specforge/product", true).wrote(),
        Wrote::Previewed
    );
    let compiled = compiled_with_builtins(on_product.path());
    assert_eq!(
        remove_over(&compiled, "@specforge/product", true).wrote(),
        Wrote::Previewed
    );
    let old = project(&[], &[("old.spec", OLD)]);
    assert_eq!(migrate_over(old.path(), true).wrote(), Wrote::Previewed);
    for mode in [Mode::Check, Mode::Preview] {
        assert_eq!(format_over(old.path(), mode).wrote(), Wrote::Previewed);
    }
}

#[test]
fn a_run_that_changes_nothing_ran() {
    let on_product = project(&["@specforge/product"], &[]);
    let none = Wrote::Ran(Writes::none());
    assert_eq!(
        add_over(on_product.path(), "@specforge/product", false).wrote(),
        none
    );
    let tidy = project(&[], &[("tidy.spec", TIDY)]);
    assert_eq!(migrate_over(tidy.path(), false).wrote(), none);
    assert_eq!(format_over(tidy.path(), Mode::Write).wrote(), none);
}

#[test]
fn a_run_that_changed_files_ran_with_them() {
    let on_product = project(&["@specforge/product"], &[]);
    let removed = remove_over(
        &compiled_with_builtins(on_product.path()),
        "@specforge/product",
        false,
    );
    assert_eq!(removed.wrote(), Wrote::Ran(removed.writes.clone()));
    assert!(!removed.writes.is_empty());
    let old = project(&[], &[("old.spec", OLD)]);
    let migrated = migrate_over(old.path(), false);
    assert_eq!(migrated.wrote(), Wrote::Ran(migrated.writes.clone()));
    assert!(!migrated.writes.is_empty());
}

fn tasks() -> crate::navigate::Compiled {
    crate::edit::project(&[("tasks.spec", crate::edit::TASKS)])
}

fn run_edits(
    c: &crate::navigate::Compiled,
    edits: &[edit::Edit],
    dry_run: bool,
) -> Result<edit::Edited, specforge_ops::Refusal<edit::EditField>> {
    edit::run(
        &ProjectView::of(&c.project),
        edits,
        dry_run,
        specforge_ops::testing::caller(),
    )
}

fn change_category() -> Vec<edit::Edit> {
    vec![crate::edit::set(
        "complete_task",
        "category",
        crate::edit::one("query"),
    )]
}

#[test]
fn an_edit_preview_writes_nothing_and_says_so() {
    let c = tasks();
    let root = c.project.environment().spec_root().to_path_buf();
    let before = crate::writes::files_under(&root);
    let edited = run_edits(&c, &change_category(), true).unwrap();
    assert_eq!(edited.wrote, Wrote::Previewed);
    assert_eq!(
        crate::writes::changed_since(&root, &before),
        Vec::<String>::new()
    );
    let document = serde_json::to_value(edited.document()).unwrap();
    assert_eq!(document["dry_run"], json!(true));
    assert!(
        document["diff"]
            .as_str()
            .unwrap()
            .contains("+  category   query")
    );
}

#[test]
fn an_edit_that_changes_nothing_ran() {
    let c = tasks();
    let same = vec![crate::edit::set(
        "complete_task",
        "category",
        crate::edit::one("command"),
    )];
    let edited = run_edits(&c, &same, false).unwrap();
    assert_eq!(edited.wrote, Wrote::Ran(Writes::none()));
    assert!(edited.report.entities.is_empty(), "{:?}", edited.report);
}

#[test]
fn an_edit_run_writes_what_it_planned() {
    let c = tasks();
    let root = c.project.environment().spec_root().to_path_buf();
    let planned = crate::edit::plan(&c, &change_category()).unwrap();
    let edited = run_edits(&c, &change_category(), false).unwrap();
    assert_eq!(&edited.report, planned.report());
    assert_eq!(
        crate::writes::listed(&edited.wrote.into_files(), &root),
        ["tasks.spec"]
    );
}

#[test]
fn an_edit_refused_by_the_plan_is_refused_by_run() {
    let c = tasks();
    let root = c.project.environment().spec_root().to_path_buf();
    let before = crate::writes::files_under(&root);
    let dangling = vec![crate::edit::link(
        "complete_task",
        "invariants",
        "nope_rule",
    )];
    let error = run_edits(&c, &dangling, false).unwrap_err().error;
    assert!(error.is(specforge_common::codes::E080), "{error:?}");
    assert_eq!(
        crate::writes::changed_since(&root, &before),
        Vec::<String>::new()
    );
}
