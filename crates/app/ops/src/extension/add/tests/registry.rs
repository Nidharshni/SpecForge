use super::*;

/// The project's peer set (ADR 0041, amendment round 7) is what an add is judged against.
/// These helpers serve the probe extensions of `testing::probes`.
fn wasm_file(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

/// A registry publishing `bytes` as the package they declare.
fn registry_publishing(bytes: &[u8]) -> MemoryRegistry {
    let declared = crate::extension::declare(&testing::probes(), bytes)
        .unwrap()
        .0;
    MemoryRegistry::new().serving(Published::new(bytes.to_vec(), declared))
}

/// Add `source` to the project at `dir`, allowing unsigned packages.
fn add_one(
    dir: &Path,
    source: Source,
    registry: &dyn Registry,
) -> Result<Added, Refusal<AddField>> {
    let root = specforge_common::ProjectRoot::of(dir).unwrap();
    add(
        &AddRequest {
            root: &root,
            extensions: vec![source.into()],
            base: None,
            allow_unsigned: true,
            dry_run: false,
            caller: crate::testing::caller(),
        },
        registry,
        &testing::probes(),
    )
}

fn package(name: &str) -> Source {
    Source::Registry(PackageRef::parse(name).unwrap())
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "add judges the project's peer set as the add leaves it: its builtins, installed extensions and .wasm file entries"
)]
fn an_install_requiring_a_builtin_out_of_range_is_refused_from_any_source() {
    use crate::config::testing::files_under;
    let files = tempfile::tempdir().unwrap();
    let config = r#"{"name": "p", "version": "0.1.0", "extensions": ["@specforge/software"]}"#;
    let local = Source::Local(wasm_file(
        files.path(),
        "needs.wasm",
        testing::NEEDS_SOFTWARE_9,
    ));
    let registry = registry_publishing(testing::NEEDS_SOFTWARE_9);
    for (source, registry) in [
        (
            local,
            &crate::registry::Unconfigured("add") as &dyn Registry,
        ),
        (package("@probe/needs-software"), &registry),
    ] {
        let dir = project_with(config);
        let before = files_under(dir.path());

        let error = add_one(dir.path(), source, registry).unwrap_err();

        assert_eq!(error.error.code, "E027", "{error:?}");
        assert_eq!(
            error.error.message,
            "extension '@probe/needs-software' requires peer dependency '@specforge/software' ^9.0 but version 1.0.0 is installed"
        );
        assert_eq!(
            error.error.suggestion.as_deref(),
            Some(
                "'@specforge/software' is the builtin this specforge embeds, at 1.0.0: install a version of '@probe/needs-software' that accepts it"
            )
        );
        assert_eq!(files_under(dir.path()), before);
    }
    let software = PackageName::parse("@specforge/software").unwrap();
    assert!(
        !registry
            .asked()
            .contains(&crate::registry::testing::Asked::Versions(software)),
        "a builtin is never unified: {:?}",
        registry.asked()
    );
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "an install whose required peer is not installed is refused with E027 before anything is written"
)]
fn an_install_whose_required_peer_is_not_installed_is_refused() {
    use crate::config::testing::files_under;
    let files = tempfile::tempdir().unwrap();
    let local = Source::Local(wasm_file(files.path(), "dep.wasm", testing::DEP_1));
    let registry = registry_publishing(testing::DEP_1);
    for (source, registry) in [
        (
            local,
            &crate::registry::Unconfigured("add") as &dyn Registry,
        ),
        (package("@probe/dep"), &registry),
    ] {
        let dir = project_with(EMPTY_PROJECT);
        let before = files_under(dir.path());

        let error = add_one(dir.path(), source, registry).unwrap_err();

        assert_eq!(error.error.code, "E027", "{error:?}");
        assert_eq!(
            error.error.message,
            "extension '@probe/dep' requires peer dependency '@probe/base' ^1.0 which is not installed"
        );
        assert_eq!(
            error.error.suggestion.as_deref(),
            Some("install it with: specforge add @probe/base")
        );
        assert_eq!(files_under(dir.path()), before);
    }
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "enabling a builtin that an enabled extension's peer range does not accept is refused with E027"
)]
fn enabling_a_builtin_an_enabled_extension_rejects_is_refused() {
    use crate::config::testing::files_under;
    // `@probe/needs-software` installed by hand, requiring software ^9.
    let mut locked = entry("@probe/needs-software", "1.0.0", "local:needs.wasm", &[]);
    locked.wasm_hash = specforge_installed::hex_sha256(testing::NEEDS_SOFTWARE_9);
    locked.peer_dependencies = vec![PeerDependency {
        name: "@specforge/software".to_string(),
        version: "^9.0".to_string(),
        optional: false,
    }];
    let dir = project(vec![locked]);
    std::fs::write(
        installed(dir.path(), "@probe/needs-software"),
        testing::NEEDS_SOFTWARE_9,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name": "p", "version": "0.1.0", "extensions": ["@probe/needs-software"]}"#,
    )
    .unwrap();
    let before = files_under(dir.path());

    let error = add_one(
        dir.path(),
        Source::Builtin("@specforge/software"),
        &crate::registry::Unconfigured("add"),
    )
    .unwrap_err();

    assert_eq!(error.error.code, "E027", "{error:?}");
    assert!(
        error
            .error
            .message
            .starts_with("enabling '@specforge/software' 1.0.0 breaks '@probe/needs-software': "),
        "{}",
        error.error.message
    );
    assert_eq!(files_under(dir.path()), before);
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "an install that breaks a .wasm file entry, or that a .wasm file entry does not satisfy, is refused with E027"
)]
fn an_install_that_breaks_a_file_entry_or_that_a_file_entry_does_not_satisfy_is_refused() {
    use crate::config::testing::files_under;
    let files = tempfile::tempdir().unwrap();
    let unconfigured = crate::registry::Unconfigured("add");

    // (a) `dep.wasm` wants `@probe/base` ^1; installing base 2.0.0 breaks it.
    let dir = project_with(r#"{"name": "p", "version": "0.1.0", "extensions": ["dep.wasm"]}"#);
    wasm_file(dir.path(), "dep.wasm", testing::DEP_1);
    let base = Source::Local(wasm_file(files.path(), "base.wasm", testing::BASE_2));
    let before = files_under(dir.path());
    let error = add_one(dir.path(), base, &unconfigured).unwrap_err();
    assert_eq!(error.error.code, "E027", "{error:?}");
    assert!(
        error
            .error
            .message
            .starts_with("installing '@probe/base' 2.0.0 breaks '@probe/dep': "),
        "{}",
        error.error.message
    );
    assert_eq!(files_under(dir.path()), before);

    // (b) `base.wasm` is 2.0.0; the dep installed beside it wants ^1.
    let dir = project_with(r#"{"name": "p", "version": "0.1.0", "extensions": ["base.wasm"]}"#);
    wasm_file(dir.path(), "base.wasm", testing::BASE_2);
    let dep = Source::Local(wasm_file(files.path(), "dep.wasm", testing::DEP_1));
    let before = files_under(dir.path());
    let error = add_one(dir.path(), dep, &unconfigured).unwrap_err();
    assert_eq!(error.error.code, "E027", "{error:?}");
    assert_eq!(
        error.error.message,
        "extension '@probe/dep' requires peer dependency '@probe/base' ^1.0 but version 2.0.0 is installed"
    );
    assert_eq!(files_under(dir.path()), before);
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "an install enables the builtins it requires first, whatever its source"
)]
fn an_install_enables_the_builtins_it_requires_first() {
    let files = tempfile::tempdir().unwrap();
    let local = Source::Local(wasm_file(
        files.path(),
        "needs.wasm",
        testing::NEEDS_SOFTWARE_1,
    ));
    let registry = registry_publishing(testing::NEEDS_SOFTWARE_1);
    for (source, registry) in [
        (
            local,
            &crate::registry::Unconfigured("add") as &dyn Registry,
        ),
        (package("@probe/needs-software"), &registry),
    ] {
        let dir = project_with(EMPTY_PROJECT);

        let added = add_one(dir.path(), source, registry).unwrap();

        assert!(
            matches!(
                &added.added[0].outcome,
                AddOutcome::Installed { peers_enabled, .. }
                    if peers_enabled == &["@specforge/software"]
            ),
            "{:?}",
            added.added[0].outcome
        );
        let config = specforge_common::read_project_config(dir.path()).config;
        assert_eq!(
            config.extensions,
            ["@specforge/software", "@probe/needs-software"]
        );
        assert!(
            added
                .writes
                .named(Some(dir.path()))
                .contains(&specforge_common::ProjectPath::of(
                    dir.path(),
                    &dir.path().join("specforge.json")
                )),
            "{:?}",
            added.writes
        );
    }
}

fn with_peer(
    mut declaration: ExtensionDeclaration,
    name: &str,
    range: &str,
) -> ExtensionDeclaration {
    declaration
        .handshake
        .peer_dependencies
        .push(PeerDependency {
            name: name.to_string(),
            version: range.to_string(),
            optional: false,
        });
    declaration
}

#[specforge_test(
    behavior = "check_registry_reply",
    verify = "add refuses a package whose binary declares other than its published declaration"
)]
fn a_binary_that_declares_other_than_its_published_declaration_is_refused() {
    // R4/R5: the published declaration differs from the binary's, in
    // its handshake (another description) or in a category (no kinds).
    // A served declaration is trusted for nothing the binary doesn't
    // declare: the package is refused before anything is installed.
    let mut description = declaration_of(&greet());
    description.handshake.description = Some("Something else".to_string());
    for published in [description, {
        let mut kinds = declaration_of(&greet());
        kinds.entities.clear();
        kinds
    }] {
        let dir = add_project(Vec::new());
        let registry = MemoryRegistry::new().serving(Published::new(greet(), published));
        let err = add_greet(dir.path(), &registry).unwrap_err();
        assert!(err.error.is(crate::registry::METADATA_MISMATCH), "{err:?}");
        assert!(err.error.message.contains("@sdk/greet@0.1.0"), "{err:?}");
        assert!(
            !installed(dir.path(), "@sdk/greet").exists(),
            "nothing is installed"
        );
    }
    // The message names the first part that differs.
    let mut kinds = declaration_of(&greet());
    kinds.entities.clear();
    let dir = add_project(Vec::new());
    let registry = MemoryRegistry::new().serving(Published::new(greet(), kinds));
    let err = add_greet(dir.path(), &registry).unwrap_err();
    assert!(err.error.message.contains("another entities"), "{err:?}");
    // The published declaration equal to the binary's installs.
    let dir = add_project(Vec::new());
    let registry = MemoryRegistry::new().serving(Published::new(greet(), declaration_of(&greet())));
    add_greet(dir.path(), &registry).unwrap();
}

