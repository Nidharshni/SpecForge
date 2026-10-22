//! What `ConfiguredRegistry` asks a registry for: the request path of every call, recorded by the real
//! registry server in process (ADR 0044).
//!
//! Plan 12 §3 R1, R4 and R6.
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

use sha2::{Digest, Sha256};
use specforge_ops::asking::Asking;
use specforge_ops::extension::resolve;
use specforge_ops::registry::{LockedPin, Registry, TrustPolicy};
use specforge_ops_registry::{ConfiguredRegistry, User};
use specforge_package::{PackageName, PackageRef, Version};
use specforge_project::testing::TestProject;
use specforge_registry_server::testing::LocalRegistry;
use specforge_registry_wire::PackageMetadata;
use specforge_test_macros::test as specforge_test;

/// The declaration of `@acme/tool@1.0.0`, as `specforge publish` uploads it.
const MANIFEST: &str = r#"{"handshake":{"protocol_version":"1.0.0","name":"@acme/tool","version":"1.0.0","peer_dependencies":[],"sandbox_policy":null}}"#;

/// A registry in process publishing `@acme/tool` at `versions`, unsigned.
fn serving(versions: &[&str]) -> LocalRegistry {
    let server = LocalRegistry::start();
    for version in versions {
        let wasm = b"\0asm-acme-tool";
        server.store(
            &PackageMetadata {
                name: "@acme/tool".to_string(),
                version: version.to_string(),
                sha256: hex::encode(Sha256::digest(wasm)),
                manifest: MANIFEST.to_string(),
                ..Default::default()
            },
            wasm,
        );
    }
    server
}

/// A project whose `registries` is `registry`, as `entry` writes it.
fn project_with(registry: &str) -> TestProject {
    TestProject::new().raw_config(format!(
        r#"{{"name":"p","version":"0.1.0","spec_root":"spec","extensions":[],"registries":[{registry}]}}"#
    ))
}

fn default_registry(url: &str) -> String {
    format!(r#"{{"alias":"main","url":"{url}","default_registry":true}}"#)
}

fn name(text: &str) -> PackageName {
    PackageName::parse(text).unwrap()
}

#[specforge_test(
    behavior = "resolve_registry_source",
    verify = "a fetch requests the name and version it was given, from the registry it was given"
)]
fn the_adapter_requests_these_paths() {
    let served = serving(&["1.4.0", "2.0.0-beta.1"]);
    let dir = project_with(&default_registry(served.root().as_str()));
    let registry = ConfiguredRegistry::for_project(&dir.project_root(), "add")
        .as_user(User::at(dir.home(), None));

    // The registry is asked for the versions, and ops picks among them:
    // `1.x`, `1.2` and `latest` are no longer fetched as versions (§3 R1,
    // R6).
    for (reference, want) in [
        ("@acme/tool@1.x", "1.4.0"),
        ("@acme/tool@1.2", "1.4.0"),
        ("@acme/tool", "1.4.0"),
        ("@acme/tool@>=2.0.0-beta.1", "2.0.0-beta.1"),
    ] {
        let version = resolve(&registry, &PackageRef::parse(reference).unwrap()).unwrap();
        assert_eq!(version.to_string(), want, "{reference}");
        assert_eq!(
            served.requests().last().map(String::as_str),
            Some("GET /v1/packages/@acme%2Ftool"),
            "{reference}"
        );
    }

    // An exact version asks the registry for nothing.
    let before = served.requests().len();
    let exact = resolve(&registry, &PackageRef::parse("@acme/tool@9.9.9").unwrap()).unwrap();
    assert_eq!(exact.to_string(), "9.9.9");
    assert_eq!(served.requests().len(), before);

    // A fetch requests the name and the version it was given: a version
    // cannot carry a `/` or a `?` into the URL, and `2.0.0+build.1` is one.
    let _ = registry.fetch(
        &name("@acme/tool"),
        &Version::new(1, 0, 0),
        TrustPolicy::new(true, Asking::Nobody),
    );
    assert_eq!(
        served.requests().last().map(String::as_str),
        Some("GET /v1/packages/@acme%2Ftool/1.0.0")
    );
    let build = "2.0.0+build.1".parse().unwrap();
    let _ = registry.fetch(
        &name("@acme/tool"),
        &build,
        TrustPolicy::new(true, Asking::Nobody),
    );
    assert_eq!(
        served.requests().last().map(String::as_str),
        Some("GET /v1/packages/@acme%2Ftool/2.0.0+build.1")
    );
}

