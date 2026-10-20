use super::*;

#[specforge_test(
    behavior = "update_all_extensions",
    verify = "an upgrade whose required peer no entry enables is refused, and update enables nothing"
)]
fn an_upgrade_whose_required_peer_no_entry_enables_is_refused() {
    let dir = enabled_project(vec![entry("@x/a", "1.0.0", "registry", &[])]);
    let (lock_before, config_before) = (
        std::fs::read(lock_path(dir.path())).unwrap(),
        std::fs::read(dir.path().join("specforge.json")).unwrap(),
    );
    // 1.1.0 requires `@specforge/testing`, which no entry enables.
    let registry = MemoryRegistry::new()
        .serving(old("@x/a", "1.0.0"))
        .serving(Published::new(
            b"\0asm new".to_vec(),
            declaration("@x/a", "1.1.0", &[("@specforge/testing", "^1.0")]),
        ));

    let outcome = update(
        &request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Reach::Compatible,
        ),
        &registry,
        &crate::testing::probes(),
    )
    .unwrap();

    assert!(!outcome.applied(), "{outcome:?}");
    let (name, error) = outcome.failures().next().unwrap();
    assert_eq!(name, "@x/a");
    assert_eq!(error.code, "E027", "{error:?}");
    assert_eq!(
        error.message,
        "extension '@x/a' requires peer dependency '@specforge/testing' ^1.0 which is not installed"
    );
    assert_eq!(std::fs::read(lock_path(dir.path())).unwrap(), lock_before);
    assert_eq!(
        std::fs::read(dir.path().join("specforge.json")).unwrap(),
        config_before
    );
}

#[specforge_test(
    behavior = "update_all_extensions",
    verify = "lock file records new binary hashes after update"
)]
fn an_update_installs_the_newer_binary_and_locks_its_hash() {
    let dir = enabled_project(vec![entry("@sdk/greet", "0.0.9", "registry", &[])]);
    let registry = MemoryRegistry::new()
        .serving(old("@sdk/greet", "0.0.9"))
        .serving(greet_published());

    let outcome = update(
        &request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Reach::Major,
        ),
        &registry,
        &runtime(),
    )
    .unwrap();

    assert!(outcome.applied(), "{outcome:?}");
    assert_eq!(
        outcome.updated().collect::<Vec<_>>(),
        [("@sdk/greet", "0.0.9", "0.1.0")]
    );
    assert_eq!(
        outcome.batch_update_completed(ended()),
        BatchUpdateCompleted {
            updated_count: 1,
            failed_count: 0,
            skipped_count: 0,
            timestamp: ENDED.into(),
        }
    );
    let lock = specforge_installed::read_lock_file(&lock_path(dir.path())).unwrap();
    assert_eq!(lock.entries[0].version, "0.1.0");
    assert_eq!(lock.entries[0].wasm_hash, hex_sha256(&greet()));
    assert_eq!(lock.entries[0].source, LockSource::Registry);
    let installed = installed(dir.path(), "@sdk/greet");
    assert_eq!(std::fs::read(installed).unwrap(), greet());
}

#[specforge_test(
    behavior = "update_all_extensions",
    verify = "failed upgrade rolls back all changes"
)]
fn one_failed_upgrade_applies_none() {
    let dir = enabled_project(vec![
        entry("@sdk/greet", "0.0.9", "registry", &[]),
        entry("@acme/liar", "1.0.0", "registry", &[]),
    ]);
    let lock_before = std::fs::read(lock_path(dir.path())).unwrap();
    // @acme/liar 1.1.0 is published as @acme/liar but its binary is greet's: it
    // declares other than was published, E028, as add refuses it.
    let registry = MemoryRegistry::new()
        .serving(greet_published())
        .serving(Published::new(
            greet(),
            declaration("@acme/liar", "1.1.0", &[]),
        ));

    let outcome = update(
        &request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Reach::Major,
        ),
        &registry,
        &runtime(),
    )
    .unwrap();

    assert!(!outcome.applied());
    let failures: Vec<(&str, &str)> = outcome
        .failures()
        .map(|(n, e)| (n, e.code.as_ref()))
        .collect();
    assert_eq!(failures, [("@acme/liar", "E028")]);
    // greet would have moved but did not: it is skipped, not updated.
    assert_eq!(
        outcome.batch_update_completed(ended()),
        BatchUpdateCompleted {
            updated_count: 0,
            failed_count: 1,
            skipped_count: 1,
            timestamp: ENDED.into(),
        }
    );
    assert_eq!(std::fs::read(lock_path(dir.path())).unwrap(), lock_before);
    let installed = installed(dir.path(), "@sdk/greet");
    assert_eq!(std::fs::read(installed).unwrap(), b"old");
}

