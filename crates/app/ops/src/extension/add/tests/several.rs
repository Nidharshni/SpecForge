use super::*;

/// A project enabling nothing yet, with `lock` locked.
/// A project locking `lock`, with a `specforge.json` enabling none of it: adding a locked
/// package installs it again.
fn disabled_project(lock: Vec<LockFileEntry>) -> tempfile::TempDir {
    let dir = project(lock);
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name": "p", "version": "0.1.0", "extensions": []}"#,
    )
    .unwrap();
    dir
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "add takes several specifiers and adds them as one change"
)]
fn several_specifiers_are_added_as_one_change() {
    let dir = project_with(EMPTY_PROJECT);
    let (_files, wasm) = greet_file();
    let shown = wasm.display().to_string();
    let extensions = vec!["@specforge/product".to_string(), shown.clone()];

    let added = add(
        &several(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            extensions,
            false,
        ),
        &crate::registry::Unconfigured("add"),
        &runtime_with_product(),
    )
    .unwrap();

    let specifiers: Vec<&str> = added.added.iter().map(|a| a.specifier.as_str()).collect();
    assert_eq!(specifiers, ["@specforge/product", shown.as_str()]);
    let outcomes = outcomes(&added);
    assert_eq!(
        outcomes[0],
        AddOutcome::Builtin {
            name: "@specforge/product",
            changed: true,
            peers_enabled: Vec::new(),
        }
    );
    assert!(
        matches!(&outcomes[1], AddOutcome::Installed { name, .. } if name == "@sdk/greet"),
        "{outcomes:?}"
    );
    assert_eq!(
        added.writes.named(Some(dir.path())),
        [
            ".specforge/extensions/@sdk/greet/extension.wasm",
            "specforge.json",
            "specforge.lock"
        ]
    );
    assert_eq!(
        specforge_common::read_project_config(dir.path())
            .config
            .extensions,
        ["@specforge/product", "@sdk/greet"]
    );
    assert_eq!(added.extensions_enabled, 2);
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "an add of several specifiers that refuses one writes nothing, its refusal naming that specifier"
)]
fn a_refused_specifier_writes_nothing_and_is_named() {
    use crate::config::testing::files_under;
    let dir = project_with(EMPTY_PROJECT);
    let (_files, wasm) = greet_file();
    let missing = dir.path().join("missing.wasm");
    let before = files_under(dir.path());

    for extensions in [
        vec![
            Source::builtin("@specforge/product").into(),
            Source::Local(missing.clone()).into(),
        ],
        vec![
            Source::Local(missing.clone()).into(),
            Source::Local(wasm.clone()).into(),
        ],
    ] {
        let error = add(
            &several(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                extensions,
                false,
            ),
            &crate::registry::Unconfigured("add"),
            &runtime_with_product(),
        )
        .unwrap_err();

        assert_eq!(error.error.code, "E040", "{error:?}");
        assert_eq!(
            error.error.message,
            format!(
                "adding '{}': no extension binary at {}",
                missing.display(),
                missing.display()
            )
        );
        assert_eq!(files_under(dir.path()), before);
    }

    // One specifier: its refusal is the add's, as it was.
    let error = add(
        &several(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            vec![Source::Local(missing.clone()).into()],
            false,
        ),
        &crate::registry::Unconfigured("add"),
        &runtime(),
    )
    .unwrap_err();
    assert_eq!(
        error.error.message,
        format!("no extension binary at {}", missing.display())
    );
}

const A2: &[u8] = b"\0asm in-process @x/a 2.0.0";
const B2: &[u8] = b"\0asm in-process @x/b 2.0.0";

/// `runtime()`, also serving `@x/a` 2.0.0 (wanting `@x/b` ^2) and `@x/b` 2.0.0.
fn coupled_runtime() -> InProcessRuntime {
    runtime()
        .binary(A2, declaring("@x/a", "2.0.0", &[("@x/b", "^2", false)]))
        .binary(B2, declaring("@x/b", "2.0.0", &[]))
}

