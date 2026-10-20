use super::*;

#[specforge_test(
    behavior = "update_all_extensions",
    verify = "major version skipped without --major flag"
)]
fn a_new_major_version_needs_major() {
    // ^0.0.9 admits only 0.0.9: 0.1.0 is a breaking change.
    let dir = enabled_project(vec![entry("@sdk/greet", "0.0.9", "registry", &[])]);
    let registry = MemoryRegistry::new()
        .serving(old("@sdk/greet", "0.0.9"))
        .serving(greet_published());

    let outcome = update(
        &request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Reach::Compatible,
        ),
        &registry,
        &runtime(),
    )
    .unwrap();

    // Asked for what ^0.0.9 admits: 0.1.0 is not it.
    assert_eq!(registry.listed(), [name("@sdk/greet")]);
    assert_eq!(
        status_of(&outcome, "@sdk/greet"),
        &UpdateStatus::UpToDate {
            version: "0.0.9".into()
        }
    );
    assert_eq!(
        specforge_installed::read_lock_file(&lock_path(dir.path()))
            .unwrap()
            .entries[0]
            .version,
        "0.0.9"
    );
}

#[specforge_test(
    behavior = "update_all_extensions",
    verify = "an update never moves an extension to its locked version or below, and a locked version no longer published is up to date"
)]
fn major_never_moves_a_pre_release_lock_to_a_lower_release() {
    // --major admits any version above 0.2.0-beta.1; 0.1.0 is below it.
    let dir = enabled_project(vec![entry("@sdk/greet", "0.2.0-beta.1", "registry", &[])]);
    let lock_before = std::fs::read(lock_path(dir.path())).unwrap();
    let registry = MemoryRegistry::new()
        .serving(greet_published())
        .serving(old("@sdk/greet", "0.2.0-beta.1"));

    let outcome = update(
        &request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Reach::Major,
        ),
        &registry,
        &runtime(),
    )
    .unwrap();

    assert_eq!(
        status_of(&outcome, "@sdk/greet"),
        &UpdateStatus::UpToDate {
            version: "0.2.0-beta.1".into()
        }
    );
    assert!(!fetched(&registry), "{:?}", registry.asked());
    assert!(outcome.extension_upgrade_completed().is_empty());
    assert_eq!(std::fs::read(lock_path(dir.path())).unwrap(), lock_before);
}

#[specforge_test(
    behavior = "update_all_extensions",
    verify = "an update never moves an extension to its locked version or below, and a locked version no longer published is up to date"
)]
fn a_vanished_lock_with_nothing_newer_is_up_to_date() {
    // 0.3.0 is no longer published; only 0.1.0 is.
    for reach in [Reach::Compatible, Reach::Major] {
        let dir = enabled_project(vec![entry("@sdk/greet", "0.3.0", "registry", &[])]);
        let lock_before = std::fs::read(lock_path(dir.path())).unwrap();
        let registry = MemoryRegistry::new().serving(greet_published());

        let outcome = update(
            &request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                reach,
            ),
            &registry,
            &runtime(),
        )
        .unwrap();

        assert!(outcome.applied(), "{reach:?}: {outcome:?}");
        assert_eq!(
            status_of(&outcome, "@sdk/greet"),
            &UpdateStatus::UpToDate {
                version: "0.3.0".into()
            },
            "{reach:?}"
        );
        assert!(!fetched(&registry), "{reach:?}: {:?}", registry.asked());
        assert!(outcome.extension_upgrade_completed().is_empty());
        assert_eq!(std::fs::read(lock_path(dir.path())).unwrap(), lock_before);
    }
}

#[specforge_test(
    behavior = "update_all_extensions",
    verify = "an update never moves an extension to its locked version or below, and a locked version no longer published is up to date"
)]
fn a_vanished_lock_holds_back_no_other_upgrade() {
    // @x/gone 1.2.0 is no longer published; only 1.0.0 is: it is up to date, and greet's
    // upgrade (0.1.0-alpha.1 -> 0.1.0, inside ^0.1.0-alpha.1) is applied.
    let dir = enabled_project(vec![
        entry("@sdk/greet", "0.1.0-alpha.1", "registry", &[]),
        entry("@x/gone", "1.2.0", "registry", &[]),
    ]);
    let registry = MemoryRegistry::new()
        .serving(greet_published())
        .serving(old("@x/gone", "1.0.0"));

    let outcome = update(
        &request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Reach::Compatible,
        ),
        &registry,
        &runtime(),
    )
    .unwrap();

    assert!(outcome.applied(), "{outcome:?}");
    assert_eq!(
        outcome.updated().collect::<Vec<_>>(),
        [("@sdk/greet", "0.1.0-alpha.1", "0.1.0")]
    );
    assert_eq!(
        status_of(&outcome, "@x/gone"),
        &UpdateStatus::UpToDate {
            version: "1.2.0".into()
        }
    );
    assert_eq!(
        outcome.batch_update_completed(ended()),
        BatchUpdateCompleted {
            updated_count: 1,
            failed_count: 0,
            skipped_count: 1,
            timestamp: ENDED.into(),
        }
    );
    assert_eq!(
        outcome.extension_upgrade_completed(),
        [UpgradeCompleted {
            extension_name: "@sdk/greet".into(),
            previous_version: "0.1.0-alpha.1".into(),
            new_version: "0.1.0".into(),
        }]
    );
    let lock = specforge_installed::read_lock_file(&lock_path(dir.path())).unwrap();
    let locked: Vec<(&str, &str)> = lock
        .entries
        .iter()
        .map(|e| (e.name.as_str(), e.version.as_str()))
        .collect();
    assert_eq!(locked, [("@sdk/greet", "0.1.0"), ("@x/gone", "1.2.0")]);
}

