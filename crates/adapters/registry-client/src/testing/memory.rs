use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};

use specforge_package::PackageName;
use specforge_package::{DeclaredPackage, Version, VersionRequirement};
use specforge_protocol_types::{DeclaredCategory, ExtensionDeclaration};
use specforge_registry_wire::{
    DEFAULT_SEARCH_LIMIT, ErrorCode, PackageMetadata, PackageSignature, PublishReceipt, SearchHit,
    SearchQuery, SignedContent, sha256_hex,
};

use crate::SigningKey;
use crate::registry_client::{RegistryClient, RegistryError};
use crate::registry_config::{RegistryConfig, RegistryCredential};

/// One call a [`MemoryClient`] answered (or failed), in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub kind: CallKind,
    /// The alias of the registry asked.
    pub registry: Option<String>,
    /// `name`, `name@version`, the query, or the download URL.
    pub subject: String,
    /// What the call sent: a read's credential, a publish's or an authenticate's.
    pub credential: Option<RegistryCredential>,
    /// A publish's signature.
    pub signature: Option<PackageSignature>,
    /// A publish's manifest JSON.
    pub manifest: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallKind {
    Versions,
    Metadata,
    Download,
    Search,
    Publish,
    Authenticate,
}

struct Failure {
    kind: CallKind,
    /// The URL of the registry it applies to; any when `None`.
    registry: Option<String>,
    error: RegistryError,
}

/// One version a registry publishes: asked for by `name` and `version`, answered with `metadata`.
struct Stored {
    name: String,
    version: String,
    metadata: PackageMetadata,
}

#[derive(Default)]
struct State {
    /// Per registry URL, the versions it publishes, in store order.
    registries: HashMap<String, Vec<Stored>>,
    /// The binary of each stored version, by the URL it downloads from.
    binaries: HashMap<String, Vec<u8>>,
    /// Accepted tokens and the account each was issued to.
    tokens: HashMap<String, String>,
    /// Which account owns a scope, by (registry URL, scope): claimed by the first publish that stores.
    owners: HashMap<(String, String), String>,
    /// The registries (by URL) whose reads require an accepted token.
    private: HashSet<String>,
    failures: Vec<Failure>,
    calls: Vec<Call>,
}

/// A package registry held in memory: the second adapter of [`RegistryClient`], beside
/// [`crate::HttpRegistryClient`]. For every call the contract exercises it answers as the registry server does.
/// It checks nothing a test [`MemoryClient::store`]s, so a test can serve any reply, honest or not, and run
/// the fetch policy without sockets. Clones share one store.
#[derive(Clone, Default)]
pub struct MemoryClient {
    state: Arc<Mutex<State>>,
}

fn key_of(registry: &RegistryConfig) -> String {
    registry.url.as_str().to_string()
}

/// Where a stored version's binary downloads from unless a test says otherwise.
fn download_url(registry: &RegistryConfig, name: &str, version: &str) -> String {
    format!("memory://{}/{name}/{version}", registry.alias)
}

impl MemoryClient {
    /// A registry holding nothing and accepting no token.
    pub fn new() -> Self {
        Self::default()
    }

    /// Accept `token` as a bearer credential for publish and authenticate, as a server accepts the
    /// tokens it issued.
    pub fn accepting(self, token: &str) -> Self {
        self.accepting_as(token, "publisher")
    }

    /// [`MemoryClient::accepting`] a token issued to `account`, which owns the scopes its publishes claim.
    pub fn accepting_as(self, token: &str, account: &str) -> Self {
        self.state
            .lock()
            .unwrap()
            .tokens
            .insert(token.to_string(), account.to_string());
        self
    }

    /// Make the reads of `registry` (versions, metadata, download, search) answer `Unauthorized` unless the
    /// credential's token is one the client accepts ([`MemoryClient::accepting`]), as a registry
    /// served with `--private` does.
    pub fn private(self, registry: &RegistryConfig) -> Self {
        self.state.lock().unwrap().private.insert(key_of(registry));
        self
    }

