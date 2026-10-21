//! The package registry's HTTP contract (ADR 0044): the address of every call under a registry's root
//! ([`address`]), the reading of an answer ([`reply`]), the JSON a registry answers with, the multipart form a
//! publish uploads, the package signature it carries and the codes its errors carry.
//! `specforge-registry-server` serializes these and mounts these routes; `specforge-registry-client` requests
//! these addresses and deserializes these bodies. Both ends parse, sign and verify the package signature with
//! [`signature`] (ADR 0061). Nothing else writes a registry's JSON: a test reaches a registry through the
//! real server in process (`specforge_registry_server::testing`) or an in-memory client
//! (`specforge_registry_client::testing`).

pub mod address;
pub mod code;
pub mod form;
pub mod reply;
pub mod signature;

pub use address::RegistryRoot;

pub use code::ErrorCode;
pub use signature::{PackageSignature, SignatureError, SignedContent, sha256_hex};

use serde::{Deserialize, Serialize};

/// `GET {root}/v1/packages/{name}`: every version a package publishes and has not yanked, oldest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionList {
    pub name: String,
    pub versions: Vec<String>,
}

/// `GET {root}/v1/packages/{name}/{version}`: what a registry stores for one version.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageMetadata {
    pub name: String,
    pub version: String,
    /// SHA-256 of the binary, lowercase hex.
    pub sha256: String,
    #[serde(default)]
    pub size_bytes: u64,
    /// From the declaration's handshake.
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    /// The account that owns the package's scope.
    #[serde(default)]
    pub publisher: String,
    /// RFC 3339.
    #[serde(default)]
    pub published_at: String,
    /// Where the binary downloads from: an `http`/`https` URL, or a path at the registry's root. The server
    /// answers [`address::Call::Download`]'s path (`/v1/packages/…/download`); a client resolves it with
    /// [`RegistryRoot::download_address`] and hands it on absolute.
    pub wasm_url: String,
    /// The publisher signature; absent when unsigned (a registry that refuses unsigned uploads serves none such).
    /// Only a well-formed one reads: a reply carrying another is not this body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<PackageSignature>,
    /// The exact manifest uploaded with the package: its extension declaration (ADR 0012). A string on purpose:
    /// the signature covers these bytes.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub manifest: String,
}

/// How many hits a search asks for when it names no limit.
pub const DEFAULT_SEARCH_LIMIT: u32 = 50;

fn default_limit() -> u32 {
    DEFAULT_SEARCH_LIMIT
}

/// `GET {root}/v1/search?q=…&limit=…`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Only packages whose declaration declares this category (a `DeclaredCategory` name, e.g. `surfaces`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contributes: Option<String>,
}

impl SearchQuery {
    /// `query` with the default limit.
    pub fn new(query: &str) -> Self {
        Self {
            q: query.to_string(),
            limit: DEFAULT_SEARCH_LIMIT,
            contributes: None,
        }
    }

    /// Only packages declaring `category`.
    pub fn contributing(mut self, category: specforge_protocol_types::DeclaredCategory) -> Self {
        self.contributes = Some(category.name().to_string());
        self
    }

    /// The URL query string, `application/x-www-form-urlencoded`, so reserved characters in `q`
    /// (`&`, `=`, `#`, `%`) cannot change the parameters: `q=a%26b&limit=50`.
    pub fn to_query_string(&self) -> String {
        let mut query = form_urlencoded::Serializer::new(String::new());
        query
            .append_pair("q", &self.q)
            .append_pair("limit", &self.limit.to_string());
        if let Some(contributes) = &self.contributes {
            query.append_pair("contributes", contributes);
        }
        query.finish()
    }
}

/// The answer to a search: the latest version of each matching package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResults {
    pub results: Vec<SearchHit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
}

/// `PUT {root}/v1/packages/{name}/{version}` answered `201 Created`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishReceipt {
    pub name: String,
    pub version: String,
    pub sha256: String,
    pub size_bytes: u64,
    /// The id of the key whose signature the registry verified over the upload.
    pub key_id: String,
}

