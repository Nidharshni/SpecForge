//! Which registries a project uses, and the adapter behind `specforge_ops::registry::Registry`: the fetch
//! policy over the package registry client, and the publish that authenticates and signs as the user.
//!
//! `specforge-ops` names only the port, so a surface that never reaches a
//! registry (the LSP) links no HTTP client or signature code
//! (ADR 0010). The CLI and MCP, whose `add` and `update` do, build a
//! [`ConfiguredRegistry`] and pass it in. What a package passes before an
//! operation sees it (ADR 0044) is [`ConfiguredRegistry`]'s doc.

mod reports;

use reports::PolicyReports;
use specforge_common::{Code, Diagnostic, codes};
use specforge_ops::registry::{
    Found, KeyPin, METADATA_MISMATCH, Manifest, NO_REGISTRY, NO_REGISTRY_FOR_NAME, Package,
    Published, Publisher, Registry, Searched, Signer, TrustPolicy, UNREADABLE_MANIFEST, UserKeys,
    no_registry,
};
use specforge_ops::{OpError, OpErrorKind};
use specforge_package::Version;
use specforge_package::{DeclaredPackage, PackageName};
use specforge_protocol_types::{DeclaredCategory, ExtensionDeclaration};
use specforge_registry_client::{
    CredentialStore, HttpRegistryClient, NewPin, PublishError, RegistryClient, RegistryConfig,
    RegistryCredential, RegistryError, Retrying, TrustCheck, parse_registries_from_config,
    publish_to_registry, verify_package_signature, verify_registry_integrity,
};
use specforge_registry_wire::{PackageMetadata, SearchQuery};
use std::sync::OnceLock;

/// The environment variable whose token `publish` authenticates with, ahead
/// of a stored credential.
pub const TOKEN_VARIABLE: &str = "SPECFORGE_REGISTRY_TOKEN";

mod user;

pub use user::{
    CredentialHealth, CredentialLevel, CredentialLine, Kept, LoggedIn, LoggedOut, RegistryRoot,
    TokenSource, User,
};

/// The registries a project configures, and what reading them reported.
#[derive(Debug, Clone)]
pub struct Configured {
    /// The entries, in declaration order.
    pub registries: Vec<RegistryConfig>,
    /// W140 for a duplicate alias, I003 when no entry is the default. The
    /// caller shows them.
    pub diagnostics: Vec<Diagnostic>,
}

impl Configured {
    /// The one registry that serves `name`: the first entry whose
    /// `scope_filter` covers it, else the first entry marked
    /// `default_registry`. With neither, R-OPS-001 naming the scope and the
    /// configured aliases, before any request. `add`, `update` and `publish`
    /// ask this one (ADR 0045); `search` asks every entry.
    pub fn registry_for(&self, name: &PackageName) -> Result<&RegistryConfig, OpError> {
        if let Some(registry) = self
            .registries
            .iter()
            .find(|r| r.scope_filter.as_ref().is_some_and(|s| s.covers(name)))
        {
            return Ok(registry);
        }
        if let Some(registry) = self.registries.iter().find(|r| r.default_registry) {
            return Ok(registry);
        }
        let (message, suggestion) = match name.scope() {
            Some(scope) => (
                format!(
                    "no registry serves {name}: no \"scope_filter\" is \"{scope}\" (configured: {}), and none is the default",
                    self.aliases()
                ),
                format!(
                    "add \"scope_filter\": \"{scope}\" to the registry that holds it, or set \"default_registry\": true on one"
                ),
            ),
            None => (
                format!("no registry serves {name}: none is the default"),
                "set \"default_registry\": true on one registry".to_string(),
            ),
        };
        Err(OpError::coded(
            OpErrorKind::PreconditionFailed,
            NO_REGISTRY_FOR_NAME,
            message,
        )
        .with_suggestion(suggestion))
    }

