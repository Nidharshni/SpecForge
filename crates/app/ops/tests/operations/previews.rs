//! What a mutation's outcome says about a preview (ADR 0162): a preview answers `Previewed` and its document says
//! `dry_run: true`; a run that found nothing to change is no preview.

use serde_json::json;
use specforge_ops::authoring::format::{self, Mode};
use specforge_ops::authoring::migrate;
use specforge_ops::extension::{self, AddRequest};

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
