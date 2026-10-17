use super::*;

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "a peer locked outside the new extension's range fails R-RES-006 naming a version that satisfies every requirer"
)]
fn a_unifiable_diamond_is_refused_with_the_version_that_unifies() {
    let installed = nowhere(base_and_other(">=1.0.0"));
    let (registry, runtime) = (base_registry(), runtime());
    let app = Published::new(
        b"\0asm app".to_vec(),
        declaration("@acme/app", "1.0.0", &[("@acme/base", "^2.0")]),
    );
    let registry = registry.serving(app);
    let mut plan = plan_from(&installed, &runtime, &registry, Verb::Install);
    stage_fetched(
        &mut plan,
        &PackageName::parse("@acme/app").unwrap(),
        Version::new(1, 0, 0),
    );

    let err = refused(plan);

    assert_eq!(err.code, "R-RES-006");
    assert_eq!(
        err.message,
        "version diamond: '@acme/app' requires peer '@acme/base' ^2.0 but 1.0.0 is locked; @acme/base 2.0.0 would satisfy every requirer"
    );
    assert_eq!(
        err.suggestion.as_deref(),
        Some(
            "install '@acme/base' at 2.0.0 (specforge add @acme/base@2.0.0), or at another version every requirer accepts, then retry; when a locked extension must move with it, specforge update --major moves them together"
        )
    );
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "a peer no single version satisfies for every requirer fails R-RES-005"
)]
fn an_impossible_diamond_is_refused_naming_each_requirer() {
    let installed = nowhere(base_and_other("^1.0"));
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

    assert_eq!(err.code, "R-RES-005");
    assert!(
        err.message.contains("@acme/other wants @acme/base ^1.0")
            && err.message.contains("@acme/app wants @acme/base ^2.0"),
        "{err:?}"
    );
}

#[test]
fn a_locked_peer_at_a_version_that_is_not_semver_is_unified_like_one_out_of_range() {
    let mut entries = base_and_other("^1.0");
    entries[0].version = "local".to_string();
    let installed = nowhere(entries);
    let registry = base_registry().serving(Published::new(
        b"\0asm app".to_vec(),
        declaration("@acme/app", "1.0.0", &[("@acme/base", ">=1.0.0")]),
    ));
    let runtime = runtime();
    let mut plan = plan_from(&installed, &runtime, &registry, Verb::Install);
    stage_fetched(
        &mut plan,
        &PackageName::parse("@acme/app").unwrap(),
        Version::new(1, 0, 0),
    );

    let err = refused(plan);

    assert_eq!(err.code, "R-RES-006", "{err:?}");
    assert!(err.message.contains("@acme/base 1.4.0"), "{err:?}");
}

#[test]
fn a_satisfied_plan_asks_the_registry_nothing_more() {
    let installed = nowhere(base_and_other("^1.0"));
    let runtime = runtime();
    let registry = base_registry().serving(Published::new(
        APP_NEEDS_BASE_1.to_vec(),
        crate::extension::declare(&runtime, APP_NEEDS_BASE_1)
            .unwrap()
            .0,
    ));
    let mut plan = plan_from(&installed, &runtime, &registry, Verb::Install);
    stage_fetched(
        &mut plan,
        &PackageName::parse("@acme/app").unwrap(),
        Version::new(1, 0, 0),
    );

    plan.check().map(drop).unwrap();

    assert_eq!(
        registry.asked(),
        [Asked::Fetch(
            PackageName::parse("@acme/app").unwrap(),
            Version::new(1, 0, 0)
        )]
    );
}

#[test]
fn a_registry_failure_listing_the_peers_versions_is_the_error() {
    let installed = nowhere(base_and_other("^1.0"));
    // `@acme/base` is not served: listing its versions fails R-RES-001.
    let registry = MemoryRegistry::new().serving(Published::new(
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

    assert_eq!(err.code, "R-RES-001", "{err:?}");
}

#[test]
fn a_diamond_counts_a_file_entrys_range_among_the_requirers() {
    const X_NEEDS_BASE_1: &[u8] = b"\0asm in-process @acme/x 1.0.0 needs base 1";
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("x.wasm"), X_NEEDS_BASE_1).unwrap();
    let installed = Installed::with_lock(
        dir.path(),
        LockState::Read(LockFile {
            lockfile_version: 1,
            entries: vec![entry("@acme/base", "1.0.0", "registry", &[])],
        }),
    );
    let runtime = runtime().binary(
        X_NEEDS_BASE_1,
        declaring("@acme/x", "1.0.0", &[("@acme/base", "^1", false)]),
    );
    // The app wants base ^2: base 2.0.0 alone would unify, but the file entry wants ^1.
    let registry = base_registry().serving(Published::new(
        b"\0asm app".to_vec(),
        declaration("@acme/app", "1.0.0", &[("@acme/base", "^2.0")]),
    ));
    let entries = ["@acme/base".to_string(), "x.wasm".to_string()];
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
        &PackageName::parse("@acme/app").unwrap(),
        Version::new(1, 0, 0),
    );

    let err = refused(plan);

    assert_eq!(err.code, "R-RES-005", "{err:?}");
    assert!(
        err.message.contains("@acme/x wants @acme/base ^1")
            && err.message.contains("@acme/app wants @acme/base ^2.0"),
        "{err:?}"
    );
}