    /// The registry `alias` names, or with none the default one: what
    /// `login` validates a token against and stores it for, and what
    /// `logout` forgets. E063 naming the configured aliases when `alias`
    /// names none, or when none is given and none is the default.
    pub fn named(&self, alias: Option<&str>) -> Result<&RegistryConfig, OpError> {
        let found = match alias {
            Some(alias) => self.registries.iter().find(|r| r.alias == alias),
            None => self.registries.iter().find(|r| r.default_registry),
        };
        found.ok_or_else(|| {
            let (message, suggestion) = match alias {
                Some(alias) => (
                    format!(
                        "no registry is named '{alias}' (configured: {})",
                        self.aliases()
                    ),
                    "name one of the configured registries by its alias".to_string(),
                ),
                None => (
                    format!(
                        "no registry is the default (configured: {})",
                        self.aliases()
                    ),
                    "name a registry by its alias, or set \"default_registry\": true on one"
                        .to_string(),
                ),
            };
            OpError::coded(OpErrorKind::PreconditionFailed, NO_REGISTRY, message)
                .with_suggestion(suggestion)
        })
    }

    fn aliases(&self) -> String {
        self.registries
            .iter()
            .map(|r| r.alias.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// The diagnostic for a registry configuration that can't be read.
const INVALID_CONFIG: Code = codes::E067;

/// The registries the project at `root` configures.
///
/// `operation` names the command for the message (`add`, `search`, ...).
/// No `specforge.json`, or one without a `registries` entry, fails with E063;
/// one that is there and unusable fails as `add` fails (`config_invalid`).
/// An entry that can't be read refuses the configuration with E067 naming it
/// (ADR 0075 D6): a scoped entry skipped would send its names to another
/// registry.
pub fn configured(
    root: &specforge_common::ProjectRoot,
    operation: &str,
) -> Result<Configured, OpError> {
    // `specforge.json` is read the way `add`, `update` and `remove` read
    // it: one that is there and unusable is refused as they refuse it
    // (`config_invalid`, E069), not as a missing registry.
    let read = specforge_ops::config::usable(root.dir())?;
    let Some(raw) = read.config.raw.as_ref().filter(|_| read.found) else {
        return Err(no_registry(operation));
    };
    let (registries, diagnostics) = parse_registries_from_config(&raw.to_string());
    let unreadable: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.is(INVALID_CONFIG))
        .map(|d| d.message.as_str())
        .collect();
    if !unreadable.is_empty() {
        return Err(OpError::coded(
            OpErrorKind::SchemaMismatch,
            INVALID_CONFIG,
            unreadable.join("; "),
        )
        .with_suggestion(
            "fix the \"registries\" entries in specforge.json: each needs an \"alias\" and a \"url\" naming the registry's root (an http or https URL such as https://registry.example), and a \"scope_filter\", when present, is a scope such as \"@acme\"; no registry is asked until every entry reads",
        ));
    }
    if registries.is_empty() {
        return Err(no_registry(operation));
    }
    Ok(Configured {
        registries,
        diagnostics,
    })
}

/// A project's package registry as the `Registry` port (ADR 0010, 0044). It reads the registries the
/// project's `specforge.json` configures and the user's credential store once: when an operation opens it
/// ([`Registry::open`]) or first asks it anything. It asks the one that serves a name, and hands ops only a package that passed the fetch policy:
///
/// 1. the reply names the package and version asked for (R-TRUST-004);
/// 2. the manifest reads as an extension declaration (R-OPS-004; a `manifest.json` from before ADR 0012 is
///    refused with a re-publish suggestion) naming the package and version asked for (R-TRUST-004);
/// 3. the lock's bytes: the version `specforge.lock` pins is served as the bytes it pins (R014);
/// 4. the publisher signature verifies over the digests the reply describes, and the publisher is decided
///    by [`TrustPolicy::decide`]: the key matches the one expected for the name (the user's pin, else the
///    key the project's lock records, else any: trust on first use) (R-TRUST-001..006), for a signed and an
///    unsigned package alike: the store is read whatever the signer (R-TRUST-006). The decision is
///    the package's publisher; [`Registry::pin`] writes it once the install is;
/// 5. the binary hashes to the reply's SHA-256 (R-OPS-002), so the signature covers it.
///
/// Steps 1 to 4 are `published` (a dry run); a fetch then downloads for 5. A refused package is never
/// downloaded (ADR 0213). A fetch writes nothing. The policy runs over any [`RegistryClient`]: HTTP unless
/// [`ConfiguredRegistry::with_client`] gives another. Built without reading, touching the network or
/// failing. [`Registry::open`] gives the refusal of the whole project (E063, E067, R012) before any
/// request; a call made without opening fails with the same refusal.
pub struct ConfiguredRegistry {
    root: specforge_common::ProjectRoot,
    operation: String,
    /// Read once, on the first call that needs a registry.
    registries: OnceLock<Result<Configured, OpError>>,
    /// The user's credential store, read once (see [`ConfiguredRegistry::store`]).
    store: OnceLock<Result<CredentialStore, Diagnostic>>,
    client: Box<dyn RegistryClient>,
    /// Whose registry files are read (pins, credentials, signing key).
    user: User,
    /// What the fetch policy reported (W155 at a fetch, W156 at a pin), in order.
    reported_by_policy: PolicyReports,
}

impl ConfiguredRegistry {
    /// The registries `root`'s `specforge.json` configures, over HTTP. Reads nothing yet; `operation`
    /// names the command in E063.
    pub fn for_project(root: &specforge_common::ProjectRoot, operation: &str) -> Self {
        Self {
            root: root.clone(),
            operation: operation.to_string(),
            registries: OnceLock::new(),
            store: OnceLock::new(),
            client: Box::new(Retrying::new(HttpRegistryClient::new())),
            user: User::current(),
            reported_by_policy: PolicyReports::default(),
        }
    }

    /// Reach the registries through `client` instead of HTTP (a test).
    pub fn with_client(mut self, client: impl RegistryClient + 'static) -> Self {
        self.client = Box::new(client);
        self
    }

    /// Read the registry files of `user` instead of the current user's (a
    /// test, or a custom home).
    pub fn as_user(mut self, user: User) -> Self {
        self.user = user;
        self
    }

    /// What this registry reported, once an operation has opened it or asked it anything: the registry
    /// configuration's diagnostics (W140 for a duplicate alias, I003 when none is the
    /// default; nothing when reading failed outright, each call then failing with
    /// that error), then the fetch policy's W155 in fetch order, then a W156 for each re-pin a
    /// committed operation saved. A surface shows these after the operation, whatever its result.
    pub fn reported(&self) -> Vec<Diagnostic> {
        let mut reported = match self.registries.get() {
            Some(Ok(configured)) => configured.diagnostics.clone(),
            _ => Vec::new(),
        };
        reported.extend(self.reported_by_policy.all());
        reported
    }

    /// The configuration, read on the first call that needs it.
    fn registries(&self) -> Result<&Configured, OpError> {
        self.registries
            .get_or_init(|| configured(&self.root, &self.operation))
            .as_ref()
            .map_err(Clone::clone)
    }

    /// The one registry that serves `name`: made once per call, and the
    /// client fetches from it without choosing again.
    fn registry_for(&self, name: &PackageName) -> Result<&RegistryConfig, OpError> {
        self.registries()?.registry_for(name)
    }
}

/// What a registry answers about `name@version` before its binary: the registry that serves it,
/// the credential used, its metadata and the declaration its manifest reads as.
struct Described<'r> {
    registry: &'r RegistryConfig,
    credential: Option<RegistryCredential>,
    metadata: PackageMetadata,
    declaration: ExtensionDeclaration,
}

impl ConfiguredRegistry {
    /// The first part of the fetch policy, before the publisher; it writes nothing and downloads nothing: the registry
    /// that serves `name`, its credential (R-AUTH-020/021, R010, R011, R012), the metadata of
    /// `name@version`, which must name that package and version (1), and its manifest, which must
    /// read as that package's declaration (3). The served manifest is the package's declaration
    /// (ADR 0012): the peers the diamond gate (ADR 0001) decides on, and what the binary must
    /// declare once loaded. One that can't be read must not pass as "no peers", and one describing
    /// another package must not be installed as this one.
    fn described(&self, name: &PackageName, version: &Version) -> Result<Described<'_>, OpError> {
        let registry = self.registry_for(name)?;
        let credential = self.read_credential(registry)?;
        let metadata = self
            .client
            .metadata(name, version, registry, credential.as_ref())
            .map_err(failure)?;
        reply_names(name, version, &metadata)?; // 1
        let declaration = read_declaration(name, version, &metadata.manifest)?; // 3
        declaration_names(name, version, &declaration)?; // 3
        Ok(Described {
            registry,
            credential,
            metadata,
            declaration,
        })
    }
}

