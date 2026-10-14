use super::*;

#[test]
fn parse_names_each_source() {
    assert_eq!(
        parse("@specforge/product"),
        Ok(Source::Builtin("@specforge/product"))
    );
    assert_eq!(
        parse("ext/greet.wasm"),
        Ok(Source::Local(PathBuf::from("ext/greet.wasm")))
    );
    assert_eq!(
        parse("./ext/dir"),
        Ok(Source::Local(PathBuf::from("./ext/dir")))
    );
    assert_eq!(
        parse("@acme/tool@^1.2"),
        Ok(Source::Registry(
            PackageRef::parse("@acme/tool@^1.2").unwrap()
        ))
    );
    assert_eq!(
        parse("git+https://example.com/x.git"),
        Ok(Source::Git {
            url: "https://example.com/x.git".into()
        })
    );
    assert_eq!(parse("").unwrap_err().code, "E054");
    // Registry packages are always `@scope/name`: anything else names a path.
    assert_eq!(parse("not-scoped"), Ok(Source::Local("not-scoped".into())));
    assert_eq!(parse("@acme/").unwrap_err().code, "E054");
}

/// What `parse` makes of every input of plan 12's table (§2.2): a
/// package reference, or the code of the refusal. Nothing reaches a
/// registry that is not a scoped package name with a requirement.
#[specforge_test(
    behavior = "parse_extension_specifier",
    verify = "each add argument reads as one extension source"
)]
fn parse_reads_each_input() {
    fn registry(reference: &str) -> Result<Source, String> {
        Ok(Source::Registry(PackageRef::parse(reference).unwrap()))
    }
    fn local(path: &str) -> Result<Source, String> {
        Ok(Source::Local(PathBuf::from(path)))
    }
    let cases: Vec<(&str, Result<Source, String>)> = vec![
        ("@acme/tool", registry("@acme/tool")),                 // I1
        ("@acme/tool@", Err("E054".into())),                    // I2
        ("@acme/tool@1.2.0", registry("@acme/tool@1.2.0")),     // I3
        ("@acme/tool@^1.2", registry("@acme/tool@^1.2")),       // I4
        ("@acme/tool@1.x", registry("@acme/tool@1.x")),         // I5
        ("@acme/tool@1.2", registry("@acme/tool@1.2")),         // I6
        ("@acme/tool@1.0.0/x", Err("R-RES-003".into())),        // I7
        ("@acme/tool@1.0.0?x=1", Err("R-RES-003".into())),      // I8
        ("foo@/bar", local("foo@/bar")),                        // I9
        ("tool@1.0.0", local("tool@1.0.0")),                    // I10
        ("tool", local("tool")),                                // I11
        ("@acme/..", Err("E054".into())),                       // I12
        ("@acme/aa/bb", Err("E054".into())),                    // I13
        ("@acme/a/b", Err("E054".into())),                      // I14
        ("@a/x", registry("@a/x")),                             // I15
        ("@acme/T ool", Err("E054".into())),                    // I16
        ("Acme@1", local("Acme@1")),                            // I17
        ("@acme/tool@latest", registry("@acme/tool")),          // I18
        ("@acme/tool@*", registry("@acme/tool")),               // I18
        ("@acme/tool@>=1, <2", registry("@acme/tool@>=1, <2")), // I19
        ("@acme/tool@^bogus", Err("R-RES-003".into())),         // I20
        (
            "@acme/tool@2.0.0+build.1",
            registry("@acme/tool@2.0.0+build.1"),
        ), // I21
        ("@scope", Err("E054".into())),                         // I22
        (
            "@specforge/software",
            Ok(Source::Builtin("@specforge/software")),
        ), // I24
        (
            "git+https://h/r#v",
            Ok(Source::Git {
                url: "https://h/r".into(),
            }),
        ), // I25
        (" @acme/tool ", registry("@acme/tool")),               // I26
    ];
    for (input, want) in cases {
        let got = parse(input).map_err(|error| error.code.to_string());
        assert_eq!(got, want, "{input:?}");
    }
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "the specifiers of an add are read in order, the first unreadable one refused and named among several"
)]
fn specifiers_are_read_in_order_and_named_among_several() {
    let read = specified(&["@specforge/product", "@acme/tool@^1"]).unwrap();
    assert_eq!(
        read,
        [
            Specified {
                specifier: "@specforge/product".into(),
                source: Source::Builtin("@specforge/product"),
            },
            Specified {
                specifier: "@acme/tool@^1".into(),
                source: Source::Registry(PackageRef::parse("@acme/tool@^1").unwrap()),
            },
        ]
    );

    let error = specified(&["@specforge/product", "@acme/tool@", "@acme/"]).unwrap_err();
    assert_eq!(error.error.code, "E054");
    assert!(
        error
            .error
            .message
            .starts_with("adding '@acme/tool@': invalid extension specifier: "),
        "{error:?}"
    );
    let alone = specified(&["@acme/"]).unwrap_err();
    assert!(
        alone
            .error
            .message
            .starts_with("invalid extension specifier: "),
        "{alone:?}"
    );

    let none = specified::<&str>(&[]).unwrap_err();
    assert_eq!(none.error.code, "E054");
    assert_eq!(none.error.message, "no extension specifier");
    let dir = project_with(EMPTY_PROJECT);
    let error = add(
        &several(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Vec::new(),
            false,
        ),
        &crate::registry::Unconfigured("add"),
        &runtime(),
    )
    .unwrap_err();
    assert_eq!(error, none);
}

