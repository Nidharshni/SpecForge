//! `specforge publish` as an operation: what it refuses and in which order, what it reports and what it
//! uploads (ADR 0045), over an in-memory registry and an in-process runtime.

use std::path::{Path, PathBuf};

use specforge_extension_sdk::{ContributionsBuilder, ExtensionMeta};
use specforge_ops::asking::Asking;
use specforge_ops::extension::{PublishReport, publish, validate};
use specforge_ops::registry::testing::{Asked, MemoryRegistry};
use specforge_ops::registry::{Registry, TrustPolicy, Unconfigured};
use specforge_ops::testing::declaring;
use specforge_package::{PackageName, Version};
use specforge_test_macros::test as specforge_test;
use specforge_wasm::testing::InProcessRuntime;

/// Bytes the runtime serves as the extension `build` declares.
fn bytes(label: &str) -> Vec<u8> {
    format!("\0asm in-process {label}").into_bytes()
}

/// A `.wasm` file holding `bytes`.
fn binary(dir: &Path, bytes: &[u8]) -> PathBuf {
    let path = dir.join("ext.wasm");
    std::fs::write(&path, bytes).unwrap();
    path
}

fn report(path: &Path, registry: &dyn Registry, runtime: &InProcessRuntime) -> PublishReport {
    publish(path, registry, runtime)
}

fn codes_of(report: &PublishReport) -> Vec<&str> {
    report.warnings.iter().map(|w| &*w.code).collect()
}

fn refused(report: &PublishReport) -> &specforge_ops::OpError {
    report.result.as_ref().expect_err("the publish is refused")
}

#[specforge_test(
    behavior = "publish_to_registry",
    verify = "publish refuses in one order, each refusal before anything after it is read or asked"
)]
fn publish_refuses_in_one_order() {
    let dir = tempfile::tempdir().unwrap();
    let registry = MemoryRegistry::new();

    // 1. The binary: none at the path.
    let runtime = InProcessRuntime::new();
    let missing = report(&dir.path().join("missing.wasm"), &registry, &runtime);
    assert_eq!(refused(&missing).code, "E040", "{missing:?}");
    assert!(missing.warnings.is_empty());
    assert!(runtime.calls().is_empty());

    // 1. The binary: a crate directory with no build.
    let krate = dir.path().join("krate");
    std::fs::create_dir_all(&krate).unwrap();
    let not_built = report(&krate, &registry, &runtime);
    assert_eq!(refused(&not_built).code, "E040");
    assert_eq!(
        refused(&not_built).suggestion.as_deref(),
        Some("cargo build --release --target wasm32-wasip2")
    );

    // 1. The binary: bytes the runtime does not know.
    let unknown = binary(dir.path(), b"\0asm nobody serves this");
    let e028 = report(&unknown, &registry, &runtime);
    assert_eq!(refused(&e028).code, "E028", "{e028:?}");

    // 2. The declaration, before the name: an error and an unscoped name.
    let both = bytes("unscoped, with an error");
    let runtime = InProcessRuntime::new().binary(&both, || {
        let mut meta = ExtensionMeta::new("greet-ext1", "0.1.0");
        meta.short = Some("Friendly greetings".into());
        ContributionsBuilder::new(meta)
    });
    let e030 = report(&binary(dir.path(), &both), &registry, &runtime);
    assert_eq!(refused(&e030).code, "E030", "{e030:?}");

    // 3. The name, before the registry: no registry is configured, and the name is the refusal.
    let unscoped = bytes("unscoped");
    let runtime = InProcessRuntime::new().binary(&unscoped, declaring("greet-ext1", "0.1.0", &[]));
    let e072 = report(
        &binary(dir.path(), &unscoped),
        &Unconfigured("publish"),
        &runtime,
    );
    assert_eq!(refused(&e072).code, "E072", "{e072:?}");

    // 4. The registry: a good package, and no registry.
    let good = bytes("good");
    let runtime = InProcessRuntime::new().binary(&good, declaring("@acme/x", "1.0.0", &[]));
    let e063 = report(
        &binary(dir.path(), &good),
        &Unconfigured("publish"),
        &runtime,
    );
    assert_eq!(refused(&e063).code, "E063", "{e063:?}");

    assert!(registry.accepted_publishes().is_empty());
    assert!(registry.asked().is_empty());
}

