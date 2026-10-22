//! `ConfiguredRegistry::fetch`, the fetch policy over the client seam: it refuses a tampered
//! download, a broken or missing signature and a re-keyed publisher, and
//! pins the key of a correctly signed package (docs/registry-trust.md).
//!
//! Each test serves one package from an in-memory client (ADR 0044) and pins
//! keys in a temporary store, never in `~/.specforge`.

use specforge_ops::asking::testing::{Asked, Scripted};
use specforge_ops::asking::{Asking, ExpectedFrom};
use specforge_ops::registry::{LockedPin, Publisher, Registry, TrustPolicy};
use specforge_ops_registry::{ConfiguredRegistry, User};
use specforge_package::PackageName;
use specforge_package::Version;
use specforge_project::testing::TestProject;
use specforge_registry_client::testing::MemoryClient;
use specforge_registry_client::{
    KnownKeys, RegistryConfig, SigningKey, load_known_keys_at, save_known_keys_at,
};
use specforge_registry_wire::{
    PackageMetadata, PackageSignature, RegistryRoot, SignedContent, sha256_hex,
};
use specforge_test_macros::test as specforge_test;
use std::path::{Path, PathBuf};

const NAME: &str = "@acme/tool";
const VERSION: &str = "1.0.0";
const WASM: &[u8] = b"\0asm-acme-tool";
/// The package's declaration, as `specforge publish` uploads it.
const MANIFEST: &str = r#"{"handshake":{"protocol_version":"1.0.0","name":"@acme/tool","version":"1.0.0","peer_dependencies":[],"sandbox_policy":null}}"#;

/// What the registry serves for `@acme/tool@1.0.0`.
struct Reply {
    name: String,
    /// The metadata's `sha256`.
    sha256: String,
    /// The downloaded bytes.
    wasm: Vec<u8>,
    manifest: String,
    /// The publisher signature, `None` when unsigned.
    signature: Option<PackageSignature>,
    /// Where the metadata says the binary is; empty is where it is.
    wasm_url: String,
}

impl Reply {
    fn unsigned() -> Self {
        Reply {
            name: NAME.to_string(),
            sha256: sha256_hex(WASM),
            wasm: WASM.to_vec(),
            manifest: MANIFEST.to_string(),
            signature: None,
            wasm_url: String::new(),
        }
    }

    /// Signed by `key` over `name`, `wasm` and `manifest`.
    fn signed_over(key: &SigningKey, name: &str, wasm: &[u8], manifest: &str) -> Self {
        let signature = key.sign(&SignedContent::of(name, VERSION, wasm, manifest));
        Reply {
            name: name.to_string(),
            wasm: wasm.to_vec(),
            sha256: sha256_hex(wasm),
            manifest: manifest.to_string(),
            signature: Some(signature),
            wasm_url: String::new(),
        }
    }

    fn signed(key: &SigningKey) -> Self {
        Reply::signed_over(key, NAME, WASM, MANIFEST)
    }

    /// The reply the client serves for `@acme/tool@1.0.0`, and its binary.
    fn store(self, client: &MemoryClient) {
        let metadata = PackageMetadata {
            name: self.name,
            version: VERSION.to_string(),
            sha256: self.sha256,
            manifest: self.manifest,
            signature: self.signature,
            wasm_url: self.wasm_url,
            ..Default::default()
        };
        // Whatever it describes is the answer for the package asked for.
        client.store_as(&registry_config(), NAME, VERSION, metadata, self.wasm);
    }
}

const REGISTRY_URL: &str = "https://local.invalid";

fn registry_config() -> RegistryConfig {
    RegistryConfig {
        alias: "local".to_string(),
        url: RegistryRoot::parse(REGISTRY_URL).expect("a valid literal"),
        scope_filter: None,
        default_registry: true,
    }
}

/// A project whose only registry serves `served`, and a known-keys store
/// path inside it.
struct Project {
    dir: TestProject,
    client: MemoryClient,
}

impl Project {
    fn on(served: Reply) -> Self {
        let client = MemoryClient::new();
        served.store(&client);
        let dir = TestProject::new().config(|c| {
            c["name"] = "p".into();
            c["registries"] =
                serde_json::json!([{ "alias": "local", "url": REGISTRY_URL, "default_registry": true }]);
        });
        Project { dir, client }
    }

    fn known_keys(&self) -> PathBuf {
        self.dir.home().join("known-keys.json")
    }

