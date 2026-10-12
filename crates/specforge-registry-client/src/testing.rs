//! A package registry in memory, behind the `RegistryClient` seam, and the contract every adapter of the
//! seam keeps (ADR 0044).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use specforge_package::PackageName;
use specforge_package::Version;
use specforge_protocol_types::{DeclaredCategory, ExtensionDeclaration};
use specforge_registry_wire::{
    DEFAULT_SEARCH_LIMIT, ErrorCode, PackageMetadata, PackageSignature, PublishReceipt, SearchHit,
    SearchQuery, SignedContent, path, sha256_hex,
};

use crate::registry_client::{RegistryClient, RegistryError};
use crate::registry_config::{RegistryConfig, RegistryCredential};
use crate::{SigningKey, TrustCheck};

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
    tokens: HashSet<String>,
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
    registry.url.trim_end_matches('/').to_string()
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
        self.state.lock().unwrap().tokens.insert(token.to_string());
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

    /// Whether `credential` is a token this registry accepts.
    fn accepts(&self, credential: Option<&RegistryCredential>) -> Result<(), RegistryError> {
        let unauthorized = |guidance: &str| RegistryError::Unauthorized {
            guidance: guidance.to_string(),
        };
        let Some(credential) = credential else {
            return Err(unauthorized("missing Authorization header"));
        };
        if self
            .state
            .lock()
            .unwrap()
            .tokens
            .contains(credential.token())
        {
            Ok(())
        } else {
            Err(unauthorized("invalid or revoked token"))
        }
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
        let needle = query.q.to_ascii_lowercase();
        let category = query
            .contributes
            .as_deref()
            .and_then(DeclaredCategory::from_name);
        let matches = |m: &Stored| {
            let declares = category.is_none_or(|category| {
                serde_json::from_str::<ExtensionDeclaration>(&m.metadata.manifest)
                    .is_ok_and(|d| d.declares(category))
            });
            declares
                && (m.name.to_ascii_lowercase().contains(&needle)
                    || m.metadata
                        .description
                        .to_ascii_lowercase()
                        .contains(&needle)
                    || m.metadata
                        .keywords
                        .iter()
                        .any(|k| k.to_ascii_lowercase().contains(&needle)))
        };
        let state = self.state.lock().unwrap();
        // The latest version of each matching package, by SemVer, then by name.
        let mut latest: HashMap<&str, &Stored> = HashMap::new();
        for stored in state
            .registries
            .get(&key_of(registry))
            .into_iter()
            .flatten()
        {
            if !matches(stored) {
                continue;
            }
            let newer = match latest.get(stored.name.as_str()) {
                Some(current) => {
                    Version::parse(&stored.version).ok() > Version::parse(&current.version).ok()
                }
                None => true,
            };
            if newer {
                latest.insert(&stored.name, stored);
            }
        }
        let mut hits: Vec<SearchHit> = latest
            .into_values()
            .map(|m| SearchHit {
                name: m.name.clone(),
                version: m.version.clone(),
                description: m.metadata.description.clone(),
            })
            .collect();
        hits.sort_by(|a, b| a.name.cmp(&b.name));
        hits.truncate(DEFAULT_SEARCH_LIMIT as usize);
        Ok(hits)
    }

    fn publish(
        &self,
        package: &[u8],
        declaration: &ExtensionDeclaration,
        manifest_json: &str,
        signature: &PackageSignature,
        registry: &RegistryConfig,
        credential: Option<&RegistryCredential>,
    ) -> Result<PublishReceipt, RegistryError> {
        let name =
            PackageName::declared_by(declaration).map_err(|why| RegistryError::InvalidPackage {
                message: why.to_string(),
            })?;
        let version =
            Version::parse(declaration.version()).map_err(|why| RegistryError::InvalidPackage {
                message: format!("'{}' is not a SemVer version: {why}", declaration.version()),
            })?;
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
        self.accepts(credential)?;
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
                name: declaration.name().to_string(),
                version: declaration.version().to_string(),
            });
        }
        // As the registry server does: a signature that doesn't verify over the upload is refused.
        signature
            .verify(&SignedContent::of(
                declaration.name(),
                &version.to_string(),
                package,
                manifest_json,
            ))
            .map_err(|why| RegistryError::Refused {
                code: ErrorCode::InvalidSignature,
                message: why.to_string(),
            })?;
        let mut metadata = package_metadata(
            declaration.name(),
            &version.to_string(),
            package,
            manifest_json,
            None,
        );
        metadata.signature = Some(signature.clone());
        let receipt = PublishReceipt {
            name: declaration.name().to_string(),
            version: version.to_string(),
            sha256: metadata.sha256.clone(),
            size_bytes: package.len() as u64,
            key_id: signature.key_id(),
        };
        self.store(registry, metadata, package.to_vec());
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
                ..Self::call(CallKind::Authenticate, Some(registry), registry.url.clone())
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

