//! How the registries a command asks are read out of `specforge.json`: the
//! way `add`, `update` and `remove` read it, so a project whose config is
//! unusable is refused the same on every command.

use specforge_common::codes;
use specforge_ops::registry::Registry;
use specforge_ops_registry::{ConfiguredRegistry, configured};
use specforge_package::PackageName;
use specforge_registry_client::testing::MemoryClient;
use specforge_test_macros::test as specforge_test;
use tempfile::TempDir;

fn project(config: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("specforge.json"), config).unwrap();
    dir
}

#[specforge_test(
    behavior = "search_registry",
    verify = "an unusable specforge.json is refused with the refusal add gives, before any network call"
)]
fn an_unusable_specforge_json_is_refused_as_add_refuses_it() {
    for config in ["{ \"extensions\": [", "[1, 2]", "{\"extensions\": 7}"] {
        let dir = project(config);
        let refused = specforge_ops::config::required(dir.path()).unwrap_err();

        for operation in ["search", "login", "publish", "add"] {
            let error = configured(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                operation,
            )
            .unwrap_err();

            assert_eq!(error, refused, "{config}: {operation}");
            assert_eq!(error.code, "config_invalid");
        }
    }
}

#[test]
fn a_usable_config_without_registries_is_e063_and_an_unreadable_entry_is_e067() {
    let none = project(r#"{"name": "p", "extensions": []}"#);
    assert_eq!(
        configured(
            &specforge_common::ProjectRoot::of(none.path()).unwrap(),
            "search"
        )
        .unwrap_err()
        .code,
        "E063"
    );

    // Not a project at all: no registry, as before.
    let nothing = TempDir::new().unwrap();
    assert_eq!(
        configured(
            &specforge_common::ProjectRoot::of(nothing.path()).unwrap(),
            "search"
        )
        .unwrap_err()
        .code,
        "E063"
    );

    let unreadable = project(r#"{"registries": [{"url": "https://r.example"}]}"#);
    assert_eq!(
        configured(
            &specforge_common::ProjectRoot::of(unreadable.path()).unwrap(),
            "search"
        )
        .unwrap_err()
        .code,
        "E067"
    );

    let good = project(
        r#"{"registries": [{"alias": "main", "url": "https://r.example", "default": true}]}"#,
    );
    let found = configured(
        &specforge_common::ProjectRoot::of(good.path()).unwrap(),
        "search",
    )
    .unwrap();
    assert_eq!(found.registries.len(), 1);
    assert_eq!(found.registries[0].alias, "main");
}

/// The registries a project configures, as `add` reads them.
fn configured_with(registries: &str) -> specforge_ops_registry::Configured {
    let dir = project(&format!(
        r#"{{"name":"p","version":"0.1.0","extensions":[],"registries":{registries}}}"#
    ));
    configured(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        "add",
    )
    .unwrap()
}

fn name(text: &str) -> specforge_package::PackageName {
    specforge_package::PackageName::parse(text).unwrap()
}

const ACME_AND_MAIN: &str = r#"[
    {"alias":"acme","url":"https://acme.example/v1","scope_filter":"@acme"},
    {"alias":"main","url":"https://main.example/v1","default_registry":true}
]"#;

const ACME_ONLY: &str =
    r#"[{"alias":"acme","url":"https://acme.example/v1","scope_filter":"@acme"}]"#;

#[specforge_test(
    behavior = "resolve_registry_source",
    verify = "scope-specific registry queried for matching scope"
)]
fn a_scoped_name_goes_to_the_registry_of_its_scope() {
    let found = configured_with(ACME_AND_MAIN);

    assert_eq!(
        found.registry_for(&name("@acme/tool")).unwrap().alias,
        "acme"
    );
}

