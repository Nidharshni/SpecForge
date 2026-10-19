use super::*;
use crate::OpError;

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
    let mut request = several(&root, vec!["ext".to_string()], false);
    request.base = Some(dir.path());

    let added = add(&request, &crate::registry::Unconfigured("add"), &runtime()).unwrap();

    let AddOutcome::Installed { name, origin, .. } = &added.added[0].outcome else {
        panic!("{:?}", added.added[0].outcome);
    };
    assert_eq!(name, "@sdk/greet");
    assert_eq!(
        origin.source(),
        "local:ext/target/wasm32-wasip2/release/greet.wasm"
    );
}

#[test]
#[ignore = "flipped by T3"]
fn a_name_file_specifier_is_a_local_path_not_a_builtin() {
    assert_eq!(
        parse("@specforge/product=./a.wasm").unwrap(),
        Source::Local("@specforge/product=./a.wasm".into())
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
    let dir = project_with(EMPTY_PROJECT);
    let root = specforge_common::ProjectRoot::of(dir.path()).unwrap();
    let files = tempfile::tempdir().unwrap();
    let refused = |specifier: &str, registry: &dyn Registry, runtime: &InProcessRuntime| {
        let extensions = vec![specifier.to_string()];
        let error = add(&several(&root, extensions, false), registry, runtime).unwrap_err();
        (
            error.error.code.to_string(),
            error.error.kind,
            error.subject,
        )
    };
    let unconfigured = crate::registry::Unconfigured("add");

    // Reading the specifiers.
    let (code, _, subject) = refused("@a/b@!!", &unconfigured, &runtime());
    assert_eq!((code.as_str(), subject), ("R-RES-003", about));
    let (code, _, subject) = refused("", &unconfigured, &runtime());
    assert_eq!((code.as_str(), subject), ("E054", about));
    let error = add(
        &several(&root, Vec::new(), false),
        &unconfigured,
        &runtime(),
    )
    .unwrap_err();
    assert_eq!(
        (
            error.error.code.as_ref(),
            error.subject,
            error.error.message.as_str()
        ),
        ("E054", about, "no extension specifier")
    );

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
