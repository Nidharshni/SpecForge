//! An ask the project already has is answered unchanged and stages nothing; a plan whose asks change
//! nothing is neither judged nor written (ADR 0138).

use super::super::super::fixtures::{enabled_project, entry};
use super::*;
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
        crate::testing::caller(),
        purpose,
    )
}

fn finished(plan: LockPlan<'_>) -> Done {
    plan.check().ok().unwrap().finish().unwrap()
}

#[test]
fn an_install_the_project_already_has_is_answered_unchanged_and_stages_nothing() {
    let dir = enabled_project(vec![entry("@acme/base", "1.0.0", "local:base.wasm", &[])]);
    let installed = Installed::at(dir.path());
    // The module the entry pins spells the declaration it records.
    let old = crate::extension::fixtures::module_of("@acme/base", "1.0.0", &[]);
    let runtime = InProcessRuntime::new();
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
    let asked = plan.install(Ask(0), local(&runtime, &old, "other.wasm"));
    assert!(plan.staged.is_empty());
    assert_eq!(asked, Answer::Unchanged);
    let done = finished(plan);
    assert!(done.writes.is_empty());
    assert_eq!(std::fs::read(installed.lock_path()).unwrap(), lock);

    // Control: no entry enables it, so the install changes the project.
    let mut plan = base_plan(&installed, &runtime, &[], Purpose::Preview, no_registry());
    let asked = plan.install(Ask(0), local(&runtime, &old, "other.wasm"));
    assert_eq!(asked, Answer::Changes);
    finished(plan);

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
    let asked = plan.install(Ask(0), local(&runtime, &old, "other.wasm"));
    assert_eq!(asked, Answer::Changes);
    finished(plan);
    assert_eq!(module.unwrap(), old);
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
        let asked = plan.install(Ask(0), install);
        assert!(plan.staged.is_empty());
        assert_eq!(asked, Answer::Unchanged);
        finished(plan);
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
            crate::testing::caller(),
            Purpose::Commit,
        );
        let asked = plan.enable(Ask(0), name);
        (plan, asked)
    };
    let (unchanged, asked) = plan("@specforge/product");
    assert_eq!(asked, Answer::Unchanged);
    assert_eq!(finished(unchanged).enabled, 1);
    assert_eq!(refused(plan("@specforge/formal").0).code, "E028");
}

#[test]
fn a_change_that_changes_nothing_writes_nothing_and_ignores_a_later_edit_of_the_config() {
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
        crate::testing::caller(),
        Purpose::Commit,
    );
    plan.enable(Ask(0), "@specforge/product");
    let checked = plan.check().ok().unwrap();
    let edited = r#"{"extensions":["@specforge/product","@acme/x"]}"#;
    std::fs::write(dir.path().join("specforge.json"), edited).unwrap();
    // A write would compare the config with what the plan opened and refuse `file_changed`; an unchanged plan
    // answers before that (ADR 0138 I4).
    let done = checked.finish().unwrap();
    assert!(done.writes.is_empty());
    assert_eq!(done.enabled, 1);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("specforge.json")).unwrap(),
        edited,
        "the config is the caller's edit, not rewritten"
    );
    assert!(
        !dir.path().join("specforge.lock").exists(),
        "no lock is written for a change that changes nothing"
    );
}

#[test]
fn an_install_the_lock_pins_at_another_version_is_staged_and_loaded() {
    // Locked at 1.0.0 with the bytes `old`, which the runtime reads as @acme/base 1.0.0.
    let dir = enabled_project(vec![entry("@acme/base", "1.0.0", "registry", &[])]);
    let installed = Installed::at(dir.path());
    let runtime = InProcessRuntime::new().binary(b"old", declaring("@acme/base", "1.0.0", &[]));
    // The registry serves those same bytes as 2.0.0.
    let registry = MemoryRegistry::new().serving(Published::new(
        b"old".to_vec(),
        declaration("@acme/base", "2.0.0", &[]),
    ));
    let mut plan = base_plan(
        &installed,
        &runtime,
        &["@acme/base"],
        Purpose::Commit,
        &registry,
    );
    let base = PackageName::parse("@acme/base").unwrap();
    let install = plan.fetch(&base, &Version::new(2, 0, 0)).unwrap();

    plan.install(Ask(0), install);

    assert_eq!(
        plan.staged.len(),
        1,
        "another version is not the project's already"
    );
    let error = refused(plan);
    assert_eq!(error.code, "E028", "{error:?}");
    assert_eq!(
        error.message,
        "registry package @acme/base@2.0.0 declares itself @acme/base@1.0.0"
    );
}
