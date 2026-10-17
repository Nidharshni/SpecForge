//! An ask the project already has is answered unchanged and stages nothing; a plan whose asks change
//! nothing is neither judged nor written (ADR 0138).

use super::super::super::fixtures::{enabled_project, entry};
use super::*;
use crate::Containment;
use crate::extension::lock_plan::Asked as Answer;
use crate::extension::lock_plan::commit::Done;

fn base_plan<'a>(
    installed: &'a Installed,
    runtime: &'a InProcessRuntime,
    enabled: &[&str],
    purpose: Purpose,
    registry: &'a MemoryRegistry,
) -> LockPlan<'a> {
    LockPlan::adding(
        installed,
        opened(enabled),
        runtime,
        fetching(registry),
        purpose,
        true,
    )
    .unwrap()
}

fn answer(plan: LockPlan<'_>, root: &Path) -> (Answer, Done) {
    let done = plan
        .check()
        .ok()
        .unwrap()
        .finish(root, Vec::new(), &Containment::unbounded())
        .unwrap();
    (done.asked(Ask(0)), done)
}

#[test]
fn an_install_the_project_already_has_is_answered_unchanged_and_stages_nothing() {
    let dir = enabled_project(vec![entry("@acme/base", "1.0.0", "local:base.wasm", &[])]);
    let installed = Installed::at(dir.path());
    let runtime = InProcessRuntime::new().binary(b"old", declaring("@acme/base", "1.0.0", &[]));
    let module = std::fs::read(installed.module_path(&PackageName::parse("@acme/base").unwrap()));
    let lock = std::fs::read(installed.lock_path()).unwrap();

    // The same bytes from another path: already there (the lock keeps the first path).
    let mut plan = base_plan(
        &installed,
        &runtime,
        &["@acme/base"],
        Purpose::Commit,
        no_registry(),
    );
    plan.install(Ask(0), local(&runtime, b"old", "other.wasm"));
    assert!(plan.staged.is_empty());
    let (asked, done) = answer(plan, dir.path());
    assert_eq!(
        asked,
        Answer::Unchanged {
            locked: Some("1.0.0".into())
        }
    );
    assert!(done.writes.is_empty());
    assert_eq!(std::fs::read(installed.lock_path()).unwrap(), lock);

    // Control: no entry enables it, so the install changes the project.
    let mut plan = base_plan(&installed, &runtime, &[], Purpose::Preview, no_registry());
    plan.install(Ask(0), local(&runtime, b"old", "other.wasm"));
    assert!(matches!(answer(plan, dir.path()).0, Answer::Changes { .. }));

    // Control: a module changed after install is reinstalled.
    let path = installed.module_path(&PackageName::parse("@acme/base").unwrap());
    std::fs::write(&path, b"changed").unwrap();
    let mut plan = base_plan(
        &installed,
        &runtime,
        &["@acme/base"],
        Purpose::Preview,
        no_registry(),
    );
    plan.install(Ask(0), local(&runtime, b"old", "other.wasm"));
    assert!(matches!(answer(plan, dir.path()).0, Answer::Changes { .. }));
    assert_eq!(module.unwrap(), b"old");
}

#[test]
fn fetching_a_package_the_project_already_has_asks_the_registry_nothing() {
    let dir = enabled_project(vec![entry("@acme/base", "1.0.0", "registry", &[])]);
    let installed = Installed::at(dir.path());
    let runtime = runtime();
    let base = PackageName::parse("@acme/base").unwrap();
    for purpose in [Purpose::Commit, Purpose::Preview] {
        let registry = base_registry();
        let mut plan = base_plan(&installed, &runtime, &["@acme/base"], purpose, &registry);
        let install = plan.fetch(&base, &Version::new(1, 0, 0)).unwrap();
        assert!(registry.asked().is_empty());
        plan.install(Ask(0), install);
        assert!(plan.staged.is_empty());
        let (asked, _) = answer(plan, dir.path());
        assert_eq!(
            asked,
            Answer::Unchanged {
                locked: Some("1.0.0".into())
            }
        );
    }
    // Control: another version is fetched.
    let registry = base_registry();
    let plan = base_plan(
        &installed,
        &runtime,
        &["@acme/base"],
        Purpose::Commit,
        &registry,
    );
    plan.fetch(&base, &Version::new(2, 0, 0)).unwrap();
    assert!(!registry.asked().is_empty());
}

#[test]
fn a_plan_whose_asks_change_nothing_is_not_judged() {
    let dir = tempfile::tempdir().unwrap();
    let installed = Installed::at(dir.path());
    // A runtime that serves no builtin: judging a builtin would refuse E028.
    let runtime = InProcessRuntime::new();
    let plan = |name: &str| {
        let mut plan = LockPlan::adding(
            &installed,
            opened(&["@specforge/product"]),
            &runtime,
            fetching(no_registry()),
            Purpose::Commit,
            false,
        )
        .unwrap();
        plan.enable(Ask(0), name);
        plan
    };
    let (asked, done) = answer(plan("@specforge/product"), dir.path());
    assert_eq!(asked, Answer::Unchanged { locked: None });
    assert_eq!(done.enabled, 1);
    assert_eq!(refused(plan("@specforge/formal")).code, "E028");
}

#[test]
fn a_change_that_changes_nothing_resolves_and_compares_nothing() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"extensions":["@specforge/product"]}"#,
    )
    .unwrap();
    let installed = Installed::at(dir.path());
    let runtime = InProcessRuntime::new();
    let mut plan = LockPlan::adding(
        &installed,
        opened_at(dir.path()),
        &runtime,
        fetching(no_registry()),
        Purpose::Commit,
        false,
    )
    .unwrap();
    plan.enable(Ask(0), "@specforge/product");
    let checked = plan.check().ok().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"extensions":["@specforge/product","@acme/x"]}"#,
    )
    .unwrap();
    let done = checked
        .finish(dir.path(), Vec::new(), &Containment::unbounded())
        .unwrap();
    assert!(done.writes.is_empty());
}