impl ConfiguredRegistry {
    /// The fetch policy up to the binary (1-4), which writes nothing and downloads nothing: the reply
    /// and its manifest (1, 3, [`Self::described`]), the lock's bytes (R014, before any question), the
    /// publisher signature over the digests the reply describes (R-TRUST-002), the user's store, whatever
    /// the signer (R-TRUST-006), and the publisher, decided by [`TrustPolicy::decide`] (R-TRUST-001, -003,
    /// -005).
    /// A dry run asks this; a fetch asks it and then downloads (ADR 0213).
    fn decided(
        &self,
        name: &PackageName,
        version: &Version,
        policy: TrustPolicy<'_>,
    ) -> Result<(Described<'_>, Publisher), OpError> {
        let described = self.described(name, version)?; // 1, 3: checked before the signature, so a refused package pins no key
        policy.admits(name, version, &described.metadata.sha256)?; // R014: the lock's bytes, before any question
        let key_id =
            match verify_package_signature(&described.metadata).map_err(refused_by_trust)? {
                TrustCheck::Unsigned => None,
                TrustCheck::Verified { key_id } => Some(key_id),
            };
        // The store is read whatever the signer: an unsigned package is held to the name's pin too (ADR 0221).
        let known = specforge_registry_client::trust_flow::keys(&self.user.known_keys())
            .map_err(refused_by_trust)?;
        let key_id = key_id.as_deref();
        let publisher = policy.decide(
            name,
            key_id.map_or(Signer::Unsigned, Signer::Key),
            UserKeys {
                pin: known.pin_for(name.as_str()),
                trusted: key_id.is_some_and(|key| known.is_trusted(key)),
                denied: key_id.is_some_and(|key| known.is_denied(key)),
            },
        )?;
        Ok((described, publisher))
    }
}

