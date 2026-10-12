//! A package registry in memory beside the `Registry` port, and the contract every adapter of the port
//! keeps (ADR 0044), as `specforge_installed::testing` and `specforge_wasm::testing` sit beside theirs.

use std::collections::BTreeMap;
use std::sync::Mutex;

use specforge_common::codes;
use specforge_installed::hex_sha256;
use specforge_package::{PackageName, Version, VersionRequirement};
use specforge_protocol_types::{DeclaredCategory, ExtensionDeclaration};

use super::{
    Found, KeyPin, Package, Published as Receipt, Publisher, Registry, Searched, TrustPolicy,
    Upload,
};
use crate::OpError;
use crate::extension::Trust;

/// One version a [`MemoryRegistry`] publishes. Its name and version are its declaration's, so it can never
/// describe another package (a reply the configured registry refuses before ops sees it).
#[derive(Debug, Clone)]
pub struct Published {
    pub wasm: Vec<u8>,
    /// Its manifest: the declaration published with it (ADR 0012). A test that wants a binary declaring
    /// otherwise publishes that binary with this declaration.
    pub declaration: ExtensionDeclaration,
    /// The publisher key id it is signed with; `None` when unsigned.
    pub key_id: Option<String>,
}

impl Published {
    /// `wasm`, published unsigned with `declaration` as its manifest.
    ///
    /// # Panics
    /// When the declaration's name is no package name or its version no SemVer version: no registry
    /// publishes that (ADR 0036).
    pub fn new(wasm: impl Into<Vec<u8>>, declaration: ExtensionDeclaration) -> Self {
        let published = Published {
            wasm: wasm.into(),
            declaration,
            key_id: None,
        };
        published
            .try_name()
            .expect("a published package has a package name");
        published
            .try_version()
            .expect("a published package has a SemVer version");
        published
    }

    /// Signed with the publisher key `key_id`.
    pub fn signed_by(mut self, key_id: &str) -> Self {
        self.key_id = Some(key_id.to_string());
        self
    }

    pub fn name(&self) -> PackageName {
        self.try_name().expect("checked by Published::new")
    }

    pub fn version(&self) -> Version {
        self.try_version().expect("checked by Published::new")
    }

    fn try_name(&self) -> Result<PackageName, String> {
        PackageName::declared_by(&self.declaration).map_err(|e| e.to_string())
    }

    fn try_version(&self) -> Result<Version, String> {
        Version::parse(self.declaration.version()).map_err(|e| e.to_string())
    }
}

/// The declaration of `name@version` that declares nothing but `peers` (`(name, range)`, required): what a
/// test publishes with bytes that are no extension.
pub fn declaration(name: &str, version: &str, peers: &[(&str, &str)]) -> ExtensionDeclaration {
    let peers: Vec<serde_json::Value> = peers
        .iter()
        .map(|(name, range)| serde_json::json!({"name": name, "version": range, "optional": false}))
        .collect();
    serde_json::from_value(serde_json::json!({
        "handshake": {
            "protocol_version": "1.0.0",
            "name": name,
            "version": version,
            "peer_dependencies": peers,
            "sandbox_policy": null,
        }
    }))
    .expect("a declaration of just a name, a version and peers")
}

/// What a [`MemoryRegistry`] was asked, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    Versions(PackageName),
    Fetch(PackageName, Version),
}

/// A package registry held in memory: the second adapter of [`Registry`], beside
/// `specforge_ops_registry::ConfiguredRegistry`. It serves what it publishes as the configured registry
/// serves a package that passed the fetch policy, refuses as it refuses (both are held to
/// [`assert_registry_contract`]), and records what it was asked and what it accepted. It verifies no
/// signature; it decides a publisher against its own pins and the policy's locked key as the configured
/// registry does, and records pins only through [`Registry::pin`]. A publish is signed by the key id
/// `"in-memory"`.
#[derive(Debug, Default)]
pub struct MemoryRegistry {
    held: Mutex<Vec<Published>>,
    listing_none: Vec<PackageName>,
    asked: Mutex<Vec<Asked>>,
    accepted: Mutex<Vec<(PackageName, Version)>>,
    /// The user's pins: what `pinning` set up and `pin` recorded.
    pins: Mutex<BTreeMap<PackageName, String>>,
    /// `pin` refuses with R-TRUST-006 and records nothing.
    refusing_pins: bool,
}