/// The declaration the contract publishes: `@contract/tool` at `version`, described "Contract tool".
fn contract_declaration(version: &str) -> ExtensionDeclaration {
    serde_json::from_value(serde_json::json!({
        "handshake": {
            "protocol_version": "1.0.0",
            "name": "@contract/tool",
            "version": version,
            "description": "Contract tool",
            "keywords": ["contract"],
            "peer_dependencies": [],
            "sandbox_policy": null
        }
    }))
    .expect("the contract declaration is a declaration")
}

/// The client contract (ADR 0044): what every `RegistryClient` adapter does against a registry that holds
/// nothing yet. `credential` must be one `registry` accepts for publishing. Panics naming the clause that
/// failed.
pub fn assert_client_contract(
    client: &dyn RegistryClient,
    registry: &RegistryConfig,
    credential: &RegistryCredential,
) {
    let tool = PackageName::parse("@contract/tool").unwrap();
    let key = SigningKey::generate();
    let base = registry.url.trim_end_matches('/');

    // K1
    let error = client.versions(&tool, registry, None).unwrap_err();
    assert!(
        matches!(error, RegistryError::NotFound { .. }),
        "K1: versions before any publish is NotFound, not {error:?}"
    );

    // K2
    let mut declarations = Vec::new();
    for version in ["1.0.0", "1.1.0"] {
        let declaration = contract_declaration(version);
        let wasm = format!("\0asm contract tool {version}").into_bytes();
        let url = crate::publish_to_registry(
            &wasm,
            &declaration,
            registry,
            Some(credential),
            client,
            &key,
        )
        .unwrap_or_else(|d| panic!("K2: publishing {version} fails: {d:?}"));
        let expected = format!(
            "{base}{}",
            path::version(&tool, &Version::parse(version).unwrap())
        );
        assert_eq!(url, expected, "K2: publish answers the version's URL");
        declarations.push((declaration, wasm));
    }

    // K3
    let mut versions = client
        .versions(&tool, registry, None)
        .expect("K3: versions");
    versions.sort();
    assert_eq!(versions, ["1.0.0", "1.1.0"], "K3: the published versions");

    // K4
    let (declaration, wasm) = &declarations[1];
    let metadata = client
        .metadata(&tool, &Version::new(1, 1, 0), registry, None)
        .expect("K4: metadata of 1.1.0");
    assert_eq!(metadata.name, "@contract/tool", "K4: name");
    assert_eq!(metadata.version, "1.1.0", "K4: version");
    assert_eq!(metadata.sha256, sha256_hex(wasm), "K4: sha256");
    assert_eq!(metadata.size_bytes, wasm.len() as u64, "K4: size_bytes");
    assert_eq!(
        metadata.manifest,
        serde_json::to_string(declaration).unwrap(),
        "K4: the manifest is the exact JSON uploaded"
    );
    assert_eq!(metadata.description, "Contract tool", "K4: description");
    assert_eq!(metadata.keywords, ["contract"], "K4: keywords");
    assert_eq!(
        metadata.signature.as_ref().map(PackageSignature::key_id),
        Some(key.key_id()),
        "K4: the signature is the publisher key's"
    );
    assert!(!metadata.wasm_url.is_empty(), "K4: wasm_url");

    // K5
    let bytes = client
        .download(&metadata.wasm_url, registry, None)
        .expect("K5: download of the metadata's wasm_url");
    assert_eq!(&bytes, wasm, "K5: the published bytes");
    assert_eq!(
        crate::verify_package_signature(&metadata, &bytes).expect("K5: the signature verifies"),
        TrustCheck::Verified {
            key_id: key.key_id()
        },
        "K5: verified by the publisher key"
    );

    // K6
    let error = client
        .metadata(&tool, &Version::new(9, 9, 9), registry, None)
        .unwrap_err();
    assert!(
        matches!(error, RegistryError::NotFound { .. }),
        "K6: metadata of an unpublished version is NotFound, not {error:?}"
    );

    // K7
    let manifest = serde_json::to_string(declaration).unwrap();
    let signature = key.sign(&SignedContent::of(
        "@contract/tool",
        "1.1.0",
        wasm,
        &manifest,
    ));
    let error = client
        .publish(
            wasm,
            declaration,
            &manifest,
            &signature,
            registry,
            Some(credential),
        )
        .unwrap_err();
    assert!(
        matches!(error, RegistryError::DuplicateVersion { .. }),
        "K7: publishing 1.1.0 again is DuplicateVersion, not {error:?}"
    );

    // K8
    let hits = client
        .search(&SearchQuery::new("contract"), registry, None)
        .expect("K8: search");
    assert_eq!(hits.len(), 1, "K8: one hit for \"contract\": {hits:?}");
    assert_eq!(hits[0].name, "@contract/tool", "K8: the hit's name");
    assert_eq!(hits[0].version, "1.1.0", "K8: the latest version");
    assert_eq!(hits[0].description, "Contract tool", "K8: the description");
    assert_eq!(
        client
            .search(&SearchQuery::new("a&b=c#d"), registry, None)
            .expect("K8: odd query"),
        vec![],
        "K8: a query cannot change the parameters"
    );

    // K9
    client
        .authenticate(registry, credential)
        .expect("K9: the credential authenticates");
    let wrong = RegistryCredential::new(credential.alias.clone(), "not-a-token");
    let error = client.authenticate(registry, &wrong).unwrap_err();
    assert!(
        matches!(error, RegistryError::Unauthorized { .. }),
        "K9: a token the registry does not know is Unauthorized, not {error:?}"
    );

    // K10
    let next = contract_declaration("2.0.0");
    let manifest = serde_json::to_string(&next).unwrap();
    let signature = key.sign(&SignedContent::of(
        "@contract/tool",
        "2.0.0",
        b"\0asm next",
        &manifest,
    ));
    let error = client
        .publish(b"\0asm next", &next, &manifest, &signature, registry, None)
        .unwrap_err();
    assert!(
        matches!(error, RegistryError::Unauthorized { .. }),
        "K10: publishing with no credential is Unauthorized, not {error:?}"
    );

    // K11
    let next_wasm = b"\0asm next";
    let mismatched = key.sign(&SignedContent::of(
        "@contract/tool",
        "2.0.0",
        b"\0asm other",
        &manifest,
    ));
    let error = client
        .publish(
            next_wasm,
            &next,
            &manifest,
            &mismatched,
            registry,
            Some(credential),
        )
        .unwrap_err();
    assert!(
        matches!(
            error,
            RegistryError::Refused {
                code: ErrorCode::InvalidSignature,
                ..
            }
        ),
        "K11: a signature over other bytes is Refused(InvalidSignature), not {error:?}"
    );
    let error = client
        .metadata(&tool, &Version::new(2, 0, 0), registry, None)
        .unwrap_err();
    assert!(
        matches!(error, RegistryError::NotFound { .. }),
        "K11: a refused publish stores nothing, not {error:?}"
    );
    let signature = key.sign(&SignedContent::of(
        "@contract/tool",
        "2.0.0",
        next_wasm,
        &manifest,
    ));
    let receipt = client
        .publish(
            next_wasm,
            &next,
            &manifest,
            &signature,
            registry,
            Some(credential),
        )
        .expect("K11: the same version, correctly signed, publishes");
    assert_eq!(
        receipt,
        PublishReceipt {
            name: "@contract/tool".into(),
            version: "2.0.0".into(),
            sha256: sha256_hex(next_wasm),
            size_bytes: next_wasm.len() as u64,
            key_id: key.key_id(),
        },
        "K11: the receipt names what was stored"
    );
}