impl Registry for ConfiguredRegistry {
    /// The configuration first (E063/E067, as publish refuses), then the user's store (R012).
    fn open(&self) -> Result<(), OpError> {
        self.registries()?;
        self.store().map_err(unusable_credential)?;
        Ok(())
    }

    fn fetch(
        &self,
        name: &PackageName,
        version: &Version,
        policy: TrustPolicy<'_>,
    ) -> Result<Package, OpError> {
        let (described, publisher) = self.decided(name, version, policy)?; // 1-4: nothing is downloaded yet
        let Described {
            registry,
            credential,
            metadata,
            declaration,
        } = described;
        let wasm = self
            .client
            .download(&metadata.wasm_url, registry, credential.as_ref())
            .map_err(failure)?;
        verify_registry_integrity(&wasm, &metadata.sha256)
            .map_err(|why| OpError::of(OpErrorKind::SchemaMismatch, why))?; // 5: the signature covered this SHA-256
        if publisher == Publisher::Unsigned {
            self.reported_by_policy.extend([
                specforge_registry_client::trust_flow::unsigned_accepted(
                    &metadata.name,
                    &metadata.version,
                ),
            ]); // W155
        }

        Ok(Package {
            name: name.clone(),
            version: version.clone(),
            sha256: metadata.sha256,
            wasm,
            declaration,
            publisher,
        })
    }

