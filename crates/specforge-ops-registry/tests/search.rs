//! `ConfiguredRegistry::search`: every configured registry is asked, each with its own credential; the
//! hits are merged by name and version, the first registry winning (ADR 0044 as amended by plan 16).

use specforge_ops::registry::Registry;
use specforge_ops_registry::{ConfiguredRegistry, User};
use specforge_protocol_types::DeclaredCategory;
use specforge_registry_client::testing::{CallKind, MemoryClient, package};
use specforge_registry_client::{RegistryConfig, RegistryError};
use specforge_test_macros::test as specforge_test;
use tempfile::TempDir;

fn registry_named(alias: &str) -> RegistryConfig {
    RegistryConfig {
        alias: alias.to_string(),
        url: format!("memory://{alias}"),
        scope_filter: None,
        default_registry: false,
    }
}

/// A project configuring `aliases` (the first is the default), and a user whose home is empty.
struct World {
    project: TempDir,
    home: TempDir,
    client: MemoryClient,
}

impl World {
    fn with(aliases: &[&str]) -> Self {
        let project = TempDir::new().unwrap();
        let registries: Vec<serde_json::Value> = aliases
            .iter()
            .enumerate()
            .map(|(i, alias)| {
                serde_json::json!({
                    "alias": alias,
                    "url": format!("memory://{alias}"),
                    "default_registry": i == 0,
                })
            })
            .collect();
        let config =
            serde_json::json!({ "name": "p", "version": "0.1.0", "registries": registries });
        std::fs::write(project.path().join("specforge.json"), config.to_string()).unwrap();
        World {
            project,
            home: TempDir::new().unwrap(),
            client: MemoryClient::new(),
        }
    }

    fn registry(&self) -> ConfiguredRegistry {
        ConfiguredRegistry::for_project(
            &specforge_common::ProjectRoot::of(self.project.path()).unwrap(),
            "search",
        )
        .as_user(User::at(self.home.path(), None))
        .with_client(self.client.clone())
    }

    /// `alias` holds `name@version`, declaring one entity kind when `entities`.
    fn store(&self, alias: &str, name: &str, version: &str, description: &str, entities: bool) {
        let manifest = serde_json::json!({
            "handshake": {
                "protocol_version": "1.0.0",
                "name": name,
                "version": version,
                "description": description,
                "peer_dependencies": [],
                "sandbox_policy": null,
            },
            "entities": if entities { serde_json::json!([{"name": "thing"}]) } else { serde_json::json!([]) },
        })
        .to_string();
        self.client.store(
            &registry_named(alias),
            package(name, version, b"\0asm", &manifest, None),
            b"\0asm".to_vec(),
        );
    }
}

#[specforge_test(
    behavior = "search_registry",
    verify = "queries all configured registries"
)]
fn search_asks_every_registry() {
    let world = World::with(&["reg-a", "reg-b"]);
    world.store("reg-a", "@alpha/ext", "1.0.0", "Alpha", false);
    world.store("reg-b", "@beta/ext", "1.0.0", "Beta", false);

    let searched = world.registry().search("ext", None).unwrap();

    assert!(searched.failures.is_empty());
    assert_eq!(searched.asked, 2);
    let names: Vec<&str> = searched.found.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["@alpha/ext", "@beta/ext"]);
}

#[specforge_test(
    behavior = "search_registry",
    verify = "deduplicates results across registries"
)]
fn one_entry_per_name_and_version_from_the_first_registry() {
    let world = World::with(&["reg-a", "reg-b"]);
    world.store("reg-a", "@specforge/software", "1.0.0", "From A", false);
    world.store("reg-b", "@specforge/software", "1.0.0", "From B", false);

    let searched = world.registry().search("software", None).unwrap();

    assert_eq!(searched.found.len(), 1, "{searched:?}");
    assert_eq!(searched.found[0].registry, "reg-a");
    assert_eq!(searched.found[0].description, "From A");
}