#[specforge_test(
    behavior = "update_all_extensions",
    verify = "newer versions detected from registry"
)]
fn an_unreadable_locked_version_moves_to_what_add_installs() {
    // A lock edited by hand to a version that is no SemVer: either reach moves it to what
    // `latest` picks (the highest release), which is what `specforge add <name>` installs.
    for reach in [Reach::Compatible, Reach::Major] {
        let dir = enabled_project(vec![entry("@sdk/greet", "not-semver", "registry", &[])]);
        let registry = MemoryRegistry::new()
            .serving(greet_published())
            .serving(old("@sdk/greet", "0.2.0-beta.1"));

        let outcome = update(
            &request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                reach,
            ),
            &registry,
            &runtime(),
        )
        .unwrap();

        assert!(outcome.applied(), "{reach:?}: {outcome:?}");
        assert_eq!(
            outcome.updated().collect::<Vec<_>>(),
            [("@sdk/greet", "not-semver", "0.1.0")],
            "{reach:?}"
        );
    }
}

const T1_5: &[u8] = b"\0asm in-process @x/t 1.5.0";
const T2: &[u8] = b"\0asm in-process @x/t 2.0.0";
const P_BETA_2: &[u8] = b"\0asm in-process @x/p 2.0.0-beta.2";

#[specforge_test(
    behavior = "update_all_extensions",
    verify = "semver-compatible upgrades applied"
)]
fn an_update_moves_to_the_newest_version_within_its_reach() {
    let runtime = crate::testing::candidates()
        .binary(T1_5, declaring("@x/t", "1.5.0", &[]))
        .binary(T2, declaring("@x/t", "2.0.0", &[]));
    for (reach, want) in [(Reach::Compatible, "1.5.0"), (Reach::Major, "2.0.0")] {
        let dir = enabled_project(vec![entry("@x/t", "1.4.0", "registry", &[])]);
        let registry = MemoryRegistry::new()
            .serving(old("@x/t", "1.4.0"))
            .serving(served(&runtime, T1_5))
            .serving(served(&runtime, T2));

        let outcome = update(
            &request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                reach,
            ),
            &registry,
            &runtime,
        )
        .unwrap();

        assert!(outcome.applied(), "{reach:?}: {outcome:?}");
        assert_eq!(
            outcome.updated().collect::<Vec<_>>(),
            [("@x/t", "1.4.0", want)],
            "{reach:?}"
        );
    }
}

#[specforge_test(
    behavior = "update_all_extensions",
    verify = "an update never moves an extension to its locked version or below, and a locked version no longer published is up to date"
)]
fn a_pre_release_lock_moves_along_its_own_track() {
    let runtime =
        crate::testing::candidates().binary(P_BETA_2, declaring("@x/p", "2.0.0-beta.2", &[]));
    for reach in [Reach::Compatible, Reach::Major] {
        let dir = enabled_project(vec![entry("@x/p", "2.0.0-beta.1", "registry", &[])]);
        let registry = MemoryRegistry::new()
            .serving(old("@x/p", "1.4.0"))
            .serving(old("@x/p", "2.0.0-beta.1"))
            .serving(served(&runtime, P_BETA_2));

        let outcome = update(
            &request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                reach,
            ),
            &registry,
            &runtime,
        )
        .unwrap();

        assert!(outcome.applied(), "{reach:?}: {outcome:?}");
        assert_eq!(
            outcome.updated().collect::<Vec<_>>(),
            [("@x/p", "2.0.0-beta.1", "2.0.0-beta.2")],
            "{reach:?}"
        );
    }
}

#[test]
fn a_local_install_is_never_asked_about() {
    let dir = enabled_project(vec![entry("@sdk/greet", "0.0.9", "local:greet.wasm", &[])]);
    let registry = MemoryRegistry::new().serving(old("@sdk/greet", "9.9.9"));

    let outcome = update(
        &request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Reach::Major,
        ),
        &registry,
        &runtime(),
    )
    .unwrap();

    assert!(registry.asked().is_empty());
    assert_eq!(
        status_of(&outcome, "@sdk/greet"),
        &UpdateStatus::NotFromRegistry {
            source: "local:greet.wasm".into()
        }
    );
}