    fn pin(&self, key: &SigningKey) {
        let mut known = KnownKeys::default();
        known.pin(NAME, &key.key_id());
        save_known_keys_at(&self.known_keys(), &known).expect("the pins file is writable");
    }

    fn registry(&self) -> ConfiguredRegistry {
        ConfiguredRegistry::for_project(&self.dir.project_root(), "add")
            .with_client(self.client.clone())
            .as_user(User::at(self.dir.home(), None))
    }

    fn fetch(
        &self,
        allow_unsigned: bool,
        asking: Asking<'_>,
    ) -> Result<specforge_ops::registry::Package, specforge_ops::OpError> {
        self.registry().fetch(
            &PackageName::parse(NAME).expect("a valid literal"),
            &Version::parse(VERSION).expect("a valid literal"),
            TrustPolicy::new(allow_unsigned, asking),
        )
    }

    fn pinned(&self) -> Option<String> {
        pinned_at(&self.known_keys())
    }
}

#[specforge_test(
    behavior = "pin_publisher_key",
    verify = "a known-keys store that can't be read refuses the install and is left as it was"
)]
fn an_unreadable_known_keys_file_refuses_the_fetch_and_is_left_as_it_was() {
    let key = SigningKey::generate();
    let project = Project::on(Reply::signed(&key));
    std::fs::create_dir_all(project.known_keys().parent().unwrap()).unwrap();
    // A trailing comma: not JSON.
    let broken = format!(
        r#"{{"pins":{{}},"trusted_keys":[],"denied_keys":["{}"],}}"#,
        key.key_id()
    );
    std::fs::write(project.known_keys(), &broken).unwrap();

    let error = project.fetch(false, Asking::Nobody).unwrap_err();

    assert_eq!(error.code, "R-TRUST-006", "{error:?}");
    assert!(
        error.message.contains("trailing comma"),
        "{}",
        error.message
    );
    assert_eq!(
        std::fs::read_to_string(project.known_keys()).unwrap(),
        broken
    );
}

/// `@acme/tool@1.0.0` fetched through `registry`.
fn fetch_through(
    registry: &ConfiguredRegistry,
    allow_unsigned: bool,
    asking: Asking<'_>,
) -> Result<specforge_ops::registry::Package, specforge_ops::OpError> {
    registry.fetch(
        &PackageName::parse(NAME).expect("a valid literal"),
        &Version::parse(VERSION).expect("a valid literal"),
        TrustPolicy::new(allow_unsigned, asking),
    )
}

fn pinned_at(path: &Path) -> Option<String> {
    load_known_keys_at(path)
        .expect("the pins file reads")
        .pin_for(NAME)
        .map(str::to_string)
}

#[specforge_test(
    behavior = "verify_registry_integrity",
    verify = "mismatched SHA256 produces hard error"
)]
fn a_download_that_does_not_match_the_registry_sha256_is_refused() {
    let key = SigningKey::generate();
    let served = Reply {
        wasm: b"\0asm-evil".to_vec(),
        ..Reply::signed(&key)
    };
    let project = Project::on(served);

    // Neither --allow-unsigned nor --yes bypasses integrity.
    let error = project.fetch(true, Asking::Yes).unwrap_err();
    assert_eq!(error.code, "R-OPS-002", "{error:?}");
    assert_eq!(
        project.pinned(),
        None,
        "nothing is pinned for a refused package"
    );
}

#[specforge_test(
    invariant = "registry_integrity",
    verify = "SHA256 mismatch produces hard error and aborts"
)]
fn a_registry_sha256_that_does_not_match_the_download_is_refused() {
    let served = Reply {
        sha256: sha256_hex(b"something else"),
        ..Reply::unsigned()
    };
    let project = Project::on(served);

    let error = project.fetch(true, Asking::Yes).unwrap_err();
    assert_eq!(error.code, "R-OPS-002", "{error:?}");
}

