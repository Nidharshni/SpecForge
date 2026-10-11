//! The `providers` specforge.json configures, registered once against the
//! loaded declarations (ADR 0004 D3-c): the entries read, the schemes
//! registered, and the W118/E057 the registration reported.

use serde_json::{Value, json};
use specforge_extension_sdk::{ContributionsBuilder, ExtensionMeta};
use specforge_project::providers::{Provider, ProviderStatus, Providers};
use specforge_protocol_types::ExtensionDeclaration;
use specforge_test_macros::test as specforge_test;

/// The declaration of the extension `name`, contributing providers when
/// `providers` (its handshake says `providers`).
fn make_declaration(name: &str, providers: bool) -> ExtensionDeclaration {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new(name, "1.0.0"));
    if providers {
        c.providers();
    }
    c.declaration()
}

/// A `providers` entry: `alias` serving `scheme`, implemented by `extension`.
fn entry(alias: &str, scheme: &str, extension: &str) -> Value {
    json!({ "scheme": scheme, "alias": alias, "extension": extension })
}

fn register(providers: Value, declarations: &[ExtensionDeclaration]) -> Providers {
    Providers::register(Some(&json!({ "providers": providers })), declarations)
}

fn listed(providers: &Providers) -> Vec<(&str, &str, &str, &'static str)> {
    providers
        .iter()
        .map(|p: &Provider| {
            (
                p.scheme.as_str(),
                p.alias.as_str(),
                p.extension.as_str(),
                p.status.as_str(),
            )
        })
        .collect()
}

fn codes(providers: &Providers) -> Vec<&str> {
    providers
        .diagnostics()
        .iter()
        .map(|d| d.code.as_str())
        .collect()
}

// The array of {scheme, alias, extension, settings} (ADR 0004 D3-c), in
// declaration order, each registered to its extension.
#[specforge_test(
    behavior = "load_provider_configurations",
    verify = "multiple aliased instances are created"
)]
fn test_load_providers_valid_array() {
    let providers = register(
        json!([
            {
                "scheme": "gh",
                "alias": "github",
                "extension": "@acme/gh",
                "settings": {"baseUrl": "https://api.github.com", "apiKeyEnv": "GITHUB_TOKEN"}
            },
            {"scheme": "jira", "alias": "jira", "extension": "@acme/jira"}
        ]),
        &[
            make_declaration("@acme/gh", true),
            make_declaration("@acme/jira", true),
        ],
    );

    assert!(
        providers.diagnostics().is_empty(),
        "{:?}",
        providers.diagnostics()
    );
    assert_eq!(
        listed(&providers),
        [
            ("gh", "github", "@acme/gh", "registered"),
            ("jira", "jira", "@acme/jira", "registered"),
        ]
    );
    // The provider's own settings are passed through as written.
    let first = providers.iter().next().unwrap();
    assert_eq!(
        Value::Object(first.settings.clone()),
        json!({"baseUrl": "https://api.github.com", "apiKeyEnv": "GITHUB_TOKEN"})
    );
    assert!(providers.iter().nth(1).unwrap().settings.is_empty());
}

// A missing providers key is no providers and no diagnostic.
#[test]
fn test_load_providers_missing_key() {
    let config = json!({ "name": "my-project", "version": "1.0.0" });

    for providers in [
        Providers::register(Some(&config), &[]),
        Providers::register(None, &[]),
    ] {
        assert_eq!(providers.iter().count(), 0);
        assert!(providers.diagnostics().is_empty());
        assert!(providers.schemes().is_empty());
    }
}

// An entry missing scheme, alias or extension is W118, and so is the old
// object-map shape.
#[test]
fn test_load_providers_invalid_entry_warns() {
    let providers = register(
        json!([
            { "scheme": "gh", "extension": "@acme/gh" },
            { "alias": "jira", "extension": "@acme/jira" },
            { "scheme": "x", "alias": "x" },
            { "scheme": "y", "alias": "y", "extension": "@acme/y", "settings": 3 }
        ]),
        &[],
    );
    assert_eq!(providers.iter().count(), 0);
    assert_eq!(
        providers.diagnostics().len(),
        4,
        "{:?}",
        providers.diagnostics()
    );
    assert!(providers.diagnostics().iter().all(|d| d.code == "W118"));
    assert!(
        providers.diagnostics()[2].message.contains("extension"),
        "{:?}",
        providers.diagnostics()[2]
    );

    let map = register(json!({ "gh": { "package": "@acme/gh" } }), &[]);
    assert_eq!(map.iter().count(), 0);
    assert_eq!(map.diagnostics().len(), 1);
    assert!(
        map.diagnostics()[0].message.contains("must be an array"),
        "{:?}",
        map.diagnostics()[0]
    );
}

