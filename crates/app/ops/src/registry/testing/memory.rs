//! The memory registry: published versions served as a `Registry`, and what it was asked.

use std::collections::BTreeMap;
use std::sync::Mutex;

use specforge_common::codes;
use specforge_installed::hex_sha256;
use specforge_package::{DeclaredPackage, PackageName, Version, VersionRequirement};
use specforge_protocol_types::{DeclaredCategory, ExtensionDeclaration};

use super::super::{
    Found, KeyPin, Manifest, Package, Published as Receipt, Publisher, Registry, Searched,
    TrustPolicy,
};
use crate::OpError;
use crate::asking::{ExpectedFrom, Question};

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
    /// A version's published manifest, read without its binary.
    Published(PackageName, Version),
    Fetch(PackageName, Version),
}

/// A package registry held in memory: the second adapter of [`Registry`], beside
/// `specforge_ops_registry::ConfiguredRegistry`. It serves what it publishes as the configured registry
/// serves a package that passed the fetch policy, refuses as it refuses (both are held to
/// [`assert_registry_contract`](super::contract::assert_registry_contract)), and records what it was asked and what it accepted. It verifies no
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
    /// A file `pin` makes read-only before it refuses: a failed change that cannot put it back.
    locking_on_refusal: Option<std::path::PathBuf>,
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
    pub fn accepted_publishes(&self) -> Vec<(PackageName, Version)> {
        self.accepted.lock().unwrap().clone()
    }

    /// The packages whose versions were listed, in order.
    pub fn listed(&self) -> Vec<PackageName> {
        self.asked()
            .into_iter()
            .filter_map(|asked| match asked {
                Asked::Versions(name) => Some(name),
                Asked::Published(..) | Asked::Fetch(..) => None,
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

    /// [`Self::refusing_pins`], and the refusal first makes `file` read-only, so a change that wrote it
    /// cannot put it back.
    pub fn refusing_pins_after_locking(mut self, file: &std::path::Path) -> Self {
        self.refusing_pins = true;
        self.locking_on_refusal = Some(file.to_path_buf());
        self
    }

    /// The publisher of a package signed by `key_id`: held to the user's pin for `name`, else to the
    /// key the project's lock records, else accepted on first use. A key change is put to the
    /// policy's `asking`.
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
        let consents = policy.asking.answers(&Question::KeyChange {
            package: name.as_str(),
            expected: &expected,
            offered: key_id,
            from: if from_lock {
                ExpectedFrom::Lock
            } else {
                ExpectedFrom::UserPin
            },
        });
        if !consents {
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
            return Err(OpError::coded(crate::OpErrorKind::PermissionDenied,
                codes::R_TRUST_003,
                format!(
                    "key change rejected for '{name}': {what} but package is signed '{key_id}'{also}"
                ),
            )
            .with_suggestion(format!(
                "if you trust the new key, accept it with `specforge add` or `specforge update` in a \
                 terminal (`specforge add --yes` and `specforge update --yes` accept it without asking); or add '{key_id}' to trusted_keys"
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
            return Err(OpError::coded(
                crate::OpErrorKind::ExtensionNotFound,
                codes::R_RES_001,
                format!("package '{name}' not found in the registry"),
            )
            .with_suggestion("check the package name and registry configuration"));
        }
        Ok(versions)
    }

    /// What it publishes as `name@version`, without its binary: R006 as `fetch` for a version it does
    /// not hold; else the declaration it was published with and `hex_sha256` of its bytes. Asks no
    /// question and decides no publisher: an unsigned package is answered too.
    fn published(&self, name: &PackageName, version: &Version) -> Result<Manifest, OpError> {
        self.asked
            .lock()
            .unwrap()
            .push(Asked::Published(name.clone(), version.clone()));
        let held = self.held.lock().unwrap();
        let Some(published) = held
            .iter()
            .find(|p| p.name() == *name && p.version() == *version)
        else {
            return Err(not_found(name, version));
        };
        Ok(Manifest {
            name: name.clone(),
            version: version.clone(),
            sha256: hex_sha256(&published.wasm),
            declaration: published.declaration.clone(),
        })
    }

    /// The package published as `name@version`: R006 "Package not found: {name}@{version}" when there is none,
    /// R-TRUST-001 "package '{name}' is not signed" (suggestion "allow unsigned packages for this install to
    /// accept the risk") when unsigned and `!allow_unsigned`; else its bytes, `hex_sha256(bytes)`, its declaration and
    /// publisher. A signed package's publisher is decided against the key expected for `name` (the
    /// user's pin, else `policy.locked_key`): first use without one, `first_use: false` when it
    /// matches the user's pin, a key change otherwise (`Repinned` when `policy.asking` consents,
    /// else R-TRUST-003). Nothing is pinned: that is [`Registry::pin`].
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
            return Err(not_found(name, version));
        };
        if published.key_id.is_none() && !policy.allow_unsigned {
            return Err(OpError::coded(
                crate::OpErrorKind::PermissionDenied,
                codes::R_TRUST_001,
                format!("package '{name}' is not signed"),
            )
            .with_suggestion("allow unsigned packages for this install to accept the risk"));
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
            if let Some(file) = &self.locking_on_refusal {
                let mut permissions = std::fs::metadata(file)
                    .expect("the file exists")
                    .permissions();
                permissions.set_readonly(true);
                std::fs::set_permissions(file, permissions).expect("lock the file");
            }
            return Err(OpError::coded(
                crate::OpErrorKind::PermissionDenied,
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
    fn publish(&self, package: &DeclaredPackage<'_>, wasm: &[u8]) -> Result<Receipt, OpError> {
        let mut held = self.held.lock().unwrap();
        if held
            .iter()
            .any(|p| p.name() == *package.name() && p.version() == *package.version())
        {
            return Err(OpError::coded(
                crate::OpErrorKind::Conflict,
                codes::R007,
                format!(
                    "Version {} already exists for package {}.",
                    package.version(),
                    package.name()
                ),
            )
            .with_suggestion("Bump the version number before publishing."));
        }
        held.push(Published::new(wasm, package.declaration().clone()).signed_by(IN_MEMORY_KEY));
        self.accepted
            .lock()
            .unwrap()
            .push((package.name().clone(), package.version().clone()));
        Ok(Receipt {
            registry: IN_MEMORY_KEY.to_string(),
            url: format!("memory://{}/{}", package.name(), package.version()),
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

/// R006 for a version a registry does not hold.
fn not_found(name: &PackageName, version: &Version) -> OpError {
    OpError::coded(
        crate::OpErrorKind::ExtensionNotFound,
        codes::R006,
        format!("Package not found: {name}@{version}"),
    )
    .with_suggestion("Verify the package name and version.")
}