#[specforge_test(
    behavior = "check_registry_reply",
    verify = "the diamond gate decides on the published declaration's peers"
)]
fn the_diamond_gate_reads_the_published_declarations_peers() {
    // R4: the published declaration names a peer @acme/x ^2 that the
    // lock holds at 1.0.0. The gate refuses before the binary (which
    // declares no peer) is loaded.
    let dir = add_project(vec![entry("@acme/x", "1.0.0", "registry", &[])]);
    let registry = MemoryRegistry::new()
        .serving(Published::new(
            greet(),
            with_peer(declaration_of(&greet()), "@acme/x", "^2"),
        ))
        .serving(Published::new(
            b"\0asm x".to_vec(),
            declaration("@acme/x", "1.0.0", &[]),
        ));
    let err = add_greet(dir.path(), &registry).unwrap_err();
    assert!(!err.error.is(crate::registry::METADATA_MISMATCH), "{err:?}");
    assert!(err.error.code.starts_with("R-RES"), "{err:?}");
    assert!(err.error.message.contains("@acme/x"), "{err:?}");
}

#[test]
fn adding_an_installed_exact_version_asks_the_registry_nothing() {
    let dir = add_project(Vec::new());
    let registry = MemoryRegistry::new().serving(Published::new(greet(), declaration_of(&greet())));
    let first = add_greet(dir.path(), &registry).unwrap();
    assert!(matches!(first, AddOutcome::Installed { .. }), "{first:?}");
    let asked = registry.asked();
    assert_eq!(asked.len(), 1, "{asked:?}");

    // The same version again: installed and enabled, so nothing is asked.
    let again = add_greet(dir.path(), &registry).unwrap();
    assert!(
        matches!(again, AddOutcome::AlreadyPresent { .. }),
        "{again:?}"
    );
    assert_eq!(
        registry.asked(),
        asked,
        "the registry was asked nothing more"
    );
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "an add that upgrades a locked package judges its peers against the lock as the add leaves it"
)]
fn an_add_that_upgrades_a_locked_package_judges_its_peers_without_its_previous_entry() {
    let dir = add_project(vec![
        entry("@x/b", "1.0.0", "registry", &[]),
        entry("@x/a", "1.0.0", "registry", &[("@x/b", "^1")]),
    ]);
    let registry = MemoryRegistry::new()
        .serving(Published::new(
            b"\0asm b1".to_vec(),
            declaration("@x/b", "1.0.0", &[]),
        ))
        .serving(Published::new(
            b"\0asm b2".to_vec(),
            declaration("@x/b", "2.0.0", &[]),
        ))
        .serving(Published::new(
            b"\0asm a2".to_vec(),
            declaration("@x/a", "2.0.0", &[("@x/b", "^2")]),
        ));
    let lock_before = std::fs::read(dir.path().join("specforge.lock")).unwrap();

    let error = add(
        &AddRequest {
            root: &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            extensions: vec![Source::Registry(PackageRef::parse("@x/a@2.0.0").unwrap()).into()],
            base: None,
            allow_unsigned: true,
            dry_run: false,
            caller: crate::testing::caller(),
        },
        &registry,
        &runtime(),
    )
    .unwrap_err();

    assert_eq!(error.error.code, "R-RES-006", "{error:?}");
    assert_eq!(
        error.error.message,
        "version diamond: '@x/a' requires peer '@x/b' ^2 but 1.0.0 is locked; @x/b 2.0.0 would satisfy every requirer"
    );
    assert_eq!(
        std::fs::read(dir.path().join("specforge.lock")).unwrap(),
        lock_before
    );
    assert_eq!(
        registry.asked(),
        [
            crate::registry::testing::Asked::Fetch(
                PackageName::parse("@x/a").unwrap(),
                specforge_package::Version::new(2, 0, 0)
            ),
            crate::registry::testing::Asked::Versions(PackageName::parse("@x/b").unwrap()),
        ]
    );
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "a version diamond names each requirer once"
)]
fn a_version_diamond_names_each_requirer_once() {
    let dir = add_project(vec![entry(
        "@acme/app",
        "1.0.0",
        "registry",
        &[("@sdk/greet", "^2.0")],
    )]);
    let registry = MemoryRegistry::new().serving(Published::new(greet(), declaration_of(&greet())));

    let error = add_greet(dir.path(), &registry).unwrap_err();

    assert_eq!(error.error.code, "R-RES-005", "{error:?}");
    assert!(
        error
            .error
            .message
            .starts_with("installing '@sdk/greet' 0.1.0 breaks '@acme/app': "),
        "{error:?}"
    );
    assert_eq!(
        error
            .error
            .message
            .matches("@acme/app wants @sdk/greet ^2.0")
            .count(),
        1,
        "{error:?}"
    );
    assert!(
        !installed(dir.path(), "@sdk/greet").exists(),
        "nothing is installed"
    );
}