#[specforge_test(
    behavior = "publish_to_registry",
    verify = "publish and extension validate refuse a binary that declares a builtin's name, as add refuses to install it"
)]
fn a_binary_declaring_a_builtins_name_is_refused_as_add_refuses_it() {
    let dir = tempfile::tempdir().unwrap();
    let wasm = bytes("@specforge/product 9.0.0");
    let runtime =
        InProcessRuntime::new().binary(&wasm, declaring("@specforge/product", "9.0.0", &[]));
    let path = binary(dir.path(), &wasm);
    let registry = MemoryRegistry::new();

    let validated = validate(&path, &runtime).unwrap_err();
    let published = report(&path, &registry, &runtime);

    for error in [&validated, refused(&published)] {
        assert_eq!(error.code, "extension_conflict", "{error:?}");
        assert_eq!(
            error.message,
            "the extension declares the name of the builtin '@specforge/product'"
        );
    }
    assert!(registry.accepted_publishes().is_empty());
    assert!(registry.asked().is_empty());
}

#[specforge_test(
    behavior = "publish_to_registry",
    verify = "publish refuses a binary whose declaration has errors before any network call"
)]
fn a_declaration_with_errors_is_refused_before_the_registry_is_asked() {
    let dir = tempfile::tempdir().unwrap();
    let registry = MemoryRegistry::new();
    let wasm = bytes("bad short");
    let runtime = InProcessRuntime::new().binary(&wasm, || {
        let mut meta = ExtensionMeta::new("@sdk/greet", "0.1.0");
        meta.short = Some("Friendly greetings".into());
        ContributionsBuilder::new(meta)
    });

    let refusal = report(&binary(dir.path(), &wasm), &registry, &runtime);

    let error = refused(&refusal);
    assert_eq!(error.code, "E030", "{error:?}");
    assert!(
        error
            .message
            .contains("@sdk/greet@0.1.0 can't be published"),
        "{error:?}"
    );
    assert!(error.message.contains("ext_short"), "{error:?}");
    assert!(registry.accepted_publishes().is_empty());
    assert!(registry.asked().is_empty());

    // A required peer that is not installed is not an error.
    let wasm = bytes("with a peer");
    let runtime = InProcessRuntime::new().binary(
        &wasm,
        declaring("@acme/x", "1.0.0", &[("@acme/base", "^1", false)]),
    );
    let published = report(&binary(dir.path(), &wasm), &registry, &runtime);
    assert!(published.result.is_ok(), "{published:?}");

    // A peer range that is no SemVer range is.
    let wasm = bytes("with a bad peer range");
    let runtime = InProcessRuntime::new().binary(
        &wasm,
        declaring("@acme/y", "1.0.0", &[("@acme/base", "one-ish", false)]),
    );
    let refusal = report(&binary(dir.path(), &wasm), &registry, &runtime);
    assert_eq!(refused(&refusal).code, "E073", "{refusal:?}");
    assert!(refused(&refusal).message.contains("can't be published"));
}

#[specforge_test(
    behavior = "publish_wasm_extension",
    verify = "publish refuses a declaration whose name or version is not publishable before it uploads"
)]
fn an_unpublishable_name_or_version_is_refused_before_the_registry_is_asked() {
    let dir = tempfile::tempdir().unwrap();
    let registry = MemoryRegistry::new();
    for (name, version, why) in [
        (
            "greet-ext1",
            "0.1.0",
            "registry packages are named @scope/name",
        ),
        ("Greet", "0.1.0", "starts with a lowercase letter or digit"),
        ("@acme/x", "1.0", "is not a SemVer version"),
    ] {
        let wasm = bytes(&format!("{name} {version}"));
        let runtime = InProcessRuntime::new().binary(&wasm, declaring(name, version, &[]));

        let refusal = report(&binary(dir.path(), &wasm), &registry, &runtime);

        let error = refused(&refusal);
        assert_eq!(error.code, "E072", "{name}@{version}: {error:?}");
        assert!(error.message.contains(why), "{name}@{version}: {error:?}");
    }
    assert!(registry.accepted_publishes().is_empty());
    assert!(registry.asked().is_empty());
}

