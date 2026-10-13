//! Where each call goes. A client joins these to the registry's base URL; the server mounts
//! [`crate::address::Endpoint::route`].

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