#[specforge_test(
    behavior = "resolve_registry_source",
    verify = "a name no registry serves is refused with R-OPS-001 before any request"
)]
fn the_adapter_asks_no_registry_for_a_name_none_serves() {
    let served = serving(&["1.0.0"]);
    let dir = project_with(&format!(
        r#"{{"alias":"acme","url":"{}","scope_filter":"@acme"}}"#,
        served.root().as_str()
    ));
    let registry = ConfiguredRegistry::for_project(&dir.project_root(), "add");

    let error = registry.versions(&name("@other/x")).unwrap_err();
    assert_eq!(error.code, "R-OPS-001", "{error:?}");
    let error = registry
        .fetch(
            &name("@other/x"),
            &Version::new(1, 0, 0),
            TrustPolicy::new(true, Asking::Nobody),
        )
        .unwrap_err();
    assert_eq!(error.code, "R-OPS-001", "{error:?}");
    assert!(served.requests().is_empty(), "{:?}", served.requests());
}

#[specforge_test(
    behavior = "resolve_registry_source",
    verify = "a fetch requests the name and version it was given, from the registry it was given"
)]
fn the_registry_is_chosen_once() {
    // One default registry: the adapter chooses it, and the client fetches
    // from it (§3 R4).
    let served = serving(&["1.0.0"]);
    let dir = project_with(&default_registry(served.root().as_str()));
    let home = User::at(dir.home(), None);
    let registry = ConfiguredRegistry::for_project(&dir.project_root(), "add").as_user(home);

    assert_eq!(
        registry.versions(&name("@acme/tool")).unwrap(),
        [Version::new(1, 0, 0)]
    );
    assert_eq!(served.requests(), ["GET /v1/packages/@acme%2Ftool"]);

    // The metadata and the download go to the same registry.
    let package = registry
        .fetch(
            &name("@acme/tool"),
            &Version::new(1, 0, 0),
            TrustPolicy::new(true, Asking::Nobody),
        )
        .unwrap();
    assert_eq!(package.wasm, b"\0asm-acme-tool");
    assert_eq!(
        served.requests(),
        [
            "GET /v1/packages/@acme%2Ftool",
            "GET /v1/packages/@acme%2Ftool/1.0.0",
            "GET /v1/packages/@acme%2Ftool/1.0.0/download",
        ]
    );
}

#[specforge_test(
    behavior = "resolve_registry_source",
    verify = "a scope_filter that is not a scope sends none of its scope's names to another registry"
)]
fn a_scope_filter_that_is_no_scope_asks_no_registry() {
    for text in ["sdk", "@sdk/", "@SDK", ""] {
        let public = serving(&["1.0.0"]);
        let private = serving(&["1.0.0"]);
        let private_entry = format!(
            r#"{{"alias":"private","url":"{}","scope_filter":"{text}"}}"#,
            private.root().as_str()
        );
        let dir = project_with(&format!(
            "{},{private_entry}",
            default_registry(public.root().as_str())
        ));
        let registry = ConfiguredRegistry::for_project(&dir.project_root(), "add");

        let error = registry.versions(&name("@sdk/zeta")).unwrap_err();

        assert_eq!(error.code, "E067", "{text:?}: {error:?}");
        assert!(
            public.requests().is_empty(),
            "{text:?}: {:?}",
            public.requests()
        );
        assert!(
            private.requests().is_empty(),
            "{text:?}: {:?}",
            private.requests()
        );
    }
}