#[specforge_test(
    behavior = "publish_to_registry",
    verify = "the declaration's warnings are reported even when publish is refused"
)]
fn the_declarations_warnings_are_reported_even_when_publish_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let wasm = bytes("warns, unscoped");
    // A field naming an edge label the extension does not declare: W021.
    let runtime = InProcessRuntime::new().binary(&wasm, || {
        let mut builder = ContributionsBuilder::new(ExtensionMeta::new("greet-ext1", "0.1.0"));
        builder.kind("greeting", |k| {
            k.field("style", |f| {
                f.field_type(specforge_extension_sdk::FieldType::String);
                f.edge("nowhere");
            });
        });
        builder
    });

    let refusal = report(&binary(dir.path(), &wasm), &MemoryRegistry::new(), &runtime);

    assert_eq!(codes_of(&refusal), ["W021"], "{refusal:?}");
    assert_eq!(refused(&refusal).code, "E072");
}

#[specforge_test(
    behavior = "publish_wasm_extension",
    verify = "publish uploads the .wasm binary and the declaration derived from it"
)]
fn a_publish_uploads_the_binary_and_the_declaration_read_from_it() {
    let dir = tempfile::tempdir().unwrap();
    let wasm = bytes("@acme/x 1.0.0");
    let runtime = InProcessRuntime::new().binary(&wasm, declaring("@acme/x", "1.0.0", &[]));
    let registry = MemoryRegistry::new();

    let published = report(&binary(dir.path(), &wasm), &registry, &runtime);

    let document = published.result.expect("published");
    let name = PackageName::parse("@acme/x").unwrap();
    let version = Version::parse("1.0.0").unwrap();
    assert_eq!(
        registry.accepted_publishes(),
        [(name.clone(), version.clone())]
    );
    assert_eq!(document.extension, "@acme/x");
    assert_eq!(document.version, "1.0.0");
    assert_eq!(document.size_bytes, wasm.len());
    assert_eq!(document.registry, "in-memory");

    let held = registry
        .fetch(&name, &version, TrustPolicy::new(false, Asking::Nobody))
        .unwrap();
    assert_eq!(held.wasm, wasm);
    let declared = specforge_ops::extension::declare(&runtime, &wasm)
        .unwrap()
        .0;
    assert_eq!(held.declaration, declared);
}

#[specforge_test(
    behavior = "publish_to_registry",
    verify = "the declaration is validated before publish"
)]
fn the_declaration_is_validated_before_publish() {
    let dir = tempfile::tempdir().unwrap();
    let wasm = specforge_ops::testing::GREET;
    let runtime = specforge_ops::testing::candidates();

    let published = report(&binary(dir.path(), wasm), &MemoryRegistry::new(), &runtime);

    assert!(published.warnings.is_empty(), "{:?}", published.warnings);
    assert!(published.result.is_ok(), "{published:?}");
}

#[specforge_test(
    behavior = "install_wasm_extension",
    verify = "add, init and publish read a candidate's declaration in the runtime their surface passes"
)]
fn publish_reads_the_binary_in_the_runtime_it_is_given() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = specforge_ops::testing::candidates();

    report(
        &binary(dir.path(), specforge_ops::testing::GREET),
        &MemoryRegistry::new(),
        &runtime,
    );

    let handshakes = runtime
        .calls()
        .into_iter()
        .filter(|call| call.extension == "__candidate" && call.export == "__handshake")
        .count();
    assert_eq!(handshakes, 1);
}

