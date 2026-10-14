//! Where each call goes (ADR 0044 as amended by ADR 0075). A user names a registry by its root
//! (`https://registry.example`); every call goes to the root, then the protocol's version prefix (`/v1`), then the
//! call's path. A client sends a call to [`RegistryRoot::address`]; the server mounts [`Endpoint::route`]; both fill
//! the one template per endpoint.

use std::fmt;

use specforge_package::{PackageName, Version};

use crate::SearchQuery;

/// The protocol version every call is served under, after a registry's root. Only this module spells it.
const PREFIX: &str = "/v1";

/// What a registry serves outside the prefix: a liveness probe. No client call goes there.
pub const HEALTH: &str = "/health";

/// One route of the protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Endpoint {
    Package,
    Version,
    Download,
    Search,
    AuthVerify,
    AdminTokens,
    AdminToken,
}

impl Endpoint {
    /// The server's route pattern in axum's syntax: the prefix, then the path, with `{name}`, `{version}` and
    /// `{prefix}` where a call puts its values. `Endpoint::Version.route()` is `/v1/packages/{name}/{version}`.
    pub fn route(self) -> String {
        format!("{PREFIX}{}", self.template())
    }

    /// The path after the prefix, with placeholders.
    fn template(self) -> &'static str {
        match self {
            Self::Package => "/packages/{name}",
            Self::Version => "/packages/{name}/{version}",
            Self::Download => "/packages/{name}/{version}/download",
            Self::Search => "/search",
            Self::AuthVerify => "/auth/verify",
            Self::AdminTokens => "/admin/tokens",
            Self::AdminToken => "/admin/tokens/{prefix}",
        }
    }
}

/// One call: its endpoint and the values that fill its route.
#[derive(Debug, Clone, Copy)]
pub enum Call<'a> {
    /// `GET` lists a package's versions ([`crate::VersionList`]).
    Package(&'a PackageName),
    /// `GET` one version's metadata, `PUT` publishes it, `DELETE` yanks it; the version as `Version` displays it,
    /// `+build` included.
    Version(&'a PackageName, &'a Version),
    /// One version's binary.
    Download(&'a PackageName, &'a Version),
    /// `GET` a search; its query string follows the path.
    Search(&'a SearchQuery),
    /// `POST` a token check.
    AuthVerify,
    /// `POST` issues a token, `GET` lists them (an admin token authenticates both).
    AdminTokens,
    /// `DELETE` revokes the token with this id (the 8 hex digits a listing shows).
    AdminToken(&'a str),
}

impl Call<'_> {
    /// The route this call goes to.
    pub fn endpoint(&self) -> Endpoint {
        match self {
            Self::Package(_) => Endpoint::Package,
            Self::Version(..) => Endpoint::Version,
            Self::Download(..) => Endpoint::Download,
            Self::Search(_) => Endpoint::Search,
            Self::AuthVerify => Endpoint::AuthVerify,
            Self::AdminTokens => Endpoint::AdminTokens,
            Self::AdminToken(_) => Endpoint::AdminToken,
        }
    }

    /// What follows a registry's root: the endpoint's route with this call's values (a package name as one segment,
    /// `PackageName::url_segment`), and a search's query string (`SearchQuery::to_query_string`).
    /// `/v1/packages/@acme%2Ftool/1.0.0`, `/v1/search?q=a%26b&limit=50`.
    pub fn path(&self) -> String {
        let route = self.endpoint().route();
        match self {
            Self::Package(name) => route.replace("{name}", &name.url_segment()),
            Self::Version(name, version) | Self::Download(name, version) => route
                .replace("{name}", &name.url_segment())
                .replace("{version}", &version.to_string()),
            Self::Search(query) => format!("{route}?{}", query.to_query_string()),
            Self::AuthVerify | Self::AdminTokens => route,
            Self::AdminToken(prefix) => route.replace("{prefix}", prefix),
        }
    }
}

/// The URL a `registries` entry names: where a registry is, without the protocol's version prefix
/// (`https://registry.example`, `https://example.com/registry`). An `http` or `https` URL with a host, no query, no
/// fragment and no user info, whose path does not end in `/v1`; held without a trailing `/`. It reads from and
/// writes to JSON as that text.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RegistryRoot(String);

impl RegistryRoot {
    /// `text` as a registry root, or why it is not one.
    pub fn parse(text: &str) -> Result<Self, RootError> {
        let text = text.trim();
        let url = url::Url::parse(text).map_err(|e| RootError::NotAUrl {
            text: text.to_string(),
            why: e.to_string(),
        })?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(RootError::NotHttp {
                text: text.to_string(),
            });
        }
        let extra = |part| RootError::Extra {
            text: text.to_string(),
            part,
        };
        if url.query().is_some() {
            return Err(extra("query"));
        }
        if url.fragment().is_some() {
            return Err(extra("fragment"));
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(extra("user name or password"));
        }
        let root = url.as_str().trim_end_matches('/');
        if url.path().trim_end_matches('/').ends_with(PREFIX) {
            let without = root.strip_suffix(PREFIX).unwrap_or(root);
            return Err(RootError::EndsInPrefix {
                text: text.to_string(),
                root: without.to_string(),
            });
        }
        Ok(Self(root.to_string()))
    }

    /// The root as text, without a trailing `/`.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The URL `call` is sent to: the root, then [`Call::path`].
    pub fn address(&self, call: &Call<'_>) -> String {
        format!("{}{}", self.0, call.path())
    }

    /// Where a metadata answer's `wasm_url` downloads from: an `http` or `https` URL as it is (a CDN may serve the
    /// binary), or a path (`/v1/packages/…/download`, as the server answers) at this root. Anything else, a
    /// network-path reference (`//host/x`) and other schemes (`file:`) included, is not the protocol's: `Err` says
    /// why, and a client refuses the answer (R009).
    pub fn download_address(&self, wasm_url: &str) -> Result<String, String> {
        if wasm_url.starts_with('/') && !wasm_url.starts_with("//") {
            return Ok(format!("{}{wasm_url}", self.0));
        }
        match url::Url::parse(wasm_url) {
            Ok(url) if matches!(url.scheme(), "http" | "https") => Ok(wasm_url.to_string()),
            _ => Err(format!(
                "its wasm_url '{wasm_url}' is neither an http or https URL nor a path at the registry's root"
            )),
        }
    }

    /// Whether `url` has this root's origin (scheme, host and port): the only place a request carries the
    /// registry's credential. A text that is no URL has no origin.
    pub fn same_origin(&self, url: &str) -> bool {
        match (url::Url::parse(url), url::Url::parse(&self.0)) {
            (Ok(a), Ok(b)) => {
                a.scheme() == b.scheme()
                    && a.host_str() == b.host_str()
                    && a.port_or_known_default() == b.port_or_known_default()
            }
            _ => false,
        }
    }
}

