use super::super::{KeyPin, Publisher, Registry, TrustPolicy, Upload};
use super::contract::refuse;
use super::*;
use crate::asking::Asking;
use specforge_common::codes;
use specforge_installed::hex_sha256;
use specforge_package::{PackageName, Version};
use specforge_protocol_types::DeclaredCategory;

#[specforge_test_macros::test(port = "Registry", verify = "Registry contract is satisfied")]
fn the_memory_registry_keeps_the_registry_contract() {
    let published = contract_packages("memory-key");
    let registry = published
        .iter()
        .cloned()
        .fold(MemoryRegistry::new(), MemoryRegistry::serving);
    assert_registry_contract(&registry, &published);
}

#[specforge_test_macros::test(
    type = "RegistryManifest",
    verify = "RegistryManifest schema is valid"
)]
fn the_registry_manifest_is_what_a_dry_run_judges() {
    let published = Published::new(
        b"\0asm tool".to_vec(),
        declaration("@acme/tool", "1.0.0", &[("@acme/base", "^1.0")]),
    );
    let registry = MemoryRegistry::new().serving(published.clone());

    let manifest = registry
        .published(&published.name(), &published.version())
        .unwrap();

    assert_eq!(manifest.name, published.name());
    assert_eq!(manifest.version, published.version());
    assert_eq!(manifest.sha256, hex_sha256(b"\0asm tool"));
    assert_eq!(manifest.declaration, published.declaration);
    assert_eq!(
        registry.asked(),
        [Asked::Published(published.name(), published.version())],
        "the manifest is asked for, the binary is not"
    );
}

#[specforge_test_macros::test(
    type = "RegistryPackage",
    verify = "RegistryPackage is what a package that passed the fetch policy hands an operation"
)]
fn a_fetched_package_carries_its_name_digest_declaration_and_signer() {
    let published = Published::new(
        b"\0asm tool".to_vec(),
        declaration("@acme/tool", "1.0.0", &[]),
    )
    .signed_by("key-1");
    let registry = MemoryRegistry::new().serving(published.clone());
    let package = registry
        .fetch(&published.name(), &published.version(), refuse(false))
        .unwrap();
    assert_eq!(package.name, published.name());
    assert_eq!(package.version, published.version());
    assert_eq!(package.sha256, hex_sha256(b"\0asm tool"));
    assert_eq!(package.declaration, published.declaration);
    assert_eq!(package.publisher.key_id(), Some("key-1"));
}

#[test]
fn it_records_what_it_was_asked() {
    let registry = MemoryRegistry::new().serving(Published::new(
        b"\0asm".to_vec(),
        declaration("@acme/tool", "1.0.0", &[]),
    ));
    let name = PackageName::parse("@acme/tool").unwrap();
    registry.versions(&name).unwrap();
    let _ = registry.fetch(&name, &Version::new(2, 0, 0), refuse(true));
    assert_eq!(
        registry.asked(),
        [
            Asked::Versions(name.clone()),
            Asked::Fetch(name.clone(), Version::new(2, 0, 0))
        ]
    );
    assert_eq!(registry.listed(), [name]);
}

#[specforge_test_macros::test(
    type = "RegistryPublished",
    verify = "RegistryPublished is what the registry that took a publish answers with"
)]
fn a_published_version_is_listed_served_and_recorded() {
    let registry = MemoryRegistry::new();
    let name = PackageName::parse("@acme/tool").unwrap();
    let version = Version::new(1, 0, 0);
    let declaration = declaration("@acme/tool", "1.0.0", &[]);
    let receipt = registry
        .publish(&Upload {
            name: &name,
            version: &version,
            wasm: b"\0asm",
            declaration: &declaration,
        })
        .unwrap();
    assert_eq!(receipt.registry, IN_MEMORY_KEY);
    assert_eq!(receipt.url, "memory://@acme/tool/1.0.0");
    assert!(!receipt.key_created);
    assert_eq!(registry.versions(&name).unwrap(), vec![version.clone()]);
    let served = registry.fetch(&name, &version, refuse(false)).unwrap();
    assert_eq!(served.publisher.key_id(), Some(IN_MEMORY_KEY));
    assert_eq!(registry.accepted_publishes(), [(name, version)]);
}

#[specforge_test_macros::test(
    type = "RegistrySearched",
    verify = "RegistrySearched is what a search over the configured registries answers with"
)]
fn a_search_answers_what_was_found_what_failed_and_how_many_were_asked() {
    let registry = MemoryRegistry::new().serving(Published::new(
        b"\0asm".to_vec(),
        declaration("@acme/tool", "1.0.0", &[]),
    ));
    let searched = registry.search("tool", None).unwrap();
    assert_eq!(searched.found.len(), 1);
    assert_eq!(searched.found[0].registry, IN_MEMORY_KEY);
    assert!(searched.failures.is_empty());
    assert_eq!(searched.asked, 1);
    assert!(!searched.failed());
}