#[specforge_test(
    invariant = "mcp_structured_error_responses",
    verify = "a package the registry or the trust check refuses is never an internal_error"
)]
fn a_trust_refusal_is_permission_denied() {
    // Unsigned, without --allow-unsigned.
    let project = Project::on(Reply::unsigned());
    let error = project.fetch(false, Asking::Nobody).unwrap_err();
    assert_eq!(error.code, "R-TRUST-001", "{error:?}");
    assert_eq!(error.kind, specforge_ops::OpErrorKind::PermissionDenied);

    // A signature over other bytes.
    let key = SigningKey::generate();
    let served = Reply {
        sha256: sha256_hex(b"\0asm-evil"),
        wasm: b"\0asm-evil".to_vec(),
        ..Reply::signed(&key)
    };
    let error = Project::on(served).fetch(true, Asking::Yes).unwrap_err();
    assert_eq!(error.code, "R-TRUST-002", "{error:?}");
    assert_eq!(error.kind, specforge_ops::OpErrorKind::PermissionDenied);
}

#[specforge_test(
    behavior = "verify_publisher_signature",
    verify = "a signature over other bytes is refused even with --allow-unsigned"
)]
fn a_signature_over_other_bytes_is_refused_even_with_allow_unsigned() {
    // The registry serves a consistent sha256 for the bytes it sends, but
    // the publisher signed different ones: integrity passes, the signature
    // must not.
    let key = SigningKey::generate();
    let served = Reply {
        sha256: sha256_hex(b"\0asm-evil"),
        wasm: b"\0asm-evil".to_vec(),
        ..Reply::signed(&key)
    };
    let project = Project::on(served);

    let error = project.fetch(true, Asking::Yes).unwrap_err();
    assert_eq!(error.code, "R-TRUST-002", "{error:?}");
    assert_eq!(project.pinned(), None);
}

#[specforge_test(
    behavior = "verify_publisher_signature",
    verify = "a swapped manifest breaks the signature"
)]
fn a_swapped_manifest_breaks_the_signature() {
    let key = SigningKey::generate();
    let served = Reply {
        manifest: MANIFEST.replace(
            "\"sandbox_policy\":null",
            "\"sandbox_policy\":null,\"theme_color\":\"#000000\"",
        ),
        ..Reply::signed(&key)
    };
    let project = Project::on(served);

    let error = project.fetch(true, Asking::Yes).unwrap_err();
    assert_eq!(error.code, "R-TRUST-002", "{error:?}");
}

#[specforge_test(
    behavior = "verify_publisher_signature",
    verify = "an unsigned package is refused without --allow-unsigned"
)]
fn an_unsigned_package_is_refused_without_allow_unsigned() {
    let project = Project::on(Reply::unsigned());

    let error = project.fetch(false, Asking::Yes).unwrap_err();
    assert_eq!(error.code, "R-TRUST-001", "{error:?}");
    assert!(
        error
            .suggestion
            .as_deref()
            .unwrap_or_default()
            .contains("`specforge add --allow-unsigned`"),
        "{error:?}"
    );
}

#[specforge_test(
    behavior = "verify_publisher_signature",
    verify = "an unsigned package is accepted with --allow-unsigned and pins no key"
)]
fn an_unsigned_package_is_accepted_with_allow_unsigned_and_pins_nothing() {
    let project = Project::on(Reply::unsigned());

    let package = project.fetch(true, Asking::Nobody).unwrap();
    assert_eq!(package.wasm, WASM);
    assert_eq!(package.publisher, Publisher::Unsigned);
    assert_eq!(project.pinned(), None);
}

#[specforge_test(
    behavior = "pin_publisher_key",
    verify = "the key of the first verified install is pinned and accepted again"
)]
fn a_correctly_signed_package_is_accepted_and_its_key_pinned() {
    let key = SigningKey::generate();
    let project = Project::on(Reply::signed(&key));
    let registry = project.registry();

    let package = fetch_through(&registry, false, Asking::Nobody).unwrap();
    assert_eq!(package.name.as_str(), NAME);
    assert_eq!(package.version.to_string(), VERSION);
    assert_eq!(package.wasm, WASM);
    assert_eq!(package.sha256, sha256_hex(WASM));
    assert_eq!(
        package.publisher,
        Publisher::Signed {
            key_id: key.key_id(),
            first_use: true
        }
    );
    assert_eq!(project.pinned(), None, "the fetch pins nothing");

    // The lock plan that fetched it pins the key when it commits.
    let name = PackageName::parse(NAME).unwrap();
    registry
        .pin(&[package.publisher.pin(&name).expect("a first use pins")])
        .unwrap();
    assert_eq!(project.pinned(), Some(key.key_id()));

    // A second install under the same pin is accepted as is.
    let again = project.fetch(false, Asking::Nobody).unwrap();
    assert_eq!(
        again.publisher,
        Publisher::Signed {
            key_id: key.key_id(),
            first_use: false
        }
    );
    assert_eq!(project.pinned(), Some(key.key_id()));
}

