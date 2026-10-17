use super::*;

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "an extension whose peer range is not SemVer is refused with E073 before anything is installed"
)]
fn an_unreadable_candidate_range_is_refused_e073() {
    let installed = nowhere(base_and_other(">=1.0.0"));
    let runtime = runtime();
    let mut plan = plan(&installed, &runtime, Verb::Install);
    stage(&mut plan, local(&runtime, ONE_ISH, "app.wasm"));

    let err = refused(plan);

    assert_eq!(err.code, "E073", "{err:?}");
    assert!(
        err.message.contains("'@acme/app'") && err.message.contains("'one-ish'"),
        "{err:?}"
    );
}

#[test]
fn an_unreadable_requirer_range_is_e073_naming_it() {
    let installed = nowhere(base_and_other("one-ish"));
    let registry = base_registry().serving(Published::new(
        b"\0asm app".to_vec(),
        declaration("@acme/app", "1.0.0", &[("@acme/base", "^2.0")]),
    ));
    let runtime = runtime();
    let mut plan = plan_from(&installed, &runtime, &registry, Verb::Install);
    stage_fetched(
        &mut plan,
        &PackageName::parse("@acme/app").unwrap(),
        Version::new(1, 0, 0),
    );

    let err = refused(plan);

    assert_eq!(err.code, "E073", "{err:?}");
    assert!(err.message.contains("'@acme/other'"), "{err:?}");
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "a local install whose peer is installed outside its range is refused with E027"
)]
fn a_local_candidate_out_of_range_is_e027() {
    let installed = nowhere(base_and_other("^1.0"));
    let runtime = runtime();
    let mut plan = plan(&installed, &runtime, Verb::Install);
    stage(&mut plan, local(&runtime, APP_NEEDS_BASE_2, "app.wasm"));

    let err = refused(plan);

    assert_eq!(err.code, "E027", "{err:?}");
    assert_eq!(
        err.message,
        "extension '@acme/app' requires peer dependency '@acme/base' ^2.0 but version 1.0.0 is installed"
    );
}

#[test]
fn a_staged_package_replaces_its_previous_entry() {
    // B1 at unit level: `@acme/app` 1.0.0 locked wanting base ^1; staging 2.0.0 (^2) must not
    // leave the old range among the requirers.
    let installed = nowhere(vec![
        entry("@acme/base", "1.0.0", "registry", &[]),
        entry("@acme/app", "1.0.0", "registry", &[("@acme/base", "^1")]),
    ]);
    let registry = base_registry().serving(Published::new(
        b"\0asm app2".to_vec(),
        declaration("@acme/app", "2.0.0", &[("@acme/base", "^2")]),
    ));
    let runtime = runtime();
    let mut plan = plan_from(&installed, &runtime, &registry, Verb::Install);
    stage_fetched(
        &mut plan,
        &PackageName::parse("@acme/app").unwrap(),
        Version::new(2, 0, 0),
    );

    // The staged entry replaced the previous one: base has one requirer, at ^2.
    let view = plan.view();
    let entries = ["@acme/base".to_string(), "@acme/app".to_string()];
    let set = ProjectPeers::of(&view, &entries, &mut Declarations::through(&runtime)).unwrap();
    let listed = set.requirers("@acme/base");
    assert_eq!(listed.len(), 1);
    assert_eq!(
        (listed[0].0, listed[0].1.version.as_str()),
        ("@acme/app", "^2")
    );

    let err = refused(plan);

    assert_eq!(err.code, "R-RES-006", "{err:?}");
}

#[test]
fn a_locked_extension_a_staged_one_breaks_is_named_with_the_install() {
    let installed = nowhere(vec![
        entry("@acme/base", "1.0.0", "registry", &[]),
        entry("@acme/app", "1.0.0", "registry", &[("@acme/base", "^1")]),
    ]);
    let runtime = runtime();
    let mut plan = plan(&installed, &runtime, Verb::Install);
    stage(&mut plan, local(&runtime, BASE, "base.wasm"));

    let err = refused(plan);

    assert_eq!(err.code, "E027", "{err:?}");
    assert!(
        err.message
            .starts_with("installing '@acme/base' 2.0.0 breaks '@acme/app': "),
        "{err:?}"
    );
}

#[test]
fn an_update_words_what_it_breaks() {
    let installed = nowhere(vec![
        entry("@acme/base", "1.0.0", "registry", &[]),
        entry("@acme/app", "1.0.0", "registry", &[("@acme/base", "^1")]),
    ]);
    let runtime = runtime();
    let mut plan = plan(&installed, &runtime, Verb::Update);
    stage(&mut plan, local(&runtime, BASE, "base.wasm"));

    let err = refused(plan);

    assert!(
        err.message
            .starts_with("updating '@acme/base' 2.0.0 breaks '@acme/app': "),
        "{err:?}"
    );
}