#[specforge_test(
    behavior = "update_all_extensions",
    verify = "peer dependency conflicts detected before applying"
)]
fn an_update_that_breaks_a_dependent_is_refused() {
    let dir = enabled_project(vec![
        entry("@sdk/greet", "0.0.9", "registry", &[]),
        entry(
            "@acme/user",
            "1.0.0",
            "local:user.wasm",
            &[("@sdk/greet", "^0.0.9")],
        ),
    ]);
    let lock_before = std::fs::read(lock_path(dir.path())).unwrap();
    let registry = MemoryRegistry::new().serving(greet_published());

    let outcome = update(
        &request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Reach::Major,
        ),
        &registry,
        &runtime(),
    )
    .unwrap();

    assert!(!outcome.applied());
    let (name, error) = outcome.failures().next().unwrap();
    assert_eq!(name, "@sdk/greet");
    assert!(
        error
            .message
            .starts_with("updating '@sdk/greet' 0.1.0 breaks '@acme/user': "),
        "{error:?}"
    );
    assert_eq!(std::fs::read(lock_path(dir.path())).unwrap(), lock_before);
}

#[cfg(unix)]
#[specforge_test(
    behavior = "update_all_extensions",
    verify = "failed upgrade rolls back all changes"
)]
fn a_lock_that_cannot_be_written_puts_the_old_binaries_back() {
    use std::os::unix::fs::PermissionsExt;
    let dir = enabled_project(vec![entry("@sdk/greet", "0.0.9", "registry", &[])]);
    let lock_before = std::fs::read(lock_path(dir.path())).unwrap();
    let registry = MemoryRegistry::new().serving(greet_published());
    // The binaries can be placed, the lock beside them cannot be written.
    let mode = |m| std::fs::Permissions::from_mode(m);
    std::fs::set_permissions(dir.path(), mode(0o555)).unwrap();
    if std::fs::write(dir.path().join("probe"), b"").is_ok() {
        // Running as a user permissions don't bind (root): nothing to test.
        std::fs::set_permissions(dir.path(), mode(0o755)).unwrap();
        return;
    }

    let result = update(
        &request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Reach::Major,
        ),
        &registry,
        &runtime(),
    );
    std::fs::set_permissions(dir.path(), mode(0o755)).unwrap();

    let error = result.unwrap_err();
    assert_eq!(error.code, "E033", "{error:?}");
    assert_eq!(std::fs::read(lock_path(dir.path())).unwrap(), lock_before);
    let installed = installed(dir.path(), "@sdk/greet");
    assert_eq!(std::fs::read(installed).unwrap(), b"old");
}