/// `@x/a` 2.0.0, `@x/b` 1.0.0 (no extension: it never loads) and `@x/b` 2.0.0 published.
fn coupled_registry(runtime: &InProcessRuntime) -> MemoryRegistry {
    let served = |wasm: &[u8]| {
        Published::new(
            wasm.to_vec(),
            crate::extension::declare(runtime, wasm).unwrap().0,
        )
    };
    MemoryRegistry::new()
        .serving(served(A2))
        .serving(Published::new(
            b"\0asm b1".to_vec(),
            declaration("@x/b", "1.0.0", &[]),
        ))
        .serving(served(B2))
}

fn registry_specified(reference: &str) -> String {
    reference.to_string()
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "an add of several specifiers judges their peers on the whole set, whatever their order"
)]
fn peers_are_judged_on_the_whole_set_whatever_the_order() {
    use crate::config::testing::files_under;
    let runtime = coupled_runtime();

    // `@x/a` 2.0.0 alone wants `@x/b` ^2, and 1.0.0 is locked.
    let dir = add_project(vec![entry("@x/b", "1.0.0", "registry", &[])]);
    let error = add(
        &several(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            vec![registry_specified("@x/a@2.0.0")],
            false,
        ),
        &coupled_registry(&runtime),
        &runtime,
    )
    .unwrap_err();
    assert_eq!(error.error.code, "R-RES-006", "{error:?}");

    // With `@x/b` 2.0.0 in the same add, the set is accepted, in either order, leaving the
    // same lock.
    let mut locks = Vec::new();
    for order in [["@x/a@2.0.0", "@x/b@2.0.0"], ["@x/b@2.0.0", "@x/a@2.0.0"]] {
        let dir = add_project(vec![entry("@x/b", "1.0.0", "registry", &[])]);
        let extensions = order.iter().map(|r| registry_specified(r)).collect();
        let added = add(
            &several(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                extensions,
                false,
            ),
            &coupled_registry(&runtime),
            &runtime,
        )
        .unwrap();
        assert!(
            outcomes(&added)
                .iter()
                .all(|o| matches!(o, AddOutcome::Installed { .. })),
            "{order:?}: {added:?}"
        );
        let lock_path = dir.path().join("specforge.lock");
        let lock = specforge_installed::read_lock_file(&lock_path).unwrap();
        let locked: Vec<(&str, &str)> = lock
            .entries
            .iter()
            .map(|e| (e.name.as_str(), e.version.as_str()))
            .collect();
        assert_eq!(locked, [("@x/b", "2.0.0"), ("@x/a", "2.0.0")], "{order:?}");
        locks.push(std::fs::read(&lock_path).unwrap());
    }
    assert_eq!(locks[0], locks[1], "the order decides nothing");

    // With `@x/b` 1.0.0 instead, the set is refused in either order, naming `@x/a`'s
    // specifier, and nothing is written.
    for order in [["@x/a@2.0.0", "@x/b@1.0.0"], ["@x/b@1.0.0", "@x/a@2.0.0"]] {
        let dir = disabled_project(vec![LockFileEntry {
            // the lock pins the bytes `coupled_registry` serves for 1.0.0
            wasm_hash: specforge_installed::hex_sha256(b"\0asm b1"),
            ..entry("@x/b", "1.0.0", "registry", &[])
        }]);
        let before = files_under(dir.path());
        let extensions = order.iter().map(|r| registry_specified(r)).collect();
        let error = add(
            &several(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                extensions,
                false,
            ),
            &coupled_registry(&runtime),
            &runtime,
        )
        .unwrap_err();
        assert_eq!(error.error.code, "R-RES-006", "{order:?}: {error:?}");
        assert_eq!(
            error.error.message,
            "adding '@x/a@2.0.0': version diamond: '@x/a' requires peer '@x/b' ^2 but this install locks 1.0.0; @x/b 2.0.0 would satisfy every requirer",
            "{order:?}"
        );
        assert_eq!(files_under(dir.path()), before, "{order:?}");
    }
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "a specifier naming an extension an earlier one adds is a duplicate, and two adding it differently are refused"
)]
fn a_repeated_extension_is_a_duplicate_and_a_different_one_a_conflict() {
    use crate::config::testing::files_under;

    // The same builtin twice, by two spellings: enabled once.
    let dir = project_with(EMPTY_PROJECT);
    let extensions = vec![
        "@specforge/product".to_string(),
        "@specforge/product@latest".to_string(),
    ];
    let added = add(
        &several(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            extensions,
            false,
        ),
        &crate::registry::Unconfigured("add"),
        &runtime_with_product(),
    )
    .unwrap();
    assert_eq!(
        outcomes(&added)[1],
        AddOutcome::Duplicate {
            name: "@specforge/product".into(),
            first: "@specforge/product".into(),
        }
    );
    assert!(added.added[1].outcome.is_duplicate());
    assert_eq!(
        specforge_common::read_project_config(dir.path())
            .config
            .extensions,
        ["@specforge/product"]
    );

    // A registry package resolving to the same version: fetched once.
    let dir = add_project(Vec::new());
    let registry = MemoryRegistry::new().serving(Published::new(greet(), declaration_of(&greet())));
    let extensions = vec![
        registry_specified("@sdk/greet@0.1.0"),
        registry_specified("@sdk/greet"),
        registry_specified("@sdk/greet@0.1.0"),
    ];
    let added = add(
        &several(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            extensions,
            false,
        ),
        &registry,
        &runtime(),
    )
    .unwrap();
    let outcomes = outcomes(&added);
    assert!(matches!(outcomes[0], AddOutcome::Installed { .. }));
    for duplicate in &outcomes[1..] {
        assert_eq!(
            *duplicate,
            AddOutcome::Duplicate {
                name: "@sdk/greet".into(),
                first: "@sdk/greet@0.1.0".into(),
            }
        );
    }
    let fetched = registry
        .asked()
        .into_iter()
        .filter(|asked| matches!(asked, crate::registry::testing::Asked::Fetch(..)))
        .count();
    assert_eq!(fetched, 1);

    // The same package at two versions: refused, naming both, nothing written.
    let runtime = coupled_runtime();
    let dir = add_project(Vec::new());
    let before = files_under(dir.path());
    let extensions = vec![
        registry_specified("@x/b@1.0.0"),
        registry_specified("@x/b@2.0.0"),
    ];
    let error = add(
        &several(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            extensions,
            false,
        ),
        &coupled_registry(&runtime),
        &runtime,
    )
    .unwrap_err();
    assert_eq!(error.error.code, "extension_conflict");
    assert_eq!(error.error.kind, OpErrorKind::Conflict);
    assert_eq!(
        error.error.message,
        "'@x/b@1.0.0' and '@x/b@2.0.0' both add '@x/b', as 1.0.0 from registry and as 2.0.0 from registry"
    );
    assert_eq!(error.error.suggestion.as_deref(), Some("add '@x/b' once"));
    assert_eq!(files_under(dir.path()), before);
}