#[specforge_test(
    behavior = "resolve_registry_source",
    verify = "default registry used when no scope filter matches"
)]
fn a_name_no_scope_matches_goes_to_the_default_registry() {
    let found = configured_with(ACME_AND_MAIN);
    assert_eq!(
        found.registry_for(&name("@other/tool")).unwrap().alias,
        "main"
    );

    // A registry that is both scoped and the default serves the rest too.
    let both = configured_with(
        r#"[{"alias":"all","url":"https://a.example/v1","scope_filter":"@acme","default_registry":true}]"#,
    );
    assert_eq!(
        both.registry_for(&name("@other/tool")).unwrap().alias,
        "all"
    );
}

#[specforge_test(
    behavior = "resolve_registry_source",
    verify = "a name no registry serves is refused with R-OPS-001 before any request"
)]
fn a_name_no_registry_serves_is_refused_with_r_ops_001() {
    let found = configured_with(ACME_ONLY);

    let error = found.registry_for(&name("@other/tool")).unwrap_err();

    assert_eq!(error.code, "R-OPS-001");
    assert_eq!(error.kind, specforge_ops::OpErrorKind::PreconditionFailed);
    assert!(
        error.message.contains("@other") && error.message.contains("acme"),
        "{error:?}"
    );
    let hint = error.suggestion.as_deref().unwrap();
    assert!(
        hint.contains("\"scope_filter\": \"@other\"")
            && hint.contains("\"default_registry\": true"),
        "{hint}"
    );
}

#[test]
fn the_first_entry_of_a_scope_serves_it() {
    let found = configured_with(
        r#"[{"alias":"one","url":"https://1.example/v1","scope_filter":"@acme"},
            {"alias":"two","url":"https://2.example/v1","scope_filter":"@acme"}]"#,
    );

    assert_eq!(
        found.registry_for(&name("@acme/tool")).unwrap().alias,
        "one"
    );
}

#[test]
fn an_unscoped_name_goes_to_the_default_registry() {
    let found = configured_with(ACME_AND_MAIN);
    assert_eq!(found.registry_for(&name("tool")).unwrap().alias, "main");

    let none = configured_with(ACME_ONLY);
    assert_eq!(
        none.registry_for(&name("tool")).unwrap_err().code,
        "R-OPS-001"
    );
}

#[specforge_test(
    behavior = "validate_registry_credentials",
    verify = "login stores the credential under the alias of the registry it validated against"
)]
fn named_is_the_alias_given_else_the_default() {
    let found = configured_with(ACME_AND_MAIN);

    assert_eq!(found.named(Some("acme")).unwrap().alias, "acme");
    assert_eq!(found.named(None).unwrap().alias, "main");

    let typo = found.named(Some("typo")).unwrap_err();
    assert_eq!(typo.code, "E063");
    assert!(
        typo.message.contains("typo") && typo.message.contains("acme, main"),
        "{typo:?}"
    );

    let error = configured_with(ACME_ONLY).named(None).unwrap_err();
    assert_eq!(error.code, "E063");
    let hint = error.suggestion.as_deref().unwrap();
    assert!(
        hint.contains("--registry") && hint.contains("\"default_registry\": true"),
        "{hint}"
    );
}

#[specforge_test(
    behavior = "configure_registries",
    verify = "an operation shows the registry configuration's diagnostics once it has asked a registry"
)]
fn the_configuration_is_reported_once_the_registry_is_asked() {
    let dir = project(
        r#"{"name": "p", "extensions": [], "registries": [
            {"alias": "main", "url": "memory://local", "default_registry": true},
            {"alias": "main", "url": "memory://other"}
        ]}"#,
    );
    let registry = ConfiguredRegistry::for_project(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        "add",
    )
    .with_client(MemoryClient::new());
    assert!(registry.reported().is_empty(), "nothing asked yet");

    let tool = PackageName::parse("@acme/tool").unwrap();
    let error = registry.versions(&tool).unwrap_err();

    assert!(error.is(codes::R_RES_001), "{error:?}");
    let reported = registry.reported();
    let reported: Vec<&str> = reported.iter().map(|d| d.code.as_str()).collect();
    assert!(reported.contains(&"W140"), "{reported:?}");
}