#[cfg(unix)]
#[specforge_test(
    behavior = "update_all_extensions",
    verify = "a write that fails while applying fails the update, naming what it could not write, and nothing is applied"
)]
fn a_write_that_fails_while_applying_fails_the_update() {
    use std::os::unix::fs::PermissionsExt;
    let dir = enabled_project(vec![
        entry("@sdk/greet", "0.0.9", "registry", &[]),
        entry("@test/probe", "0.0.9", "registry", &[]),
    ]);
    let lock_before = std::fs::read(lock_path(dir.path())).unwrap();
    let probe = crate::testing::PROBE.to_vec();
    let registry = MemoryRegistry::new()
        .serving(greet_published())
        .serving(Published::new(probe.clone(), declaration_of(&probe)));
    // The second package's directory cannot take a new file.
    let probe_dir = dir.path().join(".specforge/extensions/@test");
    let mode = |m| std::fs::Permissions::from_mode(m);
    std::fs::set_permissions(&probe_dir, mode(0o555)).unwrap();
    if std::fs::write(probe_dir.join("probe-check"), b"").is_ok() {
        // Running as a user permissions don't bind (root): nothing to test.
        std::fs::set_permissions(&probe_dir, mode(0o755)).unwrap();
        return;
    }

    let result = update(
        &request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Reach::Major,
        ),
        &registry,
        &runtime(),
    );
    std::fs::set_permissions(&probe_dir, mode(0o755)).unwrap();

    let error = result.unwrap_err();
    assert_eq!(error.code, "E032", "{error:?}");
    assert!(error.message.contains("'@test/probe'"), "{error:?}");
    assert_eq!(std::fs::read(lock_path(dir.path())).unwrap(), lock_before);
    assert_eq!(
        std::fs::read(installed(dir.path(), "@sdk/greet")).unwrap(),
        b"old"
    );
    assert_eq!(
        std::fs::read(installed(dir.path(), "@test/probe")).unwrap(),
        b"old"
    );
}

const A1_1: &[u8] = b"\0asm in-process @x/a 1.1.0";
const A2: &[u8] = b"\0asm in-process @x/a 2.0.0";
const B2: &[u8] = b"\0asm in-process @x/b 2.0.0";

/// `@x/a` 1.1.0 (wants `@x/b` ^1), `@x/a` 2.0.0 (wants ^2) and `@x/b` 2.0.0, served in process.
fn coupled_runtime() -> InProcessRuntime {
    crate::testing::candidates()
        .binary(A1_1, declaring("@x/a", "1.1.0", &[("@x/b", "^1", false)]))
        .binary(A2, declaring("@x/a", "2.0.0", &[("@x/b", "^2", false)]))
        .binary(B2, declaring("@x/b", "2.0.0", &[]))
}

/// A project locking `@x/a` 1.0.0 (wanting `@x/b` ^1) and `@x/b` 1.0.0, `@x/a` first or second.
fn coupled(a_first: bool) -> tempfile::TempDir {
    let a = entry("@x/a", "1.0.0", "registry", &[("@x/b", "^1")]);
    let b = entry("@x/b", "1.0.0", "registry", &[]);
    enabled_project(if a_first { vec![a, b] } else { vec![b, a] })
}

#[specforge_test(
    behavior = "update_all_extensions",
    verify = "every upgrade is judged against the lock as the whole update leaves it, whatever order the lock lists them in"
)]
fn an_update_is_judged_against_the_lock_it_leaves_in_any_lock_order() {
    let runtime = coupled_runtime();
    for a_first in [true, false] {
        // a -> 2.0.0 (wants b ^2) and b -> 2.0.0: fine together.
        let together = || {
            MemoryRegistry::new()
                .serving(served(&runtime, A2))
                .serving(served(&runtime, B2))
        };
        let dir = coupled(a_first);
        let outcome = update(
            &request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                Reach::Major,
            ),
            &together(),
            &runtime,
        )
        .unwrap();
        assert!(outcome.applied(), "a first: {a_first}: {outcome:?}");
        let lock = specforge_installed::read_lock_file(&lock_path(dir.path())).unwrap();
        let locked: Vec<(&str, &str)> = lock
            .entries
            .iter()
            .map(|e| (e.name.as_str(), e.version.as_str()))
            .collect();
        let mut want = [("@x/a", "2.0.0"), ("@x/b", "2.0.0")];
        if !a_first {
            want.reverse();
        }
        assert_eq!(locked, want, "the lock keeps its order");

        // a -> 1.1.0 (still wants b ^1) and b -> 2.0.0: never satisfiable together.
        let apart = MemoryRegistry::new()
            .serving(served(&runtime, A1_1))
            .serving(old("@x/b", "1.0.0"))
            .serving(served(&runtime, B2));
        let dir = coupled(a_first);
        let lock_before = std::fs::read(lock_path(dir.path())).unwrap();
        let outcome = update(
            &request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                Reach::Major,
            ),
            &apart,
            &runtime,
        )
        .unwrap();
        assert!(!outcome.applied(), "a first: {a_first}: {outcome:?}");
        let failures: Vec<(&str, &str, &str)> = outcome
            .failures()
            .map(|(n, e)| (n, e.code.as_ref(), e.message.as_str()))
            .collect();
        assert_eq!(
            failures,
            [(
                "@x/a",
                "R-RES-006",
                "version diamond: '@x/a' requires peer '@x/b' ^1 but this update locks 2.0.0; @x/b 1.0.0 would satisfy every requirer"
            )]
        );
        assert!(matches!(
            status_of(&outcome, "@x/b"),
            UpdateStatus::Updated { .. }
        ));
        assert_eq!(std::fs::read(lock_path(dir.path())).unwrap(), lock_before);
    }
}