    /// Store `metadata` and `wasm` in `registry` as one published version, unchecked. An empty
    /// `metadata.wasm_url` becomes `memory://{alias}/{name}/{version}`; any other is kept, so a download
    /// can be made to miss.
    pub fn store(&self, registry: &RegistryConfig, metadata: PackageMetadata, wasm: Vec<u8>) {
        let (name, version) = (metadata.name.clone(), metadata.version.clone());
        self.store_as(registry, &name, &version, metadata, wasm);
    }

    /// [`MemoryClient::store`] `metadata` as the answer for `name@version`, whatever package and version
    /// it describes: how a test serves a reply for another package than the one asked for.
    pub fn store_as(
        &self,
        registry: &RegistryConfig,
        name: &str,
        version: &str,
        mut metadata: PackageMetadata,
        wasm: Vec<u8>,
    ) {
        let url = download_url(registry, name, version);
        if metadata.wasm_url.is_empty() {
            metadata.wasm_url = url.clone();
        }
        let mut state = self.state.lock().unwrap();
        state.binaries.insert(url, wasm);
        state
            .registries
            .entry(key_of(registry))
            .or_default()
            .push(Stored {
                name: name.to_string(),
                version: version.to_string(),
                metadata,
            });
    }

    /// Fail the next `kind` call on `registry` (any registry when `None`) with `error`. Queued failures
    /// are consumed in order.
    pub fn fail_next(
        &self,
        kind: CallKind,
        registry: Option<&RegistryConfig>,
        error: RegistryError,
    ) {
        self.state.lock().unwrap().failures.push(Failure {
            kind,
            registry: registry.map(key_of),
            error,
        });
    }

    /// Every call answered so far, oldest first.
    pub fn calls(&self) -> Vec<Call> {
        self.state.lock().unwrap().calls.clone()
    }

    /// Record `call`, then fail it when a queued failure matches.
    fn begin(&self, call: Call, registry: Option<&RegistryConfig>) -> Result<(), RegistryError> {
        let mut state = self.state.lock().unwrap();
        let kind = call.kind;
        state.calls.push(call);
        let url = registry.map(key_of);
        let at = state
            .failures
            .iter()
            .position(|f| f.kind == kind && (f.registry.is_none() || f.registry == url));
        match at {
            Some(at) => Err(state.failures.remove(at).error),
            None => Ok(()),
        }
    }

    /// A read of `registry`: recorded with its credential, refused when the registry is private and
    /// the credential is not accepted.
    fn read(
        &self,
        kind: CallKind,
        registry: &RegistryConfig,
        subject: String,
        credential: Option<&RegistryCredential>,
    ) -> Result<(), RegistryError> {
        self.begin(
            Call {
                credential: credential.cloned(),
                ..Self::call(kind, Some(registry), subject)
            },
            Some(registry),
        )?;
        let private = self
            .state
            .lock()
            .unwrap()
            .private
            .contains(&key_of(registry));
        if private {
            self.accepts(credential)?;
        }
        Ok(())
    }

    fn call(kind: CallKind, registry: Option<&RegistryConfig>, subject: String) -> Call {
        Call {
            kind,
            registry: registry.map(|r| r.alias.clone()),
            subject,
            credential: None,
            signature: None,
            manifest: None,
        }
    }

    /// The account `credential`'s token was issued to, when this registry accepts it.
    fn accepts(&self, credential: Option<&RegistryCredential>) -> Result<String, RegistryError> {
        let unauthorized = |guidance: &str| RegistryError::Unauthorized {
            guidance: guidance.to_string(),
        };
        let Some(credential) = credential else {
            return Err(unauthorized("missing Authorization header"));
        };
        self.state
            .lock()
            .unwrap()
            .tokens
            .get(credential.token())
            .cloned()
            .ok_or_else(|| unauthorized("invalid or revoked token"))
    }
}

impl RegistryClient for MemoryClient {
    fn versions(
        &self,
        name: &PackageName,
        registry: &RegistryConfig,
        credential: Option<&RegistryCredential>,
    ) -> Result<Vec<String>, RegistryError> {
        self.read(CallKind::Versions, registry, name.to_string(), credential)?;
        let state = self.state.lock().unwrap();
        let versions: Vec<String> = state
            .registries
            .get(&key_of(registry))
            .into_iter()
            .flatten()
            .filter(|m| m.name == name.as_str())
            .map(|m| m.version.clone())
            .collect();
        if versions.is_empty() {
            return Err(RegistryError::NotFound {
                specifier: name.to_string(),
            });
        }
        Ok(versions)
    }

