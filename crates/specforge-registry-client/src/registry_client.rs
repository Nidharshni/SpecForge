use specforge_common::{Diagnostic, codes};

use super::registry_config::{RegistryConfig, RegistryCredential};
use specforge_package::PackageName;
use specforge_package::Version;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry_wire::{
    ErrorCode, PackageMetadata, PackageSignature, PublishReceipt, SearchHit, SearchQuery,
};

/// Errors that can occur during registry operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    Unauthorized {
        guidance: String,
    },
    Forbidden {
        guidance: String,
    },
    RateLimited {
        retry_after_ms: u64,
    },
    Timeout {
        url: String,
    },
    NetworkError {
        message: String,
    },
    NotFound {
        specifier: String,
    },
    DuplicateVersion {
        name: String,
        version: String,
    },
    /// The declaration to publish names no package: its name or version is
    /// not one (E072).
    InvalidPackage {
        message: String,
    },
    /// The registry read the request and refused it (HTTP 400), saying why (R008).
    Refused {
        code: ErrorCode,
        message: String,
    },
    /// The registry answered with a body that isn't the wire type of the call: a malformed publisher signature
    /// among them (R009).
    InvalidReply {
        subject: String,
        message: String,
    },
}

impl RegistryError {
    /// Convert this error into a `Diagnostic`.
    pub fn to_diagnostic(&self) -> Diagnostic {
        match self {
            RegistryError::Unauthorized { guidance } => Diagnostic::new(
                codes::R001,
                format!("Registry authentication failed: {guidance}"),
            )
            .with_suggestion(
                "Log in: `specforge login --registry <alias> --token <TOKEN>`.".to_string(),
            ),
            RegistryError::Forbidden { guidance } => Diagnostic::new(
                codes::R002,
                format!("Registry access forbidden: {guidance}"),
            )
            .with_suggestion(
                "Check your permissions for this registry or package scope.".to_string(),
            ),
            RegistryError::RateLimited { retry_after_ms } => Diagnostic::new(
                codes::R003,
                format!("Registry rate limited. Retry after {retry_after_ms}ms."),
            ),
            RegistryError::Timeout { url } => {
                Diagnostic::new(codes::R004, format!("Registry request timed out: {url}"))
                    .with_suggestion(
                        "Check your network connection or try again later, then retry.".to_string(),
                    )
            }
            RegistryError::NetworkError { message } => {
                Diagnostic::new(codes::R005, format!("Registry network error: {message}"))
                    .with_suggestion("Check your network connection, then retry.".to_string())
            }
            RegistryError::NotFound { specifier } => {
                Diagnostic::new(codes::R006, format!("Package not found: {specifier}"))
                    .with_suggestion("Verify the package name and version.".to_string())
            }
            RegistryError::DuplicateVersion { name, version } => Diagnostic::new(
                codes::R007,
                format!("Version {version} already exists for package {name}."),
            )
            .with_suggestion("Bump the version number before publishing.".to_string()),
            RegistryError::InvalidPackage { message } => {
                specforge_common::package::invalid(message)
            }
            RegistryError::Refused { code, message } => Diagnostic::new(
                codes::R008,
                format!("the registry refused the request ({code}): {message}"),
            )
            .with_suggestion(
                "fix what the registry names and try again; sending the same request again is refused the same way"
                    .to_string(),
            ),
            RegistryError::InvalidReply { subject, message } => Diagnostic::new(
                codes::R009,
                format!(
                    "the registry's answer for {subject} is not the registry protocol's JSON: {message}"
                ),
            )
            .with_suggestion(
                "install nothing from this registry until it is fixed, and check that its URL is a SpecForge registry"
                    .to_string(),
            ),
        }
    }
}

impl From<RegistryError> for Diagnostic {
    fn from(err: RegistryError) -> Self {
        err.to_diagnostic()
    }
}

/// What talks to a package registry (ADR 0044): the transport, nothing else. It chooses no registry and
/// checks no reply; the fetch policy over it is `specforge_ops_registry::ConfiguredRegistry`'s. Two adapters:
/// [`crate::HttpRegistryClient`] and, in tests, `crate::testing::MemoryClient`; both are held to
/// `crate::testing::assert_client_contract`.
pub trait RegistryClient: Send + Sync {
    /// Every version `registry` publishes of `name`, as served (not parsed). `NotFound` when it has no
    /// such package. A read carries `credential` as `Authorization: Bearer` when given; a registry that
    /// requires one answers `Unauthorized` without it.
    fn versions(
        &self,
        name: &PackageName,
        registry: &RegistryConfig,
        credential: Option<&RegistryCredential>,
    ) -> Result<Vec<String>, RegistryError>;

    /// What `registry` stores for `name@version`, its `wasm_url` absolute. `NotFound` when it has none.
    fn metadata(
        &self,
        name: &PackageName,
        version: &Version,
        registry: &RegistryConfig,
        credential: Option<&RegistryCredential>,
    ) -> Result<PackageMetadata, RegistryError>;

    /// The bytes at `wasm_url` (a [`RegistryClient::metadata`] answer's). `credential` is sent only when
    /// `wasm_url` has `registry.url`'s origin (scheme, host and port): a download served elsewhere (a CDN)
    /// never sees the registry's token.
    fn download(
        &self,
        wasm_url: &str,
        registry: &RegistryConfig,
        credential: Option<&RegistryCredential>,
    ) -> Result<Vec<u8>, RegistryError>;

    /// The latest version of each package matching `query`, and declaring its category when it names one.
    fn search(
        &self,
        query: &SearchQuery,
        registry: &RegistryConfig,
        credential: Option<&RegistryCredential>,
    ) -> Result<Vec<SearchHit>, RegistryError>;

    /// Upload `package` with `manifest_json` (the exact text `signature` covers) and `signature`; `credential`,
    /// when given, authenticates it as an `Authorization: Bearer` header. A publish is always signed. The
    /// registry's receipt: what it stored.
    fn publish(
        &self,
        package: &[u8],
        declaration: &ExtensionDeclaration,
        manifest_json: &str,
        signature: &PackageSignature,
        registry: &RegistryConfig,
        credential: Option<&RegistryCredential>,
    ) -> Result<PublishReceipt, RegistryError>;

    /// Validate that the given credential authenticates successfully.
    /// Returns the server-reported expiry (RFC3339), when the registry tracks one.
    fn authenticate(
        &self,
        registry: &RegistryConfig,
        credential: &RegistryCredential,
    ) -> Result<Option<String>, RegistryError>;
}