#[cfg(unix)]
#[test]
fn an_update_writes_nothing_outside_its_project_whatever_bound_its_caller_lends() {
    let dir = enabled_project(vec![entry("@sdk/greet", "0.0.9", "registry", &[])]);
    // The lock lives in a sibling directory the project links to.
    let sibling = tempfile::tempdir().unwrap();
    let lock = lock_path(dir.path());
    std::fs::rename(&lock, sibling.path().join("specforge.lock")).unwrap();
    std::os::unix::fs::symlink(sibling.path().join("specforge.lock"), &lock).unwrap();
    let before = std::fs::read(sibling.path().join("specforge.lock")).unwrap();
    let registry = MemoryRegistry::new()
        .serving(old("@sdk/greet", "0.0.9"))
        .serving(greet_published());
    // A bound wider than the project, holding the sibling too.
    let wide = crate::Containment::within(&std::env::temp_dir());
    let root = specforge_common::ProjectRoot::of(dir.path()).unwrap();

    let error = update(
        &UpdateRequest {
            caller: crate::testing::bounded(&wide),
            ..request(&root, Reach::Major)
        },
        &registry,
        &runtime(),
    )
    .unwrap_err();

    assert_eq!(error.code, crate::OUTSIDE_ROOT, "{error:?}");
    assert_eq!(
        std::fs::read(sibling.path().join("specforge.lock")).unwrap(),
        before
    );
    assert!(registry.pins().is_empty());
}

#[specforge_test(
    behavior = "update_all_extensions",
    verify = "an upgrade served with the bytes the lock already pins is refused, never reported updated"
)]
fn an_upgrade_served_as_the_locked_bytes_is_refused_not_reported_updated() {
    // Locked at 0.1.0 with greet's bytes, which declare 0.1.0; the registry serves them as 0.2.0.
    let dir = enabled_project(vec![LockFileEntry {
        wasm_hash: hex_sha256(&greet()),
        ..entry("@sdk/greet", "0.1.0", "registry", &[])
    }]);
    std::fs::write(installed(dir.path(), "@sdk/greet"), greet()).unwrap();
    let lock_before = std::fs::read(lock_path(dir.path())).unwrap();
    let registry = MemoryRegistry::new().serving(Published::new(
        greet(),
        declaration("@sdk/greet", "0.2.0", &[]),
    ));

    let outcome = update(
        &request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Reach::Major,
        ),
        &registry,
        &runtime(),
    )
    .unwrap();

    assert!(!outcome.applied(), "{outcome:?}");
    let failures: Vec<(&str, &str)> = outcome
        .failures()
        .map(|(n, e)| (n, e.message.as_str()))
        .collect();
    assert_eq!(
        failures,
        [(
            "@sdk/greet",
            "registry package @sdk/greet@0.2.0 declares itself @sdk/greet@0.1.0"
        )]
    );
    assert!(outcome.extension_upgrade_completed().is_empty());
    assert_eq!(std::fs::read(lock_path(dir.path())).unwrap(), lock_before);
}
