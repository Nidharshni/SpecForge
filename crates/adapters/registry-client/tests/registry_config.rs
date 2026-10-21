use specforge_common::Severity;
use specforge_package::Scope;
use specforge_registry_client::registry_config::{RegistryConfig, parse_registries_from_config};

// B:configure_registries — verify unit "RegistryConfig deserializes from JSON"
#[test]
fn registry_config_deserializes_from_json() {
    let json = r#"{
        "alias": "main",
        "url": "https://registry.example.com",
        "scope_filter": "@specforge",
        "default_registry": true
    }"#;
    let config: RegistryConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.alias, "main");
    assert_eq!(config.url.as_str(), "https://registry.example.com");
    assert_eq!(
        config.scope_filter.as_ref().map(Scope::as_str),
        Some("@specforge")
    );
    assert!(config.default_registry);
}

// B:configure_registries — verify unit "RegistryConfig deserializes with defaults for optional fields"
#[test]
fn registry_config_deserializes_with_defaults() {
    let json = r#"{
        "alias": "minimal",
        "url": "https://minimal.example.com"
    }"#;
    let config: RegistryConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.alias, "minimal");
    assert_eq!(config.url.as_str(), "https://minimal.example.com");
    assert_eq!(config.scope_filter, None);
    assert!(!config.default_registry);
}

// B:configure_registries — verify unit "parse_registries_from_config extracts registries array"
#[test]
fn parse_registries_extracts_array() {
    let json = r#"{
        "registries": [
            {
                "alias": "primary",
                "url": "https://primary.example.com",
                "default_registry": true
            },
            {
                "alias": "secondary",
                "url": "https://secondary.example.com"
            }
        ]
    }"#;
    let (registries, diagnostics) = parse_registries_from_config(json);
    assert_eq!(registries.len(), 2);
    assert_eq!(registries[0].alias, "primary");
    assert_eq!(registries[1].alias, "secondary");
    // No error diagnostics expected (there is a default)
    assert!(
        diagnostics.iter().all(|d| d.severity != Severity::Error),
        "unexpected error diagnostics: {diagnostics:?}"
    );
}

// B:configure_registries — verify unit "empty registries with no default produces I003 info diagnostic"
#[test]
fn empty_registries_no_default_produces_i003() {
    let json = r#"{ "registries": [] }"#;
    let (registries, diagnostics) = parse_registries_from_config(json);
    assert!(registries.is_empty());
    let i003 = diagnostics
        .iter()
        .find(|d| d.code == "I003")
        .expect("expected I003 diagnostic");
    assert_eq!(i003.severity, Severity::Info);
    assert!(i003.message.contains("No registries configured"));
}

// B:configure_registries — verify unit "registries present but no default_registry true produces I003"
#[test]
fn registries_without_default_produces_i003() {
    let json = r#"{
        "registries": [
            { "alias": "a", "url": "https://a.example.com" }
        ]
    }"#;
    let (registries, diagnostics) = parse_registries_from_config(json);
    assert_eq!(registries.len(), 1);
    let i003 = diagnostics
        .iter()
        .find(|d| d.code == "I003")
        .expect("expected I003 diagnostic");
    assert_eq!(i003.severity, Severity::Info);
    assert!(i003.message.contains("No registry is the default"));
}

// B:configure_registries — verify unit "duplicate alias produces W-level diagnostic"
#[test]
fn duplicate_alias_produces_warning() {
    let json = r#"{
        "registries": [
            { "alias": "dup", "url": "https://first.example.com", "default_registry": true },
            { "alias": "dup", "url": "https://second.example.com" }
        ]
    }"#;
    let (registries, diagnostics) = parse_registries_from_config(json);
    assert_eq!(registries.len(), 2);
    let warning = diagnostics
        .iter()
        .find(|d| d.code == "W140")
        .expect("expected W140 diagnostic");
    assert_eq!(warning.severity, Severity::Warning);
    assert!(warning.message.contains("Duplicate registry alias"));
    assert!(warning.message.contains("dup"));
}