/// `DELETE {root}/v1/packages/{name}/{version}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Yanked {
    pub yanked: bool,
}

/// `POST {root}/v1/auth/verify` with a bearer token the registry accepts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenVerified {
    pub valid: bool,
    /// The scope the token may publish into; `None` is every scope.
    pub scope: Option<String>,
    pub label: String,
    /// RFC 3339; `None` never expires.
    pub expires_at: Option<String>,
}

/// `POST {root}/v1/admin/tokens` body (an admin token authenticates it). `account` is required; the other fields are optional.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenRequest {
    /// The account the token is issued to (it owns the scopes the token claims); the registry refuses a
    /// request that names none.
    #[serde(default)]
    pub account: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    /// Days until expiry; `None` is 90, `0` expires at once.
    #[serde(default)]
    pub expires_in_days: Option<u64>,
}

/// The answer to a [`TokenRequest`]: the raw token, shown once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenIssued {
    pub token: String,
    /// What revokes it (`DELETE {root}/v1/admin/tokens/{prefix}`).
    pub prefix: String,
    pub expires_in_days: u64,
}

/// `GET {root}/v1/admin/tokens`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenList {
    pub tokens: Vec<TokenSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenSummary {
    pub prefix: String,
    pub account: String,
    pub scope: Option<String>,
    pub label: String,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub admin: bool,
}

/// `DELETE {root}/v1/admin/tokens/{prefix}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenRevoked {
    pub revoked: bool,
}

/// The body of every error answer: `{"error": {"code": …, "message": …}}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorDetail {
    /// What the registry refused, as one of [`ErrorCode`]'s names; [`ErrorCode::Unknown`] for a code this client does
    /// not know, or none.
    #[serde(default)]
    pub code: ErrorCode,
    pub message: String,
}

impl ErrorBody {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            error: ErrorDetail {
                code,
                message: message.into(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// `value` serializes to `expected` and `expected` reads back as `value`.
    fn is_this_json<T>(value: &T, expected: serde_json::Value)
    where
        T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug,
    {
        assert_eq!(serde_json::to_value(value).unwrap(), expected);
        assert_eq!(&serde_json::from_value::<T>(expected).unwrap(), value);
    }

    #[specforge_test_macros::test(
        type = "VersionList",
        verify = "VersionList is the JSON a registry serves for a package's versions"
    )]
    fn a_version_list_is_this_json() {
        is_this_json(
            &VersionList {
                name: "@acme/tool".into(),
                versions: vec!["1.0.0".into(), "1.1.0".into()],
            },
            json!({"name": "@acme/tool", "versions": ["1.0.0", "1.1.0"]}),
        );
    }

