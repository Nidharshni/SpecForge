use super::*;

/// `@acme/app` 1.0.0 (needs base `^1.0`), published signed by `signer` (unsigned when `None`).
fn app_registry(signer: Option<&str>) -> MemoryRegistry {
    let app = Published::new(
        APP_NEEDS_BASE_1.to_vec(),
        crate::extension::declare(&runtime(), APP_NEEDS_BASE_1)
            .expect("a publishable binary")
            .0,
    );
    MemoryRegistry::new().serving(match signer {
        Some(key) => app.signed_by(key),
        None => app,
    })
}

/// A project on disk locking base 1.0.0, with a `specforge.json`.
fn base_project() -> tempfile::TempDir {
    let dir =
        super::super::super::fixtures::project(vec![entry("@acme/base", "1.0.0", "registry", &[])]);
    std::fs::write(
        dir.path().join("specforge.json"),
        "{\"extensions\":[\"@acme/base\"]}",
    )
    .unwrap();
    dir
}

/// `@acme/app` 1.0.0 fetched from `registry` and staged over the lock in `installed`.
fn stage_app<'a>(
    registry: &'a MemoryRegistry,
    runtime: &'a InProcessRuntime,
    installed: &'a Installed,
) -> LockPlan<'a> {
    let mut plan = plan_from(installed, runtime, registry, Verb::Install);
    stage_fetched(&mut plan, &app(), Version::new(1, 0, 0));
    plan
}

fn app() -> PackageName {
    PackageName::parse("@acme/app").unwrap()
}

#[specforge_test(
    behavior = "pin_publisher_key",
    verify = "a lock plan pins the keys its fetches decided only when it commits"
)]
fn a_lock_plan_pins_what_its_fetches_decided_only_when_it_commits() {
    let runtime = runtime();

    // Refused by the gate (app wants base ^2, base 1.0.0 is locked): nothing is pinned.
    let installed = nowhere(base_and_other("^1.0"));
    let refused_app = base_registry().serving(
        Published::new(
            APP_NEEDS_BASE_2.to_vec(),
            declaration("@acme/app", "1.0.0", &[("@acme/base", "^2.0")]),
        )
        .signed_by("k1"),
    );
    let mut plan = plan_from(&installed, &runtime, &refused_app, Verb::Install);
    stage_fetched(&mut plan, &app(), Version::new(1, 0, 0));
    assert_eq!(refused(plan).code, "R-RES-005");
    assert!(refused_app.pins().is_empty());

    // Checked but not committed: nothing is pinned either.
    let dir = base_project();
    let installed = Installed::at(dir.path());
    let registry = app_registry(Some("k1"));
    let checked = stage_app(&registry, &runtime, &installed).check().unwrap();
    assert!(registry.pins().is_empty(), "a check pins nothing");
    drop(checked);
    assert!(registry.pins().is_empty());

    // Committed: pinned.
    let checked = stage_app(&registry, &runtime, &installed).check().unwrap();
    checked
        .finish(dir.path(), Vec::new(), &crate::Containment::unbounded())
        .unwrap();
    assert_eq!(registry.pins(), [(app(), "k1".to_string())].into());
}

#[specforge_test(
    invariant = "registry_integrity",
    verify = "the hash and the key id are recorded in the lock file"
)]
fn a_committed_registry_install_locks_its_hash_and_key_id() {
    let (runtime, registry) = (runtime(), app_registry(Some("k1")));
    let dir = base_project();
    let installed = Installed::at(dir.path());

    stage_app(&registry, &runtime, &installed)
        .check()
        .unwrap()
        .finish(dir.path(), Vec::new(), &crate::Containment::unbounded())
        .unwrap();

    let lock =
        specforge_installed::read_lock_file(&specforge_installed::lock_path(dir.path())).unwrap();
    let locked = lock
        .entries
        .iter()
        .find(|e| e.name == app())
        .expect("the app is locked");
    assert_eq!(
        locked.wasm_hash,
        specforge_installed::hex_sha256(APP_NEEDS_BASE_1)
    );
    assert_eq!(locked.key_id.as_deref(), Some("k1"));
}

