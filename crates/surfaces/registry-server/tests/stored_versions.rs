//! A download is the stored version it names (ADR 0192).
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use specforge_registry_server::state::{AppState, PublishLimits};
use specforge_registry_server::{handlers, testing::issue_token as issue};
use specforge_registry_wire::{SignedContent, form};
use std::sync::Arc;
use tower::ServiceExt as _;

const MANIFEST: &str = r#"{"handshake":{"protocol_version":"1","name":"@test/signed-ext","version":"1.0.0","peer_dependencies":[],"sandbox_policy":null}}"#;
const WASM: &[u8] = b"\0asm-fake-extension-bytes";

fn app_state(dir: &std::path::Path) -> Arc<AppState> {
    Arc::new(
        AppState::open(
            dir,
            PublishLimits {
                per_token: 100,
                per_ip: 10_000,
                window: std::time::Duration::from_secs(60),
            },
            specforge_registry_server::state::ReadAccess::Public,
        )
        .expect("open registry"),
    )
}

fn signature() -> String {
    specforge_registry_client::SigningKey::from_bytes(&[7; 32])
        .sign(&SignedContent::of(
            "@test/signed-ext",
            "1.0.0",
            WASM,
            MANIFEST,
        ))
        .to_json()
}

async fn publish(router: &axum::Router, token: &str) -> StatusCode {
    let request = Request::builder()
        .method("PUT")
        .uri("/v1/packages/@test%2Fsigned-ext/1.0.0")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", form::content_type("testboundary123"))
        .body(Body::from(form::body(
            "testboundary123",
            MANIFEST,
            WASM,
            Some(&signature()),
        )))
        .unwrap();
    router.clone().oneshot(request).await.unwrap().status()
}

/// The status of a GET of `uri`.
async fn status_of(router: &axum::Router, uri: &str) -> StatusCode {
    let request = Request::builder().uri(uri).body(Body::empty()).unwrap();
    router.clone().oneshot(request).await.unwrap().status()
}

#[specforge_test_macros::test(
    behavior = "publish_to_registry",
    verify = "the registry refuses a name or version that is not a package name or version"
)]
#[tokio::test]
async fn a_download_is_the_stored_version_and_nothing_else() {
    let dir = tempfile::tempdir().unwrap();
    let state = app_state(&dir.path().join("data"));
    // A binary beside the registry's data directory, which no version names.
    std::fs::write(dir.path().join("outside.wasm"), b"\0asm-outside").unwrap();
    let token = issue(&state, "pub", "pub", false);
    let router = handlers::router(Arc::clone(&state));
    assert_eq!(publish(&router, &token).await, StatusCode::CREATED);
    let download = "/v1/packages/@test%2Fsigned-ext/1.0.0/download";
    assert_eq!(status_of(&router, download).await, StatusCode::OK);

    for version in [
        "..%2F..%2Foutside",
        "%2Ftmp%2Foutside",
        "1.0.0%2F..%2F1.0.0",
        "1.x",
    ] {
        let uri = format!("/v1/packages/@test%2Fsigned-ext/{version}/download");
        assert_eq!(
            status_of(&router, &uri).await,
            StatusCode::NOT_FOUND,
            "{uri}"
        );
    }
    // A name no version was published under has nothing to serve, whatever the version says.
    let uri = "/v1/packages/@nobody%2Fnothing/..%2F..%2Foutside/download";
    assert_eq!(
        status_of(&router, uri).await,
        StatusCode::NOT_FOUND,
        "{uri}"
    );

    // A blob that no longer hashes to its record is never served.
    let package_dir = std::fs::read_dir(dir.path().join("data/packages"))
        .unwrap()
        .flatten()
        .next()
        .expect("the published package")
        .path();
    let blob = package_dir.join("1.0.0.wasm");
    std::fs::write(&blob, b"\0asm-tampered").unwrap();
    assert_eq!(
        status_of(&router, download).await,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    std::fs::write(&blob, WASM).unwrap();
    assert_eq!(status_of(&router, download).await, StatusCode::OK);

    // A yanked version is taken down: its binary goes with its metadata.
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
    assert_eq!(status_of(&router, download).await, StatusCode::NOT_FOUND);
}

#[specforge_test_macros::test(
    behavior = "publish_to_registry",
    verify = "the registry refuses a name or version that is not a package name or version"
)]
#[tokio::test]
async fn an_upload_past_the_body_limit_is_too_large() {
    let dir = tempfile::tempdir().unwrap();
    let state = app_state(dir.path());
    let token = issue(&state, "pub", "pub", false);
    let router = handlers::router(Arc::clone(&state));
    let mut wasm = b"\0asm".to_vec();
    wasm.resize(65 * 1024 * 1024, 0);
    let request = Request::builder()
        .method("PUT")
        .uri("/v1/packages/@test%2Fsigned-ext/1.0.0")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", form::content_type("testboundary123"))
        .body(Body::from(form::body(
            "testboundary123",
            MANIFEST,
            &wasm,
            Some(&signature()),
        )))
        .unwrap();
    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        json_of(response).await["error"]["code"],
        "PAYLOAD_TOO_LARGE"
    );
}

