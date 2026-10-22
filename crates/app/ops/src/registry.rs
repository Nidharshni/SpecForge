//! The package registry port the extension operations use. It lists a package's versions, fetches one,
//! pins the publisher keys a committed install accepted, and publishes one.
//!
//! SpecForge ships no registry (ADR 0004 N1): the only registries are the
//! ones the project's `specforge.json` lists under `registries`. With none,
//! a registry operation fails with E063 before it makes any network call (its `open`).
//!
//! The operations name only the [`Registry`] trait. The adapter that talks
//! HTTP, checks publisher signatures and saves the user's key pins is
//! `specforge_ops_registry::ConfiguredRegistry`, which only the surfaces that
//! reach a registry (the CLI, MCP) link (ADR 0010).

use crate::asking::Asking;
use crate::options::{Choice, OptionTable};
use crate::{OpError, OpErrorKind};
use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_common::{Code, Diagnostic, DiagnosticList, codes};
use specforge_installed::LockFileEntry;
use specforge_package::{DeclaredPackage, PackageName, Version};
use specforge_protocol_types::{DeclaredCategory, ExtensionDeclaration};

/// The diagnostic a registry operation reports when no registry is
/// configured.
pub const NO_REGISTRY: Code = codes::E063;

/// No configured registry serves the package name: no `scope_filter` is its
/// scope, and none is the default (ADR 0045).
pub const NO_REGISTRY_FOR_NAME: Code = codes::R_OPS_001;

/// A publish has no credential for its registry (none stored for its alias, no
/// token in the environment), or the registry refused the one it sent (HTTP 401).
pub const NOT_AUTHENTICATED: Code = codes::R001;

/// The user's publisher signing key can't be read or created.
pub const UNUSABLE_SIGNING_KEY: Code = codes::E074;

/// The registry already holds the version: a published version is immutable.
pub const ALREADY_PUBLISHED: Code = codes::R007;

/// A registry answered with metadata that doesn't describe the package
/// asked for: another name or version, or a declaration other than its
/// binary's.
pub const METADATA_MISMATCH: Code = codes::R_TRUST_004;

/// The registry served a package whose manifest can't be read as an
/// extension declaration.
pub const UNREADABLE_MANIFEST: Code = codes::R_OPS_004;

/// How to configure a registry, for E063's suggestion.
pub const CONFIGURE_HINT: &str = "add a \"registries\" array to specforge.json, e.g. \
     \"registries\": [{\"alias\": \"main\", \"url\": \"<registry URL>\", \"default_registry\": true}]";

/// E063 for `operation`.
pub fn no_registry(operation: &str) -> OpError {
    OpError::coded(
        OpErrorKind::PreconditionFailed,
        NO_REGISTRY,
        format!("no registry configured: `{operation}` needs one, and SpecForge has no built-in registry"),
    )
    .with_suggestion(CONFIGURE_HINT)
}

/// A package downloaded from a registry: its bytes checked against the
/// SHA-256 the registry published, its publisher signature against the
/// trust policy the caller gave.
#[derive(Debug, Clone)]
pub struct Package {
    pub name: PackageName,
    pub version: Version,
    pub wasm: Vec<u8>,
    pub sha256: String,
    /// The declaration published with it (its manifest): the diamond gate
    /// reads its peers before the binary is loaded, and the binary must then
    /// declare exactly the same (ADR 0012).
    pub declaration: ExtensionDeclaration,
    /// How the fetch policy accepted its publisher.
    pub publisher: Publisher,
}

/// What a registry says it publishes as `name@version`, without its binary: the declaration it was
/// published with (its manifest) and the SHA-256 its binary must have. Nothing here is checked against a
/// binary or a signature: it is what a preview judges, never what is installed.
#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    pub name: PackageName,
    pub version: Version,
    pub sha256: String,
    pub declaration: ExtensionDeclaration,
}