fn req(requirer: &'static str, range: &str) -> (&'static str, PeerDependency) {
    (requirer, peer("@shared/lib", range))
}

fn unified(
    vs: &[&str],
    requirers: &[(&'static str, PeerDependency)],
) -> Result<String, Diagnostic> {
    let borrowed: Vec<(&str, &PeerDependency)> = requirers.iter().map(|(n, p)| (*n, p)).collect();
    let published: Vec<Version> = vs.iter().map(|v| Version::parse(v).unwrap()).collect();
    unify("@shared/lib", &published, &borrowed).map(|version| version.to_string())
}

/// Pinned: a pre-release is unified only when every requirer's range names its
/// `MAJOR.MINOR.PATCH`, each range read on its own; build metadata is ordered; R-RES-005 lists
/// `Available:` in the registry's order. T5 keeps every answer.
#[test]
fn pin_unify_takes_a_pre_release_only_when_every_range_names_it() {
    let published = ["1.0.0", "1.5.0", "2.0.0-beta.1"];
    // (a) no range names the beta's triple.
    let result = unified(
        &published,
        &[req("@a/ext", ">=1.0.0"), req("@b/ext", "^1.0")],
    );
    assert_eq!(result.unwrap(), "1.5.0");
    // (b) both do.
    let result = unified(
        &published,
        &[
            req("@a/ext", ">=2.0.0-beta.1"),
            req("@b/ext", "^2.0.0-beta.1"),
        ],
    );
    assert_eq!(result.unwrap(), "2.0.0-beta.1");
    // (c) one does: merged into one requirement the beta would pass, read apart it does not.
    let result = unified(
        &published,
        &[req("@a/ext", ">=2.0.0-beta.1"), req("@b/ext", ">=1.0.0")],
    );
    assert_eq!(result.unwrap_err().code, "R-RES-005");
    // (d) semver's total order.
    let result = unified(
        &["1.0.0+build.2", "1.0.0+build.1"],
        &[req("@a/ext", "^1.0")],
    );
    assert_eq!(result.unwrap(), "1.0.0+build.2");
    // (e) the registry's order.
    let err = unified(&["2.0.0", "1.0.0"], &[req("@a/ext", "^3.0")]).unwrap_err();
    assert!(err.message.ends_with("Available: 2.0.0, 1.0.0"), "{err:?}");
}

#[test]
fn unify_picks_highest_version_satisfying_every_requirer() {
    let result = unified(
        &["1.0.0", "1.5.0", "2.0.0", "2.5.0", "3.0.0"],
        &[req("@a/ext", "^2.0.0"), req("@b/ext", ">=2.0.0, <3.0.0")],
    )
    .unwrap();
    assert_eq!(result, "2.5.0");
}

#[test]
fn unify_single_requirer_behaves_like_resolve_version() {
    let result = unified(&["1.0.0", "1.2.0", "2.0.0"], &[req("@a/ext", "^1.0.0")]).unwrap();
    assert_eq!(result, "1.2.0");
}

#[test]
fn unify_incompatible_ranges_reports_diamond_conflict() {
    let err = unified(
        &["1.0.0", "1.5.0", "2.0.0", "2.5.0"],
        &[req("@a/ext", "^1.0.0"), req("@b/ext", "^2.0.0")],
    )
    .unwrap_err();
    assert_eq!(err.code, "R-RES-005");
    assert!(err.message.contains("@a/ext"));
    assert!(err.message.contains("@b/ext"));
    assert!(err.message.contains("^1.0.0"));
    assert!(err.message.contains("^2.0.0"));
}

#[test]
fn unify_malformed_range_names_the_requirer() {
    let err = unified(&["1.0.0"], &[req("@a/ext", "not-a-range")]).unwrap_err();
    assert_eq!(err.code, "E073");
    assert!(err.message.contains("@a/ext"));
}

#[test]
fn unify_no_requirers_picks_highest_available() {
    let result = unified(&["1.0.0", "2.0.0"], &[]).unwrap();
    assert_eq!(result, "2.0.0");
}
