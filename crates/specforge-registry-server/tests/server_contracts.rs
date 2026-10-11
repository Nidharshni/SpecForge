//! Registry server contracts (spec #21, T3): publish-side rejection rules,
//! rate limiting, and the admin token API.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use specforge_registry_server::state::{AppState, PublishLimits};
use specforge_registry_server::{auth, handlers};
use specforge_registry_wire::{
    ErrorBody, PackageMetadata, PublishReceipt, SearchResults, TokenVerified, VersionList, Yanked,
    form,
};
use std::sync::Arc;
use tower::ServiceExt as _;

/// A package's manifest: its declaration, as `specforge publish` uploads it.
const VALID_MANIFEST: &str = r#"{"handshake":{"protocol_version":"1","name":"@test/signed-ext","version":"1.0.0","peer_dependencies":[],"sandbox_policy":null}}"#;
const WASM: &[u8] = b"\0asm-fake-extension-bytes";

fn app_state(dir: &std::path::Path, publish_limit_per_token: u32) -> Arc<AppState> {
    Arc::new(
        AppState::open(
            dir,
            PublishLimits {
                per_token: publish_limit_per_token,
                per_ip: 10_000,
                window: std::time::Duration::from_secs(60),
            },
            specforge_registry_server::state::ReadAccess::Public,
        )
        .expect("open registry"),
    )
}

fn app(state: Arc<AppState>) -> axum::Router {
    handlers::router(state)
}

fn app_clone(state: &Arc<AppState>) -> axum::Router {
    handlers::router(Arc::clone(state))
}

fn multipart_body(manifest: &str, wasm: &[u8], signature: Option<&str>) -> Body {
    Body::from(form::body("testboundary123", manifest, wasm, signature))
}

fn put_request(token: &str, name: &str, version: &str, body: Body) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(format!(
            "/v1/packages/{}/{}",
            name.replace('/', "%2F"),
            version
        ))
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", form::content_type("testboundary123"))
        .body(body)
        .unwrap()
}

#[tokio::test]
async fn unsigned_publish_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let state = app_state(dir.path(), 100);
    let raw = auth::create_token(&state.database, None, "pub", Some(90), false);

    let response = app(state)
        .oneshot(put_request(
            &raw,
            "@test%2Fsigned-ext",
            "1.0.0",
            multipart_body(VALID_MANIFEST, WASM, None),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["error"]["code"], "UNSIGNED_PACKAGE");
}