#[specforge_test(
    behavior = "pin_publisher_key",
    verify = "a fetch pins nothing; its pins are recorded when the install is written"
)]
fn a_fetch_pins_nothing() {
    let pinned = SigningKey::generate();
    let other = SigningKey::generate();
    let project = Project::on(Reply::signed(&other));

    // First use: no store is created.
    project.fetch(false, Asking::Nobody).unwrap();
    assert!(!project.known_keys().exists());

    // A consented key change: the store is as it was, byte for byte.
    project.pin(&pinned);
    let before = std::fs::read(project.known_keys()).unwrap();
    project.fetch(false, Asking::Yes).unwrap();
    project.fetch(false, Asking::Nobody).unwrap_err();
    assert_eq!(std::fs::read(project.known_keys()).unwrap(), before);
}

#[test]
fn a_pin_that_cannot_be_saved_is_r_trust_006() {
    let key = SigningKey::generate();
    let project = Project::on(Reply::signed(&key));
    let registry = project.registry();
    let package = fetch_through(&registry, false, Asking::Nobody).unwrap();
    // The store's path is a directory: it can be neither read nor replaced.
    std::fs::create_dir_all(project.known_keys()).unwrap();
    let name = PackageName::parse(NAME).unwrap();

    let error = registry
        .pin(&[package.publisher.pin(&name).unwrap()])
        .unwrap_err();

    assert_eq!(error.code, "R-TRUST-006", "{error:?}");
    assert!(project.known_keys().is_dir());
}

/// `@acme/tool@1.0.0` fetched through `registry` while the project's lock records `locked`.
fn fetch_locked(
    registry: &ConfiguredRegistry,
    locked: &str,
    asking: Asking<'_>,
) -> Result<specforge_ops::registry::Package, specforge_ops::OpError> {
    registry.fetch(
        &PackageName::parse(NAME).expect("a valid literal"),
        &Version::parse(VERSION).expect("a valid literal"),
        TrustPolicy::new(false, asking).locked(Some(LockedPin::key(locked))),
    )
}

#[specforge_test(
    behavior = "pin_publisher_key",
    verify = "a user's pin wins over the key specforge.lock records"
)]
fn a_users_pin_wins_over_the_locked_key() {
    let (pinned, locked) = (SigningKey::generate(), SigningKey::generate());

    // Signed by the user's pin: accepted as pinned, whatever the lock records.
    let project = Project::on(Reply::signed(&pinned));
    project.pin(&pinned);
    let package = fetch_locked(&project.registry(), &locked.key_id(), Asking::Nobody).unwrap();
    assert_eq!(
        package.publisher,
        Publisher::Signed {
            key_id: pinned.key_id(),
            first_use: false
        }
    );

    // Signed by the lock's key: a change from the user's pin.
    let project = Project::on(Reply::signed(&locked));
    project.pin(&pinned);
    let error = fetch_locked(&project.registry(), &locked.key_id(), Asking::Nobody).unwrap_err();
    assert_eq!(error.code, "R-TRUST-003", "{error:?}");
    assert!(
        error.message.ends_with("(the key specforge.lock records)"),
        "{}",
        error.message
    );
}

#[test]
fn with_no_pin_a_package_signed_by_the_locked_key_is_a_first_use() {
    let key = SigningKey::generate();
    let project = Project::on(Reply::signed(&key));

    let package = fetch_locked(&project.registry(), &key.key_id(), Asking::Nobody).unwrap();

    assert_eq!(
        package.publisher,
        Publisher::Signed {
            key_id: key.key_id(),
            first_use: true
        }
    );
    assert_eq!(project.pinned(), None, "the fetch pins nothing");
}

#[specforge_test(
    behavior = "pin_publisher_key",
    verify = "a package signed by another key than the pinned one is refused"
)]
fn a_package_signed_by_another_key_than_the_pinned_one_is_refused() {
    let pinned = SigningKey::generate();
    let other = SigningKey::generate();
    let project = Project::on(Reply::signed(&other));
    project.pin(&pinned);

    let error = project.fetch(false, Asking::Nobody).unwrap_err();
    assert_eq!(error.code, "R-TRUST-003", "{error:?}");
    assert!(error.message.contains(&pinned.key_id()), "{error:?}");
    assert!(error.message.contains(&other.key_id()), "{error:?}");
    // --allow-unsigned is not consent to a key change either.
    let error = project.fetch(true, Asking::Nobody).unwrap_err();
    assert_eq!(error.code, "R-TRUST-003", "{error:?}");
    assert_eq!(project.pinned(), Some(pinned.key_id()), "the pin stands");
}

