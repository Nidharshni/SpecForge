//! `specforge publish` uploads an extension binary with the declaration it
//! reads from it, against the real registry server running in process.

use crate::support::registry::{greet_named, greet_wasm};
use crate::support::{Cli, Run, TestProject};
use specforge_registry_client::{HttpRegistryClient, RegistryClient, RegistryConfig};
use specforge_registry_server::testing::LocalRegistry;
use specforge_test::prelude::*;
use specforge_wasm::WasmRuntime as _;

/// How `specforge.json` names `registry`.
fn config(registry: &LocalRegistry) -> RegistryConfig {
    RegistryConfig {
        alias: "local".to_string(),
        url: registry.root().clone(),
        scope_filter: None,
        default_registry: true,
    }
}

#[specforge_test(
    behavior = "publish_to_registry",
    verify = "publish derives the stored declaration from the binary"
)]
fn publish_stores_the_declaration_the_binary_declares() {
    let registry = LocalRegistry::start();
    let project = project_publishing_to(&registry);

    // Only the binary: it is all publish reads.
    let output = publish_greet(&project, Some(registry.token()));
    assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);
    let published = output.json();
    assert_eq!(published["name"], "@sdk/greet");
    assert_eq!(published["version"], "0.1.0");

    // The stored manifest is exactly greet's declaration.
    let served = HttpRegistryClient::new()
        .metadata(
            &specforge_package::PackageName::parse("@sdk/greet").unwrap(),
            &specforge_package::Version::new(0, 1, 0),
            &config(&registry),
            None,
        )
        .unwrap();
    let stored: specforge_protocol_types::ExtensionDeclaration =
        serde_json::from_str(&served.manifest).unwrap();
    let runtime = specforge_component::ComponentRuntime::new();
    runtime.load("@sdk/greet", &greet_wasm()).unwrap();
    let declared = specforge_wasm::protocol::load_declaration(&runtime, "@sdk/greet")
        .unwrap()
        .declaration;
    assert_eq!(stored, declared);
    assert_eq!(
        stored.handshake.description.as_deref(),
        Some("Friendly greetings")
    );
}

#[specforge_test(
    behavior = "publish_wasm_extension",
    verify = "publish refuses a declaration whose name or version is not publishable before it uploads"
)]
fn publish_refuses_an_unscoped_or_unversioned_declaration_offline() {
    // A registry nothing listens on: any request would fail differently.
    let project = TestProject::new().config(|c| {
        c["name"] = "p".into();
        c["registries"] = serde_json::json!([
            { "alias": "local", "url": "http://127.0.0.1:1", "default_registry": true }
        ]);
        c.as_object_mut().unwrap().remove("extensions");
    });

    // `greet-ext1` is a package name, but not a scoped one; `Greet-ext1`
    // is none.
    for (name, why) in [
        ("greet-ext1", "registry packages are named @scope/name"),
        ("Greet-ext1", "starts with a lowercase letter or digit"),
    ] {
        project.write("ext.wasm", greet_named(name));
        let wasm = project.root().join("ext.wasm");
        let output = Cli::on(&project)
            .env("SPECFORGE_REGISTRY_TOKEN", "t")
            .run(&["publish", wasm.to_str().unwrap(), "--format", "json"]);
        assert_eq!(output.code, 1, "{name}: {}{}", output.stdout, output.stderr);
        let text = format!("{}{}", output.stdout, output.stderr);
        assert!(text.contains("E072"), "{name}: {text}");
        assert!(text.contains(why), "{name}: {text}");
    }
}

/// A project whose default registry is `registry`, with the greet binary.
fn project_publishing_to(registry: &LocalRegistry) -> TestProject {
    TestProject::new()
        .config(|c| {
            c["name"] = "p".into();
            c["registries"] = serde_json::json!([
                { "alias": "local", "url": registry.root().as_str(), "default_registry": true }
            ]);
            c.as_object_mut().unwrap().remove("extensions");
        })
        .file("greet.wasm", greet_wasm())
}

/// `specforge publish greet.wasm --format json` in `project`; `token` is
/// `SPECFORGE_REGISTRY_TOKEN` (removed when `None`).
fn publish_greet(project: &TestProject, token: Option<&str>) -> Run {
    let wasm = project.root().join("greet.wasm");
    let cli = match token {
        Some(token) => Cli::on(project).env("SPECFORGE_REGISTRY_TOKEN", token),
        None => Cli::on(project),
    };
    cli.run(&["publish", wasm.to_str().unwrap(), "--format", "json"])
}

fn greet_is_published(registry: &LocalRegistry) -> bool {
    HttpRegistryClient::new()
        .metadata(
            &specforge_package::PackageName::parse("@sdk/greet").unwrap(),
            &specforge_package::Version::new(0, 1, 0),
            &config(registry),
            None,
        )
        .is_ok()
}

#[specforge_test(
    behavior = "publish_to_registry",
    verify = "with no credential for its registry, publish refuses with R001 before any network call and creates no signing key"
)]
fn a_publish_with_no_credential_is_refused_before_any_request() {
    let registry = LocalRegistry::start();
    let project = project_publishing_to(&registry);

    let output = publish_greet(&project, None);

    assert_eq!(output.code, 1, "{}{}", output.stdout, output.stderr);
    let json = output.json();
    assert_eq!(json["code"], "R001", "{json}");
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("no credential for registry 'local'"),
        "{json}"
    );
    assert!(
        json["suggestion"]
            .as_str()
            .unwrap()
            .contains("specforge login --registry local"),
        "{json}"
    );
    assert!(!project.home().join(".specforge/signing-key.json").exists());
    assert!(!greet_is_published(&registry));
}

#[specforge_test(
    behavior = "publish_to_registry",
    verify = "a version already published is refused with R007"
)]
fn publishing_a_version_twice_is_refused_with_r007() {
    let registry = LocalRegistry::start();
    let project = project_publishing_to(&registry);

    let first = publish_greet(&project, Some(registry.token()));
    assert_eq!(first.code, 0, "{}{}", first.stdout, first.stderr);
    let published = first.json();
    let mut keys: Vec<&str> = published
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
    assert_eq!(published["action"], "published");
    assert_eq!(published["key_created"], true);
    assert_eq!(published["registry"], "local");

    let second = publish_greet(&project, Some(registry.token()));
    assert_eq!(second.code, 1, "{}{}", second.stdout, second.stderr);
    let refused = second.json();
    assert_eq!(refused["code"], "R007", "{refused}");
}