/// What `specforge.lock` records for a name's registry install (`Installed::locked`), which every
/// fetch of the name is held to (ADR 0059 D3, ADR 0201): its publisher key at any version, its
/// bytes at its own version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LockedPin<'a> {
    pub version: &'a str,
    /// The SHA-256 it pins; empty for an entry written before installs were pinned (W149), which
    /// pins no bytes.
    pub sha256: &'a str,
    pub key_id: Option<&'a str>,
}

impl<'a> LockedPin<'a> {
    /// The pin `entry` records.
    pub(crate) fn of(entry: &'a LockFileEntry) -> LockedPin<'a> {
        LockedPin {
            version: &entry.version,
            sha256: &entry.wasm_hash,
            key_id: entry.key_id.as_deref(),
        }
    }

    /// A pin of `key_id` alone: an entry that pins no bytes.
    #[cfg(any(test, feature = "testing"))]
    pub const fn key(key_id: &'a str) -> LockedPin<'a> {
        LockedPin {
            version: "",
            sha256: "",
            key_id: Some(key_id),
        }
    }

    /// `name@version` served as `sha256` is what this pin allows: any bytes at another version or
    /// under an entry that pins none, only its own at its version. R014 otherwise (a published
    /// version never changes, R007).
    pub fn admits(
        &self,
        name: &PackageName,
        version: &Version,
        sha256: &str,
    ) -> Result<(), OpError> {
        if self.sha256.is_empty() || self.sha256 == sha256 || self.version != version.to_string() {
            return Ok(());
        }
        Err(OpError::coded(
            OpErrorKind::SchemaMismatch,
            LOCKED_BYTES_CHANGED,
            format!(
                "registry package {name}@{version} is not the binary specforge.lock pins: \
                 the lock records sha256 {}, the registry serves {sha256}",
                self.sha256
            ),
        )
        .with_suggestion(format!(
            "a published version never changes: check the registry that serves '{name}'; to install \
             the bytes it serves instead, remove the extension (`specforge remove {name}`) and add it \
             again"
        )))
    }
}

/// A registry serves the version `specforge.lock` pins as other bytes than it pins.
pub const LOCKED_BYTES_CHANGED: Code = codes::R014;

/// How a fetch decides a package's publisher: the caller's two choices, and the pin the project's
/// lock already records for the name. [`TrustPolicy::decide`] holds the rules (ADR 0213) and both
/// adapters ask it; it writes nothing, and what it decided is the package's [`Publisher`].
#[derive(Debug, Clone, Copy)]
pub struct TrustPolicy<'a> {
    /// Accept a package with no publisher signature (reported as W155).
    pub allow_unsigned: bool,
    /// Who can be asked about a key change (ADR 0110).
    pub asking: Asking<'a>,
    /// What `specforge.lock` records for the name's registry install. Private: adapters ask
    /// [`TrustPolicy::locked_key`].
    locked: Option<LockedPin<'a>>,
}

impl<'a> TrustPolicy<'a> {
    /// `allow_unsigned` and `asking`, with nothing locked for the name.
    pub const fn new(allow_unsigned: bool, asking: Asking<'a>) -> Self {
        TrustPolicy {
            allow_unsigned,
            asking,
            locked: None,
        }
    }

    /// This policy, held to the lock's `pin` for the name.
    pub fn locked(self, pin: Option<LockedPin<'a>>) -> TrustPolicy<'a> {
        TrustPolicy {
            locked: pin,
            ..self
        }
    }

    /// The key the lock records: the pin when the user has none of their own (ADR 0059 D3).
    pub fn locked_key(&self) -> Option<&'a str> {
        self.locked.and_then(|pin| pin.key_id)
    }