impl fmt::Display for RegistryRoot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for RegistryRoot {
    type Error = RootError;

    fn try_from(text: String) -> Result<Self, RootError> {
        Self::parse(&text)
    }
}

impl From<RegistryRoot> for String {
    fn from(root: RegistryRoot) -> String {
        root.0
    }
}

impl std::str::FromStr for RegistryRoot {
    type Err = RootError;

    fn from_str(text: &str) -> Result<Self, RootError> {
        Self::parse(text)
    }
}

/// Why a text is not a registry root. Its `Display` is the sentence E067 quotes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootError {
    /// `'{text}' is not a URL ({why})`.
    NotAUrl { text: String, why: String },
    /// `'{text}' is not an http or https URL`.
    NotHttp { text: String },
    /// `'{text}' carries a {part}; a registry root has none` (`query`, `fragment`, `user name or password`).
    Extra { text: String, part: &'static str },
    /// `'{text}' ends in the registry protocol's version prefix /v1; name the registry's root: '{root}'`.
    EndsInPrefix { text: String, root: String },
}

impl fmt::Display for RootError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAUrl { text, why } => write!(f, "'{text}' is not a URL ({why})"),
            Self::NotHttp { text } => write!(f, "'{text}' is not an http or https URL"),
            Self::Extra { text, part } => {
                write!(f, "'{text}' carries a {part}; a registry root has none")
            }
            Self::EndsInPrefix { text, root } => write!(
                f,
                "'{text}' ends in the registry protocol's version prefix {PREFIX}; name the registry's root: '{root}'"
            ),
        }
    }
}