#[specforge_test_macros::test(
    behavior = "search_registry",
    verify = "a registry lists each package at the version add installs from it"
)]
fn a_search_lists_the_version_add_installs() {
    let registry = MemoryRegistry::new()
        .serving(Published::new(
            b"\0asm".to_vec(),
            declaration("@acme/tool", "1.4.0", &[]),
        ))
        .serving(Published::new(
            b"\0asm".to_vec(),
            declaration("@acme/tool", "2.0.0-beta.1", &[]),
        ));
    let searched = registry.search("tool", None).unwrap();
    let found: Vec<(&str, &str)> = searched
        .found
        .iter()
        .map(|f| (f.name.as_str(), f.version.as_str()))
        .collect();
    assert_eq!(found, [("@acme/tool", "1.4.0")]);

    // The category is judged on the listed version: 1.0.0 declares entities, the listed
    // 2.0.0 does not.
    let mut with_entities = serde_json::to_value(declaration("@acme/cat", "1.0.0", &[])).unwrap();
    with_entities["entities"] = serde_json::json!([{"name": "thing"}]);
    let registry = MemoryRegistry::new()
        .serving(Published::new(
            b"\0asm".to_vec(),
            serde_json::from_value(with_entities).unwrap(),
        ))
        .serving(Published::new(
            b"\0asm".to_vec(),
            declaration("@acme/cat", "2.0.0", &[]),
        ));
    assert!(
        registry
            .search("cat", Some(DeclaredCategory::Entities))
            .unwrap()
            .found
            .is_empty()
    );
    // Under no category it is listed at 2.0.0.
    let searched = registry.search("cat", None).unwrap();
    assert_eq!(searched.found[0].version, "2.0.0");
}

#[test]
fn it_decides_a_key_change_by_the_policy_and_pins_only_through_pin() {
    let published = Published::new(
        b"\0asm tool".to_vec(),
        declaration("@acme/tool", "1.0.0", &[]),
    )
    .signed_by("k2");
    let registry = MemoryRegistry::new()
        .serving(published.clone())
        .pinning("@acme/tool", "k1");
    let (name, version) = (published.name(), published.version());

    let refused = registry.fetch(&name, &version, refuse(false)).unwrap_err();
    assert!(refused.is(codes::R_TRUST_003), "{refused:?}");
    let consented = registry
        .fetch(&name, &version, TrustPolicy::new(false, Asking::Yes))
        .unwrap();
    assert_eq!(
        consented.publisher,
        Publisher::Repinned {
            key_id: "k2".into(),
            previous: "k1".into()
        }
    );
    assert_eq!(registry.pins()[&name], "k1", "a fetch pins nothing");

    registry
        .pin(&[consented.publisher.pin(&name).unwrap()])
        .unwrap();
    assert_eq!(registry.pins()[&name], "k2");
    assert_eq!(
        registry
            .fetch(&name, &version, refuse(false))
            .unwrap()
            .publisher,
        Publisher::Signed {
            key_id: "k2".into(),
            first_use: false
        }
    );
}

#[test]
fn it_refuses_every_pin_when_told_to() {
    let registry = MemoryRegistry::new().refusing_pins();
    let name = PackageName::parse("@acme/tool").unwrap();

    let error = registry
        .pin(&[KeyPin {
            name,
            key_id: "k1".into(),
            replaces: None,
        }])
        .unwrap_err();

    assert!(error.is(codes::R_TRUST_006), "{error:?}");
    assert!(registry.pins().is_empty());
}

#[test]
fn publishing_twice_is_r007() {
    let registry = MemoryRegistry::new();
    let name = PackageName::parse("@acme/tool").unwrap();
    let version = Version::new(1, 0, 0);
    let declaration = declaration("@acme/tool", "1.0.0", &[]);
    let upload = Upload {
        name: &name,
        version: &version,
        wasm: b"\0asm",
        declaration: &declaration,
    };
    registry.publish(&upload).unwrap();
    let error = registry.publish(&upload).unwrap_err();
    assert!(error.is(super::super::ALREADY_PUBLISHED), "{error:?}");
    assert_eq!(error.kind, crate::OpErrorKind::Conflict);
    assert_eq!(registry.accepted_publishes().len(), 1);
}

#[test]
fn a_package_it_knows_but_lists_nothing_of_is_an_empty_list() {
    let registry = MemoryRegistry::new().listing_none("@acme/tool");
    let name = PackageName::parse("@acme/tool").unwrap();
    assert_eq!(registry.versions(&name).unwrap(), []);
}