    /// The lock's [`LockedPin::admits`]; `Ok` when the lock records none. Every adapter asks it once
    /// the reply names `name@version`, before the download, the signature check or any question
    /// (ADR 0201).
    pub fn admits(
        &self,
        name: &PackageName,
        version: &Version,
        sha256: &str,
    ) -> Result<(), OpError> {
        self.locked
            .map_or(Ok(()), |pin| pin.admits(name, version, sha256))
    }
}

/// How the fetch policy accepted a package's publisher. A fetch decides it; the key it calls for is
/// pinned only when the lock plan that fetched it commits ([`Registry::pin`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Publisher {
    /// Not signed; accepted because the caller allowed it (W155 was reported).
    Unsigned,
    /// Signed by `key_id`. `first_use: false`: the user's pin. `first_use: true`: the user has no pin,
    /// and the key is pinned when the install is written (trust on first use, or the key the lock
    /// records).
    Signed { key_id: String, first_use: bool },
    /// Signed by `key_id`, which replaces the pin `previous` (the user's, or the lock's) with consent;
    /// pinned, and reported as W156, when the install is written.
    Repinned { key_id: String, previous: String },
}

/// A publisher key pinned for a package name when the lock plan that fetched it commits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyPin {
    pub name: PackageName,
    pub key_id: String,
    /// The key it replaces after consent (reported as W156); `None` on first use.
    pub replaces: Option<String>,
}

impl Publisher {
    /// The publisher key id; `None` when unsigned.
    pub fn key_id(&self) -> Option<&str> {
        match self {
            Publisher::Unsigned => None,
            Publisher::Signed { key_id, .. } | Publisher::Repinned { key_id, .. } => Some(key_id),
        }
    }

    /// The pin this acceptance calls for once `name`'s install is written: its key on first use or
    /// after a consented change; none for a key already pinned or an unsigned package.
    pub fn pin(&self, name: &PackageName) -> Option<KeyPin> {
        match self {
            Publisher::Signed {
                key_id,
                first_use: true,
            } => Some(KeyPin {
                name: name.clone(),
                key_id: key_id.clone(),
                replaces: None,
            }),
            Publisher::Repinned { key_id, previous } => Some(KeyPin {
                name: name.clone(),
                key_id: key_id.clone(),
                replaces: Some(previous.clone()),
            }),
            Publisher::Signed {
                first_use: false, ..
            }
            | Publisher::Unsigned => None,
        }
    }

    /// How this publisher was accepted.
    pub fn acceptance(&self) -> Acceptance {
        match self {
            Publisher::Unsigned => Acceptance::Unsigned,
            Publisher::Signed {
                first_use: false, ..
            } => Acceptance::Pinned,
            Publisher::Signed {
                first_use: true, ..
            } => Acceptance::PinnedNow,
            Publisher::Repinned { .. } => Acceptance::Repinned,
        }
    }
}

/// How a fetch accepted a package's publisher, as every document names it:
/// `unsigned`, `pinned`, `pinned_now` or `repinned`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specforge_common::shape::Shape)]
#[serde(rename_all = "snake_case")]
pub enum Acceptance {
    /// Not signed; the caller allowed it (W155).
    Unsigned,
    /// Signed by the pinned key.
    Pinned,
    /// Signed, and its key pinned when the install was written.
    PinnedNow,
    /// Signed by a key that replaced the pin with consent (W156).
    Repinned,
}

/// What a registry answered a publish with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    /// The registry that took the package: its `alias` in `specforge.json`.
    pub registry: String,
    /// Where the registry serves the published version.
    pub url: String,
    /// The publisher key the package is signed with.
    pub key_id: String,
    /// Whether that key was created for this publish (the user's first).
    pub key_created: bool,
}

/// One package a search found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Shape)]
pub struct Found {
    pub name: String,
    pub version: String,
    pub description: String,
    /// The alias of the registry it was found in: the first in `specforge.json` order that lists it.
    pub registry: String,
}

/// What a search found, and the registries that failed (each failure is reported, the others still asked).
#[derive(Debug, Clone, PartialEq)]
pub struct Searched {
    pub found: Vec<Found>,
    pub failures: Vec<Diagnostic>,
    /// How many registries were asked.
    pub asked: usize,
}

/// What `specforge search --format json` prints (CLI-only, ADR 0052 D5).
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct SearchDocument {
    pub query: String,
    /// The `--contributes` category's name; absent when none was given.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contributes: Option<String>,
    pub results: Vec<Found>,
    /// Each registry that failed, once (the shared diagnostic shape).
    pub diagnostics: DiagnosticList,
}