#[specforge_test(
    invariant = "extension_operation_atomicity",
    verify = "a pin that cannot be saved puts back everything the install wrote"
)]
fn a_pin_that_cannot_be_saved_puts_back_everything_the_install_wrote() {
    let (runtime, registry) = (runtime(), app_registry(Some("k1")));
    let registry = registry.refusing_pins();
    let dir = base_project();
    let installed = Installed::at(dir.path());
    let lock_file = specforge_installed::lock_path(dir.path());
    let config = dir.path().join("specforge.json");
    let module = installed.module_path(&app());
    let (lock_before, config_before) = (
        std::fs::read(&lock_file).unwrap(),
        std::fs::read(&config).unwrap(),
    );

    let error = stage_app(&registry, &runtime, &installed)
        .check()
        .unwrap()
        .finish(dir.path(), Vec::new(), &crate::Containment::unbounded())
        .unwrap_err();

    assert!(error.is(codes::R_TRUST_006), "{error:?}");
    assert_eq!(std::fs::read(&lock_file).unwrap(), lock_before);
    assert_eq!(std::fs::read(&config).unwrap(), config_before);
    assert!(!module.exists(), "the placed module is taken away");
    assert!(registry.pins().is_empty());
}

#[test]
fn an_already_pinned_or_unsigned_install_pins_nothing() {
    let runtime = runtime();
    let dir = base_project();
    let installed = Installed::at(dir.path());

    // The user already pinned this key.
    let registry = app_registry(Some("k1")).pinning("@acme/app", "k1");
    stage_app(&registry, &runtime, &installed)
        .check()
        .unwrap()
        .finish(dir.path(), Vec::new(), &crate::Containment::unbounded())
        .unwrap();
    assert_eq!(registry.pins(), [(app(), "k1".to_string())].into());

    // Unsigned, accepted with allow_unsigned.
    let dir = base_project();
    let installed = Installed::at(dir.path());
    let registry = app_registry(None);
    stage_app(&registry, &runtime, &installed)
        .check()
        .unwrap()
        .finish(dir.path(), Vec::new(), &crate::Containment::unbounded())
        .unwrap();
    assert!(registry.pins().is_empty());
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "add enables the builtins a required builtin peer requires, dependencies first"
)]
fn each_ask_is_answered_with_the_builtins_its_extension_required() {
    let builtin = |runtime, name: &'static str, peers| {
        crate::testing::serving_builtin(runtime, name, declaring(name, "1.0.0", peers))
    };
    let runtime = builtin(
        builtin(
            runtime(),
            "@specforge/formal",
            &[("@specforge/software", "^1.0", false)],
        ),
        "@specforge/software",
        &[],
    );
    let enabling = |asked: &[&'static str]| {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("specforge.json"),
            "{\"name\":\"p\",\"version\":\"0.1.0\",\"extensions\":[]}",
        )
        .unwrap();
        let installed = Installed::at(dir.path());
        let mut plan = LockPlan::adding(
            &installed,
            opened_at(dir.path()),
            &runtime,
            fetching(no_registry()),
            Purpose::Commit,
            false,
        )
        .unwrap();
        for (at, name) in asked.iter().enumerate() {
            plan.enable(Ask(at), name);
        }
        plan.check()
            .ok()
            .unwrap()
            .finish(dir.path(), Vec::new(), &crate::Containment::unbounded())
            .unwrap()
    };

    // software first: formal's requirement is already met by the earlier ask.
    let done = enabling(&["@specforge/software", "@specforge/formal"]);
    assert_eq!(done.asked(Ask(0)).peers_enabled, Vec::<&str>::new());
    assert_eq!(done.asked(Ask(1)).peers_enabled, Vec::<&str>::new());
    assert!(done.asked(Ask(0)).appended && done.asked(Ask(1)).appended);

    // formal alone enables software with it.
    let done = enabling(&["@specforge/formal"]);
    assert_eq!(done.asked(Ask(0)).peers_enabled, ["@specforge/software"]);
    assert_eq!(done.enabled, 2);
}

