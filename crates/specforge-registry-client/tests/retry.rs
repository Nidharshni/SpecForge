//! `Retrying`: a rate-limited call is sent again by the spec's backoff (ADR 0044 as amended by plan 16).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use specforge_package::PackageName;
use specforge_registry_client::Retrying;
use specforge_registry_client::registry_client::{RegistryClient, RegistryError};
use specforge_registry_client::registry_config::RegistryConfig;
use specforge_registry_client::testing::{CallKind, MemoryClient, package};
use specforge_registry_wire::SearchQuery;

fn registry() -> RegistryConfig {
    RegistryConfig {
        alias: "local".to_string(),
        url: "memory://local".to_string(),
        scope_filter: None,
        default_registry: true,
    }
}

fn name() -> PackageName {
    PackageName::parse("@acme/x").unwrap()
}

/// A client holding `@acme/x` 1.0.0, and a `Retrying` over it that records its waits.
fn retrying() -> (
    MemoryClient,
    Retrying<MemoryClient>,
    Arc<Mutex<Vec<Duration>>>,
) {
    let client = MemoryClient::new();
    let manifest = r#"{"handshake":{"protocol_version":"1.0.0","name":"@acme/x","version":"1.0.0","peer_dependencies":[],"sandbox_policy":null}}"#;
    client.store(
        &registry(),
        package("@acme/x", "1.0.0", b"\0asm", manifest, None),
        b"\0asm".to_vec(),
    );
    let waits = Arc::new(Mutex::new(Vec::new()));
    let record = Arc::clone(&waits);
    let retrying =
        Retrying::new(client.clone()).with_sleep(move |d| record.lock().unwrap().push(d));
    (client, retrying, waits)
}

fn rate_limited(client: &MemoryClient, kind: CallKind, times: usize, retry_after_ms: u64) {
    for _ in 0..times {
        client.fail_next(kind, None, RegistryError::RateLimited { retry_after_ms });
    }
}

fn count(client: &MemoryClient, kind: CallKind) -> usize {
    client.calls().iter().filter(|c| c.kind == kind).count()
}

fn secs(waits: &Arc<Mutex<Vec<Duration>>>) -> Vec<u64> {
    waits.lock().unwrap().iter().map(|d| d.as_secs()).collect()
}

#[specforge_test_macros::test(
    behavior = "retry_registry_request",
    verify = "429 response retries with exponential backoff"
)]
fn a_rate_limited_call_is_sent_again_after_the_backoff() {
    let (client, retrying, waits) = retrying();
    rate_limited(&client, CallKind::Versions, 2, 0);

    let versions = retrying.versions(&name(), &registry(), None).unwrap();

    assert_eq!(versions, ["1.0.0"]);
    assert_eq!(secs(&waits), [1, 2]);
    assert_eq!(count(&client, CallKind::Versions), 3);
}

#[specforge_test_macros::test(
    behavior = "retry_registry_request",
    verify = "max retries exceeded produces final error"
)]
fn three_retries_then_the_last_rate_limit_is_the_answer() {
    let (client, retrying, waits) = retrying();
    rate_limited(&client, CallKind::Versions, 4, 0);

    let error = retrying.versions(&name(), &registry(), None).unwrap_err();

    assert!(
        matches!(error, RegistryError::RateLimited { .. }),
        "{error:?}"
    );
    assert_eq!(secs(&waits), [1, 2, 4]);
    assert_eq!(count(&client, CallKind::Versions), 4);
}

#[specforge_test_macros::test(
    behavior = "retry_registry_request",
    verify = "a Retry-After longer than the longest backoff is not waited for"
)]
fn a_wait_longer_than_the_longest_backoff_is_not_taken() {
    let (client, retrying, waits) = retrying();
    rate_limited(&client, CallKind::Versions, 1, 60_000);

    let error = retrying.versions(&name(), &registry(), None).unwrap_err();

    assert_eq!(
        error,
        RegistryError::RateLimited {
            retry_after_ms: 60_000
        }
    );
    assert!(waits.lock().unwrap().is_empty());
    assert_eq!(count(&client, CallKind::Versions), 1);
}

#[test]
fn a_retry_waits_what_the_registry_asks_when_longer() {
    let (client, retrying, waits) = retrying();
    rate_limited(&client, CallKind::Versions, 1, 5_000);

    retrying.versions(&name(), &registry(), None).unwrap();

    assert_eq!(secs(&waits), [5]);
}

#[test]
fn every_call_is_retried_alike() {
    let (client, retrying, _) = retrying();
    let registry = registry();
    let version = specforge_package::Version::parse("1.0.0").unwrap();

    rate_limited(&client, CallKind::Metadata, 1, 0);
    let metadata = retrying
        .metadata(&name(), &version, &registry, None)
        .unwrap();
    assert_eq!(count(&client, CallKind::Metadata), 2);

    rate_limited(&client, CallKind::Download, 1, 0);
    retrying
        .download(&metadata.wasm_url, &registry, None)
        .unwrap();
    assert_eq!(count(&client, CallKind::Download), 2);

    rate_limited(&client, CallKind::Search, 1, 0);
    retrying
        .search(&SearchQuery::new("acme"), &registry, None)
        .unwrap();
    assert_eq!(count(&client, CallKind::Search), 2);
}

#[specforge_test_macros::test(
    behavior = "retry_registry_request",
    verify = "network timeout produces ExtensionError with retry guidance"
)]
fn a_timeout_is_not_retried() {
    let (client, retrying, waits) = retrying();
    client.fail_next(
        CallKind::Versions,
        None,
        RegistryError::Timeout {
            url: "memory://local".to_string(),
        },
    );

    let error = retrying.versions(&name(), &registry(), None).unwrap_err();

    assert!(matches!(error, RegistryError::Timeout { .. }));
    assert!(waits.lock().unwrap().is_empty());
    assert_eq!(count(&client, CallKind::Versions), 1);
    let diagnostic = error.to_diagnostic();
    assert_eq!(diagnostic.code, "R004");
    assert!(diagnostic.suggestion.unwrap().contains("retry"));
}

#[test]
fn other_errors_pass_through() {
    let errors = [
        RegistryError::NotFound {
            specifier: "x".into(),
        },
        RegistryError::Unauthorized {
            guidance: "x".into(),
        },
        RegistryError::Forbidden {
            guidance: "x".into(),
        },
        RegistryError::DuplicateVersion {
            name: "x".into(),
            version: "1.0.0".into(),
        },
    ];
    for error in errors {
        let (client, retrying, waits) = retrying();
        client.fail_next(CallKind::Versions, None, error.clone());
        assert_eq!(
            retrying.versions(&name(), &registry(), None).unwrap_err(),
            error
        );
        assert!(waits.lock().unwrap().is_empty());
        assert_eq!(count(&client, CallKind::Versions), 1);
    }
}
