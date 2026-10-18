//! A relation is declared from one side (ADR 0150): the builtins together declare no relation
//! from both ends.

use serde_json::json;
use specforge_component::ComponentRuntime;
use specforge_component::builtins::BUILTIN_EXTENSIONS;
use specforge_project::Environment;
use tempfile::TempDir;

#[specforge_test_macros::test(
    behavior = "registry_build_edges",
    verify = "the builtin extensions declare no relation from both ends"
)]
fn no_builtin_declares_a_relation_from_both_sides() {
    let dir = TempDir::new().unwrap();
    let all: Vec<&str> = BUILTIN_EXTENSIONS.iter().map(|(name, _)| *name).collect();
    std::fs::write(
        dir.path().join("specforge.json"),
        json!({ "name": "p", "version": "0.1.0", "extensions": all }).to_string(),
    )
    .unwrap();
    let env = Environment::load(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Some(std::sync::Arc::new(ComponentRuntime::new())),
    );
    let registries = env.registries();
    let both: Vec<_> = env.diagnostics().filter(|d| d.code == "W162").collect();
    assert!(both.is_empty(), "{both:?}");
    assert!(
        !registries.rules().iter().any(|rule| rule.code() == "E051"),
        "E051 validates `triggers`, a field no extension declares"
    );
}