// B:configure_registries — verify unit "no hardcoded URLs - all from config"
#[test]
fn no_hardcoded_urls_all_from_config() {
    // Parse with custom URLs only - verify the returned registries use exactly those URLs
    let json = r#"{
        "registries": [
            {
                "alias": "custom",
                "url": "https://my-private-registry.example.com",
                "default_registry": true
            }
        ]
    }"#;
    let (registries, _) = parse_registries_from_config(json);
    assert_eq!(registries.len(), 1);
    assert_eq!(
        registries[0].url.as_str(),
        "https://my-private-registry.example.com"
    );

    // Parse empty config - no URLs should appear
    let empty_json = r#"{ "registries": [] }"#;
    let (empty_registries, _) = parse_registries_from_config(empty_json);
    assert!(empty_registries.is_empty());

    // No missing "registries" key - still no hardcoded fallback
    let no_key_json = r#"{}"#;
    let (no_key_registries, _) = parse_registries_from_config(no_key_json);
    assert!(no_key_registries.is_empty());
}

// B:configure_registries — verify unit "invalid JSON produces error diagnostic"
#[test]
fn invalid_json_produces_error() {
    let bad_json = "not valid json";
    let (registries, diagnostics) = parse_registries_from_config(bad_json);
    assert!(registries.is_empty());
    let error = diagnostics
        .iter()
        .find(|d| d.code == "E067")
        .expect("expected E067 diagnostic");
    assert_eq!(error.severity, Severity::Error);
}

#[specforge_test_macros::test(
    behavior = "configure_registries",
    verify = "a registry url is the registry's root, and one ending in the protocol's version prefix is refused with E067 naming the root"
)]
fn a_url_ending_in_the_version_prefix_is_refused_naming_the_root() {
    let json = r#"{"registries": [
        {"alias": "main", "url": "https://r.example/v1", "default_registry": true}
    ]}"#;

    let (registries, diagnostics) = parse_registries_from_config(json);

    assert!(registries.is_empty());
    let refused: Vec<_> = diagnostics.iter().filter(|d| d.code == "E067").collect();
    assert_eq!(refused.len(), 1, "{diagnostics:?}");
    assert_eq!(
        refused[0].message,
        "Failed to parse registry entry at index 0: 'https://r.example/v1' ends in the registry protocol's version prefix /v1; name the registry's root: 'https://r.example'"
    );
}

#[specforge_test_macros::test(
    behavior = "configure_registries",
    verify = "a registry url is the registry's root, and one ending in the protocol's version prefix is refused with E067 naming the root"
)]
fn a_url_that_is_not_an_http_url_is_refused() {
    for url in ["registry.example.com", "ftp://r.example"] {
        let json = format!(r#"{{"registries": [{{"alias": "main", "url": "{url}"}}]}}"#);

        let (registries, diagnostics) = parse_registries_from_config(&json);

        assert!(registries.is_empty(), "{url}");
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == "E067" && d.message.contains("registry entry at index 0")),
            "{url}: {diagnostics:?}"
        );
    }
}

#[specforge_test_macros::test(
    behavior = "configure_registries",
    verify = "a scope_filter that is not a scope refuses every registry operation with E067 naming the scope to write"
)]
fn a_scope_filter_that_is_no_scope_is_e067_naming_the_scope_to_write() {
    for (text, write) in [
        ("sdk", Some("@sdk")),
        ("@sdk/", Some("@sdk")),
        ("@SDK", Some("@sdk")),
        ("@sdk/zeta", Some("@sdk")),
        ("", None),
    ] {
        let json = format!(
            r#"{{"registries": [
                {{"alias": "public", "url": "https://public.example", "default_registry": true}},
                {{"alias": "private", "url": "https://private.example", "scope_filter": "{text}"}}
            ]}}"#
        );

        let (registries, diagnostics) = parse_registries_from_config(&json);

        assert_eq!(registries.len(), 1, "{text:?}: {diagnostics:?}");
        let refused: Vec<_> = diagnostics.iter().filter(|d| d.code == "E067").collect();
        assert_eq!(refused.len(), 1, "{text:?}: {diagnostics:?}");
        let head = "Failed to parse registry entry at index 1: ";
        match write {
            Some(write) => assert_eq!(
                refused[0].message,
                format!("{head}'{text}' is not a scope: write '{write}'")
            ),
            None => assert!(
                refused[0]
                    .message
                    .starts_with(&format!("{head}'' is not a scope: a scope is @")),
                "{}",
                refused[0].message
            ),
        }
    }
}