    fn metadata(
        &self,
        name: &PackageName,
        version: &Version,
        registry: &RegistryConfig,
        credential: Option<&RegistryCredential>,
    ) -> Result<PackageMetadata, RegistryError> {
        let subject = format!("{name}@{version}");
        self.read(CallKind::Metadata, registry, subject.clone(), credential)?;
        let state = self.state.lock().unwrap();
        state
            .registries
            .get(&key_of(registry))
            .into_iter()
            .flatten()
            .find(|m| m.name == name.as_str() && m.version == version.to_string())
            .map(|m| m.metadata.clone())
            .ok_or(RegistryError::NotFound { specifier: subject })
    }

    fn download(
        &self,
        wasm_url: &str,
        registry: &RegistryConfig,
        credential: Option<&RegistryCredential>,
    ) -> Result<Vec<u8>, RegistryError> {
        self.read(
            CallKind::Download,
            registry,
            wasm_url.to_string(),
            credential,
        )?;
        let state = self.state.lock().unwrap();
        state
            .binaries
            .get(wasm_url)
            .cloned()
            .ok_or(RegistryError::NotFound {
                specifier: wasm_url.to_string(),
            })
    }

    fn search(
        &self,
        query: &SearchQuery,
        registry: &RegistryConfig,
        credential: Option<&RegistryCredential>,
    ) -> Result<Vec<SearchHit>, RegistryError> {
        self.read(CallKind::Search, registry, query.q.clone(), credential)?;
        let category = query
            .contributes
            .as_deref()
            .and_then(DeclaredCategory::from_name);
        let matches_term = |m: &Stored| {
            let keywords: Vec<&str> = m.metadata.keywords.iter().map(String::as_str).collect();
            specforge_package::search::matches(
                &query.q,
                &m.name,
                &m.metadata.description,
                &keywords,
            )
        };
        let declares = |m: &Stored| {
            category.is_none_or(|category| {
                serde_json::from_str::<ExtensionDeclaration>(&m.metadata.manifest)
                    .is_ok_and(|d| d.declares(category))
            })
        };
        let state = self.state.lock().unwrap();
        // Each package one of whose versions matches, at the version `latest` picks (what `add`
        // installs), kept when that version declares the category; by name.
        let mut by_name: BTreeMap<&str, Vec<&Stored>> = BTreeMap::new();
        for stored in state
            .registries
            .get(&key_of(registry))
            .into_iter()
            .flatten()
        {
            by_name.entry(&stored.name).or_default().push(stored);
        }
        let mut hits: Vec<SearchHit> = by_name
            .into_values()
            .filter(|versions| versions.iter().any(|m| matches_term(m)))
            .filter_map(|versions| {
                VersionRequirement::Latest.pick_from(versions, |m| Version::parse(&m.version).ok())
            })
            .filter(|m| declares(m))
            .map(|m| SearchHit {
                name: m.name.clone(),
                version: m.version.clone(),
                description: m.metadata.description.clone(),
            })
            .collect();
        hits.truncate(DEFAULT_SEARCH_LIMIT as usize);
        Ok(hits)
    }

