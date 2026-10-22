//! The contract every adapter of the `Registry` port keeps (ADR 0044).

use specforge_common::codes;
use specforge_installed::hex_sha256;
use specforge_package::{DeclaredPackage, PackageName, Version};
use specforge_protocol_types::DeclaredCategory;

use super::super::{KeyPin, LockedPin, Publisher, Registry, TrustPolicy};
use super::{Published, declaration};
use crate::asking::testing::{Asked, Scripted};
use crate::asking::{Asking, ExpectedFrom};

/// The packages the contract runs over, the signed ones signed by `key_id`:
/// `@contract/base` 1.0.0, 1.1.0 and 2.0.0-beta.1 (signed); `@contract/app` 1.0.0 (signed, a required peer
/// `@contract/base ^1.0`); `@contract/plain` 1.0.0 (unsigned); `@contract/denied` 1.0.0 (signed by
/// `denied_key_id`, which the adapter under test must be set up to find on the user's deny list). Each binary
/// is `"\0asm {name} {version}"`.
pub fn contract_packages(key_id: &str, denied_key_id: &str) -> Vec<Published> {
    let package = |name: &str, version: &str, peers: &[(&str, &str)]| {
        Published::new(
            format!("\0asm {name} {version}").into_bytes(),
            declaration(name, version, peers),
        )
    };
    vec![
        package("@contract/base", "1.0.0", &[]).signed_by(key_id),
        package("@contract/base", "1.1.0", &[]).signed_by(key_id),
        package("@contract/base", "2.0.0-beta.1", &[]).signed_by(key_id),
        package("@contract/app", "1.0.0", &[("@contract/base", "^1.0")]).signed_by(key_id),
        package("@contract/plain", "1.0.0", &[]),
        package("@contract/denied", "1.0.0", &[]).signed_by(denied_key_id),
    ]
}

/// The policy the contract fetches under: nobody consents to a key change, no key is locked.
pub(super) fn refuse(allow_unsigned: bool) -> TrustPolicy<'static> {
    TrustPolicy::new(allow_unsigned, Asking::Nobody)
}