    #[specforge_test_macros::test(
        type = "PackageMetadata",
        verify = "PackageMetadata is the JSON a registry serves for one version"
    )]
    fn package_metadata_is_this_json() {
        use chrono::TimeZone;
        let signature = PackageSignature::sign(
            &ed25519_dalek::SigningKey::from_bytes(&[7; 32]),
            &SignedContent::of("@acme/tool", "1.0.0", b"\0asm", "{}"),
            chrono::Utc.with_ymd_and_hms(2026, 10, 8, 0, 0, 0).unwrap(),
        );
        let signed = PackageMetadata {
            name: "@acme/tool".into(),
            version: "1.0.0".into(),
            sha256: "ab".repeat(32),
            size_bytes: 12,
            description: "A tool".into(),
            keywords: vec!["tool".into()],
            publisher: "acct_1".into(),
            published_at: "2026-10-08T00:00:00+00:00".into(),
            wasm_url: "/v1/packages/@acme%2Ftool/1.0.0/download".into(),
            signature: Some(signature.clone()),
            manifest: "{}".into(),
        };
        is_this_json(
            &signed,
            json!({
                "name": "@acme/tool",
                "version": "1.0.0",
                "sha256": "ab".repeat(32),
                "size_bytes": 12,
                "description": "A tool",
                "keywords": ["tool"],
                "publisher": "acct_1",
                "published_at": "2026-10-08T00:00:00+00:00",
                "wasm_url": "/v1/packages/@acme%2Ftool/1.0.0/download",
                "signature": serde_json::from_str::<serde_json::Value>(&signature.to_json()).unwrap(),
                "manifest": "{}",
            }),
        );

        // An unsigned one omits the signature and the manifest.
        let unsigned = PackageMetadata {
            signature: None,
            manifest: String::new(),
            ..signed
        };
        let value = serde_json::to_value(&unsigned).unwrap();
        let object = value.as_object().unwrap();
        for key in ["signature", "manifest"] {
            assert!(!object.contains_key(key), "{key}");
        }
        assert_eq!(object.len(), 9);

        // A signature that isn't one makes the reply unreadable.
        let malformed = json!({
            "name": "@acme/tool", "version": "1.0.0", "sha256": "ab", "wasm_url": "/x",
            "signature": {"sig": "x", "public_key": "z", "signed_at": "now"},
        });
        let error = serde_json::from_value::<PackageMetadata>(malformed).unwrap_err();
        assert!(
            error.to_string().contains("`sig` is not 128 hex"),
            "{error}"
        );

        // A reply with only the required four keys reads.
        let minimal: PackageMetadata = serde_json::from_value(
            json!({"name": "@acme/tool", "version": "1.0.0", "sha256": "ab", "wasm_url": "/x"}),
        )
        .unwrap();
        assert_eq!(minimal.size_bytes, 0);
        assert!(minimal.keywords.is_empty() && minimal.manifest.is_empty());
    }

    #[specforge_test_macros::test(
        type = "SearchResults",
        verify = "SearchResults is the JSON a registry answers a search with"
    )]
    fn search_results_are_this_json() {
        is_this_json(
            &SearchResults {
                results: vec![SearchHit {
                    name: "@acme/tool".into(),
                    version: "1.0.0".into(),
                    description: "A tool".into(),
                }],
            },
            json!({"results": [{"name": "@acme/tool", "version": "1.0.0", "description": "A tool"}]}),
        );
    }

    #[specforge_test_macros::test(
        type = "SearchHit",
        verify = "SearchHit is the JSON of one search hit"
    )]
    fn a_search_hit_is_this_json() {
        is_this_json(
            &SearchHit {
                name: "@acme/tool".into(),
                version: "1.0.0".into(),
                description: String::new(),
            },
            json!({"name": "@acme/tool", "version": "1.0.0", "description": ""}),
        );
        // A hit without a description reads.
        let bare: SearchHit =
            serde_json::from_value(json!({"name": "@acme/tool", "version": "1.0.0"})).unwrap();
        assert_eq!(bare.description, "");
    }

    #[specforge_test_macros::test(
        type = "PublishReceipt",
        verify = "PublishReceipt is the JSON a registry answers a publish with"
    )]
    fn a_publish_receipt_is_this_json() {
        is_this_json(
            &PublishReceipt {
                name: "@acme/tool".into(),
                version: "1.0.0".into(),
                sha256: "ab".into(),
                size_bytes: 7,
                key_id: "k1".into(),
            },
            json!({"name": "@acme/tool", "version": "1.0.0", "sha256": "ab", "size_bytes": 7, "key_id": "k1"}),
        );
    }

    #[specforge_test_macros::test(
        type = "TokenVerified",
        verify = "TokenVerified is the JSON a registry answers a token check with"
    )]
    fn a_token_verification_is_this_json() {
        is_this_json(
            &TokenVerified {
                valid: true,
                scope: None,
                label: "ci".into(),
                expires_at: Some("2027-01-01T00:00:00+00:00".into()),
            },
            json!({"valid": true, "scope": null, "label": "ci", "expires_at": "2027-01-01T00:00:00+00:00"}),
        );
    }

    #[specforge_test_macros::test(
        type = "RegistryErrorBody",
        verify = "RegistryErrorBody is the JSON of every registry error"
    )]
    fn an_error_body_is_this_json() {
        is_this_json(
            &ErrorBody::new(ErrorCode::NotFound, "package not found"),
            json!({"error": {"code": "NOT_FOUND", "message": "package not found"}}),
        );
        // An error from a registry that sends no code, or one this client does not know, still reads.
        let codeless: ErrorBody =
            serde_json::from_value(json!({"error": {"message": "no"}})).unwrap();
        assert_eq!(codeless.error.code, ErrorCode::Unknown);
        let unknown: ErrorBody =
            serde_json::from_value(json!({"error": {"code": "SOMETHING_NEW", "message": "no"}}))
                .unwrap();
        assert_eq!(unknown.error.code, ErrorCode::Unknown);
    }

    #[test]
    fn the_admin_bodies_are_this_json() {
        is_this_json(
            &TokenIssued {
                token: "t".into(),
                prefix: "abcd1234".into(),
                expires_in_days: 90,
            },
            json!({"token": "t", "prefix": "abcd1234", "expires_in_days": 90}),
        );
        is_this_json(
            &TokenList {
                tokens: vec![TokenSummary {
                    prefix: "abcd1234".into(),
                    account: "acme".into(),
                    scope: Some("@acme".into()),
                    label: "ci".into(),
                    created_at: "c".into(),
                    expires_at: None,
                    admin: false,
                }],
            },
            json!({"tokens": [{"prefix": "abcd1234", "account": "acme", "scope": "@acme", "label": "ci", "created_at": "c", "expires_at": null, "admin": false}]}),
        );
        is_this_json(&TokenRevoked { revoked: true }, json!({"revoked": true}));
        is_this_json(&Yanked { yanked: true }, json!({"yanked": true}));
        let empty: TokenRequest = serde_json::from_value(json!({})).unwrap();
        assert_eq!(empty, TokenRequest::default());
    }

    #[test]
    fn form_body_is_the_bytes_the_client_sent() {
        let signed = form::body("B", "{\"m\":1}", b"\0asm", Some("{\"s\":1}"));
        assert_eq!(
            String::from_utf8_lossy(&signed),
            "--B\r\nContent-Disposition: form-data; name=\"manifest\"\r\nContent-Type: application/json\r\n\r\n{\"m\":1}\r\n\
             --B\r\nContent-Disposition: form-data; name=\"wasm\"; filename=\"extension.wasm\"\r\nContent-Type: application/wasm\r\n\r\n\0asm\r\n\
             --B\r\nContent-Disposition: form-data; name=\"signature\"\r\n\r\n{\"s\":1}\r\n\
             --B--\r\n"
        );
        let unsigned = form::body("B", "{}", b"\0asm", None);
        assert!(!String::from_utf8_lossy(&unsigned).contains("signature"));
        assert_eq!(form::content_type("B"), "multipart/form-data; boundary=B");
    }

    #[test]
    fn search_query_encodes_reserved_characters() {
        assert_eq!(
            SearchQuery::new("a&b=c#d e").to_query_string(),
            "q=a%26b%3Dc%23d+e&limit=50"
        );
    }

    #[test]
    fn search_query_preserves_param_structure() {
        let pairs: Vec<(String, String)> =
            form_urlencoded::parse(SearchQuery::new("a&b=c#d e").to_query_string().as_bytes())
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();
        assert_eq!(
            pairs,
            vec![
                ("q".to_string(), "a&b=c#d e".to_string()),
                ("limit".to_string(), "50".to_string()),
            ]
        );
    }

    #[specforge_test_macros::test(
        type = "SearchQuery",
        verify = "SearchQuery is the query string a registry search reads"
    )]
    fn a_search_query_names_its_category() {
        use specforge_protocol_types::DeclaredCategory;
        assert_eq!(
            SearchQuery::new("a&b")
                .contributing(DeclaredCategory::Surfaces)
                .to_query_string(),
            "q=a%26b&limit=50&contributes=surfaces"
        );
        assert!(
            !SearchQuery::new("a")
                .to_query_string()
                .contains("contributes")
        );
    }

    #[test]
    fn search_query_plain_text_is_stable() {
        assert_eq!(
            SearchQuery::new("widget").to_query_string(),
            "q=widget&limit=50"
        );
    }
}
