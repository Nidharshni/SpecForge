//! A lock plan bounds its own writes to its project, whatever bound its caller lends (ADR 0137).

use super::commit::{app, app_registry, base_project, staging};
use super::*;
use crate::config::testing::files_under;
use crate::{Containment, OUTSIDE_ROOT};
use std::path::PathBuf;

/// A plan adding `@acme/app` over `installed`, opened with `caller`.
fn adding_app<'a>(
    installed: &'a Installed,
    runtime: &'a InProcessRuntime,
    registry: &'a MemoryRegistry,
    caller: crate::Caller<'a>,
    purpose: Purpose,
) -> LockPlan<'a> {
    let mut plan = LockPlan::adding(
        installed,
        opened(&["@acme/base"]),
        runtime,
        fetching(registry),
        caller,
        purpose,
        true,
    )
    .unwrap();
    stage_fetched(&mut plan, &app(), Version::new(1, 0, 0));
    plan
}

/// A bound wider than any project: the system temp directory, which holds the sibling too.
fn wide() -> Containment {
    Containment::within(&std::env::temp_dir())
}

#[cfg(unix)]
#[test]
fn every_write_of_a_lock_plan_lands_inside_its_project_whatever_bound_its_caller_lends() {
    use std::os::unix::fs::symlink;
    let runtime = runtime();
    let wide = wide();
    for linked in ["lock", "config", "modules", "also"] {
        let (registry, dir, outside) = (
            app_registry(Some("k1")),
            base_project(),
            tempfile::tempdir().unwrap(),
        );
        let root = dir.path();
        let link = |path: PathBuf| {
            let target = outside.path().join(path.file_name().unwrap());
            std::fs::rename(&path, &target).unwrap();
            symlink(&target, &path).unwrap();
        };
        let mut also = Vec::new();
        match linked {
            "lock" => link(specforge_installed::lock_path(root)),
            "config" => link(root.join("specforge.json")),
            "modules" => link(root.join(".specforge/extensions")),
            _ => {
                let target = outside.path().join("victim");
                std::fs::write(&target, "original\n").unwrap();
                symlink(&target, root.join(".gitignore")).unwrap();
                also.push(staging(&root.join(".gitignore"), "x"));
            }
        }
        let before = files_under(outside.path());
        let installed = Installed::at(root);

        let error = adding_app(
            &installed,
            &runtime,
            &registry,
            crate::testing::bounded(&wide),
            Purpose::Commit,
        )
        .check()
        .ok()
        .unwrap()
        .writing(also)
        .finish()
        .unwrap_err();

        assert_eq!(error.code, OUTSIDE_ROOT, "{linked}: {error:?}");
        assert_eq!(files_under(outside.path()), before, "{linked}");
        assert!(registry.pins().is_empty(), "{linked}: nothing is pinned");
        assert!(
            !root.join(".specforge/extensions/@acme/app").exists(),
            "{linked}: no module was placed"
        );
    }
}

#[cfg(unix)]
#[test]
fn an_unbounded_caller_is_not_refused_for_its_own_link() {
    let (runtime, registry, dir) = (runtime(), app_registry(Some("k1")), base_project());
    let outside = tempfile::tempdir().unwrap();
    let lock = specforge_installed::lock_path(dir.path());
    let target = outside.path().join("specforge.lock");
    std::fs::rename(&lock, &target).unwrap();
    std::os::unix::fs::symlink(&target, &lock).unwrap();
    let installed = Installed::at(dir.path());

    adding_app(
        &installed,
        &runtime,
        &registry,
        crate::testing::caller(),
        Purpose::Commit,
    )
    .check()
    .ok()
    .unwrap()
    .finish()
    .unwrap();

    // The CLI's own link is the user's: the change is written, not refused (ADR 0092 D6).
    let lock = specforge_installed::read_lock_file(&lock).unwrap();
    assert!(lock.entries.iter().any(|e| e.name == app()));
}

#[test]
fn a_bound_that_does_not_hold_the_project_is_refused_not_widened() {
    let (runtime, registry, dir) = (runtime(), app_registry(Some("k1")), base_project());
    std::fs::create_dir(dir.path().join("spec")).unwrap();
    let narrow = Containment::within(&dir.path().join("spec"));
    let before = files_under(dir.path());
    let installed = Installed::at(dir.path());

    let error = adding_app(
        &installed,
        &runtime,
        &registry,
        crate::testing::bounded(&narrow),
        Purpose::Commit,
    )
    .check()
    .ok()
    .unwrap()
    .finish()
    .unwrap_err();

    assert_eq!(error.code, OUTSIDE_ROOT, "{error:?}");
    assert_eq!(files_under(dir.path()), before);
}

#[cfg(unix)]
#[test]
fn a_preview_is_bounded_like_a_commit() {
    let (runtime, registry, dir) = (runtime(), app_registry(Some("k1")), base_project());
    let outside = tempfile::tempdir().unwrap();
    let lock = specforge_installed::lock_path(dir.path());
    let target = outside.path().join("specforge.lock");
    std::fs::rename(&lock, &target).unwrap();
    std::os::unix::fs::symlink(&target, &lock).unwrap();
    let wide = wide();
    let installed = Installed::at(dir.path());

    let error = adding_app(
        &installed,
        &runtime,
        &registry,
        crate::testing::bounded(&wide),
        Purpose::Preview,
    )
    .check()
    .ok()
    .unwrap()
    .finish()
    .unwrap_err();

    assert_eq!(error.code, OUTSIDE_ROOT, "{error:?}");
}