    fn publish(
        &self,
        package: &DeclaredPackage<'_>,
        wasm: &[u8],
        manifest_json: &str,
        signature: &PackageSignature,
        registry: &RegistryConfig,
        credential: Option<&RegistryCredential>,
    ) -> Result<PublishReceipt, RegistryError> {
        let (name, version) = (package.name(), package.version());
        self.begin(
            Call {
                credential: credential.cloned(),
                signature: Some(signature.clone()),
                manifest: Some(manifest_json.to_string()),
                ..Self::call(
                    CallKind::Publish,
                    Some(registry),
                    format!("{name}@{version}"),
                )
            },
            Some(registry),
        )?;
        let account = self.accepts(credential)?;
        // As the registry server does: a scope another account owns is closed to this one.
        let scope = name.scope().map(|s| (key_of(registry), s.to_string()));
        if let Some(scope @ (_, scope_name)) = &scope
            && let Some(owner) = self.state.lock().unwrap().owners.get(scope)
            && *owner != account
        {
            return Err(RegistryError::Forbidden {
                guidance: format!(
                    "scope '{scope_name}' is owned by account '{owner}' — only tokens issued to it may publish or yank there"
                ),
            });
        }
        let held = self
            .state
            .lock()
            .unwrap()
            .registries
            .get(&key_of(registry))
            .into_iter()
            .flatten()
            .any(|m| m.name == name.as_str() && m.version == version.to_string());
        if held {
            return Err(RegistryError::DuplicateVersion {
                name: name.to_string(),
                version: version.to_string(),
            });
        }
        // As the registry server does: the manifest is the declaration of the package the upload names.
        let manifest: ExtensionDeclaration =
            serde_json::from_str(manifest_json).map_err(|why| RegistryError::Refused {
                code: ErrorCode::InvalidManifest,
                message: format!("manifest is not an extension declaration: {why}"),
            })?;
        if manifest.name() != name.as_str() || manifest.version() != version.to_string() {
            return Err(RegistryError::Refused {
                code: ErrorCode::NameMismatch,
                message: format!(
                    "manifest identifies {}@{} but the upload path is {name}@{version}",
                    manifest.name(),
                    manifest.version()
                ),
            });
        }
        // As the registry server does: a signature that doesn't verify over the upload is refused.
        signature
            .verify(&SignedContent::of(
                name.as_str(),
                &version.to_string(),
                wasm,
                manifest_json,
            ))
            .map_err(|why| RegistryError::Refused {
                code: ErrorCode::InvalidSignature,
                message: why.to_string(),
            })?;
        let mut metadata = package_metadata(
            name.as_str(),
            &version.to_string(),
            wasm,
            manifest_json,
            None,
        );
        metadata.signature = Some(signature.clone());
        let receipt = PublishReceipt {
            name: name.to_string(),
            version: version.to_string(),
            sha256: metadata.sha256.clone(),
            size_bytes: wasm.len() as u64,
            key_id: signature.key_id(),
        };
        self.store(registry, metadata, wasm.to_vec());
        if let Some(scope) = scope {
            self.state.lock().unwrap().owners.insert(scope, account);
        }
        Ok(receipt)
    }

    fn authenticate(
        &self,
        registry: &RegistryConfig,
        credential: &RegistryCredential,
    ) -> Result<Option<String>, RegistryError> {
        self.begin(
            Call {
                credential: Some(credential.clone()),
                ..Self::call(
                    CallKind::Authenticate,
                    Some(registry),
                    registry.url.as_str().to_string(),
                )
            },
            Some(registry),
        )?;
        self.accepts(Some(credential))?;
        Ok(None)
    }
}

fn package_metadata(
    name: &str,
    version: &str,
    wasm: &[u8],
    manifest: &str,
    key: Option<&SigningKey>,
) -> PackageMetadata {
    let sha256 = sha256_hex(wasm);
    let (description, keywords) = serde_json::from_str::<ExtensionDeclaration>(manifest)
        .map(|d| {
            (
                d.handshake.description.clone().unwrap_or_default(),
                d.handshake.keywords.clone(),
            )
        })
        .unwrap_or_default();
    let signature = key.map(|key| key.sign(&SignedContent::of(name, version, wasm, manifest)));
    PackageMetadata {
        name: name.to_string(),
        version: version.to_string(),
        sha256,
        size_bytes: wasm.len() as u64,
        description,
        keywords,
        wasm_url: String::new(),
        signature,
        manifest: manifest.to_string(),
        ..Default::default()
    }
}

/// What a registry stores for `name@version` built from `wasm` and `manifest`: the SHA-256 and size of
/// `wasm`, the description and keywords of the manifest's handshake (empty when it is not a declaration),
/// and, when `key` is given, the publisher signature over them and its key id. `wasm_url` is empty.
pub fn package(
    name: &str,
    version: &str,
    wasm: &[u8],
    manifest: &str,
    key: Option<&SigningKey>,
) -> PackageMetadata {
    package_metadata(name, version, wasm, manifest, key)
}