#[specforge_test(
    invariant = "mcp_structured_error_responses",
    verify = "an operation states the request field each refusal is about, and a refusal of the project or the environment is about none"
)]
fn a_fetched_binary_that_is_not_what_was_published_is_a_schema_mismatch() {
    // A binary that loads as no extension (E028), and one that declares another name than
    // the registry published it under (E028): the registry's fault, not a value the caller
    // gave, so neither is invalid input and neither is about the specifiers.
    let impostor = b"\0asm declares another name".to_vec();
    let runtime_of_impostor = runtime().binary(&impostor, declaring("@sdk/other", "0.1.0", &[]));
    for (bytes, runtime) in [
        (b"\0asm not an extension".to_vec(), runtime()),
        (impostor, runtime_of_impostor),
    ] {
        let dir = add_project(Vec::new());
        let registry = MemoryRegistry::new().serving(Published::new(
            bytes,
            declaration("@sdk/greet", "0.1.0", &[]),
        ));
        let refusal = add(
            &AddRequest {
                root: &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                extensions: vec![
                    Source::Registry(PackageRef::parse("@sdk/greet@0.1.0").unwrap()).into(),
                ],
                base: None,
                allow_unsigned: true,
                dry_run: false,
                caller: crate::testing::caller(),
            },
            &registry,
            &runtime,
        )
        .unwrap_err();
        assert_eq!(refusal.error.code, "E028", "{refusal:?}");
        assert_eq!(
            refusal.error.kind,
            OpErrorKind::SchemaMismatch,
            "{refusal:?}"
        );
        assert_eq!(refusal.subject, None, "{refusal:?}");
    }
}
