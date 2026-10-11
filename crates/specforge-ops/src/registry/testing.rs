//! A package registry in memory beside the `Registry` port, and the contract every adapter of the port
//! keeps (ADR 0044), as `specforge_installed::testing` and `specforge_wasm::testing` sit beside theirs.

use std::sync::Mutex;

use specforge_common::codes;
use specforge_installed::hex_sha256;
use specforge_package::{PackageName, Version};
use specforge_protocol_types::{DeclaredCategory, ExtensionDeclaration};

use super::{Found, Package, Published as Receipt, Publisher, Registry, Searched, Upload};
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
            "contribution_flags": {},
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
/// signature and pins no key. A publish is signed by the key id `"in-memory"`.
#[derive(Debug, Default)]
pub struct MemoryRegistry {
    held: Mutex<Vec<Published>>,
    listing_none: Vec<PackageName>,
    asked: Mutex<Vec<Asked>>,
    accepted: Mutex<Vec<(PackageName, Version)>>,
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
    /// key id. `trust` is not read: no key is pinned here.
    fn fetch(
        &self,
        name: &PackageName,
        version: &Version,
        allow_unsigned: bool,
        _trust: Trust,
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
        if published.key_id.is_none() && !allow_unsigned {
            return Err(OpError::diagnostic(
                codes::R_TRUST_001,
                format!("package '{name}' is not signed"),
            )
            .with_suggestion("re-run with --allow-unsigned to accept the risk"));
        }
        Ok(Package {
            name: name.clone(),
            version: version.clone(),
            sha256: hex_sha256(&published.wasm),
            wasm: published.wasm.clone(),
            declaration: published.declaration.clone(),
            publisher: match &published.key_id {
                Some(key_id) => Publisher::Signed {
                    key_id: key_id.clone(),
                    first_use: false,
                },
                None => Publisher::Unsigned,
            },
        })
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

    /// The latest version of each held package matching `query` in its name, description or keywords,
    /// declaring `contributes` when given, sorted by name; every hit names the registry
    /// [`IN_MEMORY_KEY`].
    fn search(
        &self,
        query: &str,
        contributes: Option<DeclaredCategory>,
    ) -> Result<Searched, OpError> {
        let needle = query.to_ascii_lowercase();
        let held = self.held.lock().unwrap();
        let mut latest: Vec<&Published> = Vec::new();
        for published in held.iter() {
            let handshake = &published.declaration.handshake;
            let matches = published
                .name()
                .as_str()
                .to_ascii_lowercase()
                .contains(&needle)
                || handshake
                    .description
                    .as_deref()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .contains(&needle)
                || handshake
                    .keywords
                    .iter()
                    .any(|k| k.to_ascii_lowercase().contains(&needle));
            if !matches || contributes.is_some_and(|c| !Self::declares(published, c)) {
                continue;
            }
            match latest.iter().position(|p| p.name() == published.name()) {
                Some(at) if latest[at].version() >= published.version() => {}
                Some(at) => latest[at] = published,
                None => latest.push(published),
            }
        }
        latest.sort_by_key(|p| p.name().to_string());
        Ok(Searched {
            found: latest
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

impl MemoryRegistry {
    /// Whether `published` declares `category`.
    fn declares(published: &Published, category: DeclaredCategory) -> bool {
        published
            .declaration
            .describe_items(category.name())
            .and_then(|items| items.as_array().map(|a| !a.is_empty()))
            .unwrap_or(false)
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

/// The `Registry` contract (ADR 0044): what every adapter of the port does with `published` (built by
/// [`contract_packages`]) published and nothing pinned. Panics naming the clause that failed.
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
        .fetch(&base, &one_one, false, Trust::Refuse)
        .unwrap_or_else(|e| panic!("C3: fetch of base 1.1.0 fails: {e:?}"));
    assert_eq!(fetched.name, base, "C3: name");
    assert_eq!(fetched.version, one_one, "C3: version");
    assert_eq!(fetched.wasm, expected.wasm, "C3: the published bytes");
    assert_eq!(fetched.sha256, hex_sha256(&fetched.wasm), "C3: sha256");
    assert_eq!(fetched.declaration, expected.declaration, "C3: declaration");
    assert_eq!(
        fetched.publisher.key_id(),
        expected.key_id.as_deref(),
        "C3: the signer"
    );
    let again = registry
        .fetch(&base, &one_one, false, Trust::Refuse)
        .unwrap_or_else(|e| panic!("C3: a second fetch under the same key fails: {e:?}"));
    assert_eq!(
        again.publisher.key_id(),
        expected.key_id.as_deref(),
        "C3: the same signer again"
    );

    // C4
    let error = registry
        .fetch(
            &base,
            &Version::parse("9.9.9").unwrap(),
            false,
            Trust::Refuse,
        )
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
            false,
            Trust::Refuse,
        )
        .unwrap_or_else(|e| panic!("C5: fetch of app fails: {e:?}"));
    let peers = app.declaration.peers();
    assert_eq!(peers.len(), 1, "C5: app's peers: {peers:?}");
    assert_eq!(peers[0].name, "@contract/base", "C5: the peer");
    assert_eq!(peers[0].version, "^1.0", "C5: the peer's range");

    // C6
    let plain = PackageName::parse("@contract/plain").unwrap();
    let one = Version::parse("1.0.0").unwrap();
    let error = registry
        .fetch(&plain, &one, false, Trust::Refuse)
        .unwrap_err();
    assert!(
        error.is(codes::R_TRUST_001),
        "C6: an unsigned package: {error:?}"
    );
    let allowed = registry
        .fetch(&plain, &one, true, Trust::Refuse)
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
        .fetch(&fresh, &fresh_version, false, Trust::Refuse)
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
            .fetch(
                &published.name(),
                &published.version(),
                false,
                Trust::Refuse,
            )
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
        let _ = registry.fetch(&name, &Version::new(2, 0, 0), true, Trust::Refuse);
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
        let served = registry
            .fetch(&name, &version, false, Trust::Refuse)
            .unwrap();
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