#[specforge_test(
    behavior = "pin_publisher_key",
    verify = "consent to a key change re-pins the new key"
)]
fn consent_to_a_key_change_re_pins_the_new_key() {
    let pinned = SigningKey::generate();
    let other = SigningKey::generate();
    let project = Project::on(Reply::signed(&other));
    project.pin(&pinned);
    let registry = project.registry();

    let package = fetch_through(&registry, false, Asking::Yes).unwrap();
    assert_eq!(
        package.publisher,
        Publisher::Repinned {
            key_id: other.key_id(),
            previous: pinned.key_id()
        }
    );
    assert_eq!(
        project.pinned(),
        Some(pinned.key_id()),
        "the fetch pins nothing"
    );

    let name = PackageName::parse(NAME).unwrap();
    registry
        .pin(&[package.publisher.pin(&name).expect("a re-pin pins")])
        .unwrap();
    assert_eq!(project.pinned(), Some(other.key_id()));
}

#[specforge_test(
    behavior = "pin_publisher_key",
    verify = "a denied key is refused even with consent"
)]
fn a_denied_key_is_refused_even_with_consent() {
    let key = SigningKey::generate();
    let project = Project::on(Reply::signed(&key));
    let mut known = KnownKeys::default();
    known.denied_keys.push(key.key_id());
    save_known_keys_at(&project.known_keys(), &known).unwrap();

    let error = project.fetch(true, Asking::Yes).unwrap_err();
    assert_eq!(error.code, "R-TRUST-005", "{error:?}");
}

#[specforge_test(
    behavior = "check_registry_reply",
    verify = "a reply for another package is refused and pins nothing"
)]
fn a_registry_answering_with_another_package_is_refused() {
    // `@acme/tool` is pinned; a registry that answers the request with a
    // validly signed `@evil/tool` must not get it past the pin.
    let pinned = SigningKey::generate();
    let evil = SigningKey::generate();
    let project = Project::on(Reply::signed_over(&evil, "@evil/tool", WASM, MANIFEST));
    project.pin(&pinned);

    let error = project.fetch(false, Asking::Yes).unwrap_err();
    assert_eq!(error.code, "R-TRUST-004", "{error:?}");
    assert!(error.message.contains("@evil/tool"), "{error:?}");
    assert_eq!(project.pinned(), Some(pinned.key_id()));
    let known = load_known_keys_at(&project.known_keys()).unwrap();
    assert_eq!(
        known.pin_for("@evil/tool"),
        None,
        "nothing is pinned for it"
    );
}

#[specforge_test(
    behavior = "check_registry_reply",
    verify = "the peers the served manifest declares reach the package"
)]
fn a_correctly_signed_package_carries_the_peers_its_manifest_declares() {
    let key = SigningKey::generate();
    let manifest = r#"{"handshake":{"protocol_version":"1.0.0","name":"@acme/tool","version":"1.0.0","peer_dependencies":[{"name":"@acme/base","version":"^1.0"}],"sandbox_policy":null}}"#;
    let project = Project::on(Reply::signed_over(&key, NAME, WASM, manifest));

    let package = project.fetch(false, Asking::Nobody).unwrap();
    let peers = package.declaration.peers();
    assert_eq!(peers.len(), 1, "{peers:?}");
    assert_eq!(peers[0].name, "@acme/base");
    assert_eq!(peers[0].version, "^1.0");
}

#[specforge_test(
    behavior = "check_registry_reply",
    verify = "a manifest that cannot be read is refused and pins nothing"
)]
fn a_manifest_that_cannot_be_read_is_refused_and_pins_nothing() {
    // Signed over the unreadable manifest, so only the manifest itself is
    // wrong. Read as "no peers", it would slip past the diamond gate.
    let key = SigningKey::generate();
    let manifest = r#"{"handshake":{"name":"@acme/tool","peer_dependencies":"not a list"}}"#;
    let project = Project::on(Reply::signed_over(&key, NAME, WASM, manifest));

    let error = project.fetch(true, Asking::Yes).unwrap_err();
    assert_eq!(error.code, "R-OPS-004", "{error:?}");
    assert_eq!(project.pinned(), None, "nothing is pinned for it");
}