/// The `Registry` contract (ADR 0044): what every adapter of the port does with `published` (built by
/// [`contract_packages`]) published, nothing pinned and the signer of `@contract/denied` on the user's deny list. Panics naming the clause that failed: C0 opens, C1–C7
/// list, fetch and publish; R-S finds a package by its name and category; R-S2 lists a package once,
/// at the version `specforge add <name>` installs from it.
pub fn assert_registry_contract(registry: &dyn Registry, published: &[Published]) {
    let find = |name: &str, version: &str| {
        published
            .iter()
            .find(|p| p.name().as_str() == name && p.version().to_string() == version)
            .unwrap_or_else(|| panic!("the contract packages hold {name}@{version}"))
    };
    let base = PackageName::parse("@contract/base").unwrap();

    // C0
    registry
        .open()
        .expect("C0: a registry that serves the contract packages opens");
    registry.open().expect("C0: opening is idempotent");

    // C1
    let mut versions = registry.versions(&base).expect("C1: versions of base");
    versions.sort();
    let want: Vec<Version> = ["1.0.0", "1.1.0", "2.0.0-beta.1"]
        .iter()
        .map(|v| Version::parse(v).unwrap())
        .collect();
    assert_eq!(versions, want, "C1: base's versions");

    // C2
    let error = registry
        .versions(&PackageName::parse("@contract/missing").unwrap())
        .unwrap_err();
    assert!(
        error.is(codes::R_RES_001),
        "C2: an unknown package: {error:?}"
    );

    // C3
    let one_one = Version::parse("1.1.0").unwrap();
    let expected = find("@contract/base", "1.1.0");
    let fetched = registry
        .fetch(&base, &one_one, refuse(false))
        .unwrap_or_else(|e| panic!("C3: fetch of base 1.1.0 fails: {e:?}"));
    assert_eq!(fetched.name, base, "C3: name");
    assert_eq!(fetched.version, one_one, "C3: version");
    assert_eq!(fetched.wasm, expected.wasm, "C3: the published bytes");
    assert_eq!(fetched.sha256, hex_sha256(&fetched.wasm), "C3: sha256");
    assert_eq!(fetched.declaration, expected.declaration, "C3: declaration");
    let signer = expected.key_id.clone().expect("base is signed");
    let first_use = Publisher::Signed {
        key_id: signer.clone(),
        first_use: true,
    };
    assert_eq!(fetched.publisher, first_use, "C3: the signer, on first use");
    let again = registry
        .fetch(&base, &one_one, refuse(false))
        .unwrap_or_else(|e| panic!("C3: a second fetch under the same key fails: {e:?}"));
    assert_eq!(
        again.publisher, first_use,
        "C3: a fetch pins nothing, so the same signer is a first use again"
    );

    // C4
    let error = registry
        .fetch(&base, &Version::parse("9.9.9").unwrap(), refuse(false))
        .unwrap_err();
    assert!(error.is(codes::R006), "C4: an unknown version: {error:?}");
    assert!(
        error.message.contains("@contract/base@9.9.9"),
        "C4: the message names the package: {error:?}"
    );

    // C5
    let app = registry
        .fetch(
            &PackageName::parse("@contract/app").unwrap(),
            &Version::parse("1.0.0").unwrap(),
            refuse(false),
        )
        .unwrap_or_else(|e| panic!("C5: fetch of app fails: {e:?}"));
    let peers = app.declaration.peers();
    assert_eq!(peers.len(), 1, "C5: app's peers: {peers:?}");
    assert_eq!(peers[0].name, "@contract/base", "C5: the peer");
    assert_eq!(peers[0].version, "^1.0", "C5: the peer's range");

    // C6
    let plain = PackageName::parse("@contract/plain").unwrap();
    let one = Version::parse("1.0.0").unwrap();
    let error = registry.fetch(&plain, &one, refuse(false)).unwrap_err();
    assert!(
        error.is(codes::R_TRUST_001),
        "C6: an unsigned package: {error:?}"
    );
    let allowed = registry
        .fetch(&plain, &one, refuse(true))
        .unwrap_or_else(|e| panic!("C6: unsigned allowed fails: {e:?}"));
    assert_eq!(
        allowed.publisher,
        Publisher::Unsigned,
        "C6: an unsigned package has no key"
    );

    // C14: a key on the user's deny list is refused, whatever the consent and --allow-unsigned
    let denied = PackageName::parse("@contract/denied").unwrap();
    let error = registry
        .fetch(&denied, &one, TrustPolicy::new(true, Asking::Yes))
        .unwrap_err();
    assert!(
        error.is(codes::R_TRUST_005),
        "C14: a denied key beats consent: {error:?}"
    );

    // C7
    let fresh = PackageName::parse("@contract/fresh").unwrap();
    let fresh_version = Version::parse("1.0.0").unwrap();
    let wasm = b"\0asm @contract/fresh 1.0.0";
    let declaration = declaration("@contract/fresh", "1.0.0", &[]);
    let package = DeclaredPackage::of(&declaration).unwrap();
    let receipt = registry
        .publish(&package, wasm)
        .unwrap_or_else(|e| panic!("C7: publish fails: {e:?}"));
    assert!(!receipt.url.is_empty(), "C7: the receipt names a url");
    assert!(!receipt.key_id.is_empty(), "C7: the receipt names the key");
    let listed = registry
        .versions(&fresh)
        .unwrap_or_else(|e| panic!("C7: versions of a published package: {e:?}"));
    assert_eq!(listed, vec![fresh_version.clone()], "C7: it is listed");
    let served = registry
        .fetch(&fresh, &fresh_version, refuse(false))
        .unwrap_or_else(|e| panic!("C7: fetch of a published package: {e:?}"));
    assert_eq!(served.wasm, wasm, "C7: its bytes are served");
    assert_eq!(served.declaration, declaration, "C7: its declaration");
    assert_eq!(
        served.publisher.key_id(),
        Some(receipt.key_id.as_str()),
        "C7: signed by the key the receipt names"
    );
    let again = registry.publish(&package, wasm).unwrap_err();
    assert!(again.is(codes::R007), "C7: a held version: {again:?}");
    assert_eq!(
        again.kind,
        crate::OpErrorKind::Conflict,
        "C7: a held version is a conflict"
    );

    // C8: a pin is recorded
    registry
        .pin(&[KeyPin {
            name: base.clone(),
            key_id: signer.clone(),
            replaces: None,
        }])
        .unwrap_or_else(|e| panic!("C8: pin fails: {e:?}"));
    let pinned = registry
        .fetch(&base, &one_one, refuse(false))
        .unwrap_or_else(|e| panic!("C8: fetch after a pin fails: {e:?}"));
    assert_eq!(
        pinned.publisher,
        Publisher::Signed {
            key_id: signer.clone(),
            first_use: false
        },
        "C8: the pinned signer is accepted as pinned"
    );

    // C9: a key change is decided, not written
    let app_name = PackageName::parse("@contract/app").unwrap();
    let app_version = Version::parse("1.0.0").unwrap();
    let app_signer = find("@contract/app", "1.0.0")
        .key_id
        .clone()
        .expect("app is signed");
    registry
        .pin(&[KeyPin {
            name: app_name.clone(),
            key_id: "contract-other".to_string(),
            replaces: None,
        }])
        .unwrap_or_else(|e| panic!("C9: pin fails: {e:?}"));
    let error = registry
        .fetch(&app_name, &app_version, refuse(false))
        .unwrap_err();
    assert!(
        error.is(codes::R_TRUST_003),
        "C9: another key than the pinned one: {error:?}"
    );
    assert!(
        error.message.contains("contract-other") && error.message.contains(&app_signer),
        "C9: the message names both keys: {error:?}"
    );
    let consented = registry
        .fetch(
            &app_name,
            &app_version,
            TrustPolicy::new(false, Asking::Yes),
        )
        .unwrap_or_else(|e| panic!("C9: a consented key change fails: {e:?}"));
    assert_eq!(
        consented.publisher,
        Publisher::Repinned {
            key_id: app_signer.clone(),
            previous: "contract-other".to_string()
        },
        "C9: consent replaces the pin"
    );
    let error = registry
        .fetch(&app_name, &app_version, refuse(false))
        .unwrap_err();
    assert!(
        error.is(codes::R_TRUST_003),
        "C9: the consented fetch pinned nothing: {error:?}"
    );

    // C11: a key change is put to whoever can be asked, once, naming both keys
    let declining = Scripted::answering(false);
    let error = registry
        .fetch(
            &app_name,
            &app_version,
            TrustPolicy::new(false, Asking::Ask(&declining)),
        )
        .unwrap_err();
    assert!(
        error.is(codes::R_TRUST_003),
        "C11: an asker that answers no refuses: {error:?}"
    );
    assert_eq!(
        declining.asked(),
        vec![Asked::KeyChange {
            package: "@contract/app".to_string(),
            expected: "contract-other".to_string(),
            offered: app_signer.clone(),
            from: ExpectedFrom::UserPin,
        }],
        "C11: the asker is put the one key change"
    );
    let accepting = Scripted::answering(true);
    let accepted = registry
        .fetch(
            &app_name,
            &app_version,
            TrustPolicy::new(false, Asking::Ask(&accepting)),
        )
        .unwrap_or_else(|e| panic!("C11: an asker that answers yes fails: {e:?}"));
    assert_eq!(
        accepted.publisher,
        Publisher::Repinned {
            key_id: app_signer.clone(),
            previous: "contract-other".to_string()
        },
        "C11: an asker that answers yes re-pins"
    );

    // C10: the lock's key is the pin when the user has none
    let error = registry
        .fetch(
            &fresh,
            &fresh_version,
            refuse(false).locked(Some(LockedPin::key("contract-other"))),
        )
        .unwrap_err();
    assert!(
        error.is(codes::R_TRUST_003),
        "C10: another key than the lock's: {error:?}"
    );
    assert!(
        error.message.contains("specforge.lock"),
        "C10: the message names the lock: {error:?}"
    );
    let locked = registry
        .fetch(
            &fresh,
            &fresh_version,
            refuse(false).locked(Some(LockedPin::key(&receipt.key_id))),
        )
        .unwrap_or_else(|e| panic!("C10: the locked key is accepted: {e:?}"));
    assert_eq!(
        locked.publisher,
        Publisher::Signed {
            key_id: receipt.key_id.clone(),
            first_use: true
        },
        "C10: the lock's key is accepted, and still pinned when the install is written"
    );

    // C12: the lock's bytes hold the fetch, before the signature and before any question
    let pin = |version, sha256| LockedPin {
        version,
        sha256,
        key_id: None,
    };
    let error = registry
        .fetch(
            &plain,
            &one,
            refuse(false).locked(Some(pin("1.0.0", "other"))),
        )
        .unwrap_err();
    assert!(
        error.is(codes::R014),
        "C12: other bytes at the locked version, before the signature: {error:?}"
    );
    let asker = Scripted::answering(true);
    let error = registry
        .fetch(
            &app_name,
            &app_version,
            TrustPolicy::new(false, Asking::Ask(&asker)).locked(Some(pin("1.0.0", "other"))),
        )
        .unwrap_err();
    assert!(
        error.is(codes::R014),
        "C12: other bytes at the locked version, before the key question: {error:?}"
    );
    assert!(asker.asked().is_empty(), "C12: nobody was asked");
    let plain_bytes = hex_sha256(&find("@contract/plain", "1.0.0").wasm);
    for (version, sha256, why) in [
        ("1.0.0", plain_bytes.as_str(), "its bytes at its version"),
        ("0.9.0", "other", "other bytes at another version"),
        ("1.0.0", "", "an entry that pins no hash"),
    ] {
        registry
            .fetch(
                &plain,
                &one,
                refuse(true).locked(Some(pin(version, sha256))),
            )
            .unwrap_or_else(|e| panic!("C12: {why} is accepted: {e:?}"));
    }

    // C11: the published manifest is what a fetch answers, without the binary
    let manifest = registry
        .published(&fresh, &fresh_version, refuse(false))
        .unwrap_or_else(|e| panic!("C11: published of a held version fails: {e:?}"));
    assert_eq!(
        (manifest.name.clone(), manifest.version.clone()),
        (fresh.clone(), fresh_version.clone()),
        "C11: the package asked for"
    );
    assert_eq!(manifest.sha256, served.sha256, "C11: sha256 as fetch");
    assert_eq!(
        manifest.declaration, served.declaration,
        "C11: declaration as fetch"
    );
    assert_eq!(
        manifest.publisher, served.publisher,
        "C11: publisher as fetch"
    );
    let error = registry
        .published(&base, &Version::parse("9.9.9").unwrap(), refuse(false))
        .unwrap_err();
    assert!(
        error.is(codes::R006),
        "C11: an unknown version as fetch refuses it: {error:?}"
    );

    // C13: published decides as fetch decides, downloading nothing
    let published_as =
        |name: &PackageName, version: &Version, policy| registry.published(name, version, policy);
    let error = published_as(&plain, &one, refuse(false)).unwrap_err();
    assert!(
        error.is(codes::R_TRUST_001),
        "C13: an unsigned package is refused as fetch refuses it: {error:?}"
    );
    assert_eq!(
        published_as(&plain, &one, refuse(true)).unwrap().publisher,
        Publisher::Unsigned,
        "C13: an unsigned package allowed has no key"
    );
    assert_eq!(
        published_as(&base, &one_one, refuse(false))
            .unwrap()
            .publisher,
        Publisher::Signed {
            key_id: signer.clone(),
            first_use: false
        },
        "C13: the pinned signer, as fetch answered after C8"
    );
    let error = published_as(&app_name, &app_version, refuse(false)).unwrap_err();
    assert!(
        error.is(codes::R_TRUST_003),
        "C13: a key change is refused as fetch refuses it: {error:?}"
    );
    assert_eq!(
        published_as(
            &app_name,
            &app_version,
            TrustPolicy::new(false, Asking::Yes)
        )
        .unwrap()
        .publisher,
        Publisher::Repinned {
            key_id: app_signer.clone(),
            previous: "contract-other".to_string()
        },
        "C13: consent to a key change, as fetch"
    );
    let error = published_as(&denied, &one, TrustPolicy::new(true, Asking::Yes)).unwrap_err();
    assert!(
        error.is(codes::R_TRUST_005),
        "C13: a denied key beats consent: {error:?}"
    );

    // R-S
    let searched = registry
        .search("contract/fresh", None)
        .unwrap_or_else(|e| panic!("R-S: search fails: {e:?}"));
    assert!(!searched.failed(), "R-S: {searched:?}");
    let names: Vec<&str> = searched.found.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        ["@contract/fresh"],
        "R-S: a published package is found by a substring of its name"
    );
    let searched = registry
        .search("contract/fresh", Some(DeclaredCategory::Analyzers))
        .unwrap_or_else(|e| panic!("R-S: filtered search fails: {e:?}"));
    assert!(
        searched.found.is_empty(),
        "R-S: not found under a category it does not declare: {searched:?}"
    );

    // R-S2
    let searched = registry
        .search("contract/base", None)
        .unwrap_or_else(|e| panic!("R-S2: search fails: {e:?}"));
    let listed: Vec<(&str, &str)> = searched
        .found
        .iter()
        .map(|f| (f.name.as_str(), f.version.as_str()))
        .collect();
    assert_eq!(
        listed,
        [("@contract/base", "1.1.0")],
        "R-S2: a package is listed once, at the version `add` installs (the highest release), not 2.0.0-beta.1"
    );
}