#[specforge_test(
    behavior = "load_provider_configurations",
    verify = "Load Provider Configurations: provider configuration loading holds — extension_manifests_loaded_fired, specforge_json_available, provider_instances_created, aliased_instances_distinct, no_hardcoded_schemes, provider_configured_emitted"
)]
fn test_load_providers_contract() {
    // ensures: valid → parsed correctly
    let providers = register(
        json!([{ "scheme": "gh", "alias": "gh", "extension": "@acme/gh" }]),
        &[make_declaration("@acme/gh", true)],
    );
    assert_eq!(providers.iter().count(), 1);
    assert!(providers.diagnostics().is_empty());

    // ensures: empty config → empty result
    let empty = Providers::register(Some(&json!({})), &[]);
    assert_eq!(empty.iter().count(), 0);
    assert!(empty.diagnostics().is_empty());

    // ensures: invalid entry → W118
    let invalid = register(json!([{}]), &[]);
    assert!(invalid.diagnostics().iter().any(|d| d.code == "W118"));
}

#[specforge_test(
    behavior = "register_provider_schemes",
    verify = "provider schemes registered from manifest"
)]
fn test_register_schemes_from_manifest() {
    let providers = register(
        json!([entry("github", "gh", "@specforge/github")]),
        &[make_declaration("@specforge/github", true)],
    );

    assert!(providers.diagnostics().is_empty());
    assert_eq!(
        listed(&providers),
        [("gh", "github", "@specforge/github", "registered")]
    );
    assert_eq!(
        providers.schemes(),
        ["gh".to_string()].into_iter().collect()
    );
}

#[specforge_test(
    behavior = "register_provider_schemes",
    verify = "duplicate scheme resolved by specforge.json declaration order"
)]
fn test_register_schemes_duplicate_produces_e057() {
    let providers = register(
        json!([entry("gh-a", "gh", "@ext/a"), entry("gh-b", "gh", "@ext/b")]),
        &[
            make_declaration("@ext/a", true),
            make_declaration("@ext/b", true),
        ],
    );

    // The provider declared first keeps the scheme; the later one is E057.
    assert_eq!(codes(&providers), ["E057"], "{:?}", providers.diagnostics());
    assert!(
        providers.diagnostics()[0]
            .message
            .contains("already registered by provider 'gh-a'"),
        "{:?}",
        providers.diagnostics()
    );
    assert_eq!(
        listed(&providers),
        [
            ("gh", "gh-a", "@ext/a", "registered"),
            ("gh", "gh-b", "@ext/b", "scheme_taken"),
        ]
    );
}

// A provider naming an extension that is not loaded, or loaded but
// contributing no providers, is W118.
#[test]
fn test_register_schemes_no_manifest_warns() {
    let providers = register(json!([entry("github", "gh", "@specforge/github")]), &[]);
    assert_eq!(codes(&providers), ["W118"], "{:?}", providers.diagnostics());
    assert_eq!(
        listed(&providers),
        [("gh", "github", "@specforge/github", "extension_not_loaded")]
    );

    let not_a_provider = register(
        json!([entry("github", "gh", "@ext/other")]),
        &[make_declaration("@ext/other", false)],
    );
    assert_eq!(codes(&not_a_provider), ["W118"]);
    assert_eq!(
        listed(&not_a_provider),
        [("gh", "github", "@ext/other", "not_a_provider")]
    );
    assert!(not_a_provider.schemes().is_empty());
}

#[specforge_test(
    behavior = "register_provider_schemes",
    verify = "Register Provider Schemes: provider scheme registration holds — provider_configured_fired, wasm_runtime_available, schemes_registered, duplicate_scheme_warned, declaration_order_tiebreak, schemes_registered_emitted"
)]
fn test_register_schemes_contract() {
    let config = json!([entry("github", "gh", "@ext/gh")]);

    // ensures: registered correctly
    let providers = register(config.clone(), &[make_declaration("@ext/gh", true)]);
    assert!(providers.diagnostics().is_empty());
    assert!(providers.schemes().contains("gh"));

    // ensures: a non-contributing extension is not matched
    let providers = register(config, &[make_declaration("@ext/other", false)]);
    assert!(providers.schemes().is_empty());
}

// Each provider is registered to the extension it names only.
#[test]
fn test_provider_scheme_isolation_each_registered_to_owner() {
    let providers = register(
        json!([
            entry("github", "gh", "@ext/github"),
            entry("jira", "jira", "@ext/jira")
        ]),
        &[
            make_declaration("@ext/github", true),
            make_declaration("@ext/jira", true),
        ],
    );

    assert!(
        providers.diagnostics().is_empty(),
        "expected no diagnostics for isolated providers, got: {:?}",
        providers.diagnostics()
    );
    assert_eq!(
        listed(&providers),
        [
            ("gh", "github", "@ext/github", "registered"),
            ("jira", "jira", "@ext/jira", "registered"),
        ]
    );
}

#[test]
fn a_status_names_why_a_scheme_is_or_is_not_registered() {
    for (status, name) in [
        (ProviderStatus::Registered, "registered"),
        (ProviderStatus::ExtensionNotLoaded, "extension_not_loaded"),
        (ProviderStatus::NotAProvider, "not_a_provider"),
        (ProviderStatus::SchemeTaken, "scheme_taken"),
    ] {
        assert_eq!(status.as_str(), name);
    }
}