impl std::error::Error for RootError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(text: &str) -> RegistryRoot {
        RegistryRoot::parse(text).unwrap()
    }

    #[specforge_test_macros::test(
        behavior = "configure_registries",
        verify = "a registry url is the registry's root, and one ending in the protocol's version prefix is refused with E067 naming the root"
    )]
    fn a_registry_root_is_an_http_url_without_the_version_prefix() {
        for (text, expected) in [
            ("https://registry.example", "https://registry.example"),
            ("https://registry.example/", "https://registry.example"),
            ("HTTPS://Registry.Example:443/", "https://registry.example"),
            ("http://h:8080/registry/", "http://h:8080/registry"),
        ] {
            assert_eq!(root(text).as_str(), expected, "{text}");
        }
        for (text, expected) in [
            ("https://h/v1", "https://h"),
            ("https://h/v1/", "https://h"),
            ("https://h/api/v1", "https://h/api"),
        ] {
            let error = RegistryRoot::parse(text).unwrap_err();
            assert_eq!(
                error,
                RootError::EndsInPrefix {
                    text: text.to_string(),
                    root: expected.to_string()
                },
                "{text}"
            );
        }
        assert_eq!(
            RegistryRoot::parse("https://h/v1").unwrap_err().to_string(),
            "'https://h/v1' ends in the registry protocol's version prefix /v1; name the registry's root: 'https://h'"
        );
        for text in ["registry.example.com", "127.0.0.1:4873"] {
            assert!(
                matches!(RegistryRoot::parse(text), Err(RootError::NotAUrl { .. })),
                "{text}"
            );
        }
        for text in ["ftp://h", "memory://x", "localhost:4873"] {
            assert!(
                matches!(RegistryRoot::parse(text), Err(RootError::NotHttp { .. })),
                "{text}"
            );
        }
        for (text, part) in [
            ("https://h?x=1", "query"),
            ("https://h#x", "fragment"),
            ("https://u:p@h", "user name or password"),
        ] {
            assert_eq!(
                RegistryRoot::parse(text).unwrap_err(),
                RootError::Extra {
                    text: text.to_string(),
                    part
                },
                "{text}"
            );
        }

        let read: RegistryRoot = serde_json::from_str("\"https://h/\"").unwrap();
        assert_eq!(serde_json::to_string(&read).unwrap(), "\"https://h\"");
        let refused = serde_json::from_str::<RegistryRoot>("\"https://h/v1\"").unwrap_err();
        assert!(
            refused
                .to_string()
                .contains("'https://h/v1' ends in the registry protocol's version prefix /v1"),
            "{refused}"
        );
    }

    /// The route of `call` with its placeholders filled the way a client fills them, the query string left off.
    fn filled(call: &Call<'_>, name: &PackageName, version: &Version) -> String {
        let route = call.endpoint().route();
        match call.endpoint() {
            Endpoint::Package => route.replace("{name}", &name.url_segment()),
            Endpoint::Version | Endpoint::Download => route
                .replace("{name}", &name.url_segment())
                .replace("{version}", &version.to_string()),
            Endpoint::Search | Endpoint::AuthVerify | Endpoint::AdminTokens => route,
            Endpoint::AdminToken => route.replace("{prefix}", "abcd1234"),
        }
    }

    #[specforge_test_macros::test(
        behavior = "resolve_registry_source",
        verify = "every call goes to its address under the registry's root, where the server mounts it"
    )]
    fn every_call_goes_where_its_route_is_mounted() {
        let name = PackageName::parse("@acme/tool").unwrap();
        let version = Version::parse("1.0.0").unwrap();
        let query = SearchQuery::new("tool");
        let calls = [
            Call::Package(&name),
            Call::Version(&name, &version),
            Call::Download(&name, &version),
            Call::Search(&query),
            Call::AuthVerify,
            Call::AdminTokens,
            Call::AdminToken("abcd1234"),
        ];
        // A new endpoint does not compile without a case here.
        for call in calls {
            match call.endpoint() {
                Endpoint::Package
                | Endpoint::Version
                | Endpoint::Download
                | Endpoint::Search
                | Endpoint::AuthVerify
                | Endpoint::AdminTokens
                | Endpoint::AdminToken => {}
            }
            let path = call.path();
            let path = path.split('?').next().unwrap();
            assert_eq!(path, filled(&call, &name, &version), "{call:?}");
        }

        let tool = PackageName::parse("@acme/tool").unwrap();
        assert_eq!(
            Call::Download(&tool, &version).path(),
            "/v1/packages/@acme%2Ftool/1.0.0/download"
        );
        assert_eq!(
            Call::Version(&tool, &Version::parse("2.0.0+build.1").unwrap()).path(),
            "/v1/packages/@acme%2Ftool/2.0.0+build.1"
        );
        assert_eq!(
            Call::Search(&SearchQuery::new("a&b")).path(),
            "/v1/search?q=a%26b&limit=50"
        );
        assert_eq!(
            Call::AdminToken("abcd1234").path(),
            "/v1/admin/tokens/abcd1234"
        );
        assert_eq!(
            root("http://h/registry").address(&Call::Package(&tool)),
            "http://h/registry/v1/packages/@acme%2Ftool"
        );
    }

    #[specforge_test_macros::test(
        behavior = "check_registry_reply",
        verify = "a download URL is an http or https URL or a path at the registry's root, else R009"
    )]
    fn a_download_address_is_absolute_or_a_path_at_the_root() {
        let root = root("https://h/registry");
        assert_eq!(
            root.download_address("/v1/packages/x/download").unwrap(),
            "https://h/registry/v1/packages/x/download"
        );
        assert_eq!(
            root.download_address("https://cdn.example/x").unwrap(),
            "https://cdn.example/x"
        );
        for wasm_url in ["download", "//cdn/x", "file:///etc/passwd", "ftp://h/x", ""] {
            let why = root.download_address(wasm_url).unwrap_err();
            assert!(
                why.contains("is neither an http or https URL nor a path at the registry's root"),
                "{wasm_url}: {why}"
            );
        }
    }

    #[specforge_test_macros::test(
        behavior = "support_private_registries",
        verify = "a download from another origin carries no credential"
    )]
    fn a_download_elsewhere_carries_no_credential() {
        let root = root("http://a:1");
        assert!(root.same_origin("http://a:1/v1/packages/x/download"));
        assert!(!root.same_origin("http://b:1/v1/packages/x/download"));
        assert!(!root.same_origin("https://a:1/v1/packages/x/download"));
        assert!(!root.same_origin("http://a:2/v1/packages/x/download"));
        assert!(!root.same_origin("not a url"));
    }
}