/// The private client contract (plan 16): what every `RegistryClient` adapter does against a registry that
/// requires a token to read and holds nothing yet. `credential` must be one `registry` accepts.
pub fn assert_private_client_contract(
    client: &dyn RegistryClient,
    registry: &RegistryConfig,
    credential: &RegistryCredential,
) {
    let tool = PackageName::parse("@contract/tool").unwrap();
    let unauthorized = |what: &str, result: Result<(), RegistryError>| {
        assert!(
            matches!(result, Err(RegistryError::Unauthorized { .. })),
            "K-P1: {what} without a credential is Unauthorized, not {result:?}"
        );
    };

    // K-P1
    unauthorized(
        "versions",
        client.versions(&tool, registry, None).map(|_| ()),
    );
    unauthorized(
        "metadata",
        client
            .metadata(&tool, &Version::new(1, 0, 0), registry, None)
            .map(|_| ()),
    );
    unauthorized(
        "download",
        client
            .download(
                &format!(
                    "{}{}",
                    registry.url.trim_end_matches('/'),
                    path::download(&tool, &Version::new(1, 0, 0))
                ),
                registry,
                None,
            )
            .map(|_| ()),
    );
    unauthorized(
        "search",
        client
            .search(&SearchQuery::new("x"), registry, None)
            .map(|_| ()),
    );

    // K-P2
    let error = client
        .versions(&tool, registry, Some(credential))
        .unwrap_err();
    assert!(
        matches!(error, RegistryError::NotFound { .. }),
        "K-P2: versions with the credential is NotFound, not {error:?}"
    );
    assert_eq!(
        client
            .search(&SearchQuery::new("x"), registry, Some(credential))
            .expect("K-P2: search with the credential"),
        vec![],
        "K-P2: nothing is published yet"
    );

    // K-P3
    let declaration = contract_declaration("1.0.0");
    let wasm = b"\0asm contract tool private".to_vec();
    let key = SigningKey::generate();
    crate::publish_to_registry(
        &wasm,
        &declaration,
        registry,
        Some(credential),
        client,
        &key,
    )
    .expect("K-P3: publishing with the credential");
    let metadata = client
        .metadata(&tool, &Version::new(1, 0, 0), registry, Some(credential))
        .expect("K-P3: metadata with the credential");
    let bytes = client
        .download(&metadata.wasm_url, registry, Some(credential))
        .expect("K-P3: download with the credential");
    assert_eq!(bytes, wasm, "K-P3: the published bytes");
    unauthorized(
        "a published version's metadata",
        client
            .metadata(&tool, &Version::new(1, 0, 0), registry, None)
            .map(|_| ()),
    );
    unauthorized(
        "a published version's download",
        client
            .download(&metadata.wasm_url, registry, None)
            .map(|_| ()),
    );

    // K-P4
    client
        .authenticate(registry, credential)
        .expect("K-P4: the credential authenticates");
    let wrong = RegistryCredential::new(credential.alias.clone(), "not-a-token");
    assert!(
        matches!(
            client.authenticate(registry, &wrong),
            Err(RegistryError::Unauthorized { .. })
        ),
        "K-P4: a token the registry does not know is Unauthorized"
    );
}