    fn published(
        &self,
        name: &PackageName,
        version: &Version,
        policy: TrustPolicy<'_>,
    ) -> Result<Manifest, OpError> {
        let (described, publisher) = self.decided(name, version, policy)?;
        Ok(Manifest {
            name: name.clone(),
            version: version.clone(),
            sha256: described.metadata.sha256,
            declaration: described.declaration,
            publisher,
        })
    }

    fn pin(&self, pins: &[KeyPin]) -> Result<(), OpError> {
        let new: Vec<NewPin<'_>> = pins
            .iter()
            .map(|p| NewPin {
                package: p.name.as_str(),
                key_id: &p.key_id,
                replaces: p.replaces.as_deref(),
            })
            .collect();
        let repinned = specforge_registry_client::trust_flow::record(&new, &self.user.known_keys())
            .map_err(refused_by_trust)?; // R-TRUST-006, nothing saved
        self.reported_by_policy.extend(repinned); // W156, one per re-pin
        Ok(())
    }

    fn versions(&self, name: &PackageName) -> Result<Vec<Version>, OpError> {
        let registry = self.registry_for(name)?;
        let credential = self.read_credential(registry)?;
        let published = self
            .client
            .versions(name, registry, credential.as_ref())
            .map_err(|error| match error {
                RegistryError::NotFound { .. } => OpError::of(
                    OpErrorKind::ExtensionNotFound,
                    Diagnostic::new(
                        codes::R_RES_001,
                        format!(
                            "package '{name}' not found in registry '{}'",
                            registry.alias
                        ),
                    )
                    .with_suggestion(
                        "check the package name and registry configuration".to_string(),
                    ),
                ),
                other => failure(other),
            })?;
        Ok(published
            .iter()
            .filter_map(|text| Version::parse(text).ok())
            .collect())
    }

    fn search(
        &self,
        query: &str,
        contributes: Option<DeclaredCategory>,
    ) -> Result<Searched, OpError> {
        self.open()?;
        let registries = &self.registries()?.registries;
        let mut wire = SearchQuery::new(query);
        if let Some(category) = contributes {
            wire = wire.contributing(category);
        }
        let mut found: Vec<Found> = Vec::new();
        let mut failures = Vec::new();
        for registry in registries {
            let hits = self.stored_credential(registry).and_then(|credential| {
                self.client
                    .search(&wire, registry, credential.as_ref())
                    .map_err(|error| error.to_diagnostic())
            });
            match hits {
                Ok(hits) => {
                    for hit in hits {
                        // The first registry that lists a name and version wins.
                        if !found
                            .iter()
                            .any(|f| f.name == hit.name && f.version == hit.version)
                        {
                            found.push(Found {
                                name: hit.name,
                                version: hit.version,
                                description: hit.description,
                                registry: registry.alias.clone(),
                            });
                        }
                    }
                }
                Err(mut diagnostic) => {
                    diagnostic.message = format!(
                        "search failed on registry '{}': {}",
                        registry.alias, diagnostic.message
                    );
                    failures.push(diagnostic);
                }
            }
        }
        found.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.version.cmp(&b.version)));
        Ok(Searched {
            found,
            failures,
            asked: registries.len(),
        })
    }

    fn publish(&self, package: &DeclaredPackage<'_>, wasm: &[u8]) -> Result<Published, OpError> {
        let registry = self.registry_for(package.name())?; // E063, E067, R-OPS-001
        let credential = self.credential_for(registry)?; // R001, R012, R-AUTH-020/021
        let (key, key_created) = self.signing_key()?; // E074
        let url = publish_to_registry(
            package,
            wasm,
            registry,
            Some(&credential),
            &*self.client,
            &key,
        )
        .map_err(|error| match error {
            PublishError::Invalid(diagnostic) => OpError::of(OpErrorKind::InvalidInput, diagnostic),
            PublishError::Registry(error) => failure(error),
        })?; // R007, R001, R002, R004, R005
        Ok(Published {
            registry: registry.alias.clone(),
            url,
            key_id: key.key_id(),
            key_created,
        })
    }
}

