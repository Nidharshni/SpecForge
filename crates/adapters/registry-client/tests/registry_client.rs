use specforge_common::Severity;
use specforge_registry_client::registry_client::{RegistryClient, RegistryError};
use specforge_registry_client::registry_config::{RegistryConfig, RegistryCredential};
use specforge_registry_client::testing::{CallKind, MemoryClient};
use specforge_registry_wire::RegistryRoot;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn test_registry() -> RegistryConfig {
    RegistryConfig {
        alias: "test".to_string(),
        url: RegistryRoot::parse("https://registry.specforge.dev").unwrap(),
        scope_filter: None,
        default_registry: true,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

// B:auth-forbidden — verify unit "403 produces E-level diagnostic with permission guidance"
#[test]
fn auth_403_produces_permission_error() {
    let client = MemoryClient::new().accepting("some-token");
    client.fail_next(
        CallKind::Authenticate,
        None,
        RegistryError::Forbidden {
            guidance: "insufficient scope".into(),
        },
    );

    let cred = RegistryCredential::new(test_registry().url, "some-token");
    let err = client
        .authenticate(&test_registry(), &cred)
        .unwrap_err()
        .to_diagnostic();
    assert_eq!(err.severity, Severity::Error);
    assert_eq!(err.code, "R002");
    assert!(err.message.contains("forbidden"));
}

// B:auth-sanitization — verify unit "error diagnostics never contain raw token"
#[test]
fn diagnostics_never_contain_raw_token() {
    let raw_token = "super-secret-token-value";
    let err = RegistryError::Unauthorized {
        guidance: "invalid credentials".into(),
    };
    let diag = err.to_diagnostic();
    assert!(!diag.message.contains(raw_token));
    assert!(
        !diag
            .suggestion
            .as_ref()
            .is_some_and(|s| s.contains(raw_token))
    );
}

// B:registry-timeout — verify unit "timeout error produces diagnostic"
#[test]
fn timeout_error_produces_diagnostic() {
    let err = RegistryError::Timeout {
        url: "https://registry.specforge.dev/pkg".into(),
    };
    let diag = err.to_diagnostic();
    assert_eq!(diag.severity, Severity::Error);
    assert_eq!(diag.code, "R004");
    assert!(diag.message.contains("timed out"));
    assert!(diag.message.contains("registry.specforge.dev"));
}

// B:registry-error-to-diagnostic — verify unit "all error variants convert to diagnostics"
#[test]
fn all_registry_errors_convert_to_diagnostics() {
    let errors: Vec<RegistryError> = vec![
        RegistryError::Unauthorized {
            guidance: "g".into(),
        },
        RegistryError::Forbidden {
            guidance: "g".into(),
        },
        RegistryError::RateLimited {
            retry_after_ms: 5000,
        },
        RegistryError::Timeout {
            url: "https://x".into(),
        },
        RegistryError::NetworkError {
            message: "m".into(),
        },
        RegistryError::NotFound {
            specifier: "s".into(),
        },
        RegistryError::DuplicateVersion {
            name: "n".into(),
            version: "v".into(),
        },
        RegistryError::Refused {
            code: specforge_registry_wire::ErrorCode::BadRequest,
            message: "m".into(),
        },
        RegistryError::InvalidReply {
            subject: "s".into(),
            message: "m".into(),
        },
    ];

    let codes = [
        "R001", "R002", "R003", "R004", "R005", "R006", "R007", "R008", "R009",
    ];

    for (err, expected_code) in errors.into_iter().zip(codes.iter()) {
        let diag: specforge_common::Diagnostic = err.into();
        assert_eq!(&diag.code, expected_code, "wrong code for {expected_code}");
    }
}

#[specforge_test_macros::test(
    behavior = "resolve_registry_source",
    verify = "network error produces ExtensionError with retry guidance"
)]
fn a_network_error_suggests_a_retry() {
    let network = RegistryError::NetworkError {
        message: "connection refused".into(),
    }
    .to_diagnostic();
    assert_eq!(network.code, "R005");
    assert_eq!(network.severity, Severity::Error);
    assert!(network.message.contains("connection refused"));
    assert!(network.suggestion.as_ref().unwrap().contains("retry"));

    let timeout = RegistryError::Timeout {
        url: "http://registry.invalid/v1/packages/@acme%2Ftool".into(),
    }
    .to_diagnostic();
    assert_eq!(timeout.code, "R004");
    assert_eq!(timeout.severity, Severity::Error);
    assert!(timeout.suggestion.as_ref().unwrap().contains("retry"));
}
