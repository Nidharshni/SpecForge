//! The registry build's declaration consistency check, as the real load
//! reports it: W021 for a field's reference no loaded peer resolves, once
//! every configured extension is in, among the environment's diagnostics.
//! `crates/pipeline/registry/tests/build/consistency.rs` proves the rule through
//! `build_registries`; these prove the load reports what the build found.

use specforge_common::{Diagnostic, Severity};
use specforge_extension_sdk::PeerDependency;
use specforge_extension_sdk::prelude::*;
use specforge_project::Environment;
use specforge_project::testing::TestProject;
use specforge_test::prelude::*;
use specforge_wasm::testing::InProcessRuntime;
use std::sync::Arc;

/// Extensions, served in process, that contribute entity kinds (with their
/// fields) and edge types. Each is `{ "name", "peers": [..], "entities":
/// [..], "edges": [..] }`: its kinds and edges are the descriptor JSON the
/// protocol carries, declared as given (`raw_category`).
fn kind_extensions(specs: Vec<serde_json::Value>) -> InProcessRuntime {
    specs
        .into_iter()
        .fold(InProcessRuntime::new(), |runtime, spec| {
            runtime.with(move || {
                let name = spec["name"].as_str().unwrap();
                let mut meta = ExtensionMeta::new(name, "1.0.0");
                meta.peer_dependencies = spec["peers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|peer| PeerDependency {
                        name: peer.as_str().unwrap().to_string(),
                        version: "^1.0.0".to_string(),
                        optional: false,
                    })
                    .collect();
                let mut c = ContributionsBuilder::new(meta);
                for category in [
                    specforge_extension_sdk::DeclaredCategory::Entities,
                    specforge_extension_sdk::DeclaredCategory::Edges,
                ] {
                    if !spec[category.name()].is_null() {
                        c.raw_category(category, spec[category.name()].clone());
                    }
                }
                c
            })
        })
}

/// Load a project configuring `extensions` (in order) from `runtime`.
fn load(runtime: InProcessRuntime, extensions: &[&str]) -> Environment {
    let dir = TestProject::new()
        .config(|c| c["name"] = "p".into())
        .enabling(extensions)
        .installing_enabled();
    dir.environment_in(std::sync::Arc::new(runtime))
}

fn w021(env: &Environment) -> Vec<Diagnostic> {
    env.diagnostics()
        .filter(|d| d.code == "W021")
        .cloned()
        .collect()
}

/// `@test/tasks`: a `task` whose `owner` is a `person` over `owned_by`,
/// with `person` declared by its peer `@test/people`.
fn tasks_and_people() -> InProcessRuntime {
    kind_extensions(vec![
        serde_json::json!({
            "name": "@test/people",
            "entities": [{ "name": "person" }]
        }),
        serde_json::json!({
            "name": "@test/tasks",
            "peers": ["@test/people"],
            "entities": [{
                "name": "task",
                "fields": [{
                    "name": "owner", "field_type": "reference",
                    "target_kind": "person", "edge": "owned_by"
                }]
            }],
            "edges": [{ "label": "owned_by", "source_kind": "task", "target_kind": "person" }]
        }),
    ])
}

/// A field's `target_kind` naming a kind its peer registers is accepted.
#[specforge_test(
    behavior = "registry_build_declaration_consistency",
    verify = "a target_kind the extension or a loaded peer declares passes"
)]
fn a_target_kind_another_extension_registers_resolves() {
    let env = load(tasks_and_people(), &["@test/people", "@test/tasks"]);

    assert!(env.registries().kinds().contains("person"));
    let owner = env.registries().fields().get("task", "owner").unwrap();
    assert_eq!(owner.declared().target_kind.as_deref(), Some("person"));
    assert!(w021(&env).is_empty(), "{:?}", w021(&env));
}

/// A field's edge label naming an edge type the extension declares is
/// accepted.
#[specforge_test(
    behavior = "registry_build_declaration_consistency",
    verify = "an edge label the extension declares an edge type for passes"
)]
fn an_edge_label_the_extension_declares_resolves() {
    let env = load(tasks_and_people(), &["@test/people", "@test/tasks"]);

    let owned_by = env.registries().edges().get("owned_by").unwrap();
    assert_eq!(
        owned_by.declared().unwrap().target_kind.as_deref(),
        Some("person")
    );
    assert!(w021(&env).is_empty(), "{:?}", w021(&env));
}

/// A `target_kind` no loaded extension declares is a W021 warning naming
/// the field, the kind and the reference; the extension still registers.
#[specforge_test(
    behavior = "registry_build_declaration_consistency",
    verify = "a target_kind no loaded extension declares is a W021 warning"
)]
fn an_unresolved_target_kind_is_w021() {
    let runtime = kind_extensions(vec![serde_json::json!({
        "name": "@test/tasks",
        "entities": [{
            "name": "task",
            "fields": [{ "name": "owner", "field_type": "reference", "target_kind": "robot" }]
        }]
    })]);

    let env = load(runtime, &["@test/tasks"]);

    let warnings = w021(&env);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(warnings[0].severity, Severity::Warning);
    for part in ["'owner'", "'task'", "target_kind 'robot'"] {
        assert!(
            warnings[0].message.contains(part),
            "{}",
            warnings[0].message
        );
    }
    assert!(env.registries().kinds().contains("task"));
}

