//! The field forms a document is written in are its own project's, found from its place (ADR 0147 D6).

use super::*;
use specforge_project::{Environment, RuntimeSource, SharedRuntime};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

const TAGGED: &str =
    "behavior bb \"B\" {\n  contract \"The system MUST x\"\n  category \"auth\"\n}\n";
const BARE: &str = "behavior bb \"B\" {\n  contract \"The system MUST x\"\n  category auth\n}\n";
const SOFTWARE: &str = r#"{"extensions": ["@specforge/software"]}"#;

/// A source that loads into one runtime and counts its loads.
fn counting() -> (RuntimeSource, Arc<AtomicUsize>) {
    let loads = Arc::new(AtomicUsize::new(0));
    let runtime: SharedRuntime = Arc::new(specforge_component::ComponentRuntime::new());
    let count = Arc::clone(&loads);
    let source = RuntimeSource::Build(Arc::new(move |_, _| {
        count.fetch_add(1, Ordering::SeqCst);
        Arc::clone(&runtime)
    }));
    (source, loads)
}

fn software_project() -> tempfile::TempDir {
    project_with(SOFTWARE, &[("spec/a.spec", TAGGED)])
}

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "a document of the project the caller holds is written in its lent forms, without loading them again"
)]
fn a_document_of_the_lent_project_is_written_in_its_forms_and_loads_nothing() {
    let dir = software_project();
    let root = ProjectRoot::of(dir.path()).unwrap();
    let (source, loads) = counting();
    let env = Environment::load(
        &root,
        Some(Arc::new(specforge_component::ComponentRuntime::new())),
    );
    let file = dir.path().join("spec/a.spec");
    let formatted = document(
        Place::File(&file),
        TAGGED,
        None,
        None,
        &Forms::lending(&env, source),
    );
    assert_eq!(formatted.formatted, BARE);
    assert_eq!(loads.load(Ordering::SeqCst), 0);
}

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "another project's forms load once per run"
)]
fn another_projects_documents_load_its_forms_once_per_run() {
    let dir = project_with(
        r#"{"extensions": []}"#,
        &[
            ("inner/specforge.json", SOFTWARE),
            ("inner/spec/a.spec", TAGGED),
            ("inner/spec/b.spec", TAGGED),
        ],
    );
    let root = ProjectRoot::of(dir.path()).unwrap();
    let env = Environment::load(&root, None);
    let (source, loads) = counting();
    let outcome = run(&Request {
        root: &root,
        paths: &[dir.path().join("inner")],
        mode: Mode::Check,
        caller: crate::testing::caller(),
        forms: Forms::lending(&env, source),
    });
    assert_eq!(outcome.changes.len(), 2, "{:?}", outcome.changes);
    assert!(outcome.changes.iter().all(|c| c.after == BARE));
    assert_eq!(loads.load(Ordering::SeqCst), 1);
}

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "a document outside any project is written as it is"
)]
fn a_document_outside_any_project_is_written_as_it_is() {
    let held = software_project();
    let env = Environment::load(&ProjectRoot::of(held.path()).unwrap(), None);
    let outside = tempfile::TempDir::new().unwrap();
    assert!(ProjectRoot::enclosing(outside.path()).is_none());
    let file = outside.path().join("loose.spec");
    let (source, loads) = counting();
    let formatted = document(
        Place::File(&file),
        TAGGED,
        None,
        None,
        &Forms::lending(&env, source),
    );
    assert_eq!(formatted.formatted, TAGGED);
    assert_eq!(loads.load(Ordering::SeqCst), 0);
}

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "an environment loaded at no project root lends no forms"
)]
fn an_environment_with_no_project_root_lends_nothing() {
    let dir = software_project();
    let file = dir.path().join("spec/a.spec");
    let env = Environment::empty();
    let forms = Forms::lending(&env, RuntimeSource::Fixed(None));
    let formatted = document(Place::File(&file), TAGGED, None, None, &forms);
    assert_eq!(formatted.formatted, TAGGED);
}

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "text of a project that is no file is written in that project's forms"
)]
fn stdin_text_is_written_in_the_forms_of_its_project() {
    let dir = software_project();
    let (source, loads) = counting();
    let forms = Forms::loading(source);
    let place = Place::InProject {
        root: dir.path(),
        dir: &dir.path().join("spec"),
    };
    assert_eq!(document(place, TAGGED, None, None, &forms).formatted, BARE);
    assert_eq!(loads.load(Ordering::SeqCst), 1);

    let loose = tempfile::TempDir::new().unwrap();
    let place = Place::InProject {
        root: loose.path(),
        dir: loose.path(),
    };
    assert_eq!(
        document(place, TAGGED, None, None, &forms).formatted,
        TAGGED
    );
}