async fn json_of(response: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn send(router: &axum::Router, method: &str, uri: &str, token: &str) -> StatusCode {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    router.clone().oneshot(request).await.unwrap().status()
}

const SEGMENTS: [&str; 5] = [
    "..%2F..%2Foutside",
    "%2Ftmp%2Foutside",
    "1.0.0%2F..%2F1.0.0",
    "1.x",
    "latest",
];

#[specforge_test_macros::test(
    behavior = "publish_to_registry",
    verify = "the registry refuses a name or version that is not a package name or version"
)]
#[tokio::test]
async fn a_segment_that_is_no_version_is_not_found_in_every_route() {
    let dir = tempfile::tempdir().unwrap();
    let state = app_state(dir.path());
    let token = issue(&state, "pub", "pub", false);
    let router = handlers::router(Arc::clone(&state));
    assert_eq!(publish(&router, &token).await, StatusCode::CREATED);

    for segment in SEGMENTS {
        let base = format!("/v1/packages/@test%2Fsigned-ext/{segment}");
        assert_eq!(
            status_of(&router, &base).await,
            StatusCode::NOT_FOUND,
            "{base}"
        );
        let download = format!("{base}/download");
        assert_eq!(
            status_of(&router, &download).await,
            StatusCode::NOT_FOUND,
            "{download}"
        );
        assert_eq!(
            send(&router, "DELETE", &base, &token).await,
            StatusCode::NOT_FOUND,
            "yank {base}"
        );
    }
    // The versions list of a name that is not a package.
    for name in ["..%2F..", "tool", "%2Ftmp"] {
        let uri = format!("/v1/packages/{name}");
        assert_eq!(
            status_of(&router, &uri).await,
            StatusCode::NOT_FOUND,
            "{uri}"
        );
    }
    // Nothing above took the version down.
    assert_eq!(
        status_of(&router, "/v1/packages/@test%2Fsigned-ext/1.0.0/download").await,
        StatusCode::OK
    );
}

#[specforge_test_macros::test(
    behavior = "publish_to_registry",
    verify = "the registry refuses a name or version that is not a package name or version"
)]
#[tokio::test]
async fn a_yank_of_a_missing_version_is_not_found_and_a_yanked_one_is_gone_everywhere() {
    let dir = tempfile::tempdir().unwrap();
    let state = app_state(dir.path());
    let token = issue(&state, "pub", "pub", false);
    let router = handlers::router(Arc::clone(&state));
    assert_eq!(publish(&router, &token).await, StatusCode::CREATED);
    let version = "/v1/packages/@test%2Fsigned-ext/1.0.0";

    let missing = "/v1/packages/@test%2Fsigned-ext/9.9.9";
    assert_eq!(
        send(&router, "DELETE", missing, &token).await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(status_of(&router, version).await, StatusCode::OK);

    assert_eq!(
        send(&router, "DELETE", version, &token).await,
        StatusCode::OK
    );
    assert_eq!(status_of(&router, version).await, StatusCode::NOT_FOUND);
    assert_eq!(
        status_of(&router, &format!("{version}/download")).await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        status_of(&router, "/v1/packages/@test%2Fsigned-ext").await,
        StatusCode::NOT_FOUND,
        "a package with no fetchable version lists none"
    );
}

#[specforge_test_macros::test(
    behavior = "publish_to_registry",
    verify = "the registry refuses a name or version that is not a package name or version"
)]
#[tokio::test]
async fn a_blob_that_misses_its_digest_is_an_integrity_violation() {
    let dir = tempfile::tempdir().unwrap();
    let state = app_state(dir.path());
    let token = issue(&state, "pub", "pub", false);
    let router = handlers::router(Arc::clone(&state));
    assert_eq!(publish(&router, &token).await, StatusCode::CREATED);
    let blob = std::fs::read_dir(dir.path().join("packages"))
        .unwrap()
        .flatten()
        .next()
        .unwrap()
        .path()
        .join("1.0.0.wasm");
    std::fs::write(&blob, b"\0asm-tampered").unwrap();

    let request = Request::builder()
        .uri("/v1/packages/@test%2Fsigned-ext/1.0.0/download")
        .body(Body::empty())
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        json_of(response).await["error"]["code"],
        "INTEGRITY_VIOLATION"
    );
}