#[specforge_test(
    invariant = "registry_reply_binding",
    verify = "a package served without a manifest is refused"
)]
fn a_package_served_without_a_manifest_is_refused() {
    let served = Reply {
        manifest: String::new(),
        ..Reply::unsigned()
    };
    let project = Project::on(served);

    let error = project.fetch(true, Asking::Yes).unwrap_err();
    assert_eq!(error.code, "R-OPS-004", "{error:?}");
    assert!(error.message.contains("served none"), "{error:?}");
}

#[specforge_test(
    invariant = "registry_reply_binding",
    verify = "a manifest describing another package is refused"
)]
fn a_manifest_describing_another_package_is_refused() {
    let key = SigningKey::generate();
    let manifest = r#"{"handshake":{"protocol_version":"1.0.0","name":"@evil/tool","version":"1.0.0","peer_dependencies":[],"sandbox_policy":null}}"#;
    let project = Project::on(Reply::signed_over(&key, NAME, WASM, manifest));

    let error = project.fetch(true, Asking::Yes).unwrap_err();
    assert_eq!(error.code, "R-TRUST-004", "{error:?}");
    assert!(error.message.contains("@evil/tool"), "{error:?}");
    assert_eq!(project.pinned(), None);
}

#[specforge_test(
    behavior = "check_registry_reply",
    verify = "a package published with a legacy manifest is refused with a re-publish suggestion"
)]
fn a_package_published_with_a_legacy_manifest_is_refused() {
    // Published before ADR 0012: its manifest is the camelCase
    // manifest file, not a declaration.
    let key = SigningKey::generate();
    let legacy =
        r#"{"name":"@acme/tool","version":"1.0.0","manifestVersion":2,"wasmPath":"tool.wasm"}"#;
    let project = Project::on(Reply::signed_over(&key, NAME, WASM, legacy));

    let error = project.fetch(true, Asking::Yes).unwrap_err();
    assert_eq!(error.code, "R-OPS-004", "{error:?}");
    assert!(error.message.contains("manifest.json"), "{error:?}");
    let suggestion = error.suggestion.as_deref().unwrap_or_default();
    assert!(
        suggestion.contains("re-publish @acme/tool@1.0.0"),
        "{suggestion}"
    );
    assert_eq!(project.pinned(), None, "nothing is pinned for it");
}

#[test]
fn a_download_that_misses_is_refused_and_pins_nothing() {
    // The reply is signed and describes the package, but the binary is
    // not where it says: the policy stops at the download.
    let key = SigningKey::generate();
    let served = Reply {
        wasm_url: "memory://nowhere".to_string(),
        ..Reply::signed(&key)
    };
    let project = Project::on(served);

    let error = project.fetch(true, Asking::Yes).unwrap_err();
    assert_eq!(error.code, "R006", "{error:?}");
    assert_eq!(error.kind, specforge_ops::OpErrorKind::ExtensionNotFound);
    assert_eq!(project.pinned(), None, "nothing is pinned for it");
}

fn store_credentials(project: &Project, json: &str) {
    let home = project.dir.home();
    std::fs::create_dir_all(home).expect("the home directory is created");
    let store = format!(r#"{{"registries":{{"{REGISTRY_URL}":{json}}}}}"#);
    std::fs::write(home.join("credentials.json"), store).expect("the credentials file is writable");
}

/// Every read the client answered: versions, metadata and download calls.
fn reads(project: &Project) -> Vec<specforge_registry_client::testing::Call> {
    use specforge_registry_client::testing::CallKind;
    project
        .client
        .calls()
        .into_iter()
        .filter(|call| {
            matches!(
                call.kind,
                CallKind::Versions | CallKind::Metadata | CallKind::Download
            )
        })
        .collect()
}

#[specforge_test(
    behavior = "support_private_registries",
    verify = "authentication occurs before fetch from private registry"
)]
fn a_read_carries_the_stored_credential() {
    let project = Project::on(Reply::unsigned());
    store_credentials(&project, r#"{"token":"t"}"#);
    let registry = project.registry();
    let name = PackageName::parse(NAME).unwrap();

    registry.versions(&name).unwrap();
    registry
        .fetch(
            &name,
            &Version::parse(VERSION).unwrap(),
            TrustPolicy::new(true, Asking::Nobody),
        )
        .unwrap();

    let reads = reads(&project);
    assert_eq!(reads.len(), 3, "{reads:?}");
    assert!(
        reads
            .iter()
            .all(|call| call.credential.as_ref().map(|c| c.token()) == Some("t")),
        "{reads:?}"
    );
}

#[specforge_test(
    behavior = "support_private_registries",
    verify = "an unusable stored credential refuses the read before any request"
)]
fn an_expired_stored_credential_refuses_a_read() {
    let project = Project::on(Reply::unsigned());
    store_credentials(
        &project,
        r#"{"token":"t","expires_at":"2000-01-01T00:00:00Z"}"#,
    );

    let error = project
        .registry()
        .versions(&PackageName::parse(NAME).unwrap())
        .unwrap_err();

    assert_eq!(error.code, "R-AUTH-020", "{error:?}");
    assert!(project.client.calls().is_empty());
}