#[specforge_test(behavior = "search_registry", verify = "output is deterministic")]
fn found_is_sorted_by_name_then_version() {
    let world = World::with(&["main"]);
    for name in ["@z/ext", "@a/ext", "@m/ext"] {
        world.store("main", name, "1.0.0", "ext", false);
    }

    let searched = world.registry().search("ext", None).unwrap();

    let names: Vec<&str> = searched.found.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["@a/ext", "@m/ext", "@z/ext"]);
}

#[specforge_test(
    behavior = "search_registry",
    verify = "error from one registry does not abort search of others"
)]
fn a_failed_registry_is_reported_and_the_others_asked() {
    let world = World::with(&["failing", "working"]);
    world.client.fail_next(
        CallKind::Search,
        Some(&registry_named("failing")),
        RegistryError::Timeout {
            url: "memory://failing".into(),
        },
    );
    world.store("working", "@work/ext", "1.0.0", "Works", false);

    let searched = world.registry().search("ext", None).unwrap();

    assert_eq!(searched.found.len(), 1);
    assert_eq!(searched.failures.len(), 1);
    assert!(
        searched.failures[0].message.contains("'failing'"),
        "{searched:?}"
    );
    assert_eq!(searched.failures[0].code, "R004");
    assert!(!searched.failed());
}

#[specforge_test(
    behavior = "search_registry",
    verify = "each failed registry is reported once, and search fails when every registry failed"
)]
fn a_search_fails_when_every_registry_failed() {
    let world = World::with(&["a", "b"]);
    for alias in ["a", "b"] {
        world.client.fail_next(
            CallKind::Search,
            Some(&registry_named(alias)),
            RegistryError::NetworkError {
                message: "down".into(),
            },
        );
    }

    let searched = world.registry().search("x", None).unwrap();

    assert_eq!(searched.failures.len(), 2);
    assert!(searched.failed());
}

#[specforge_test(behavior = "search_registry", verify = "filters by declared category")]
fn a_search_by_category_keeps_only_packages_declaring_it() {
    let world = World::with(&["main"]);
    world.store("main", "@acme/with", "1.0.0", "x", true);
    world.store("main", "@acme/without", "1.0.0", "x", false);

    let searched = world
        .registry()
        .search("acme", Some(DeclaredCategory::Entities))
        .unwrap();

    let names: Vec<&str> = searched.found.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["@acme/with"]);
}

#[specforge_test(
    behavior = "search_registry",
    verify = "a registry lists each package at the version add installs from it"
)]
fn a_search_lists_the_version_add_installs() {
    let world = World::with(&["main"]);
    world.store("main", "@acme/tool", "1.4.0", "t", false);
    world.store("main", "@acme/tool", "2.0.0-beta.1", "t", false);

    let searched = world.registry().search("tool", None).unwrap();

    assert_eq!(searched.found.len(), 1, "{searched:?}");
    assert_eq!(searched.found[0].version, "1.4.0");

    // A package one of whose versions matches is listed at the version `add` installs.
    world.store("main", "@acme/old", "1.0.0", "widget", false);
    world.store("main", "@acme/old", "1.1.0", "tool", false);

    let searched = world.registry().search("widget", None).unwrap();

    let listed: Vec<(&str, &str, &str)> = searched
        .found
        .iter()
        .map(|f| (f.name.as_str(), f.version.as_str(), f.description.as_str()))
        .collect();
    assert_eq!(listed, [("@acme/old", "1.1.0", "tool")]);
}

#[test]
fn each_registry_is_asked_with_its_own_credential() {
    let world = World::with(&["a", "b"]);
    std::fs::write(
        world.home.path().join("credentials.json"),
        r#"{"registries":{"a":{"token":"ta"},"b":{"token":"tb"}}}"#,
    )
    .unwrap();

    world.registry().search("x", None).unwrap();

    let calls = world.client.calls();
    let sent: Vec<(String, String)> = calls
        .iter()
        .map(|c| {
            (
                c.registry.clone().unwrap(),
                c.credential.as_ref().unwrap().token().to_string(),
            )
        })
        .collect();
    assert_eq!(
        sent,
        [
            ("a".to_string(), "ta".to_string()),
            ("b".to_string(), "tb".to_string())
        ]
    );
}