/// The publisher key id a [`MemoryRegistry`] signs what it accepts with.
pub const IN_MEMORY_KEY: &str = "in-memory";

impl MemoryRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Serve `published` too: what a test sets up. [`Registry::publish`] is the port's call.
    pub fn serving(mut self, published: Published) -> Self {
        self.held.get_mut().unwrap().push(published);
        self
    }

    /// Everything asked so far, oldest first.
    pub fn asked(&self) -> Vec<Asked> {
        self.asked.lock().unwrap().clone()
    }

    /// The publishes it accepted through [`Registry::publish`], in order.
    pub fn published(&self) -> Vec<(PackageName, Version)> {
        self.accepted.lock().unwrap().clone()
    }

    /// The packages whose versions were listed, in order.
    pub fn listed(&self) -> Vec<PackageName> {
        self.asked()
            .into_iter()
            .filter_map(|asked| match asked {
                Asked::Versions(name) => Some(name),
                Asked::Fetch(..) => None,
            })
            .collect()
    }

    /// `name` already pinned to `key_id` for the user.
    pub fn pinning(self, name: &str, key_id: &str) -> Self {
        self.pins.lock().unwrap().insert(
            PackageName::parse(name).expect("a package name"),
            key_id.to_string(),
        );
        self
    }

    /// What is pinned now, by name.
    pub fn pins(&self) -> BTreeMap<PackageName, String> {
        self.pins.lock().unwrap().clone()
    }

    /// Every `pin` refused with R-TRUST-006, recording nothing: a known-keys store that can't be saved.
    pub fn refusing_pins(mut self) -> Self {
        self.refusing_pins = true;
        self
    }

    /// The publisher of a package signed by `key_id`: held to the user's pin for `name`, else to the
    /// key the project's lock records, else accepted on first use. A key change is consented to
    /// under `Trust::AssumeYes` only.
    fn decide(
        &self,
        name: &PackageName,
        key_id: &str,
        policy: TrustPolicy<'_>,
    ) -> Result<Publisher, OpError> {
        let signed = |first_use| Publisher::Signed {
            key_id: key_id.to_string(),
            first_use,
        };
        let users = self.pins.lock().unwrap().get(name).cloned();
        let (expected, from_lock) = match (users, policy.locked_key) {
            (Some(pinned), _) => (pinned, false),
            (None, Some(locked)) => (locked.to_string(), true),
            (None, None) => return Ok(signed(true)),
        };
        if expected == key_id {
            return Ok(signed(from_lock));
        }
        if policy.trust != Trust::AssumeYes {
            let what = if from_lock {
                format!("specforge.lock records '{expected}'")
            } else {
                format!("pinned '{expected}'")
            };
            let locked_too = !from_lock && policy.locked_key == Some(key_id);
            let also = if locked_too {
                " (the key specforge.lock records)"
            } else {
                ""
            };
            return Err(OpError::diagnostic(
                codes::R_TRUST_003,
                format!(
                    "key change rejected for '{name}': {what} but package is signed '{key_id}'{also}"
                ),
            )
            .with_suggestion(format!(
                "if you trust the new key, re-run with --yes (or add '{key_id}' to trusted_keys)"
            )));
        }
        Ok(Publisher::Repinned {
            key_id: key_id.to_string(),
            previous: expected,
        })
    }

    /// A package this registry knows but lists no version of: `versions` answers `Ok(vec![])`, as a
    /// registry may (the wire allows an empty list; ours answers 404). Not part of the contract.
    pub fn listing_none(mut self, name: &str) -> Self {
        self.listing_none
            .push(PackageName::parse(name).expect("a package name"));
        self
    }
}