/// A package registry's failure as an operation reports it: its kind is the failure's, decided by
/// variant (a new variant is a compile error here).
pub fn failure(error: RegistryError) -> OpError {
    let kind = match &error {
        RegistryError::Unauthorized { .. } | RegistryError::Forbidden { .. } => {
            OpErrorKind::PermissionDenied
        }
        RegistryError::RateLimited { .. } | RegistryError::Timeout { .. } => OpErrorKind::Timeout,
        RegistryError::NetworkError { .. } => OpErrorKind::Internal,
        RegistryError::NotFound { .. } => OpErrorKind::ExtensionNotFound,
        RegistryError::DuplicateVersion { .. } => OpErrorKind::Conflict,
        RegistryError::Refused { .. } => OpErrorKind::InvalidInput,
        // The registry's answer is not the call's wire type: not a value the caller gave.
        RegistryError::InvalidReply { .. } => OpErrorKind::SchemaMismatch,
    };
    OpError::of(kind, error.to_diagnostic())
}

/// The trust check's refusal (R-TRUST-001..006): the package is not allowed.
fn refused_by_trust(diagnostic: Diagnostic) -> OpError {
    OpError::of(OpErrorKind::PermissionDenied, diagnostic)
}

/// A stored credential that cannot be used (R010, R011, R012, R-AUTH-020/021).
pub fn unusable_credential(diagnostic: Diagnostic) -> OpError {
    OpError::of(OpErrorKind::PreconditionFailed, diagnostic)
}

/// The signature covers the name and version the registry answers
/// with, and the pin is keyed by that name: an answer for another
/// package (or another version) would be verified, pinned and
/// installed in place of the one asked for.
fn reply_names(
    name: &PackageName,
    version: &Version,
    metadata: &PackageMetadata,
) -> Result<(), OpError> {
    if metadata.name != name.as_str() || metadata.version != version.to_string() {
        return Err(OpError::coded(
            OpErrorKind::SchemaMismatch,
            METADATA_MISMATCH,
            format!(
                "registry answered {name}@{version} with {}@{}",
                metadata.name, metadata.version
            ),
        )
        .with_suggestion("don't install the package, and check the registry"));
    }
    Ok(())
}

/// The declaration the registry served must be this package's own.
fn declaration_names(
    name: &PackageName,
    version: &Version,
    declaration: &ExtensionDeclaration,
) -> Result<(), OpError> {
    if declaration.name() != name.as_str() || declaration.version() != version.to_string() {
        return Err(OpError::coded(
            OpErrorKind::SchemaMismatch,
            METADATA_MISMATCH,
            format!(
                "registry served {name}@{version} with the declaration of {}@{}",
                declaration.name(),
                declaration.version()
            ),
        )
        .with_suggestion("don't install the package, and check the registry"));
    }
    Ok(())
}