impl Searched {
    /// The document `specforge search` prints for `query`.
    pub fn document(&self, query: &str, contributes: Option<DeclaredCategory>) -> SearchDocument {
        SearchDocument {
            query: query.to_string(),
            contributes: contributes.map(|c| c.name().to_string()),
            results: self.found.clone(),
            diagnostics: DiagnosticList(self.failures.clone()),
        }
    }

    /// Every registry asked failed: nothing was searched (the CLI exits 1).
    pub fn failed(&self) -> bool {
        self.asked > 0 && self.failures.len() == self.asked
    }
}

const fn category(
    name: &'static str,
    help: &'static str,
    value: DeclaredCategory,
) -> Choice<DeclaredCategory> {
    Choice {
        name,
        aliases: &[],
        help,
        value,
    }
}

/// `specforge search --contributes`: a declared category. A filter, so no default.
pub const CONTRIBUTES: OptionTable<DeclaredCategory> = OptionTable {
    argument: "contribution",
    choices: &[
        category("entities", "entity kinds", DeclaredCategory::Entities),
        category("edges", "edge types", DeclaredCategory::Edges),
        category(
            "shared_fields",
            "shared fields",
            DeclaredCategory::SharedFields,
        ),
        category(
            "enhancements",
            "enhancements of other extensions' kinds",
            DeclaredCategory::Enhancements,
        ),
        category(
            "validation_rules",
            "validation rules",
            DeclaredCategory::ValidationRules,
        ),
        category(
            "surfaces",
            "commands, MCP tools and resources",
            DeclaredCategory::Surfaces,
        ),
        category(
            "collectors",
            "test collectors",
            DeclaredCategory::Collectors,
        ),
        category("analyzers", "source analyzers", DeclaredCategory::Analyzers),
        category("passes", "compiler passes", DeclaredCategory::Passes),
        category(
            "feature_flags",
            "feature flags",
            DeclaredCategory::FeatureFlags,
        ),
    ],
    default: None,
};