#[test]
fn a_required_peer_no_member_has_is_refused_e027() {
    let installed = nowhere(Vec::new());
    let runtime = runtime();
    let mut plan = plan(&installed, &runtime, Verb::Install);
    stage(&mut plan, local(&runtime, APP_NEEDS_BASE_2, "app.wasm"));

    let err = refused(plan);

    assert_eq!(err.code, "E027", "{err:?}");
    assert_eq!(
        err.message,
        "extension '@acme/app' requires peer dependency '@acme/base' ^2.0 which is not installed"
    );
    assert_eq!(
        err.suggestion.as_deref(),
        Some("install it with: specforge add @acme/base")
    );
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "an extension no specforge.json entry enables is not judged"
)]
fn a_locked_extension_no_entry_enables_is_not_judged() {
    let installed = nowhere(vec![
        entry("@acme/base", "1.0.0", "registry", &[]),
        entry(
            "@acme/other",
            "1.0.0",
            "registry",
            &[("@acme/base", "^1.0")],
        ),
    ]);
    let runtime = runtime();
    // No entry enables `@acme/other`: it is in the lock only.
    let mut plan = LockPlan::adding(
        &installed,
        opened::<&str>(&[]),
        &runtime,
        fetching(no_registry()),
        crate::testing::caller(),
        Purpose::Commit,
        true,
    )
    .unwrap();
    stage(&mut plan, local(&runtime, BASE, "base.wasm"));

    assert!(plan.check().is_ok());
}

#[test]
fn an_unchanged_extension_left_broken_is_not_this_plans() {
    // `@acme/other` already wants a base that is not locked at ^9: the plan touches neither.
    let installed = nowhere(vec![
        entry("@acme/base", "1.0.0", "registry", &[]),
        entry("@acme/other", "1.0.0", "registry", &[("@acme/base", "^9")]),
    ]);
    let runtime = runtime();
    let mut plan = plan(&installed, &runtime, Verb::Install);
    stage(&mut plan, local(&runtime, APP, "app.wasm"));

    assert!(plan.check().is_ok());
}

#[test]
fn edges_where_both_ends_move_are_judged_against_both_new_versions() {
    // app (wants base ^2) and base both move: judged against base 2.0.0, in either order.
    let runtime = runtime();
    for app_first in [true, false] {
        let installed = nowhere(vec![
            entry("@acme/base", "1.0.0", "registry", &[]),
            entry("@acme/app", "1.0.0", "registry", &[("@acme/base", "^1")]),
        ]);
        let mut plan = plan(&installed, &runtime, Verb::Update);
        let (app, base) = (
            local(&runtime, APP_NEEDS_BASE_2, "app.wasm"),
            local(&runtime, BASE, "base.wasm"),
        );
        if app_first {
            stage(&mut plan, app);
            stage(&mut plan, base);
        } else {
            stage(&mut plan, base);
            stage(&mut plan, app);
        }
        assert!(plan.check().is_ok(), "app first: {app_first}");
    }
}

#[test]
fn a_staged_extensions_own_peers_are_reported_before_what_it_breaks() {
    // `@acme/app` 1.0.0 is staged wanting base ^2 (locked 1.0.0); the locked `@acme/dep`
    // wants app ^2. The app's own peer comes first.
    let installed = nowhere(vec![
        entry("@acme/base", "1.0.0", "registry", &[]),
        entry("@acme/dep", "1.0.0", "registry", &[("@acme/app", "^2")]),
        entry("@acme/app", "2.0.0", "registry", &[]),
    ]);
    let runtime = runtime();
    let mut plan = plan(&installed, &runtime, Verb::Install);
    stage(&mut plan, local(&runtime, APP_NEEDS_BASE_2, "app.wasm"));

    let err = refused(plan);

    assert_eq!(
        err.message,
        "extension '@acme/app' requires peer dependency '@acme/base' ^2.0 but version 1.0.0 is installed"
    );
}

#[test]
fn a_refused_plan_loads_no_binary() {
    let installed = nowhere(base_and_other("^1.0"));
    let registry = base_registry().serving(Published::new(
        APP_NEEDS_BASE_2.to_vec(),
        declaration("@acme/app", "1.0.0", &[("@acme/base", "^2.0")]),
    ));
    let runtime = runtime();
    let mut plan = plan_from(&installed, &runtime, &registry, Verb::Install);
    stage_fetched(
        &mut plan,
        &PackageName::parse("@acme/app").unwrap(),
        Version::new(1, 0, 0),
    );

    let err = refused(plan);

    assert_eq!(err.code, "R-RES-005", "{err:?}");
    assert!(
        runtime
            .calls()
            .iter()
            .all(|call| call.extension != "__candidate"),
        "{:?}",
        runtime.calls()
    );
}

#[test]
fn a_fetched_binary_that_declares_otherwise_than_published_is_refused_after_the_gate() {
    let installed = nowhere(Vec::new());
    // Published without the peer the binary declares.
    let registry = MemoryRegistry::new().serving(Published::new(
        APP_NEEDS_BASE_2.to_vec(),
        declaration("@acme/app", "1.0.0", &[]),
    ));
    let runtime = runtime();
    let mut plan = plan_from(&installed, &runtime, &registry, Verb::Install);
    stage_fetched(
        &mut plan,
        &PackageName::parse("@acme/app").unwrap(),
        Version::new(1, 0, 0),
    );

    let err = refused(plan);

    assert!(err.is(METADATA_MISMATCH), "{err:?}");
}