#[cfg(unix)]
#[test]
fn a_change_whose_lock_is_a_symlink_that_leaves_the_project_writes_nothing() {
    let (runtime, registry) = (runtime(), app_registry(Some("k1")));
    let dir = base_project();
    let outside = tempfile::tempdir().unwrap();
    let elsewhere = outside.path().join("specforge.lock");
    let lock = specforge_installed::lock_path(dir.path());
    std::fs::copy(&lock, &elsewhere).unwrap();
    std::fs::remove_file(&lock).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &lock).unwrap();
    let before = std::fs::read(&elsewhere).unwrap();
    let installed = Installed::at(dir.path());

    let error = stage_app(&registry, &runtime, &installed)
        .check()
        .unwrap()
        .finish(
            dir.path(),
            Vec::new(),
            &crate::Containment::within(dir.path()),
        )
        .unwrap_err();

    assert_eq!(error.code, crate::OUTSIDE_ROOT, "{error:?}");
    assert_eq!(std::fs::read(&elsewhere).unwrap(), before);
    assert!(registry.pins().is_empty(), "nothing is pinned either");
    assert!(
        !dir.path().join(".specforge/extensions/@acme/app").exists(),
        "no module was placed"
    );
}

#[cfg(unix)]
#[test]
fn a_pin_failure_whose_rollback_cannot_put_the_config_back_names_it() {
    let (runtime, registry) = (runtime(), app_registry(Some("k1")));
    let dir = base_project();
    let config = dir.path().join("specforge.json");
    let probe = dir.path().join("probe");
    std::fs::write(&probe, "x").unwrap();
    let mut readonly = std::fs::metadata(&probe).unwrap().permissions();
    readonly.set_readonly(true);
    std::fs::set_permissions(&probe, readonly).unwrap();
    if std::fs::OpenOptions::new().write(true).open(&probe).is_ok() {
        return; // permissions don't bind this user (root)
    }
    let registry = registry.refusing_pins_after_locking(&config);
    let installed = Installed::at(dir.path());
    let lock_file = specforge_installed::lock_path(dir.path());
    let module = installed.module_path(&app());
    let lock_before = std::fs::read(&lock_file).unwrap();

    let error = stage_app(&registry, &runtime, &installed)
        .check()
        .unwrap()
        .finish(dir.path(), Vec::new(), &crate::Containment::unbounded())
        .unwrap_err();

    assert!(error.is(codes::R_TRUST_006), "{error:?}");
    assert_eq!(
        error.writes.paths().collect::<Vec<_>>(),
        [config.as_path()],
        "the config the change wrote and could not put back is named"
    );
    assert!(
        std::fs::read_to_string(&config)
            .unwrap()
            .contains("@acme/app"),
        "left as the change wrote it, whole"
    );
    assert_eq!(std::fs::read(&lock_file).unwrap(), lock_before);
    assert!(!module.exists(), "the placed module is taken away");
}

