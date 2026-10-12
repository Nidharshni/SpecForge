use std::time::SystemTime;

use reqwest::StatusCode;
use reqwest::blocking::{Client, RequestBuilder, Response};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, RETRY_AFTER};
use specforge_registry_wire::{
    ErrorBody, PackageMetadata, PackageSignature, SearchHit, SearchQuery, SearchResults,
    TokenVerified, VersionList, form, path,
};

use super::registry_client::{RegistryClient, RegistryError};
use super::registry_config::{RegistryConfig, RegistryCredential};
use specforge_package::PackageName;
use specforge_package::Version;
use specforge_protocol_types::ExtensionDeclaration;

pub struct HttpRegistryClient {
    client: Client,
    timeout: std::time::Duration,
}

impl HttpRegistryClient {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("failed to build HTTP client"),
            timeout: std::time::Duration::from_secs(30),
        }
    }

    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = timeout;
        self.client = Client::builder()
            .timeout(timeout)
            .build()
            .expect("failed to build HTTP client");
        self
    }

    fn base_url(registry: &RegistryConfig) -> String {
        registry.url.trim_end_matches('/').to_string()
    }

    /// Send `request` to `url`: a timeout is `Timeout { url }`, any other transport failure `NetworkError`.
    fn send(&self, request: RequestBuilder, url: &str) -> Result<Response, RegistryError> {
        request.send().map_err(|e| {
            if e.is_timeout() {
                RegistryError::Timeout {
                    url: url.to_string(),
                }
            } else {
                RegistryError::NetworkError {
                    message: e.to_string(),
                }
            }
        })
    }
}

impl Default for HttpRegistryClient {
    fn default() -> Self {
        Self::new()
    }
}

impl RegistryClient for HttpRegistryClient {
    fn versions(
        &self,
        name: &PackageName,
        registry: &RegistryConfig,
        credential: Option<&RegistryCredential>,
    ) -> Result<Vec<String>, RegistryError> {
        let url = format!("{}{}", Self::base_url(registry), path::package(name));
        let resp = self.send(authorized(self.client.get(&url), credential), &url)?;
        match resp.status() {
            StatusCode::OK => {
                let body: VersionList = resp.json().map_err(|e| RegistryError::NetworkError {
                    message: format!("invalid response body: {}", e),
                })?;
                Ok(body.versions)
            }
            _ => Err(failure_of(resp, name.as_str())),
        }
    }

    fn metadata(
        &self,
        name: &PackageName,
        version: &Version,
        registry: &RegistryConfig,
        credential: Option<&RegistryCredential>,
    ) -> Result<PackageMetadata, RegistryError> {
        let base = Self::base_url(registry);
        let url = format!("{base}{}", path::version(name, version));
        let resp = self.send(authorized(self.client.get(&url), credential), &url)?;
        match resp.status() {
            StatusCode::OK => {
                let mut body: PackageMetadata =
                    resp.json().map_err(|e| RegistryError::NetworkError {
                        message: format!("invalid response body: {}", e),
                    })?;
                if body.wasm_url.starts_with('/') {
                    // the server may return a root-relative download path;
                    // resolve it against the configured registry base
                    body.wasm_url = format!("{base}{}", body.wasm_url);
                }
                Ok(body)
            }
            _ => Err(failure_of(resp, &format!("{name}@{version}"))),
        }
    }

    fn download(
        &self,
        wasm_url: &str,
        registry: &RegistryConfig,
        credential: Option<&RegistryCredential>,
    ) -> Result<Vec<u8>, RegistryError> {
        // The registry's token goes to the registry, not to wherever it points a download.
        let credential = credential.filter(|_| same_origin(wasm_url, &Self::base_url(registry)));
        let resp = self.send(authorized(self.client.get(wasm_url), credential), wasm_url)?;
        match resp.status() {
            StatusCode::OK => {
                resp.bytes()
                    .map(|b| b.to_vec())
                    .map_err(|e| RegistryError::NetworkError {
                        message: format!("failed to read response bytes: {}", e),
                    })
            }
            _ => Err(failure_of(resp, wasm_url)),
        }
    }

