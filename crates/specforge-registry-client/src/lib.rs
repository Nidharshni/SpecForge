//! The package-registry client: talking to a SpecForge package registry
//! over HTTP (search, resolve, publish), registry credentials in the OS
//! keyring, publisher trust (TOFU key pinning) and ed25519 package signing.
//!
//! It is not the Registry build (`specforge-registry`), which is pure and
//! which this crate does not depend on. It reads the extension declaration
//! and the package name from `specforge-protocol-types` and diagnostics from
//! `specforge-common`.

pub mod credential_health;
pub mod credentials;
pub mod http_client;
pub mod registry_client;
pub mod registry_config;
pub mod registry_ops;
pub mod retry;
pub mod secrets;
pub mod signing;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
pub mod trust;
pub mod trust_flow;

pub use credentials::{CredentialStore, read_credentials, user_dir, write_credentials};
pub use http_client::HttpRegistryClient;
pub use registry_client::{RegistryClient, RegistryError};
pub use registry_config::{RegistryConfig, RegistryCredential, parse_registries_from_config};
pub use registry_ops::{
    TrustCheck, publish_to_registry, verify_package_signature, verify_registry_integrity,
};
pub use retry::{RetryPolicy, Retrying};
pub use signing::{SigningKey, load_or_create_signing_key_at, signing_key_path};
pub use trust::{KnownKeys, known_keys_path, load_known_keys_at, save_known_keys_at};
pub use trust_flow::{Accepted, KeyChange, NewPin, PinSource, Trusted};