#[test]
fn refused_names_each_staged_package_once_in_staging_order() {
    // The app wants base ^3 and the base breaks `other`: both are refused, once each.
    let installed = nowhere(base_and_other("^1.0"));
    let runtime = runtime();
    let mut plan = plan(&installed, &runtime, Verb::Install);
    plan.install(Ask(0), local(&runtime, APP_NEEDS_BASE_3, "app.wasm"));
    plan.install(Ask(1), local(&runtime, BASE, "base.wasm"));

    let refused = plan.check().map(drop).unwrap_err();

    let asks: Vec<Ask> = refused.failures().iter().map(|(ask, _)| *ask).collect();
    assert_eq!(asks, [Ask(0), Ask(1)]);
}

/// `bytes` published as the package the in-process probe runtime says they declare.
fn published_probe(bytes: &[u8]) -> Published {
    let declared = crate::extension::declare(&crate::testing::probes(), bytes)
        .unwrap()
        .0;
    Published::new(bytes.to_vec(), declared)
}

#[test]
fn a_builtin_peer_out_of_range_is_e027_naming_the_embedded_version() {
    let installed = nowhere(Vec::new());
    let runtime = crate::testing::probes();
    let registry = MemoryRegistry::new().serving(published_probe(crate::testing::NEEDS_SOFTWARE_9));
    let entries = ["@specforge/software".to_string()];
    let mut plan = LockPlan::adding(
        &installed,
        opened(&entries),
        &runtime,
        fetching(&registry),
        crate::testing::caller(),
        Purpose::Commit,
        true,
    )
    .unwrap();
    stage_fetched(
        &mut plan,
        &PackageName::parse("@probe/needs-software").unwrap(),
        Version::new(1, 0, 0),
    );

    let err = refused(plan);

    assert_eq!(err.code, "E027", "{err:?}");
    assert_eq!(
        err.message,
        "extension '@probe/needs-software' requires peer dependency '@specforge/software' ^9.0 but version 1.0.0 is installed"
    );
    assert_eq!(
        err.suggestion.as_deref(),
        Some(
            "'@specforge/software' is the builtin this specforge embeds, at 1.0.0: install a version of '@probe/needs-software' that accepts it"
        )
    );
    assert_eq!(
        registry.asked(),
        [Asked::Fetch(
            PackageName::parse("@probe/needs-software").unwrap(),
            Version::new(1, 0, 0)
        )],
        "a builtin is never unified"
    );
}

#[test]
fn enabling_a_builtin_a_member_rejects_is_named_with_the_enable() {
    let installed = nowhere(vec![entry(
        "@probe/needs-software",
        "1.0.0",
        "local:needs.wasm",
        &[("@specforge/software", "^9.0")],
    )]);
    let runtime = crate::testing::probes();
    let mut plan = plan(&installed, &runtime, Verb::Install);
    plan.enable(Ask(0), "@specforge/software");

    let err = refused(plan);

    assert_eq!(err.code, "E027", "{err:?}");
    assert!(
        err.message
            .starts_with("enabling '@specforge/software' 1.0.0 breaks '@probe/needs-software': "),
        "{err:?}"
    );
}

#[test]
fn a_file_entry_is_a_member_both_ways() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = crate::testing::probes();
    let installed = Installed::with_lock(
        dir.path(),
        LockState::Read(LockFile {
            lockfile_version: 1,
            entries: Vec::new(),
        }),
    );

    // `dep.wasm` wants `@probe/base` ^1: installing base 2.0.0 breaks it.
    std::fs::write(dir.path().join("dep.wasm"), crate::testing::DEP_1).unwrap();
    let entries = ["dep.wasm".to_string()];
    let mut plan = LockPlan::adding(
        &installed,
        opened(&entries),
        &runtime,
        fetching(no_registry()),
        crate::testing::caller(),
        Purpose::Commit,
        true,
    )
    .unwrap();
    stage(
        &mut plan,
        local(&runtime, crate::testing::BASE_2, "base.wasm"),
    );
    let err = refused(plan);
    assert_eq!(err.code, "E027", "{err:?}");
    assert!(
        err.message
            .starts_with("installing '@probe/base' 2.0.0 breaks '@probe/dep': "),
        "{err:?}"
    );

    // `base.wasm` is 2.0.0: the dep installed beside it does not get the ^1 it wants.
    std::fs::write(dir.path().join("base.wasm"), crate::testing::BASE_2).unwrap();
    let entries = ["base.wasm".to_string()];
    let mut plan = LockPlan::adding(
        &installed,
        opened(&entries),
        &runtime,
        fetching(no_registry()),
        crate::testing::caller(),
        Purpose::Commit,
        true,
    )
    .unwrap();
    stage(
        &mut plan,
        local(&runtime, crate::testing::DEP_1, "dep.wasm"),
    );
    let err = refused(plan);
    assert_eq!(
        err.message,
        "extension '@probe/dep' requires peer dependency '@probe/base' ^1.0 but version 2.0.0 is installed"
    );
}