impl Registry for MemoryRegistry {
    /// Its published versions of `name`, in publish order; R-RES-001 when none (as production).
    fn versions(&self, name: &PackageName) -> Result<Vec<Version>, OpError> {
        self.asked
            .lock()
            .unwrap()
            .push(Asked::Versions(name.clone()));
        let versions: Vec<Version> = self
            .held
            .lock()
            .unwrap()
            .iter()
            .filter(|p| p.name() == *name)
            .map(Published::version)
            .collect();
        if versions.is_empty() && !self.listing_none.contains(name) {
            return Err(OpError::diagnostic(
                codes::R_RES_001,
                format!("package '{name}' not found in the registry"),
            )
            .with_suggestion("check the package name and registry configuration"));
        }
        Ok(versions)
    }

    /// The package published as `name@version`: R006 "Package not found: {name}@{version}" when there is none,
    /// R-TRUST-001 "package '{name}' is not signed" (suggestion "re-run with --allow-unsigned to accept the
    /// risk") when unsigned and `!allow_unsigned`; else its bytes, `hex_sha256(bytes)`, its declaration and
    /// publisher. A signed package's publisher is decided against the key expected for `name` (the
    /// user's pin, else `policy.locked_key`): first use without one, `first_use: false` when it
    /// matches the user's pin, a key change otherwise (`Repinned` under `Trust::AssumeYes`, else
    /// R-TRUST-003). Nothing is pinned: that is [`Registry::pin`].
    fn fetch(
        &self,
        name: &PackageName,
        version: &Version,
        policy: TrustPolicy<'_>,
    ) -> Result<Package, OpError> {
        self.asked
            .lock()
            .unwrap()
            .push(Asked::Fetch(name.clone(), version.clone()));
        let held = self.held.lock().unwrap();
        let Some(published) = held
            .iter()
            .find(|p| p.name() == *name && p.version() == *version)
        else {
            return Err(OpError::diagnostic(
                codes::R006,
                format!("Package not found: {name}@{version}"),
            )
            .with_suggestion("Verify the package name and version."));
        };
        if published.key_id.is_none() && !policy.allow_unsigned {
            return Err(OpError::diagnostic(
                codes::R_TRUST_001,
                format!("package '{name}' is not signed"),
            )
            .with_suggestion("re-run with --allow-unsigned to accept the risk"));
        }
        let publisher = match &published.key_id {
            None => Publisher::Unsigned,
            Some(key_id) => self.decide(name, key_id, policy)?,
        };
        Ok(Package {
            name: name.clone(),
            version: version.clone(),
            sha256: hex_sha256(&published.wasm),
            wasm: published.wasm.clone(),
            declaration: published.declaration.clone(),
            publisher,
        })
    }

    /// Record each pin for the user, all or none; R-TRUST-006 (and nothing recorded) when told to
    /// refuse them.
    fn pin(&self, pins: &[KeyPin]) -> Result<(), OpError> {
        if self.refusing_pins {
            return Err(OpError::diagnostic(
                codes::R_TRUST_006,
                "the known-keys store can't be saved",
            )
            .with_suggestion("check permissions on the file"));
        }
        let mut held = self.pins.lock().unwrap();
        for pin in pins {
            held.insert(pin.name.clone(), pin.key_id.clone());
        }
        Ok(())
    }

    /// Hold `package` as a version signed by [`IN_MEMORY_KEY`]. A version already held is R007
    /// "Version {version} already exists for package {name}." (suggestion "Bump the version number
    /// before publishing."). The receipt names registry and key `"in-memory"` and the url
    /// `memory://{name}/{version}`; no key is created.
    fn publish(&self, package: &Upload<'_>) -> Result<Receipt, OpError> {
        let mut held = self.held.lock().unwrap();
        if held
            .iter()
            .any(|p| p.name() == *package.name && p.version() == *package.version)
        {
            return Err(OpError::diagnostic(
                codes::R007,
                format!(
                    "Version {} already exists for package {}.",
                    package.version, package.name
                ),
            )
            .with_suggestion("Bump the version number before publishing."));
        }
        held.push(
            Published::new(package.wasm, package.declaration.clone()).signed_by(IN_MEMORY_KEY),
        );
        self.accepted
            .lock()
            .unwrap()
            .push((package.name.clone(), package.version.clone()));
        Ok(Receipt {
            registry: IN_MEMORY_KEY.to_string(),
            url: format!("memory://{}/{}", package.name, package.version),
            key_id: IN_MEMORY_KEY.to_string(),
            key_created: false,
        })
    }