#[specforge_test(
    behavior = "check_registry_reply",
    verify = "a fetch of the version specforge.lock pins served as other bytes is refused before its download"
)]
fn a_locked_version_served_as_other_bytes_is_refused_before_its_download() {
    let served = serving(&["1.0.0"]);
    let dir = project_with(&default_registry(served.root().as_str()));
    let registry = ConfiguredRegistry::for_project(&dir.project_root(), "add")
        .as_user(User::at(dir.home(), None));

    let error = registry
        .fetch(
            &name("@acme/tool"),
            &Version::new(1, 0, 0),
            TrustPolicy::new(true, Asking::Nobody).locked(Some(LockedPin {
                version: "1.0.0",
                sha256: "other",
                key_id: None,
            })),
        )
        .unwrap_err();

    assert!(error.is(specforge_common::codes::R014), "{error:?}");
    assert_eq!(served.requests(), ["GET /v1/packages/@acme%2Ftool/1.0.0"]);
}

#[specforge_test(
    behavior = "verify_publisher_signature",
    verify = "a package the trust decision refuses is refused before its download"
)]
fn an_unsigned_package_is_refused_before_its_download() {
    let served = serving(&["1.0.0"]); // unsigned
    let dir = project_with(&default_registry(served.root().as_str()));
    let registry = ConfiguredRegistry::for_project(&dir.project_root(), "add")
        .as_user(User::at(dir.home(), None));

    let error = registry
        .fetch(
            &name("@acme/tool"),
            &Version::new(1, 0, 0),
            TrustPolicy::new(false, Asking::Nobody),
        )
        .unwrap_err();

    assert!(error.is(specforge_common::codes::R_TRUST_001), "{error:?}");
    assert_eq!(served.requests(), ["GET /v1/packages/@acme%2Ftool/1.0.0"]);
}

/// The user's known-keys store, as `User::at(dir.home(), _)` reads it.
fn store(dir: &TestProject, text: &str) {
    std::fs::create_dir_all(dir.home()).unwrap();
    std::fs::write(dir.home().join("known-keys.json"), text).unwrap();
}

#[specforge_test(
    behavior = "pin_publisher_key",
    verify = "an unsigned package for a pinned name is refused before its download"
)]
fn an_unsigned_package_for_a_pinned_name_is_refused_before_its_download() {
    let served = serving(&["1.0.0"]); // unsigned
    let dir = project_with(&default_registry(served.root().as_str()));
    store(
        &dir,
        r#"{"pins":{"@acme/tool":"k1"},"trusted_keys":[],"denied_keys":[]}"#,
    );
    let registry = ConfiguredRegistry::for_project(&dir.project_root(), "add")
        .as_user(User::at(dir.home(), None));

    let error = registry
        .fetch(
            &name("@acme/tool"),
            &Version::new(1, 0, 0),
            TrustPolicy::new(true, Asking::Yes),
        )
        .unwrap_err();

    assert_eq!(error.code, "R-TRUST-007", "{error:?}");
    assert_eq!(served.requests(), ["GET /v1/packages/@acme%2Ftool/1.0.0"]);
}

#[specforge_test(
    behavior = "pin_publisher_key",
    verify = "a known-keys store that can't be read refuses an unsigned install too"
)]
fn an_unreadable_known_keys_store_refuses_an_unsigned_package_before_its_download() {
    let served = serving(&["1.0.0"]); // unsigned
    let dir = project_with(&default_registry(served.root().as_str()));
    let broken = r#"{"pins":{},}"#;
    store(&dir, broken);
    let registry = ConfiguredRegistry::for_project(&dir.project_root(), "add")
        .as_user(User::at(dir.home(), None));

    let error = registry
        .fetch(
            &name("@acme/tool"),
            &Version::new(1, 0, 0),
            TrustPolicy::new(true, Asking::Nobody),
        )
        .unwrap_err();

    assert_eq!(error.code, "R-TRUST-006", "{error:?}");
    assert_eq!(
        std::fs::read_to_string(dir.home().join("known-keys.json")).unwrap(),
        broken,
        "the store is left as it was"
    );
    assert_eq!(served.requests(), ["GET /v1/packages/@acme%2Ftool/1.0.0"]);
}
