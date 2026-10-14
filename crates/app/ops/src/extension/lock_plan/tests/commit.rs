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
    checked.finish(dir.path(), Vec::new()).unwrap();
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
        .finish(dir.path(), Vec::new())
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
        .finish(dir.path(), Vec::new())
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
        .finish(dir.path(), Vec::new())
        .unwrap();
    assert_eq!(registry.pins(), [(app(), "k1".to_string())].into());

    // Unsigned, accepted with allow_unsigned.
    let dir = base_project();
    let installed = Installed::at(dir.path());
    let registry = app_registry(None);
    stage_app(&registry, &runtime, &installed)
        .check()
        .unwrap()
        .finish(dir.path(), Vec::new())
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
            &[],
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
            .finish(dir.path(), Vec::new())
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
