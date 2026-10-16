//! A compiled project a test assembles in memory (`testing::Assembled`, ADR 0108).

use specforge_common::{Diagnostic, Severity};
use specforge_project::testing::Assembled;
use specforge_registry::RegistryBuild;

#[test]
fn an_assembled_project_reports_what_it_is_told_and_scores_its_own_graph() {
    let w002 = Diagnostic::untyped("W002", Severity::Warning, "unused");
    let project = Assembled::new()
        .source("behavior a \"A\" {\n}\n")
        .reporting(vec![w002.clone()]);

    assert_eq!(project.compiled().diagnostics(), vec![w002.clone()]);
    assert_eq!(project.detached().diagnostics(), vec![w002]);
    assert_eq!(project.compiled().entities().kind_of("a"), Some("behavior"));
    assert!(std::ptr::eq(project.compiled(), project.compiled()));
    assert!(Assembled::new().compiled().diagnostics().is_empty());
}

#[test]
fn an_assembled_project_is_rooted_at_its_directory_or_nowhere() {
    let dir = tempfile::tempdir().unwrap();
    for project in [
        Assembled::new(),
        Assembled::at(dir.path()),
        Assembled::over(RegistryBuild::default()),
    ] {
        assert_eq!(project.compiled().root(), Some(project.root()));
        assert_eq!(project.compiled().environment().root(), project.root());
        assert_eq!(project.detached().root(), None);
    }
    let over = Assembled::over(RegistryBuild::default());
    assert_eq!(over.compiled().environment().spec_root(), over.root());
}

#[test]
#[should_panic(expected = "environment was made whole")]
fn a_parts_setter_after_a_registry_build_panics() {
    let _ = Assembled::over(RegistryBuild::default()).config(&[]);
}

#[test]
fn lock_writes_the_lock_the_compile_read() {
    let project = Assembled::new().lock(&[("@acme/x", "1.0.0", "registry")]);

    let lock = project.compiled().environment().installed().lock();
    assert!(matches!(lock, specforge_installed::LockState::Read(l) if l.entries.len() == 1));
    assert!(project.root().join("specforge.lock").exists());
}