/// The registry port the extension operations use. It lists a package's versions, reads one's
/// published manifest without its binary (what a dry run judges),
/// fetches one, pins the publisher keys a committed install accepted and publishes one; it does not resolve a requirement (ADR 0036): ops
/// does, with [`crate::extension::resolve`]. It opens (the refusal of the whole project, once) and each
/// call asks the one registry that serves the name (ADR 0045).
///
/// Two adapters (ADR 0044): `specforge_ops_registry::ConfiguredRegistry` in production and
/// [`testing::MemoryRegistry`] in tests, both held to [`testing::assert_registry_contract`].
pub trait Registry {
    /// Whether this registry can be asked at all, for the operation it was built for: the refusals that
    /// are the whole project's, not a package's: no registry configured (E063), a `registries` entry
    /// that can't be read (E067), the user's credential store that can't be read (R012). Asks the
    /// registry nothing: no request is made.
    ///
    /// An adapter reads what it needs once. After `open` answered `Ok`, no call fails with these; after
    /// it refused, every call fails with that same refusal, so an operation that never opens still
    /// refuses at its first ask. An operation that will ask (update with a registry install selected,
    /// search) opens first and gives the refusal as its own; one that may ask nothing (an add of an
    /// installed exact version, ADR 0044 D5) need not.
    fn open(&self) -> Result<(), OpError>;
    /// Every version `name` publishes (unparseable ones dropped);
    /// R-RES-001 when the registry has no such package.
    fn versions(&self, name: &PackageName) -> Result<Vec<Version>, OpError>;
    /// `name@version`, downloaded and integrity-checked, its publisher signature checked and its
    /// publisher decided under `policy`. A package with none is refused unless `allow_unsigned`.
    /// The expected key is the user's pin for the name, else `policy.locked_key()`, else none (trust
    /// on first use). Another key is a key change, put to `policy.asking` (R-TRUST-003 when it answers no).
    /// An answer for another package or version, or one whose manifest can't be read, is refused.
    /// Refused with R014, after the reply names `name@version` and before the download, the
    /// signature check or any question, when `policy`'s lock pin pins this version as other bytes.
    /// **Writes nothing**: the decision is [`Package::publisher`], pinned by [`Registry::pin`] once
    /// the operation's lock plan commits.
    fn fetch(
        &self,
        name: &PackageName,
        version: &Version,
        policy: TrustPolicy<'_>,
    ) -> Result<Package, OpError>;
    /// `name@version`'s [`Manifest`], read from the registry that serves the name without
    /// downloading the binary: the reply must name the package and version asked for and its manifest
    /// must read as that package's declaration (the fetch policy's steps 1 and 3), else the fetch's
    /// refusal. Writes nothing, asks no question and decides no publisher.
    fn published(&self, name: &PackageName, version: &Version) -> Result<Manifest, OpError>;
    /// Pin `pins` for the user, all or none: what a committed lock plan's fetches decided. Refused
    /// with R-TRUST-006 when the pins can't be saved or the store can't be read, and nothing is
    /// saved then. Each pin that replaces another is reported as W156 by an adapter that reports.
    fn pin(&self, pins: &[KeyPin]) -> Result<(), OpError>;
    /// Publish `package` (its declaration is the manifest, ADR 0012) with its binary `wasm` to the
    /// registry that serves its name (the one `fetch` asks for it), signed with the user's publisher key and
    /// authenticated as the user is to that registry.
    ///
    /// Refused before any request, in this order:
    /// - E063/E067: the registry configuration;
    /// - R-OPS-001: no registry serves the name;
    /// - R001: there is no credential for it (R012, R-AUTH-020, R-AUTH-021
    ///   when the stored one can't be used);
    /// - E074: the signing key can't be read or created.
    ///
    /// Then the registry's own refusals: R007 for a version it holds,
    /// R001/R002 for a credential it refuses.
    fn publish(&self, package: &DeclaredPackage<'_>, wasm: &[u8]) -> Result<Published, OpError>;
    /// The latest version of each package whose name, description or keywords contain `query`, in every
    /// configured registry: one entry per name and version, from the first registry `specforge.json` lists
    /// that has it, sorted by name, then version. `contributes`: only packages whose declaration declares
    /// that category. A registry that fails is in `failures`; the others are still asked.
    fn search(
        &self,
        query: &str,
        contributes: Option<DeclaredCategory>,
    ) -> Result<Searched, OpError>;
}

mod trust;
pub use trust::{Signer, UserKeys};

#[cfg(any(test, feature = "testing"))]
pub mod testing;

/// No registry: every call fails with E063 naming `operation`. What an
/// operation that never reaches a registry (`init`, which installs only
/// builtins and local files) passes where one is asked for.
pub struct Unconfigured(pub &'static str);

impl Registry for Unconfigured {
    fn open(&self) -> Result<(), OpError> {
        Err(no_registry(self.0))
    }

    fn versions(&self, _: &PackageName) -> Result<Vec<Version>, OpError> {
        Err(no_registry(self.0))
    }

    fn fetch(&self, _: &PackageName, _: &Version, _: TrustPolicy<'_>) -> Result<Package, OpError> {
        Err(no_registry(self.0))
    }

    fn published(&self, _: &PackageName, _: &Version) -> Result<Manifest, OpError> {
        Err(no_registry(self.0))
    }

    fn pin(&self, _: &[KeyPin]) -> Result<(), OpError> {
        Err(no_registry(self.0))
    }

    fn publish(&self, _: &DeclaredPackage<'_>, _: &[u8]) -> Result<Published, OpError> {
        Err(no_registry(self.0))
    }

    fn search(&self, _: &str, _: Option<DeclaredCategory>) -> Result<Searched, OpError> {
        Err(no_registry(self.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[specforge_test_macros::test(
        type = "KeyPin",
        verify = "KeyPin is what a committed lock plan pins for a package name"
    )]
    fn a_publisher_names_the_pin_it_calls_for() {
        let name = PackageName::parse("@acme/x").unwrap();
        let signed = |first_use| Publisher::Signed {
            key_id: "k2".into(),
            first_use,
        };

        assert_eq!(
            signed(true).pin(&name),
            Some(KeyPin {
                name: name.clone(),
                key_id: "k2".into(),
                replaces: None
            })
        );
        assert_eq!(signed(false).pin(&name), None);
        assert_eq!(
            Publisher::Repinned {
                key_id: "k2".into(),
                previous: "k1".into()
            }
            .pin(&name),
            Some(KeyPin {
                name: name.clone(),
                key_id: "k2".into(),
                replaces: Some("k1".into())
            })
        );
        assert_eq!(Publisher::Unsigned.pin(&name), None);
    }

    #[specforge_test_macros::test(
        type = "TrustPolicy",
        verify = "TrustPolicy is what a fetch decides a publisher by: the caller's choices and the pin the lock records"
    )]
    fn a_trust_policy_holds_the_lock_pin_only_when_given() {
        let policy = TrustPolicy::new(true, Asking::Nobody);

        assert_eq!(policy.locked_key(), None);
        let pinned = policy.locked(Some(LockedPin::key("k")));
        assert_eq!(pinned.locked_key(), Some("k"));
    }

    #[specforge_test_macros::test(
        type = "LockedPin",
        verify = "LockedPin is what specforge.lock records for a name's registry install"
    )]
    fn a_locked_pin_is_what_a_registry_lock_entry_records() {
        let entry = LockFileEntry {
            name: PackageName::parse("@a/x").unwrap(),
            version: "1.2.0".into(),
            source: specforge_installed::LockSource::Registry,
            wasm_hash: "hash".into(),
            key_id: Some("k".into()),
            peer_dependencies: Vec::new(),
        };

        let pin = LockedPin::of(&entry);

        assert_eq!(pin.version, "1.2.0");
        assert_eq!(pin.sha256, "hash");
        assert_eq!(pin.key_id, Some("k"));
    }

    #[specforge_test_macros::test(
        type = "LockedPin",
        verify = "a locked pin admits other bytes only at another version, or when it pins no hash"
    )]
    fn a_locked_pin_admits_other_bytes_only_at_another_version() {
        let name = PackageName::parse("@a/x").unwrap();
        let pin = |version, sha256| LockedPin {
            version,
            sha256,
            key_id: None,
        };
        let v = |text: &str| Version::parse(text).unwrap();
        let admits = |pin: LockedPin, version: &str, served| pin.admits(&name, &v(version), served);

        assert!(admits(pin("1.0.0", "A"), "1.0.0", "A").is_ok(), "its bytes");
        assert!(
            admits(pin("1.0.0", "A"), "2.0.0", "B").is_ok(),
            "another version"
        );
        assert!(
            admits(pin("1.0.0", ""), "1.0.0", "B").is_ok(),
            "no hash pinned"
        );
        let error = admits(pin("1.0.0", "A"), "1.0.0", "B").unwrap_err();
        assert!(error.is(LOCKED_BYTES_CHANGED), "{error:?}");
        assert_eq!(
            error.message,
            "registry package @a/x@1.0.0 is not the binary specforge.lock pins: \
             the lock records sha256 A, the registry serves B"
        );
        let unpinned = TrustPolicy::new(true, Asking::Nobody);
        assert!(unpinned.admits(&name, &v("1.0.0"), "B").is_ok());
        let pinned = unpinned.locked(Some(pin("1.0.0", "A")));
        assert!(pinned.admits(&name, &v("1.0.0"), "B").is_err());
    }

    #[test]
    fn an_unconfigured_registry_refuses_a_publish_with_e063() {
        let declaration = testing::declaration("@acme/x", "1.0.0", &[]);
        let package = DeclaredPackage::of(&declaration).unwrap();
        let error = Unconfigured("publish")
            .publish(&package, b"\0asm")
            .unwrap_err();
        assert!(error.is(NO_REGISTRY), "{error:?}");
        assert!(error.message.contains("`publish`"), "{error:?}");
    }
}
