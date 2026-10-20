use super::*;

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "a dry run of an extension already present answers as the add would, whatever its source"
)]
fn a_dry_run_of_a_present_extension_answers_as_the_add_would() {
    fn dry(root: &specforge_common::ProjectRoot, source: Source) -> AddRequest<'_> {
        AddRequest {
            root,
            extensions: vec![source.into()],
            base: None,
            allow_unsigned: true,
            dry_run: true,
            caller: crate::testing::caller(),
        }
    }

    // A registry install, present: already present first.
    let dir = add_project(Vec::new());
    let registry = MemoryRegistry::new().serving(Published::new(greet(), declaration_of(&greet())));
    add_greet(dir.path(), &registry).unwrap();
    let source = Source::Registry(PackageRef::parse("@sdk/greet@0.1.0").unwrap());
    let outcome = add(
        &dry(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            source,
        ),
        &registry,
        &runtime(),
    )
    .unwrap()
    .added[0]
        .outcome
        .clone();
    assert_eq!(
        outcome,
        AddOutcome::AlreadyPresent {
            name: "@sdk/greet".into(),
            version: "0.1.0".into()
        }
    );

    // A local install, present: already present too.
    let dir = project_with(EMPTY_PROJECT);
    let files = tempfile::tempdir().unwrap();
    let wasm = files.path().join("greet.wasm");
    std::fs::write(&wasm, GREET).unwrap();
    let unconfigured = crate::registry::Unconfigured("add");
    add(
        &local_request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            &wasm,
        ),
        &unconfigured,
        &runtime(),
    )
    .unwrap();
    let outcome = add(
        &dry(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Source::Local(wasm),
        ),
        &unconfigured,
        &runtime(),
    )
    .unwrap()
    .added[0]
        .outcome
        .clone();
    assert_eq!(
        outcome,
        AddOutcome::AlreadyPresent {
            name: "@sdk/greet".into(),
            version: "0.1.0".into()
        }
    );

    // A builtin, enabled: the no-op the real add returns.
    let dir = project_with(r#"{"name":"p","version":"0.1.0","extensions":["@specforge/product"]}"#);
    let source = Source::Builtin("@specforge/product");
    let before = crate::config::testing::files_under(dir.path());
    let added = add(
        &dry(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            source,
        ),
        &unconfigured,
        &runtime(),
    )
    .unwrap();
    assert_eq!(
        added.added[0].outcome,
        AddOutcome::Builtin {
            name: "@specforge/product",
            changed: false,
            peers_enabled: Vec::new()
        }
    );
    assert!(added.writes.is_empty());
    assert_eq!(crate::config::testing::files_under(dir.path()), before);
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "a dry run of several specifiers previews each and writes nothing"
)]
fn a_dry_run_of_several_specifiers_previews_each() {
    use crate::config::testing::files_under;
    let dir = project_with(EMPTY_PROJECT);
    let (_files, wasm) = greet_file();
    let before = files_under(dir.path());
    let extensions = vec![
        Source::Builtin("@specforge/product").into(),
        Source::Local(wasm.clone()).into(),
        Source::Builtin("@specforge/product").into(),
    ];

    let added = add(
        &several(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            extensions,
            true,
        ),
        &crate::registry::Unconfigured("add"),
        &runtime_with_product(),
    )
    .unwrap();

    let outcomes = outcomes(&added);
    assert_eq!(
        outcomes[0],
        AddOutcome::Planned {
            name: "@specforge/product".into(),
            version: None,
            origin: Origin::Builtin,
            peers_enabled: vec![],
        }
    );
    assert!(
        matches!(&outcomes[1], AddOutcome::Planned { name, version: Some(v), .. }
            if name == "@sdk/greet" && v == "0.1.0"),
        "{outcomes:?}"
    );
    assert!(matches!(outcomes[2], AddOutcome::Duplicate { .. }));
    assert!(added.writes.is_empty());
    assert_eq!(files_under(dir.path()), before);

    // A dry run refuses what the add would refuse, naming it.
    let missing = dir.path().join("missing.wasm");
    let extensions = vec![
        Source::Builtin("@specforge/product").into(),
        Source::Local(missing).into(),
    ];
    let error = add(
        &several(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            extensions,
            true,
        ),
        &crate::registry::Unconfigured("add"),
        &runtime_with_product(),
    )
    .unwrap_err();
    assert_eq!(error.error.code, "E040");
    assert!(error.error.message.starts_with("adding '"), "{error:?}");
    assert_eq!(files_under(dir.path()), before);
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "a dry run of several specifiers previews each and writes nothing"
)]
fn a_dry_run_reports_the_builtins_it_would_enable() {
    let dir = project_with(EMPTY_PROJECT);
    let chain = serving_builtin(
        serving_builtin(
            runtime_with_product(),
            "@specforge/formal",
            declaring(
                "@specforge/formal",
                "1.0.0",
                &[("@specforge/software", "^1.0", false)],
            ),
        ),
        "@specforge/software",
        declaring("@specforge/software", "1.0.0", &[]),
    );

    let added = add(
        &several(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            vec![Source::Builtin("@specforge/formal").into()],
            true,
        ),
        &crate::registry::Unconfigured("add"),
        &chain,
    )
    .unwrap();

    let document = serde_json::to_value(added.document()).unwrap();
    assert_eq!(document["dry_run"], true);
    assert_eq!(document["added"][0]["action"], "planned");
    assert_eq!(
        document["added"][0]["peers_enabled"],
        serde_json::json!(["@specforge/software"]),
        "{document}"
    );
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "a dry run judges a registry package by the declaration it was published with, downloading nothing"
)]
fn a_dry_run_judges_a_registry_package_by_its_published_declaration() {
    let published = || {
        MemoryRegistry::new()
            .serving(Published::new(
                b"\0asm base1".to_vec(),
                declaration("@acme/base", "1.0.0", &[]),
            ))
            .serving(Published::new(
                b"\0asm base2".to_vec(),
                declaration("@acme/base", "2.0.0", &[]),
            ))
            .serving(Published::new(
                b"\0asm app".to_vec(),
                declaration("@acme/app", "1.0.0", &[("@acme/base", "^2.0")]),
            ))
    };
    let dir = add_project(vec![entry("@acme/base", "1.0.0", "registry", &[])]);
    let root = specforge_common::ProjectRoot::of(dir.path()).unwrap();
    let request = |dry_run| AddRequest {
        root: &root,
        extensions: vec![Source::Registry(PackageRef::parse("@acme/app@1.0.0").unwrap()).into()],
        base: None,
        allow_unsigned: true,
        dry_run,
        caller: crate::testing::caller(),
    };

    let real = add(&request(false), &published(), &runtime()).unwrap_err();
    assert_eq!(real.error.code, "R-RES-006", "{real:?}");
    assert!(real.error.message.contains("2.0.0"), "{real:?}");

    let registry = published();
    let dry = add(&request(true), &registry, &runtime()).unwrap_err();
    assert_eq!(
        (dry.error.code, dry.error.message),
        (real.error.code, real.error.message)
    );
    let asked = registry.asked();
    assert!(
        asked.contains(&crate::registry::testing::Asked::Published(
            PackageName::parse("@acme/app").unwrap(),
            specforge_package::Version::new(1, 0, 0)
        )),
        "{asked:?}"
    );
    assert!(
        !asked
            .iter()
            .any(|asked| matches!(asked, crate::registry::testing::Asked::Fetch(..))),
        "a dry run downloads nothing: {asked:?}"
    );
}