#[test]
fn a_peer_refusal_is_reported_before_a_binary_that_did_not_load() {
    use crate::config::testing::files_under;
    let dir = add_project(Vec::new());
    let needs = dir.path().join("needs.wasm");
    std::fs::write(&needs, testing::NEEDS_SOFTWARE_9).unwrap();
    // `@sdk/greet`'s binary does not declare what its registry published: its load fails.
    let mut published = declaration_of(&greet());
    published.handshake.description = Some("Something else".into());
    let registry = MemoryRegistry::new().serving(Published::new(greet(), published));
    let before = files_under(dir.path());
    let shown = needs.display().to_string();

    let error = add(
        &AddRequest {
            allow_unsigned: true,
            ..several(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                vec![
                    "@sdk/greet@0.1.0".to_string(),
                    shown.clone(),
                    "@specforge/software".to_string(),
                ],
                false,
            )
        },
        &registry,
        &testing::probes(),
    )
    .unwrap_err();

    // The peer rule's refusal, not the earlier ask's load failure, is the decisive one.
    assert_eq!(error.error.code, "E027", "{error:?}");
    assert!(
        error
            .error
            .message
            .starts_with(&format!("adding '{shown}': ")),
        "{error:?}"
    );
    assert!(
        error.error.message.ends_with(
            "requires peer dependency '@specforge/software' ^9.0 but version 1.0.0 is installed"
        ),
        "{error:?}"
    );
    assert_eq!(files_under(dir.path()), before);
}