/// The declaration a registry stores as `name@version`'s manifest. A
/// manifest from before ADR 0012 (the camelCase `manifest.json` form, with
/// `manifestVersion`) can't be read as one: it is refused with the
/// suggestion to re-publish the package.
fn read_declaration(
    name: &PackageName,
    version: &Version,
    manifest: &str,
) -> Result<ExtensionDeclaration, OpError> {
    let unreadable = |why: String| {
        OpError::coded(
            OpErrorKind::SchemaMismatch,
            UNREADABLE_MANIFEST,
            format!("the manifest of {name}@{version} can't be read: {why}"),
        )
        .with_suggestion("don't install the package, and check the registry")
    };
    if manifest.trim().is_empty() {
        return Err(unreadable("the registry served none".to_string()));
    }
    let value: serde_json::Value =
        serde_json::from_str(manifest).map_err(|e| unreadable(e.to_string()))?;
    if value.get("manifestVersion").is_some() {
        return Err(unreadable(
            "it is a manifest.json from before extension declarations".to_string(),
        )
        .with_suggestion(format!(
            "re-publish {name}@{version} with this version of specforge publish, which uploads \
             the declaration it reads from the binary"
        )));
    }
    serde_json::from_value(value).map_err(|e| unreadable(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[specforge_test_macros::test(
        invariant = "mcp_structured_error_responses",
        verify = "an operation's failure kind decides its McpError code, never its code text"
    )]
    fn a_registry_failure_has_the_kind_of_its_variant() {
        let s = || "x".to_string();
        for (error, kind) in [
            (
                RegistryError::Unauthorized { guidance: s() },
                OpErrorKind::PermissionDenied,
            ),
            (
                RegistryError::Forbidden { guidance: s() },
                OpErrorKind::PermissionDenied,
            ),
            (
                RegistryError::RateLimited { retry_after_ms: 1 },
                OpErrorKind::Timeout,
            ),
            (RegistryError::Timeout { url: s() }, OpErrorKind::Timeout),
            (
                RegistryError::NetworkError { message: s() },
                OpErrorKind::Internal,
            ),
            (
                RegistryError::NotFound { specifier: s() },
                OpErrorKind::ExtensionNotFound,
            ),
            (
                RegistryError::DuplicateVersion {
                    name: s(),
                    version: s(),
                },
                OpErrorKind::Conflict,
            ),
            (
                RegistryError::InvalidReply {
                    subject: s(),
                    message: s(),
                },
                OpErrorKind::SchemaMismatch,
            ),
        ] {
            assert_eq!(failure(error.clone()).kind, kind, "{error:?}");
        }
    }

    #[test]
    fn an_unconfigured_project_fails_each_call_with_e063_before_any_request() {
        let dir = tempfile::tempdir().unwrap();
        let registry = ConfiguredRegistry::for_project(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            "update",
        );
        assert!(registry.reported().is_empty());
        let sdk = PackageName::parse("@sdk/greet").unwrap();
        let error = registry.versions(&sdk).unwrap_err();
        assert!(error.is(specforge_ops::registry::NO_REGISTRY), "{error:?}");
        assert!(error.message.contains("`update`"), "{error:?}");
        let error = registry
            .fetch(
                &sdk,
                &Version::new(0, 1, 0),
                TrustPolicy::new(false, specforge_ops::asking::Asking::Nobody),
            )
            .unwrap_err();
        assert!(error.is(specforge_ops::registry::NO_REGISTRY), "{error:?}");
        assert!(registry.reported().is_empty());
    }

    #[specforge_test_macros::test(
        invariant = "mcp_structured_error_responses",
        verify = "an operation states the request field each refusal is about, and a refusal of the project or the environment is about none"
    )]
    fn a_registry_reply_that_is_no_wire_type_is_a_schema_mismatch() {
        // The reply is the registry's fault, not a value the caller gave: no argument is about
        // it. The registry's own refusal of a request (a 400) is the caller's.
        let invalid = failure(RegistryError::InvalidReply {
            subject: "x".to_string(),
            message: "x".to_string(),
        });
        assert_eq!(invalid.kind, OpErrorKind::SchemaMismatch);
        assert!(!invalid.kind.rejects_a_value());
        assert!(OpErrorKind::InvalidInput.rejects_a_value());
    }
}
