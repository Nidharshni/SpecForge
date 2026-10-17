//! A compiled project says once whether it is rooted: `root()` (ADR 0047 D4); the project root it was
//! loaded at is its environment's (ADR 0053 D1, ADR 0121).

use std::path::Path;

use specforge_common::ProjectRoot;
use specforge_project::testing::{Assembled, TestProject};
use specforge_project::{CompiledProject, Environment, ProjectSession};
use specforge_registry::RegistryBuild;

/// F1 answers what F2 says, for a project production makes.
fn rooted_where_loaded(project: &CompiledProject) {
    let loaded = project.environment().project_root().map(ProjectRoot::dir);
    assert_eq!(project.root(), loaded);
    if let Some(dir) = loaded {
        assert_eq!(project.environment().root(), dir);
    }
}

#[test]
fn every_project_production_makes_is_rooted_where_its_environment_was_loaded() {
    let dir = TestProject::new().file("a.spec", "behavior a \"A\" {\n}\n");

    let compiled = dir.compile_bare();
    rooted_where_loaded(&compiled);
    assert_eq!(compiled.root(), Some(dir.root()));

    let of = CompiledProject::of(Environment::load(&dir.project_root(), None));
    rooted_where_loaded(&of);
    assert_eq!(of.root(), Some(dir.root()));

    let mut session = dir.session_bare();
    rooted_where_loaded(session.project());
    assert_eq!(session.project().root(), Some(dir.root()));

    let unread = CompiledProject::unread(session.project().shared_environment());
    rooted_where_loaded(&unread);
    assert_eq!(unread.root(), Some(dir.root()));

    session.reload_environment();
    rooted_where_loaded(session.project());
    assert_eq!(session.project().root(), Some(dir.root()));

    let detached = ProjectSession::detached();
    rooted_where_loaded(detached.project());
    assert_eq!(detached.project().root(), None);
    assert_eq!(detached.project().environment().root(), Path::new(""));
    assert_eq!(detached.inputs().root(), None);
    assert_eq!(detached.inputs().spec_root(), None);
}

#[test]
fn a_session_is_on_disk_exactly_when_its_ledger_is() {
    let dir = TestProject::new().file("a.spec", "behavior a \"A\" {\n}\n");
    for session in [dir.session_bare(), ProjectSession::detached()] {
        assert_eq!(
            session.project().root().is_some(),
            session.inputs().root().is_some()
        );
    }
}

#[test]
fn an_assembled_project_is_rooted_at_a_directory_it_was_not_loaded_at() {
    let dir = tempfile::tempdir().unwrap();
    for project in [
        Assembled::new(),
        Assembled::at(dir.path()),
        Assembled::over(RegistryBuild::default()),
    ] {
        assert_eq!(project.compiled().root(), Some(project.root()));
        assert_eq!(project.compiled().environment().root(), project.root());
        assert!(project.compiled().environment().project_root().is_none());
        assert!(project.detached().root().is_none());
    }
}