    fn search(
        &self,
        query: &SearchQuery,
        registry: &RegistryConfig,
        credential: Option<&RegistryCredential>,
    ) -> Result<Vec<SearchHit>, RegistryError> {
        let url = format!(
            "{}{}?{}",
            Self::base_url(registry),
            path::SEARCH,
            query.to_query_string()
        );
        let resp = self.send(authorized(self.client.get(&url), credential), &url)?;
        match resp.status() {
            StatusCode::OK => {
                let body: SearchResults = resp.json().map_err(|e| RegistryError::NetworkError {
                    message: format!("invalid search response: {}", e),
                })?;
                Ok(body.results)
            }
            _ => Err(failure_of(resp, &query.q)),
        }
    }

    fn publish(
        &self,
        package: &[u8],
        declaration: &ExtensionDeclaration,
        manifest_json: &str,
        signature: &PackageSignature,
        registry: &RegistryConfig,
        credential: Option<&RegistryCredential>,
    ) -> Result<String, RegistryError> {
        let name =
            PackageName::declared_by(declaration).map_err(|why| RegistryError::InvalidPackage {
                message: why.to_string(),
            })?;
        let version =
            Version::parse(declaration.version()).map_err(|why| RegistryError::InvalidPackage {
                message: format!("'{}' is not a SemVer version: {why}", declaration.version()),
            })?;
        let url = format!(
            "{}{}",
            Self::base_url(registry),
            path::version(&name, &version)
        );

        // Build the multipart body explicitly: reqwest's blocking multipart
        // wrapper can fail with a body error on large wasm parts, while an
        // explicit body is deterministic and length-known.
        let boundary = format!(
            "specforge-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let body = form::body(
            &boundary,
            manifest_json,
            package,
            Some(&signature.to_json()),
        );

        let mut request = self
            .client
            .put(&url)
            .header(CONTENT_TYPE, form::content_type(&boundary))
            .body(body);
        if let Some(credential) = credential {
            request = request.header(AUTHORIZATION, format!("Bearer {}", credential.token()));
        }

        let resp = self.send(request, &url)?;
        match resp.status() {
            StatusCode::OK | StatusCode::CREATED => Ok(url),
            StatusCode::CONFLICT => Err(RegistryError::DuplicateVersion {
                name: declaration.name().to_string(),
                version: declaration.version().to_string(),
            }),
            _ => Err(failure_of(resp, &format!("{name}@{version}"))),
        }
    }

    fn authenticate(
        &self,
        registry: &RegistryConfig,
        credential: &RegistryCredential,
    ) -> Result<Option<String>, RegistryError> {
        let url = format!("{}{}", Self::base_url(registry), path::AUTH_VERIFY);
        let request = self
            .client
            .post(&url)
            .header(AUTHORIZATION, format!("Bearer {}", credential.token()))
            .header(CONTENT_TYPE, "application/json")
            .body("{}");
        let resp = self.send(request, &url)?;
        match resp.status() {
            StatusCode::OK => {
                let body: TokenVerified = resp.json().map_err(|e| RegistryError::NetworkError {
                    message: format!("invalid auth response: {}", e),
                })?;
                Ok(body.expires_at)
            }
            _ => Err(failure_of(resp, &registry.url)),
        }
    }
}

/// `request` with `credential` as its bearer token, when there is one.
fn authorized(request: RequestBuilder, credential: Option<&RegistryCredential>) -> RequestBuilder {
    match credential {
        Some(credential) => request.header(AUTHORIZATION, format!("Bearer {}", credential.token())),
        None => request,
    }
}

/// Whether `url` has `base`'s origin: the same scheme, host and port.
fn same_origin(url: &str, base: &str) -> bool {
    match (reqwest::Url::parse(url), reqwest::Url::parse(base)) {
        (Ok(a), Ok(b)) => {
            a.scheme() == b.scheme()
                && a.host_str() == b.host_str()
                && a.port_or_known_default() == b.port_or_known_default()
        }
        _ => false,
    }
}