#[specforge_test(
    behavior = "publish_to_registry",
    verify = "what publish uploads is what add installs"
)]
fn what_publish_uploads_is_what_add_installs() {
    use specforge_installed::{Installed, read_lock_file};
    use specforge_ops::extension::{AddOutcome, AddRequest, add};

    let files = tempfile::tempdir().unwrap();
    let wasm = bytes("@acme/greet 1.0.0");
    let runtime = InProcessRuntime::new().binary(&wasm, declaring("@acme/greet", "1.0.0", &[]));
    let registry = MemoryRegistry::new();
    let published = report(&binary(files.path(), &wasm), &registry, &runtime);
    published.result.expect("published");

    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("specforge.json"),
        r#"{"name": "p", "version": "0.1.0", "extensions": []}"#,
    )
    .unwrap();
    let added = add(
        &AddRequest {
            root: &specforge_common::ProjectRoot::of(project.path()).unwrap(),
            extensions: vec!["@acme/greet@1.0.0".to_string()],
            base: None,
            allow_unsigned: false,
            dry_run: false,
            caller: specforge_ops::testing::caller(),
        },
        &registry,
        &runtime,
    )
    .unwrap();

    let AddOutcome::Installed { version, .. } = &added.added[0].outcome else {
        panic!("not installed: {:?}", added.added[0].outcome);
    };
    assert_eq!(version, "1.0.0");
    let name = PackageName::parse("@acme/greet").unwrap();
    let installed = Installed::unread(project.path());
    assert_eq!(std::fs::read(installed.module_path(&name)).unwrap(), wasm);
    let lock = read_lock_file(&installed.lock_path()).unwrap();
    assert_eq!(lock.entries.len(), 1);
    assert_eq!(lock.entries[0].version, "1.0.0");
    let uploaded = registry
        .fetch(
            &name,
            &Version::parse("1.0.0").unwrap(),
            TrustPolicy::new(false, Asking::Nobody),
        )
        .unwrap()
        .declaration;
    assert_eq!(
        uploaded,
        specforge_ops::extension::declare(&runtime, &wasm)
            .unwrap()
            .0
    );
    assert!(
        registry
            .asked()
            .iter()
            .any(|a| matches!(a, Asked::Fetch(..)))
    );
}

#[specforge_test(
    behavior = "validate_wasm_extension_locally",
    verify = "specforge extension validate reports the declaration's registry build diagnostics"
)]
fn validate_reads_and_declares_the_binary_publish_reads() {
    let dir = tempfile::tempdir().unwrap();
    let good = bytes("v");
    let runtime = InProcessRuntime::new().binary(&good, declaring("@acme/x", "1.0.0", &[]));
    let path = binary(dir.path(), &good);

    let validated = validate(&path, &runtime).unwrap();

    assert_eq!(validated.binary, path);
    assert_eq!(validated.wasm, good);
    assert_eq!(validated.declaration.name(), "@acme/x");
    assert!(validated.valid());
    assert!(validated.document().diagnostics.0.is_empty());

    // A declaration with a registry-build error: not valid, and publish refuses the same binary with its code.
    let bad = bytes("with an error");
    let runtime = InProcessRuntime::new().binary(&bad, || {
        let mut meta = ExtensionMeta::new("@acme/y", "0.1.0");
        meta.short = Some("Friendly greetings".into());
        ContributionsBuilder::new(meta)
    });
    let path = binary(dir.path(), &bad);
    let validated = validate(&path, &runtime).unwrap();
    assert!(!validated.valid());
    let first_error = validated
        .diagnostics
        .iter()
        .find(|d| d.severity == specforge_common::Severity::Error)
        .unwrap();
    let published = report(&path, &MemoryRegistry::new(), &runtime);
    assert_eq!(refused(&published).code, first_error.code, "{published:?}");
}

#[specforge_test(
    behavior = "publish_to_registry",
    verify = "publish refuses in one order, each refusal before anything after it is read or asked"
)]
fn validate_and_publish_refuse_a_missing_binary_alike() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.wasm");
    let runtime = InProcessRuntime::new();

    let validate_error = validate(&missing, &runtime).unwrap_err();
    let published = report(&missing, &MemoryRegistry::new(), &runtime);

    assert_eq!(validate_error.code, "E040");
    assert_eq!(validate_error.code, refused(&published).code);
    assert_eq!(validate_error.message, refused(&published).message);
}

#[specforge_test(
    behavior = "publish_to_registry",
    verify = "publish refuses in one order, each refusal before anything after it is read or asked"
)]
fn the_publish_document_names_the_extension_and_what_was_done() {
    let dir = tempfile::tempdir().unwrap();
    let wasm = bytes("@acme/x 1.0.0");
    let runtime = InProcessRuntime::new().binary(&wasm, declaring("@acme/x", "1.0.0", &[]));
    let document = report(&binary(dir.path(), &wasm), &MemoryRegistry::new(), &runtime)
        .result
        .expect("published");

    let json = serde_json::to_value(document).unwrap();

    let mut keys: Vec<&str> = json
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "action",
            "extension",
            "key_created",
            "key_id",
            "registry",
            "size_bytes",
            "url",
            "version"
        ]
    );
    assert_eq!(json["action"], "published");
    assert_eq!(json["extension"], "@acme/x");
    assert_eq!(json["size_bytes"], wasm.len());
}
