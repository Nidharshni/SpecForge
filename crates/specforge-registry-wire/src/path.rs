//! Where each call goes. A client joins these to the registry's base URL; the server mounts [`route`]'s
//! patterns, which are [`PREFIX`] followed by the same paths.

use specforge_package::PackageName;
use specforge_package::Version;

/// What a registry's base URL ends with, and what the server mounts every path under.
pub const PREFIX: &str = "/v1";

/// A package: `GET` lists its versions ([`crate::VersionList`]). `/packages/@acme%2Ftool`.
pub fn package(name: &PackageName) -> String {
    format!("/packages/{}", name.url_segment())
}

/// One version: `GET` its metadata, `PUT` publishes it, `DELETE` yanks it. `/packages/@acme%2Ftool/1.0.0`
/// (the version as `Version` displays it, `+build` included).
pub fn version(name: &PackageName, version: &Version) -> String {
    format!("{}/{version}", package(name))
}

/// One version's binary. `/packages/@acme%2Ftool/1.0.0/download`.
pub fn download(name: &PackageName, version: &Version) -> String {
    format!("{}/download", self::version(name, version))
}

pub const SEARCH: &str = "/search";
pub const AUTH_VERIFY: &str = "/auth/verify";
pub const ADMIN_TOKENS: &str = "/admin/tokens";

/// `/admin/tokens/{prefix}`.
pub fn admin_token(prefix: &str) -> String {
    format!("{ADMIN_TOKENS}/{prefix}")
}

/// The server's routes, as axum path patterns.
pub mod route {
    pub const PACKAGE: &str = "/v1/packages/{name}";
    pub const VERSION: &str = "/v1/packages/{name}/{version}";
    pub const DOWNLOAD: &str = "/v1/packages/{name}/{version}/download";
    pub const SEARCH: &str = "/v1/search";
    pub const AUTH_VERIFY: &str = "/v1/auth/verify";
    pub const ADMIN_TOKENS: &str = "/v1/admin/tokens";
    pub const ADMIN_TOKEN: &str = "/v1/admin/tokens/{prefix}";
    /// Outside the prefix: a liveness probe.
    pub const HEALTH: &str = "/health";
}
