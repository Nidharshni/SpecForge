//! `specforge publish` uploads an extension binary with the declaration it
//! reads from it, against the real registry server running in process.

use crate::support::registry::{greet_named, greet_wasm};
use specforge_registry_client::{HttpRegistryClient, RegistryClient, RegistryConfig};
use specforge_registry_server::testing::LocalRegistry;
use specforge_test::prelude::*;
use specforge_wasm::WasmRuntime as _;
use tempfile::TempDir;

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
    let project = TempDir::new().unwrap();
    std::fs::write(
        project.path().join("specforge.json"),
        serde_json::json!({
            "name": "p", "version": "0.1.0",
            "registries": [{ "alias": "local", "url": registry.root().as_str(), "default_registry": true }]
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(project.path().join("greet.wasm"), greet_wasm()).unwrap();
    let home = TempDir::new().unwrap();

    // Only the binary: it is all publish reads.
    let output = std::process::Command::new(assert_cmd::cargo_bin!("specforge"))
        .arg("publish")
        .arg(project.path().join("greet.wasm"))
        .arg("--path")
        .arg(project.path())
        .args(["--format", "json"])
        .env("HOME", home.path())
        .env("SPECFORGE_REGISTRY_TOKEN", registry.token())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let published: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
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
    let project = TempDir::new().unwrap();
    // A registry nothing listens on: any request would fail differently.
    std::fs::write(
        project.path().join("specforge.json"),
        serde_json::json!({
            "name": "p", "version": "0.1.0",
            "registries": [{ "alias": "local", "url": "http://127.0.0.1:1", "default_registry": true }]
        })
        .to_string(),
    )
    .unwrap();
    let home = TempDir::new().unwrap();

    // `greet-ext1` is a package name, but not a scoped one; `Greet-ext1`
    // is none.
    for (name, why) in [
        ("greet-ext1", "registry packages are named @scope/name"),
        ("Greet-ext1", "starts with a lowercase letter or digit"),
    ] {
        let wasm = project.path().join("ext.wasm");
        std::fs::write(&wasm, greet_named(name)).unwrap();
        let output = std::process::Command::new(assert_cmd::cargo_bin!("specforge"))
            .arg("publish")
            .arg(&wasm)
            .arg("--path")
            .arg(project.path())
            .args(["--format", "json"])
            .env("HOME", home.path())
            .env("SPECFORGE_REGISTRY_TOKEN", "t")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{name}: {output:?}");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(text.contains("E072"), "{name}: {text}");
        assert!(text.contains(why), "{name}: {text}");
    }
}

/// A project whose default registry is `registry`, with the greet binary.
fn project_publishing_to(registry: &LocalRegistry) -> TempDir {
    let project = TempDir::new().unwrap();
    std::fs::write(
        project.path().join("specforge.json"),
        serde_json::json!({
            "name": "p", "version": "0.1.0",
            "registries": [{ "alias": "local", "url": registry.root().as_str(), "default_registry": true }]
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(project.path().join("greet.wasm"), greet_wasm()).unwrap();
    project
}

/// `specforge publish greet.wasm --format json` in `project`; `token` is
/// `SPECFORGE_REGISTRY_TOKEN` (removed when `None`).
fn publish_greet(project: &TempDir, home: &TempDir, token: Option<&str>) -> std::process::Output {
    let mut command = std::process::Command::new(assert_cmd::cargo_bin!("specforge"));
    command
        .arg("publish")
        .arg(project.path().join("greet.wasm"))
        .arg("--path")
        .arg(project.path())
        .args(["--format", "json"])
        .env("HOME", home.path());
    match token {
        Some(token) => command.env("SPECFORGE_REGISTRY_TOKEN", token),
        None => command.env_remove("SPECFORGE_REGISTRY_TOKEN"),
    };
    command.output().unwrap()
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
    let home = TempDir::new().unwrap();

    let output = publish_greet(&project, &home, None);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
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
    assert!(!home.path().join(".specforge/signing-key.json").exists());
    assert!(!greet_is_published(&registry));
}

#[specforge_test(
    behavior = "publish_to_registry",
    verify = "a version already published is refused with R007"
)]
fn publishing_a_version_twice_is_refused_with_r007() {
    let registry = LocalRegistry::start();
    let project = project_publishing_to(&registry);
    let home = TempDir::new().unwrap();

    let first = publish_greet(&project, &home, Some(registry.token()));
    assert!(first.status.success(), "{first:?}");
    let published: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
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
            "key_created",
            "key_id",
            "name",
            "registry",
            "signed",
            "size_bytes",
            "url",
            "version"
        ]
    );
    assert_eq!(published["key_created"], true);
    assert_eq!(published["registry"], "local");

    let second = publish_greet(&project, &home, Some(registry.token()));
    assert_eq!(second.status.code(), Some(1), "{second:?}");
    let refused: serde_json::Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_eq!(refused["code"], "R007", "{refused}");
}
