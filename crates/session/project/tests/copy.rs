//! A compiled project copies as its graph and shares the rest (ADR 0108).

use std::sync::Arc;

use specforge_project::CompiledProject;
use specforge_project::testing::{TestProject, disk_changed};

#[test]
fn a_copy_reads_as_its_original_until_the_session_moves_on() {
    let dir = TestProject::new().file("a.spec", "behavior a \"A\" {\n}\n");
    let mut session = dir.session_bare();
    let copy = session.project().clone();

    assert!(std::ptr::eq(
        copy.environment(),
        session.project().environment()
    ));
    assert!(Arc::ptr_eq(
        &copy.shared_entities(),
        &session.project().shared_entities()
    ));
    assert!(Arc::ptr_eq(
        &copy.source_text("a.spec").unwrap(),
        &session.project().source_text("a.spec").unwrap()
    ));
    assert_eq!(copy.root(), session.project().root());
    assert_eq!(copy.diagnostics(), session.project().diagnostics());

    dir.write("b.spec", "behavior b \"B\" {\n}\n");
    disk_changed(&mut session, &["b.spec"]).unwrap();

    assert!(copy.graph().node("b").is_none());
    assert!(session.project().graph().node("b").is_some());
}

#[test]
fn an_unread_project_reports_its_environment_and_holds_no_source() {
    let dir = TestProject::new().file("a.spec", "behavior a \"A\" {\n}\n");
    let session = dir.session_bare();
    let environment = session.project().shared_environment();
    let unread = CompiledProject::unread(Arc::clone(&environment));

    assert!(unread.graph().nodes().is_empty());
    assert!(unread.source_keys().is_empty());
    assert_eq!(unread.root(), session.project().root());
    assert!(unread.root().is_some());
    let own: Vec<_> = environment.diagnostics().cloned().collect();
    assert_eq!(unread.diagnostics(), own);
}