#[test]
fn the_environment_token_is_not_sent_on_a_read() {
    let project = Project::on(Reply::unsigned());
    let registry = ConfiguredRegistry::for_project(&project.dir.project_root(), "add")
        .with_client(project.client.clone())
        .as_user(User::at(project.dir.home(), Some("env".to_string())));

    registry
        .versions(&PackageName::parse(NAME).unwrap())
        .unwrap();

    let reads = reads(&project);
    assert_eq!(reads.len(), 1);
    assert!(reads[0].credential.is_none());
}

#[specforge_test(
    behavior = "verify_publisher_signature",
    verify = "an unsigned package accepted with --allow-unsigned is reported as W155"
)]
fn an_unsigned_fetch_is_reported_as_w155() {
    let project = Project::on(Reply::unsigned());
    let registry = project.registry();

    let package = registry
        .fetch(
            &PackageName::parse(NAME).unwrap(),
            &Version::parse(VERSION).unwrap(),
            TrustPolicy::new(true, Asking::Nobody),
        )
        .unwrap();

    assert_eq!(package.publisher, Publisher::Unsigned);
    let reported = registry.reported();
    let w155: Vec<_> = reported.iter().filter(|d| d.code == "W155").collect();
    assert_eq!(w155.len(), 1, "{reported:?}");
    assert!(w155[0].message.contains("@acme/tool 1.0.0"), "{w155:?}");
}

#[specforge_test(
    behavior = "pin_publisher_key",
    verify = "consent to a key change is reported as W156"
)]
fn a_consented_key_change_is_reported_as_w156() {
    let pinned = SigningKey::generate();
    let other = SigningKey::generate();
    let project = Project::on(Reply::signed(&other));
    project.pin(&pinned);
    let registry = project.registry();

    let package = fetch_through(&registry, false, Asking::Yes).unwrap();

    let name = PackageName::parse(NAME).unwrap();
    let w156_count = |registry: &ConfiguredRegistry| {
        registry
            .reported()
            .iter()
            .filter(|d| d.code == "W156")
            .count()
    };
    assert_eq!(w156_count(&registry), 0, "{:?}", registry.reported());
    registry
        .pin(&[package.publisher.pin(&name).expect("a re-pin pins")])
        .unwrap();

    let reported = registry.reported();
    let w156: Vec<_> = reported.iter().filter(|d| d.code == "W156").collect();
    assert_eq!(w156.len(), 1, "{reported:?}");
    assert!(w156[0].message.contains(&pinned.key_id()), "{w156:?}");
    assert!(w156[0].message.contains(&other.key_id()), "{w156:?}");
}

#[specforge_test(
    behavior = "pin_publisher_key",
    verify = "a key change is decided by the surface that can ask"
)]
fn a_key_change_is_decided_by_the_asker() {
    let pinned = SigningKey::generate();
    let other = SigningKey::generate();
    let name = PackageName::parse(NAME).unwrap();
    let version = Version::parse(VERSION).unwrap();

    let project = Project::on(Reply::signed(&other));
    project.pin(&pinned);
    let scripted = Scripted::answering(true);
    project
        .registry()
        .fetch(
            &name,
            &version,
            TrustPolicy::new(false, Asking::Ask(&scripted)),
        )
        .expect("the asker said yes");

    assert_eq!(
        scripted.asked(),
        vec![Asked::KeyChange {
            package: NAME.to_string(),
            expected: pinned.key_id(),
            offered: other.key_id(),
            from: ExpectedFrom::UserPin,
        }]
    );
}