#[tokio::test]
async fn name_mismatch_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let state = app_state(dir.path(), 100);
    let raw = auth::create_token(&state.database, None, "pub", Some(90), false);

    // Path says 2.0.0, manifest says 1.0.0.
    let response = app(state)
        .oneshot(put_request(
            &raw,
            "@test%2Fsigned-ext",
            "2.0.0",
            multipart_body(
                VALID_MANIFEST,
                WASM,
                Some(r#"{"sig":"x","keyId":"y","pubkey":"z","signedAt":"now"}"#),
            ),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["error"]["code"], "NAME_MISMATCH");
}

#[tokio::test]
async fn non_wasm_bytes_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let state = app_state(dir.path(), 100);
    let raw = auth::create_token(&state.database, None, "pub", Some(90), false);

    let response = app(state)
        .oneshot(put_request(
            &raw,
            "@test%2Fsigned-ext",
            "1.0.0",
            multipart_body(
                VALID_MANIFEST,
                b"not a wasm module",
                Some(r#"{"sig":"x","keyId":"y","pubkey":"z","signedAt":"now"}"#),
            ),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["error"]["code"], "INVALID_WASM");
}

#[tokio::test]
async fn invalid_manifest_schema_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let state = app_state(dir.path(), 100);
    let raw = auth::create_token(&state.database, None, "pub", Some(90), false);

    // Missing required fields entirely.
    let response = app(state)
        .oneshot(put_request(
            &raw,
            "@test%2Fsigned-ext",
            "1.0.0",
            multipart_body(
                r#"{"description":"no name or version"}"#,
                WASM,
                Some(r#"{"sig":"x","keyId":"y","pubkey":"z","signedAt":"now"}"#),
            ),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["error"]["code"], "INVALID_MANIFEST");
}

#[tokio::test]
async fn publish_rate_limit_returns_429() {
    let dir = tempfile::tempdir().unwrap();
    let state = app_state(dir.path(), 2); // 2 publishes per window per token
    let raw = auth::create_token(&state.database, None, "pub", Some(90), false);
    let router = app(state);

    // The signature object content is irrelevant to the validation contract;
    // dedup prevents version reuse, so vary versions per attempt.
    for version in ["1.0.0", "1.0.1"] {
        let manifest = VALID_MANIFEST.replace("1.0.0", version);
        let response = router
            .clone()
            .oneshot(put_request(
                &raw,
                "@test%2Fsigned-ext",
                version,
                multipart_body(
                    &manifest,
                    WASM,
                    Some(r#"{"sig":"x","keyId":"y","pubkey":"z","signedAt":"now"}"#),
                ),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    // Third publish inside the window: rate limited.
    let manifest = VALID_MANIFEST.replace("1.0.0", "1.0.2");
    let response = router
        .oneshot(put_request(
            &raw,
            "@test%2Fsigned-ext",
            "1.0.2",
            multipart_body(
                &manifest,
                WASM,
                Some(r#"{"sig":"x","keyId":"y","pubkey":"z","signedAt":"now"}"#),
            ),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["error"]["code"], "RATE_LIMITED");
}

#[tokio::test]
async fn admin_api_requires_admin_token_and_manages_lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    let state = app_state(dir.path(), 100);
    let router = app_clone(&state);

    let publisher = auth::create_token(&state.database, None, "publisher", Some(90), false);
    let admin = auth::create_token(&state.database, None, "admin", Some(90), true);

    // Publisher token: forbidden on admin endpoints.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/admin/tokens")
                .header("authorization", format!("Bearer {}", publisher))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // No auth: unauthorized.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/admin/tokens")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // Admin creates a scoped publish token (expires in 90 days by default).
    let create_body = r#"{"scope":"@acme","label":"ci","expires_in_days":30}"#;
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/tokens")
                .header("authorization", format!("Bearer {}", admin))
                .header("content-type", "application/json")
                .body(Body::from(create_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    let new_token = body["token"].as_str().unwrap().to_string();
    let prefix = body["prefix"].as_str().unwrap().to_string();

    // The created token authenticates and may publish within its scope.
    let verify = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/verify")
                .header("authorization", format!("Bearer {}", new_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(verify.status(), StatusCode::OK);

    // Admin revokes it by prefix; the token stops working.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/admin/tokens/{}", prefix))
                .header("authorization", format!("Bearer {}", admin))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let verify = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/verify")
                .header("authorization", format!("Bearer {}", new_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(verify.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// Namespace ownership (T6)
// ---------------------------------------------------------------------------

mod ownership {
    use super::*;

    fn signed_for(name: &str, version: &str) -> axum::body::Body {
        let manifest = format!(
            r#"{{"handshake":{{"protocol_version":"1","name":"{name}","version":"{version}","peer_dependencies":[],"sandbox_policy":null}}}}"#
        );
        multipart_body(
            &manifest,
            WASM,
            Some(r#"{"sig":"aa","keyId":"bb","pubkey":"cc","signedAt":"now"}"#),
        )
    }

    #[tokio::test]
    async fn first_publish_claims_scope_and_blocks_other_tokens() {
        let dir = tempfile::tempdir().unwrap();
        let state = app_state(dir.path(), 10_000);
        let router = app_clone(&state);

        let token_a = auth::create_token(&state.database, None, "pub-a", Some(90), false);
        let token_b = auth::create_token(&state.database, None, "pub-b", Some(90), false);

        // Publisher A claims @acme with @acme/utils.
        let response = router
            .clone()
            .oneshot(put_request(
                &token_a,
                "@acme%2Futils",
                "1.0.0",
                signed_for("@acme/utils", "1.0.0"),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        // Publisher B is locked out of @acme — even for a different name.
        let response = router
            .clone()
            .oneshot(put_request(
                &token_b,
                "@acme%2Fother",
                "1.0.0",
                signed_for("@acme/other", "1.0.0"),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 1_000_000)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body["error"]["code"], "SCOPE_OWNED");

        // Publisher A keeps publishing into @acme.
        let response = router
            .clone()
            .oneshot(put_request(
                &token_a,
                "@acme%2Fother",
                "1.0.0",
                signed_for("@acme/other", "1.0.0"),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        // Metadata carries the registry-assigned publisher account id.
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/packages/%40acme%2Futils/1.0.0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 1_000_000)
                .await
                .unwrap(),
        )
        .unwrap();
        let publisher = body["publisher"].as_str().unwrap();
        assert!(
            publisher.starts_with("acct_"),
            "publisher was {}",
            publisher
        );
    }

    #[tokio::test]
    async fn scoped_token_cannot_publish_outside_its_scope() {
        let dir = tempfile::tempdir().unwrap();
        let state = app_state(dir.path(), 10_000);
        let router = app_clone(&state);

        // @web-scoped token.
        let scoped = auth::create_token(&state.database, Some("@web"), "web-pub", Some(90), false);

        let response = router
            .oneshot(put_request(
                &scoped,
                "@other%2Ftool",
                "1.0.0",
                signed_for("@other/tool", "1.0.0"),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}

/// The body of an answer.
async fn json_of(response: axum::response::Response) -> serde_json::Value {
    serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap()
}

const SIGNATURE: &str = r#"{"sig":"x","keyId":"y","pubkey":"z","signedAt":"now"}"#;

#[specforge_test_macros::test(
    behavior = "publish_to_registry",
    verify = "the registry refuses a manifest that is not an extension declaration"
)]
#[tokio::test]
async fn a_manifest_that_is_not_a_declaration_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let state = app_state(dir.path(), 100);
    let raw = auth::create_token(&state.database, None, "pub", Some(90), false);
    // The camelCase manifest file of before ADR 0012.
    let legacy = r#"{"name":"@test/signed-ext","version":"1.0.0","manifestVersion":2,"wasmPath":"ext.wasm"}"#;
    let response = app(state)
        .oneshot(put_request(
            &raw,
            "@test%2Fsigned-ext",
            "1.0.0",
            multipart_body(legacy, WASM, Some(SIGNATURE)),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_of(response).await;
    assert_eq!(body["error"]["code"], "INVALID_MANIFEST");
    let message = body["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("not an extension declaration"),
        "{message}"
    );
}

#[specforge_test_macros::test(
    behavior = "publish_to_registry",
    verify = "the registry takes a package's description and keywords from its declaration"
)]
#[tokio::test]
async fn description_and_keywords_come_from_the_declaration() {
    let dir = tempfile::tempdir().unwrap();
    let state = app_state(dir.path(), 100);
    let raw = auth::create_token(&state.database, None, "pub", Some(90), false);
    let router = app_clone(&state);
    let manifest = r#"{"handshake":{"protocol_version":"1","name":"@test/signed-ext","version":"1.0.0","peer_dependencies":[],"sandbox_policy":null,"description":"Reports over the graph","keywords":["reports","dashboards"]}}"#;
    let response = router
        .clone()
        .oneshot(put_request(
            &raw,
            "@test%2Fsigned-ext",
            "1.0.0",
            multipart_body(manifest, WASM, Some(SIGNATURE)),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // One query the description answers, one only a keyword does.
    for query in ["graph", "dashboards"] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/search?q={query}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = json_of(response).await;
        let hit = &body["results"][0];
        assert_eq!(hit["name"], "@test/signed-ext", "{query}: {body}");
        assert_eq!(hit["description"], "Reports over the graph");
    }
}

/// The code a publish of `name` at `version` (unsigned, a valid manifest
/// of another name) is answered with: the name and version checks come
/// first, so `INVALID_NAME` and `INVALID_VERSION` are theirs.
async fn publish_code(name: &str, version: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let state = app_state(dir.path(), 100);
    let raw = auth::create_token(&state.database, None, "pub", Some(90), false);
    let response = app(state)
        .oneshot(put_request(
            &raw,
            name,
            version,
            multipart_body(VALID_MANIFEST, WASM, None),
        ))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "{name}@{version}"
    );
    let body = json_of(response).await;
    body["error"]["code"].as_str().unwrap().to_string()
}

/// What `publish_package` makes of a name and a version (plan 12 §3 R5): a
/// name the package module refuses never reaches the signature check.
#[specforge_test_macros::test(
    behavior = "publish_to_registry",
    verify = "the registry refuses a name or version that is not a package name or version"
)]
#[tokio::test]
async fn publish_refuses_what_is_not_a_package() {
    // Past the name check: the signature is the next refusal.
    assert_eq!(
        publish_code("@a%2Fx", "1.0.0").await,
        "UNSIGNED_PACKAGE",
        "@a/x"
    );
    for name in [
        "@acme%2F..",
        "@acme%2FT%20ool",
        "@acme%2Ftool@",
        "tool",
        "@scope",
        "@a%2Fb%2Fc",
    ] {
        assert_eq!(publish_code(name, "1.0.0").await, "INVALID_NAME", "{name}");
    }
    for version in ["1.x", "1.2"] {
        assert_eq!(
            publish_code("@test%2Fsigned-ext", version).await,
            "INVALID_VERSION",
            "{version}"
        );
    }
    // Build metadata is a version.
    assert_ne!(
        publish_code("@test%2Fsigned-ext", "2.0.0+build.1").await,
        "INVALID_VERSION"
    );
}

#[specforge_test_macros::test(
    behavior = "publish_to_registry",
    verify = "the registry refuses a name or version that is not a package name or version"
)]
#[tokio::test]
async fn a_read_of_a_name_that_is_not_one_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let state = app_state(dir.path(), 100);
    for uri in [
        "/v1/packages/@acme%2F..",
        "/v1/packages/@acme%2F../1.0.0",
        "/v1/packages/@acme%2F../1.0.0/download",
    ] {
        let response = app_clone(&state)
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
    }
}

/// `value` reads as a `T` and writes back as the same JSON: the wire type is the body.
fn round_trips<T: serde::Serialize + serde::de::DeserializeOwned>(value: &serde_json::Value) {
    let typed: T = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(&serde_json::to_value(typed).unwrap(), value);
}

/// The sorted keys of a JSON object.
fn keys_of(value: &serde_json::Value) -> Vec<String> {
    let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    keys
}

async fn get_json(router: &axum::Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    (status, json_of(response).await)
}

#[specforge_test_macros::test(
    behavior = "resolve_registry_source",
    verify = "the registry server answers every call in the JSON its client reads"
)]
#[tokio::test]
async fn the_server_answers_in_these_json_shapes() {
    let dir = tempfile::tempdir().unwrap();
    let state = app_state(dir.path(), 100);
    let token = auth::create_token(&state.database, None, "pub", Some(90), false);
    let router = app_clone(&state);

    // publish: 201 and the receipt.
    let response = router
        .clone()
        .oneshot(put_request(
            &token,
            "@test%2Fsigned-ext",
            "1.0.0",
            multipart_body(VALID_MANIFEST, WASM, Some(SIGNATURE)),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let receipt = json_of(response).await;
    assert_eq!(
        keys_of(&receipt),
        ["key_id", "name", "sha256", "size_bytes", "version"]
    );
    round_trips::<PublishReceipt>(&receipt);

    // the version list.
    let (status, list) = get_json(&router, "/v1/packages/@test%2Fsigned-ext").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(keys_of(&list), ["name", "versions"]);
    assert_eq!(list["versions"], serde_json::json!(["1.0.0"]));
    round_trips::<VersionList>(&list);

    // one version's metadata.
    let (status, metadata) = get_json(&router, "/v1/packages/@test%2Fsigned-ext/1.0.0").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        keys_of(&metadata),
        [
            "description",
            "key_id",
            "keywords",
            "manifest",
            "name",
            "published_at",
            "publisher",
            "sha256",
            "signature",
            "size_bytes",
            "version",
            "wasm_url"
        ]
    );
    assert_eq!(
        metadata["wasm_url"],
        "/packages/@test%2Fsigned-ext/1.0.0/download"
    );
    round_trips::<PackageMetadata>(&metadata);

    // search.
    let (status, search) = get_json(&router, "/v1/search?q=signed").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(keys_of(&search), ["results"]);
    let hits = search["results"].as_array().unwrap();
    assert_eq!(hits.len(), 1, "{search}");
    assert_eq!(keys_of(&hits[0]), ["description", "name", "version"]);
    round_trips::<SearchResults>(&search);

    // token check.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/verify")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let verified = json_of(response).await;
    assert_eq!(
        keys_of(&verified),
        ["expires_at", "label", "scope", "valid"]
    );
    round_trips::<TokenVerified>(&verified);

    // an error.
    let (status, missing) = get_json(&router, "/v1/packages/@test%2Fnone").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(keys_of(&missing), ["error"]);
    assert_eq!(keys_of(&missing["error"]), ["code", "message"]);
    assert_eq!(missing["error"]["code"], "NOT_FOUND");
    round_trips::<ErrorBody>(&missing);

    // yank.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/v1/packages/@test%2Fsigned-ext/1.0.0")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let yanked = json_of(response).await;
    assert_eq!(yanked, serde_json::json!({"yanked": true}));
    round_trips::<Yanked>(&yanked);
}

/// The answer to a publish of `@test/signed-ext` at `version` with its manifest, `wasm` and `signature`.
async fn publish_signed_ext(
    router: &axum::Router,
    token: &str,
    version: &str,
    manifest: &str,
    wasm: &[u8],
    signature: &str,
) -> (StatusCode, serde_json::Value) {
    let response = router
        .clone()
        .oneshot(put_request(
            token,
            "@test%2Fsigned-ext",
            version,
            multipart_body(manifest, wasm, Some(signature)),
        ))
        .await
        .unwrap();
    let status = response.status();
    (status, json_of(response).await)
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

// bug: plan 12 §3 R1, flipped by T4
#[tokio::test]
async fn signatures_that_are_not_signatures_are_stored_today() {
    let dir = tempfile::tempdir().unwrap();
    let state = app_state(dir.path(), 100);
    let token = auth::create_token(&state.database, None, "pub", Some(90), false);
    let router = app_clone(&state);
    for (version, signature, key_id) in [
        (
            "1.0.0",
            r#"{"sig":"x","keyId":"y","pubkey":"z","signedAt":"now"}"#,
            "y",
        ),
        ("1.0.1", "hello", ""),
        ("1.0.2", r#"{"sig":"aa"}"#, ""),
    ] {
        let manifest = VALID_MANIFEST.replace("1.0.0", version);
        let (status, receipt) =
            publish_signed_ext(&router, &token, version, &manifest, WASM, signature).await;
        assert_eq!(status, StatusCode::CREATED, "{version}: {receipt}");
        assert_eq!(receipt["key_id"], key_id, "{version}");
    }
}

// bug: plan 12 §3 R2, flipped by T4
#[tokio::test]
async fn a_signature_over_other_bytes_is_stored_today() {
    let dir = tempfile::tempdir().unwrap();
    let state = app_state(dir.path(), 100);
    let token = auth::create_token(&state.database, None, "pub", Some(90), false);
    let key = specforge_registry_client::SigningKey::from_bytes(&[7; 32]);
    let signature = key.sign_package(
        "@test/signed-ext",
        "1.0.0",
        &sha256_hex(b"\0asm-other"),
        &sha256_hex(VALID_MANIFEST.as_bytes()),
        "2026-10-11T00:00:00+00:00",
    );
    let (status, receipt) = publish_signed_ext(
        &app_clone(&state),
        &token,
        "1.0.0",
        VALID_MANIFEST,
        WASM,
        &serde_json::to_string(&signature).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{receipt}");
}

// bug: plan 12 §3 R2, flipped by T4
#[tokio::test]
async fn a_signature_of_another_version_is_stored_today() {
    let dir = tempfile::tempdir().unwrap();
    let state = app_state(dir.path(), 100);
    let token = auth::create_token(&state.database, None, "pub", Some(90), false);
    let key = specforge_registry_client::SigningKey::from_bytes(&[7; 32]);
    // Made for 1.0.0, replayed with the 1.0.1 manifest at 1.0.1.
    let signature = key.sign_package(
        "@test/signed-ext",
        "1.0.0",
        &sha256_hex(WASM),
        &sha256_hex(VALID_MANIFEST.as_bytes()),
        "2026-10-11T00:00:00+00:00",
    );
    let (status, receipt) = publish_signed_ext(
        &app_clone(&state),
        &token,
        "1.0.1",
        &VALID_MANIFEST.replace("1.0.0", "1.0.1"),
        WASM,
        &serde_json::to_string(&signature).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{receipt}");
}

#[tokio::test]
async fn a_private_registry_refuses_anonymous_reads() {
    let dir = tempfile::tempdir().unwrap();
    let state = Arc::new(
        AppState::open(
            dir.path(),
            PublishLimits {
                per_token: 100,
                per_ip: 100,
                window: std::time::Duration::from_secs(60),
            },
            specforge_registry_server::state::ReadAccess::Token,
        )
        .unwrap(),
    );
    let raw = auth::create_token(&state.database, None, "reader", Some(90), false);
    let reads = [
        "/v1/packages/%40acme%2Fx",
        "/v1/packages/%40acme%2Fx/1.0.0",
        "/v1/packages/%40acme%2Fx/1.0.0/download",
        "/v1/search?q=x",
    ];
    for uri in reads {
        let anonymous = app(Arc::clone(&state))
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED, "{uri}");
        let body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(anonymous.into_body(), 1_000_000)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body["error"]["code"], "UNAUTHORIZED", "{uri}");

        let authorized = app(Arc::clone(&state))
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .header("authorization", format!("Bearer {raw}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(authorized.status(), StatusCode::UNAUTHORIZED, "{uri}");
    }
}

fn stored(state: &AppState, name: &str, declares_entities: bool) {
    let entities = if declares_entities {
        serde_json::json!([{ "name": "thing" }])
    } else {
        serde_json::json!([])
    };
    let manifest = serde_json::json!({
        "handshake": {
            "protocol_version": "1.0.0",
            "name": name,
            "version": "1.0.0",
            "peer_dependencies": [],
            "sandbox_policy": null
        },
        "entities": entities,
    })
    .to_string();
    state
        .database
        .insert_package(&specforge_registry_server::db::PackageVersion {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            sha256: "00".to_string(),
            size_bytes: 1,
            description: String::new(),
            keywords: String::new(),
            publisher: String::new(),
            published_at: "2026-01-01T00:00:00+00:00".to_string(),
            signature: String::new(),
            key_id: String::new(),
            manifest,
        })
        .unwrap();
}

#[specforge_test_macros::test(
    behavior = "search_registry",
    verify = "filters by declared category"
)]
#[tokio::test]
async fn search_filters_by_declared_category() {
    let dir = tempfile::tempdir().unwrap();
    let state = app_state(dir.path(), 100);
    stored(&state, "@acme/with", true);
    stored(&state, "@acme/without", false);
    let get = |uri: &'static str| {
        let state = Arc::clone(&state);
        async move {
            let response = app(state)
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            let status = response.status();
            let body: serde_json::Value = serde_json::from_slice(
                &axum::body::to_bytes(response.into_body(), 1_000_000)
                    .await
                    .unwrap(),
            )
            .unwrap();
            (status, body)
        }
    };

    let (status, body) = get("/v1/search?q=&contributes=entities").await;
    assert_eq!(status, StatusCode::OK);
    let names: Vec<&str> = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["@acme/with"]);

    let (status, body) = get("/v1/search?q=").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["results"].as_array().unwrap().len(), 2);

    let (status, body) = get("/v1/search?q=&contributes=widgets").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("surfaces"),
        "{body}"
    );
}
