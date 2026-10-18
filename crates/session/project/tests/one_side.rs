//! A relation is declared from one side (ADR 0150): the builtins together declare no relation
//! from both ends.

use serde_json::json;
use specforge_component::ComponentRuntime;
use specforge_component::builtins::BUILTIN_EXTENSIONS;
use specforge_project::Environment;
use tempfile::TempDir;

#[test]
#[ignore = "flipped by T10"]
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

    let collisions: Vec<String> = registries
        .fields()
        .iter()
        .filter_map(|(kind, field, entry)| {
            let declared = entry.declared();
            let (target, inverse) = (
                declared.target_kind.as_deref()?,
                declared.inverse_of.as_deref()?,
            );
            registries
                .fields()
                .contains(target, inverse)
                .then(|| format!("{kind}.{field} names '{inverse}', a field of {target}"))
        })
        .collect();
    assert!(collisions.is_empty(), "{collisions:?}");
    assert!(
        !registries.rules().iter().any(|rule| rule.code() == "E051"),
        "E051 validates `triggers`, a field no extension declares"
    );
}