#[specforge_test(
    behavior = "parse_extension_specifier",
    verify = "a specifier that is no builtin's name, git+ URL or @scope/name reference is a local path, a bare crate directory included"
)]
fn a_bare_path_is_a_local_source() {
    assert_eq!(parse("ext").unwrap(), Source::Local("ext".into()));
    assert_eq!(
        parse("tools/greet").unwrap(),
        Source::Local("tools/greet".into())
    );
    assert!(matches!(parse("@acme/x").unwrap(), Source::Registry(_)));
    assert_eq!(
        parse("@acme/x.wasm").unwrap(),
        Source::Local("@acme/x.wasm".into())
    );

    let dir = project_with(EMPTY_PROJECT);
    let krate = dir.path().join("ext");
    let release = krate.join("target/wasm32-wasip2/release");
    std::fs::create_dir_all(&release).unwrap();
    std::fs::write(krate.join("Cargo.toml"), "[package]\nname = \"greet\"\n").unwrap();
    std::fs::write(release.join("greet.wasm"), GREET).unwrap();
    let root = specforge_common::ProjectRoot::of(dir.path()).unwrap();
    let specified = Specified {
        specifier: "ext".into(),
        source: parse("ext").unwrap(),
    }
    .relative_to(dir.path());

    let added = add(
        &several(&root, vec![specified], false),
        &crate::registry::Unconfigured("add"),
        &runtime(),
    )
    .unwrap();

    let AddOutcome::Installed { name, origin, .. } = &added.added[0].outcome else {
        panic!("{:?}", added.added[0].outcome);
    };
    assert_eq!(name, "@sdk/greet");
    assert_eq!(
        origin.source(),
        "local:ext/target/wasm32-wasip2/release/greet.wasm"
    );
}

/// A registry whose every answer for a name's versions is R008, the registry's own refusal
/// of a request (a 400); everything else is as `Unconfigured`.
struct Refusing;

impl Registry for Refusing {
    fn versions(&self, _: &PackageName) -> Result<Vec<specforge_package::Version>, OpError> {
        Err(OpError::coded(
            OpErrorKind::InvalidInput,
            codes::R008,
            "the registry refused the request (UNKNOWN): the name is refused",
        ))
    }

    fn fetch(
        &self,
        name: &PackageName,
        version: &specforge_package::Version,
        policy: crate::registry::TrustPolicy<'_>,
    ) -> Result<crate::registry::Package, OpError> {
        crate::registry::Unconfigured("add").fetch(name, version, policy)
    }

    fn published(
        &self,
        name: &PackageName,
        version: &specforge_package::Version,
    ) -> Result<crate::registry::Manifest, OpError> {
        crate::registry::Unconfigured("add").published(name, version)
    }