/// Read a failure answer: its status, `Retry-After` and body, as [`failure`] sees them.
fn failure_of(resp: Response, subject: &str) -> RegistryError {
    let status = resp.status();
    let retry_after = resp
        .headers()
        .get(RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let body = resp.text().unwrap_or_default();
    failure(
        status,
        retry_after.as_deref(),
        &body,
        subject,
        SystemTime::now(),
    )
}

/// What an answer other than success means, the same for every call (ADR 0044): 401 `Unauthorized` and
/// 403 `Forbidden` (both carrying the registry's message as guidance, else a default), 404 `NotFound`
/// naming `subject`, 429 `RateLimited` (its `Retry-After`, read as `now` sees it), anything else
/// `NetworkError` with the registry's message, else "registry answered {status}". Publish reads 409 as
/// `DuplicateVersion` before asking this.
fn failure(
    status: StatusCode,
    retry_after: Option<&str>,
    body: &str,
    subject: &str,
    now: SystemTime,
) -> RegistryError {
    let message = serde_json::from_str::<ErrorBody>(body)
        .ok()
        .map(|e| e.error.message);
    match status {
        StatusCode::UNAUTHORIZED => RegistryError::Unauthorized {
            guidance: message.unwrap_or_else(|| "token expired or invalid".to_string()),
        },
        StatusCode::FORBIDDEN => RegistryError::Forbidden {
            guidance: message.unwrap_or_else(|| "insufficient permissions".to_string()),
        },
        StatusCode::NOT_FOUND => RegistryError::NotFound {
            specifier: subject.to_string(),
        },
        StatusCode::TOO_MANY_REQUESTS => RegistryError::RateLimited {
            retry_after_ms: parse_retry_after_ms(retry_after, now),
        },
        _ => RegistryError::NetworkError {
            message: message.unwrap_or_else(|| format!("registry answered {status}")),
        },
    }
}

/// Fallback delay when a 429 response carries no usable `Retry-After`.
const DEFAULT_RETRY_AFTER_MS: u64 = 5000;

/// Parse a `Retry-After` header value into a delay in milliseconds.
///
/// Per RFC 9110 the value is either delta-seconds or an HTTP-date. Falls back
/// to [`DEFAULT_RETRY_AFTER_MS`] when the header is absent or malformed. An
/// HTTP-date in the past yields `0`.
fn parse_retry_after_ms(header: Option<&str>, now: SystemTime) -> u64 {
    let Some(value) = header.map(str::trim) else {
        return DEFAULT_RETRY_AFTER_MS;
    };

    if let Ok(seconds) = value.parse::<u64>() {
        return seconds.saturating_mul(1000);
    }

    match httpdate::parse_http_date(value) {
        Ok(retry_at) => match retry_at.duration_since(now) {
            Ok(wait) => wait.as_millis() as u64,
            Err(_) => 0,
        },
        Err(_) => DEFAULT_RETRY_AFTER_MS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: SystemTime = SystemTime::UNIX_EPOCH;

    #[specforge_test_macros::test(
        behavior = "support_private_registries",
        verify = "a download from another origin carries no credential"
    )]
    fn a_download_elsewhere_carries_no_credential() {
        let base = "http://a:1/v1";
        assert!(same_origin("http://a:1/v1/packages/x/download", base));
        assert!(!same_origin("http://b:1/v1/packages/x/download", base));
        assert!(!same_origin("https://a:1/v1/packages/x/download", base));
        assert!(!same_origin("http://a:2/v1/packages/x/download", base));
        assert!(!same_origin("not a url", base));
    }

    fn error_body(message: &str) -> String {
        serde_json::to_string(&ErrorBody::new("ANY", message)).unwrap()
    }

    #[specforge_test_macros::test(
        behavior = "authenticate_registry_request",
        verify = "every registry call reads an answer's status as one error"
    )]
    fn every_call_reads_a_status_alike() {
        let read = |status: u16, retry_after: Option<&str>, body: &str| {
            failure(
                StatusCode::from_u16(status).unwrap(),
                retry_after,
                body,
                "@acme/tool@1.0.0",
                NOW,
            )
        };
        assert_eq!(
            read(401, None, &error_body("token revoked")),
            RegistryError::Unauthorized {
                guidance: "token revoked".into()
            }
        );
        assert_eq!(
            read(401, None, ""),
            RegistryError::Unauthorized {
                guidance: "token expired or invalid".into()
            }
        );
        assert_eq!(
            read(403, None, &error_body("token lacks scope @acme")),
            RegistryError::Forbidden {
                guidance: "token lacks scope @acme".into()
            }
        );
        assert_eq!(
            read(404, None, &error_body("gone")),
            RegistryError::NotFound {
                specifier: "@acme/tool@1.0.0".into()
            }
        );
        assert_eq!(
            read(429, Some("60"), &error_body("slow down")),
            RegistryError::RateLimited {
                retry_after_ms: 60_000
            }
        );
        assert_eq!(
            read(429, None, ""),
            RegistryError::RateLimited {
                retry_after_ms: DEFAULT_RETRY_AFTER_MS
            }
        );
        assert_eq!(
            read(500, None, &error_body("database is down")),
            RegistryError::NetworkError {
                message: "database is down".into()
            }
        );
        assert_eq!(
            read(502, None, ""),
            RegistryError::NetworkError {
                message: "registry answered 502 Bad Gateway".into()
            }
        );
    }

    #[specforge_test_macros::test(
        behavior = "authenticate_registry_request",
        verify = "403 response produces E-level diagnostic with permission guidance"
    )]
    fn a_403_is_forbidden_whatever_the_call() {
        // §3 R2: the same 403 was R005 for a version list and R002 for one version.
        let body = error_body("token lacks scope @acme");
        for subject in [
            "@acme/tool",
            "@acme/tool@1.0.0",
            "http://r/v1/x/download",
            "q",
        ] {
            let error = failure(StatusCode::FORBIDDEN, None, &body, subject, NOW);
            assert_eq!(
                error,
                RegistryError::Forbidden {
                    guidance: "token lacks scope @acme".into()
                }
            );
            let diagnostic = error.to_diagnostic();
            assert_eq!(diagnostic.code, "R002");
            assert!(diagnostic.message.contains("token lacks scope @acme"));
        }
    }

    // bug: plan 12 §3 R4, flipped by T3
    #[test]
    fn a_registry_refusal_reads_as_a_network_error_today() {
        let body = serde_json::to_string(&ErrorBody::new(
            specforge_registry_wire::code::UNSIGNED_PACKAGE,
            "packages must be signed",
        ))
        .unwrap();
        let error = failure(
            StatusCode::BAD_REQUEST,
            None,
            &body,
            "@acme/tool@1.0.0",
            NOW,
        );
        assert_eq!(
            error,
            RegistryError::NetworkError {
                message: "packages must be signed".into()
            }
        );
        let diagnostic = error.to_diagnostic();
        assert_eq!(diagnostic.code, "R005");
        assert!(diagnostic.suggestion.unwrap().contains("network"));
    }

    #[test]
    fn retry_after_delta_seconds() {
        let now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
        assert_eq!(parse_retry_after_ms(Some("120"), now), 120_000);
        assert_eq!(parse_retry_after_ms(Some(" 30 "), now), 30_000);
        assert_eq!(parse_retry_after_ms(Some("0"), now), 0);
    }

    #[test]
    fn retry_after_http_date() {
        let now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
        let future = now + std::time::Duration::from_secs(90);
        let past = now - std::time::Duration::from_secs(90);
        assert_eq!(
            parse_retry_after_ms(Some(&httpdate::fmt_http_date(future)), now),
            90_000
        );
        assert_eq!(
            parse_retry_after_ms(Some(&httpdate::fmt_http_date(past)), now),
            0
        );
    }

    #[test]
    fn retry_after_absent_or_malformed_falls_back_to_default() {
        let now = SystemTime::UNIX_EPOCH;
        assert_eq!(parse_retry_after_ms(None, now), DEFAULT_RETRY_AFTER_MS);
        assert_eq!(
            parse_retry_after_ms(Some("soon"), now),
            DEFAULT_RETRY_AFTER_MS
        );
    }
}