    /// Each held package one of whose versions matches `query` in its name, description or keywords,
    /// at the version `latest` picks, kept when that version declares `contributes`, sorted by name;
    /// every hit names the registry [`IN_MEMORY_KEY`].
    fn search(
        &self,
        query: &str,
        contributes: Option<DeclaredCategory>,
    ) -> Result<Searched, OpError> {
        let matches_term = |published: &Published| {
            let handshake = &published.declaration.handshake;
            let keywords: Vec<&str> = handshake.keywords.iter().map(String::as_str).collect();
            specforge_package::search::matches(
                query,
                published.name().as_str(),
                handshake.description.as_deref().unwrap_or_default(),
                &keywords,
            )
        };
        let held = self.held.lock().unwrap();
        let mut by_name: BTreeMap<PackageName, Vec<&Published>> = BTreeMap::new();
        for published in held.iter() {
            by_name.entry(published.name()).or_default().push(published);
        }
        let listed: Vec<&Published> = by_name
            .into_values()
            .filter(|versions| versions.iter().any(|p| matches_term(p)))
            .filter_map(|versions| {
                VersionRequirement::Latest.pick_from(versions, |p| Some(p.version()))
            })
            .filter(|p| contributes.is_none_or(|c| p.declaration.declares(c)))
            .collect();
        Ok(Searched {
            found: listed
                .into_iter()
                .map(|p| Found {
                    name: p.name().to_string(),
                    version: p.version().to_string(),
                    description: p
                        .declaration
                        .handshake
                        .description
                        .clone()
                        .unwrap_or_default(),
                    registry: IN_MEMORY_KEY.to_string(),
                })
                .collect(),
            failures: Vec::new(),
            asked: 1,
        })
    }
}

/// The packages the contract runs over, the signed ones signed by `key_id`:
/// `@contract/base` 1.0.0, 1.1.0 and 2.0.0-beta.1 (signed); `@contract/app` 1.0.0 (signed, a required peer
/// `@contract/base ^1.0`); `@contract/plain` 1.0.0 (unsigned). Each binary is `"\0asm {name} {version}"`.
pub fn contract_packages(key_id: &str) -> Vec<Published> {
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
    ]
}

/// The policy the contract fetches under: nobody consents to a key change, no key is locked.
fn refuse(allow_unsigned: bool) -> TrustPolicy<'static> {
    TrustPolicy::new(allow_unsigned, Trust::Refuse)
}

/// The `Registry` contract (ADR 0044): what every adapter of the port does with `published` (built by
/// [`contract_packages`]) published and nothing pinned. Panics naming the clause that failed: C1–C7
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

    // C7
    let fresh = PackageName::parse("@contract/fresh").unwrap();
    let fresh_version = Version::parse("1.0.0").unwrap();
    let wasm = b"\0asm @contract/fresh 1.0.0";
    let declaration = declaration("@contract/fresh", "1.0.0", &[]);
    let upload = Upload {
        name: &fresh,
        version: &fresh_version,
        wasm,
        declaration: &declaration,
    };
    let receipt = registry
        .publish(&upload)
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
    let again = registry.publish(&upload).unwrap_err();
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
            TrustPolicy::new(false, Trust::AssumeYes),
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

    // C10: the lock's key is the pin when the user has none
    let error = registry
        .fetch(
            &fresh,
            &fresh_version,
            refuse(false).locked(Some("contract-other")),
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
            refuse(false).locked(Some(receipt.key_id.as_str())),
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(registry.published(), [(name, version)]);
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
        let mut with_entities =
            serde_json::to_value(declaration("@acme/cat", "1.0.0", &[])).unwrap();
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
            .fetch(&name, &version, TrustPolicy::new(false, Trust::AssumeYes))
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
        assert_eq!(registry.published().len(), 1);
    }

    #[test]
    fn a_package_it_knows_but_lists_nothing_of_is_an_empty_list() {
        let registry = MemoryRegistry::new().listing_none("@acme/tool");
        let name = PackageName::parse("@acme/tool").unwrap();
        assert_eq!(registry.versions(&name).unwrap(), []);
    }
}
