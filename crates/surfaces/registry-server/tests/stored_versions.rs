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
    let status = router.oneshot(request).await.unwrap().status();
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
}
