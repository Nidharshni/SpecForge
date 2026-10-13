//! The package registry port the extension operations use. It lists a package's versions, fetches one,
//! pins the publisher keys a committed install accepted, and publishes one.
//!
//! SpecForge ships no registry (ADR 0004 N1): the only registries are the
//! ones the project's `specforge.json` lists under `registries`. With none,
//! a registry operation fails with E063 before it makes any network call.
//!
//! The operations name only the [`Registry`] trait. The adapter that talks
//! HTTP, checks publisher signatures and saves the user's key pins is
//! `specforge_ops_registry::ConfiguredRegistry`, which only the surfaces that
//! reach a registry (the CLI, MCP) link (ADR 0010).

use crate::extension::Trust;
use crate::options::{Choice, OptionTable};
use crate::{OpError, OpErrorKind};
use specforge_common::{Code, Diagnostic, codes};
use specforge_package::{PackageName, Version};
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

/// How a fetch decides a package's publisher: the caller's two choices, and the key the project
/// already records for the name. The adapter applies it (ADR 0010, 0044, 0059) and writes nothing;
/// what it decided is the package's [`Publisher`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrustPolicy<'a> {
    /// Accept a package with no publisher signature (reported as W155).
    pub allow_unsigned: bool,
    /// How a key change is decided.
    pub trust: Trust,
    /// The publisher key `specforge.lock` records for the name's registry install
    /// (`specforge_installed::Installed::locked_key`): the pin when the user has none of their own.
    pub locked_key: Option<&'a str>,
}

impl TrustPolicy<'static> {
    /// `allow_unsigned` and `trust`, with no key locked for the name.
    pub const fn new(allow_unsigned: bool, trust: Trust) -> Self {
        TrustPolicy {
            allow_unsigned,
            trust,
            locked_key: None,
        }
    }
}

impl<'a> TrustPolicy<'a> {
    /// This policy, expecting `key` as the lock's key for the name.
    pub fn locked(self, key: Option<&'a str>) -> TrustPolicy<'a> {
        TrustPolicy {
            locked_key: key,
            ..self
        }
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

/// A package `publish` hands the registry: a scoped package name and a full
/// version (both checked by ops, E072), the binary, and the declaration read
/// from it, which the registry stores as the package's manifest (ADR 0012).
#[derive(Debug, Clone, Copy)]
pub struct Upload<'a> {
    pub name: &'a PackageName,
    pub version: &'a Version,
    pub wasm: &'a [u8],
    pub declaration: &'a ExtensionDeclaration,
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
#[derive(Debug, Clone, PartialEq, Eq)]
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

impl Searched {
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
/// does, with [`crate::extension::resolve`]. Each call asks the one registry that
/// serves the name (ADR 0045).
///
/// Two adapters (ADR 0044): `specforge_ops_registry::ConfiguredRegistry` in production and
/// [`testing::MemoryRegistry`] in tests, both held to [`testing::assert_registry_contract`].
pub trait Registry {
    /// Every version `name` publishes (unparseable ones dropped);
    /// R-RES-001 when the registry has no such package.
    fn versions(&self, name: &PackageName) -> Result<Vec<Version>, OpError>;
    /// `name@version`, downloaded and integrity-checked, its publisher signature checked and its
    /// publisher decided under `policy`. A package with none is refused unless `allow_unsigned`.
    /// The expected key is the user's pin for the name, else `policy.locked_key`, else none (trust
    /// on first use). Another key is a key change, decided by `trust` (R-TRUST-003 when refused).
    /// An answer for another package or version, or one whose manifest can't be read, is refused.
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
    /// Publish `package` to the registry that serves its name (the one
    /// `fetch` asks for it), signed with the user's publisher key and
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
    fn publish(&self, package: &Upload<'_>) -> Result<Published, OpError>;
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

#[cfg(any(test, feature = "testing"))]
pub mod testing;

/// No registry: every call fails with E063 naming `operation`. What an
/// operation that never reaches a registry (`init`, which installs only
/// builtins and local files) passes where one is asked for.
pub struct Unconfigured(pub &'static str);

impl Registry for Unconfigured {
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

    fn publish(&self, _: &Upload<'_>) -> Result<Published, OpError> {
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
        verify = "TrustPolicy is what a fetch decides a publisher by: the caller's choices and the key the lock records"
    )]
    fn a_trust_policy_names_the_lock_key_only_when_given() {
        let policy = TrustPolicy::new(true, Trust::Refuse);

        assert_eq!(policy.locked_key, None);
        assert_eq!(policy.locked(Some("k")).locked_key, Some("k"));
    }

    #[test]
    fn an_unconfigured_registry_refuses_a_publish_with_e063() {
        let name = PackageName::parse("@acme/x").unwrap();
        let version = Version::new(1, 0, 0);
        let declaration = testing::declaration("@acme/x", "1.0.0", &[]);
        let error = Unconfigured("publish")
            .publish(&Upload {
                name: &name,
                version: &version,
                wasm: b"\0asm",
                declaration: &declaration,
            })
            .unwrap_err();
        assert!(error.is(NO_REGISTRY), "{error:?}");
        assert!(error.message.contains("`publish`"), "{error:?}");
    }
}