/// An edge label no edge type of the extension declares is a W021 warning
/// naming the field and the label.
#[specforge_test(
    behavior = "registry_build_declaration_consistency",
    verify = "an edge label the extension declares no edge type for is a W021 warning"
)]
fn an_unresolved_edge_label_is_w021() {
    let runtime = kind_extensions(vec![serde_json::json!({
        "name": "@test/tasks",
        "entities": [
            {
                "name": "task",
                "fields": [{
                    "name": "owner", "field_type": "reference",
                    "target_kind": "person", "edge": "assigned_to"
                }]
            },
            { "name": "person" }
        ]
    })]);

    let env = load(runtime, &["@test/tasks"]);

    let warnings = w021(&env);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(warnings[0].severity, Severity::Warning);
    for part in ["'owner'", "edge label 'assigned_to'"] {
        assert!(
            warnings[0].message.contains(part),
            "{}",
            warnings[0].message
        );
    }
}

/// A domain the host knows nothing of (cooking) cross-validates by
/// structure alone: its references resolve, so nothing is reported.
#[specforge_test(
    behavior = "registry_build_declaration_consistency",
    verify = "cross-validation uses no domain-specific logic"
)]
fn a_domain_the_host_does_not_know_cross_validates_cleanly() {
    let runtime = kind_extensions(vec![serde_json::json!({
        "name": "@custom/cooking",
        "entities": [
            {
                "name": "recipe",
                "fields": [{
                    "name": "ingredients", "field_type": "reference_list",
                    "edge": "uses", "target_kind": "ingredient"
                }]
            },
            { "name": "ingredient" }
        ],
        "edges": [{ "label": "uses", "source_kind": "recipe", "target_kind": "ingredient" }]
    })]);

    let env = load(runtime, &["@custom/cooking"]);

    assert!(env.registries().kinds().contains("recipe"));
    let diagnostics: Vec<&Diagnostic> = env.diagnostics().collect();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

/// Once every extension is in, resolvable references pass and unresolvable
/// ones are warnings only: the extension still loads its kinds, so the
/// user's compile is not failed by an extension's authoring error.
#[specforge_test(
    behavior = "registry_build_declaration_consistency",
    verify = "a declaration's W021 does not fail the compile, and its kinds still register"
)]
fn field_cross_validation_holds_on_load() {
    let runtime = kind_extensions(vec![
        serde_json::json!({
            "name": "@test/people",
            "entities": [{ "name": "person" }]
        }),
        serde_json::json!({
            "name": "@test/tasks",
            "peers": ["@test/people"],
            "entities": [{
                "name": "task",
                "fields": [
                    {
                        "name": "owner", "field_type": "reference",
                        "target_kind": "person", "edge": "owned_by"
                    },
                    { "name": "robot", "field_type": "reference", "target_kind": "robot" },
                    {
                        "name": "reviewer", "field_type": "reference",
                        "target_kind": "person", "edge": "reviewed_by"
                    }
                ]
            }],
            "edges": [{ "label": "owned_by", "source_kind": "task", "target_kind": "person" }]
        }),
    ]);

    // The dependency loads after the extension that needs it: the check
    // waits for both.
    let env = load(runtime, &["@test/tasks", "@test/people"]);

    let warnings = w021(&env);
    let mut messages: Vec<&str> = warnings.iter().map(|d| d.message.as_str()).collect();
    messages.sort();
    assert_eq!(messages.len(), 2, "{messages:?}");
    assert!(
        messages[0].contains("edge label 'reviewed_by'"),
        "{messages:?}"
    );
    assert!(messages[1].contains("target_kind 'robot'"), "{messages:?}");
    assert!(
        !messages.iter().any(|m| m.contains("'owner'")),
        "{messages:?}"
    );
    assert!(warnings.iter().all(|d| d.severity == Severity::Warning));
    assert!(
        !env.diagnostics().any(|d| d.severity == Severity::Error),
        "{:?}",
        env.diagnostics().collect::<Vec<_>>()
    );
    assert!(
        env.registries().kinds().contains("task") && env.registries().kinds().contains("person")
    );
}

/// The extension whose `task.owner` targets `person` loads before the peer
/// that declares `person`: a validation run while only the first extension
/// was registered would report the target (W021) and the `person` entity
/// (E024). A real compile reports neither and resolves `owner` to `p1`, so
/// every kind was registered before the first check.
#[specforge_test(
    behavior = "build_registries_from_declarations",
    verify = "every loaded declaration is registered before a compile checks anything"
)]
fn population_completes_before_any_validation() {
    let runtime = kind_extensions(vec![
        serde_json::json!({
            "name": "@test/tasks",
            "peers": ["@test/people"],
            "entities": [{
                "name": "task",
                "fields": [{
                    "name": "owner", "field_type": "reference",
                    "target_kind": "person", "edge": "owned_by"
                }]
            }],
            "edges": [{ "label": "owned_by", "source_kind": "task", "target_kind": "person" }]
        }),
        serde_json::json!({
            "name": "@test/people",
            "entities": [{ "name": "person" }]
        }),
    ]);
    let dir = TestProject::new()
        .config(|c| c["name"] = "p".into())
        .enabling(&["@test/tasks", "@test/people"])
        .installing_enabled()
        .file(
            "main.spec",
            "task t1 \"T\" {\n  owner p1\n}\n\nperson p1 \"P\" {\n}\n",
        );

    let project = dir.compile_in(Arc::new(runtime));

    let loaded: Vec<&str> = project
        .environment()
        .registries()
        .declarations()
        .iter()
        .map(|d| d.name())
        .collect();
    assert_eq!(
        loaded,
        ["@test/people", "@test/tasks"],
        "load order: the peer first, though the dependent is listed first"
    );
    let diagnostics = project.diagnostics();
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == "W021" || d.code == "E024"),
        "{diagnostics:?}"
    );
    assert!(
        project
            .graph()
            .edges_from("t1")
            .iter()
            .any(|e| e.label == "owner" && e.target == "p1"),
        "{:?}",
        project.graph().edges()
    );
}