/// An [`Also`] that stages a new file `at` with `text`.
fn staging(at: &Path, text: &'static str) -> Also<'static> {
    Also {
        path: at.to_path_buf(),
        stage: Box::new(move |held, change| {
            change.stage(held, None, text.as_bytes().to_vec());
            Ok(())
        }),
        code: "init_write_failed",
    }
}

#[cfg(unix)]
#[specforge_test(
    behavior = "uninstall_wasm_extension",
    verify = "rolls back on failure"
)]
fn an_entries_only_plan_accepts_an_unreadable_lock_and_puts_the_config_back() {
    use std::os::unix::fs::PermissionsExt;
    let runtime = crate::testing::serving_builtin(
        runtime(),
        "@specforge/software",
        declaring("@specforge/software", "1.0.0", &[]),
    );
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("specforge.json");
    let before = "{\"name\":\"p\",\"version\":\"0.1.0\",\"extensions\":[]}";
    std::fs::write(&config, before).unwrap();
    // A lock that cannot be read: a change that writes it is refused (E033), one that does not accepts it.
    let lock = specforge_installed::lock_path(dir.path());
    std::fs::create_dir_all(lock.parent().unwrap()).unwrap();
    std::fs::write(&lock, "{ not a lock").unwrap();
    let locked = dir.path().join("locked");
    std::fs::create_dir(&locked).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
    if std::fs::write(locked.join("probe"), "x").is_ok() {
        return; // permissions don't bind this user (root)
    }
    let installed = Installed::at(dir.path());
    let mut plan = LockPlan::adding(
        &installed,
        opened_at(dir.path()),
        &runtime,
        fetching(no_registry()),
        Purpose::Commit,
        false,
    )
    .unwrap();
    plan.enable(Ask(0), "@specforge/software");

    // The second file cannot be created: the config the change wrote is put back.
    let error = plan
        .check()
        .ok()
        .unwrap()
        .finish(
            dir.path(),
            vec![staging(&locked.join("new.txt"), "x")],
            &crate::Containment::unbounded(),
        )
        .unwrap_err();

    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(error.code, "init_write_failed", "{error:?}");
    assert!(error.writes.is_empty(), "{error:?}");
    assert_eq!(std::fs::read_to_string(&config).unwrap(), before);
    assert_eq!(std::fs::read_to_string(&lock).unwrap(), "{ not a lock");
}

#[specforge_test(
    invariant = "extension_operation_atomicity",
    verify = "a failed change puts back every project file it wrote and removes the directories it created"
)]
fn a_failed_change_puts_back_every_project_file_and_the_directories_it_made() {
    let (runtime, registry) = (runtime(), app_registry(Some("k1")).refusing_pins());
    let dir = base_project();
    let gitignore = dir.path().join(".gitignore");
    std::fs::write(&gitignore, "target\n").unwrap();
    let installed = Installed::at(dir.path());
    let config_before = std::fs::read(dir.path().join("specforge.json")).unwrap();
    let also = vec![
        staging(&dir.path().join("made/deeper/starter.spec"), "spec"),
        Also {
            path: gitignore.clone(),
            stage: Box::new(|held, change| {
                let before = held.read_if_there().unwrap();
                change.stage(held, before, b"target\n.specforge\n".to_vec());
                Ok(())
            }),
            code: "init_write_failed",
        },
    ];

    let error = stage_app(&registry, &runtime, &installed)
        .check()
        .unwrap()
        .finish(dir.path(), also, &crate::Containment::unbounded())
        .unwrap_err();

    assert!(error.is(codes::R_TRUST_006), "{error:?}");
    assert!(error.writes.is_empty(), "{error:?}");
    assert_eq!(std::fs::read_to_string(&gitignore).unwrap(), "target\n");
    assert_eq!(
        std::fs::read(dir.path().join("specforge.json")).unwrap(),
        config_before
    );
    assert!(
        !dir.path().join("made").exists(),
        "the directories it made are removed"
    );
    assert!(!installed.module_path(&app()).exists());
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "an add over a specforge.json changed since it was read writes nothing"
)]
fn an_add_over_a_config_changed_since_it_was_read_writes_nothing() {
    let (runtime, registry) = (runtime(), app_registry(Some("k1")));
    let dir = base_project();
    let installed = Installed::at(dir.path());
    let lock_file = specforge_installed::lock_path(dir.path());
    let config = dir.path().join("specforge.json");
    let module = installed.module_path(&app());
    let lock_before = std::fs::read(&lock_file).unwrap();
    let checked = stage_app(&registry, &runtime, &installed).check().unwrap();
    // The base is disabled by hand after the plan judged on it.
    std::fs::write(&config, "{\"extensions\":[]}").unwrap();

    let error = checked
        .finish(dir.path(), Vec::new(), &crate::Containment::unbounded())
        .unwrap_err();

    assert_eq!(error.code, "file_changed", "{error:?}");
    assert_eq!(std::fs::read(&lock_file).unwrap(), lock_before);
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        "{\"extensions\":[]}"
    );
    assert!(!module.exists());
    assert!(registry.pins().is_empty());
}