    fn pin(&self, pins: &[crate::registry::KeyPin]) -> Result<(), OpError> {
        crate::registry::Unconfigured("add").pin(pins)
    }

    fn publish(
        &self,
        upload: &crate::registry::Upload<'_>,
    ) -> Result<crate::registry::Published, OpError> {
        crate::registry::Unconfigured("add").publish(upload)
    }

    fn search(
        &self,
        query: &str,
        contributes: Option<specforge_protocol_types::DeclaredCategory>,
    ) -> Result<crate::registry::Searched, OpError> {
        crate::registry::Unconfigured("add").search(query, contributes)
    }
}

#[specforge_test(
    invariant = "mcp_structured_error_responses",
    verify = "an operation states the request field each refusal is about, and a refusal of the project or the environment is about none"
)]
fn every_refusal_of_a_specifier_is_about_the_extensions() {
    let about = Some(AddField::Extensions);
    // Reading the specifiers.
    let refusal = specified(&["@a/b@!!"]).unwrap_err();
    assert_eq!(
        (refusal.error.code.as_ref(), refusal.subject),
        ("R-RES-003", about)
    );
    let refusal = specified(&[""]).unwrap_err();
    assert_eq!(
        (refusal.error.code.as_ref(), refusal.subject),
        ("E054", about)
    );
    let refusal = specified::<&str>(&[]).unwrap_err();
    assert_eq!(
        (refusal.error.code.as_ref(), refusal.subject),
        ("E054", about)
    );

    // Resolving them.
    let dir = project_with(EMPTY_PROJECT);
    let root = specforge_common::ProjectRoot::of(dir.path()).unwrap();
    let files = tempfile::tempdir().unwrap();
    let refused = |specifier: &str, registry: &dyn Registry, runtime: &InProcessRuntime| {
        let extensions = vec![Specified {
            specifier: specifier.to_string(),
            source: parse(specifier).unwrap(),
        }];
        let error = add(&several(&root, extensions, false), registry, runtime).unwrap_err();
        (
            error.error.code.to_string(),
            error.error.kind,
            error.subject,
        )
    };
    let unconfigured = crate::registry::Unconfigured("add");

    let (code, kind, subject) = refused("git+https://example.com/x", &unconfigured, &runtime());
    assert_eq!(
        (code.as_str(), kind, subject),
        ("E064", OpErrorKind::InvalidInput, about)
    );

    let not_wasm = files.path().join("notwasm.txt");
    std::fs::write(&not_wasm, "not wasm").unwrap();
    let (code, _, subject) = refused(not_wasm.to_str().unwrap(), &unconfigured, &runtime());
    assert_eq!((code.as_str(), subject), ("E028", about));

    let bad_name = files.path().join("bad.wasm");
    std::fs::write(&bad_name, b"\0asm bad name").unwrap();
    let impostor = runtime().binary(b"\0asm bad name", declaring("Bad Name", "1.0.0", &[]));
    let (code, _, subject) = refused(bad_name.to_str().unwrap(), &unconfigured, &impostor);
    assert_eq!((code.as_str(), subject), ("E072", about));

    // The registry.
    let registry = MemoryRegistry::new().serving(Published::new(greet(), declaration_of(&greet())));
    let (code, _, subject) = refused("@sdk/greet@^9", &registry, &runtime());
    assert_eq!((code.as_str(), subject), ("R-RES-004", about));
    let (code, _, subject) = refused("@sdk/greet", &Refusing, &runtime());
    assert_eq!((code.as_str(), subject), ("R008", about));
    let (code, kind, subject) = refused("@sdk/greet", &MemoryRegistry::new(), &runtime());
    assert_eq!(
        (code.as_str(), kind, subject),
        ("R-RES-001", OpErrorKind::ExtensionNotFound, about)
    );

    // No registry is configured: a refusal of the project, about no field.
    let (code, kind, subject) = refused("@sdk/greet", &unconfigured, &runtime());
    assert_eq!(
        (code.as_str(), kind, subject),
        ("E063", OpErrorKind::PreconditionFailed, None)
    );
}
