//! The environment's assembly (plan 06): what it derives from the config
//! read, the installed extensions and the extension load, in one order.

use std::sync::Arc;

use serde_json::json;
use specforge_extension_sdk::prelude::*;
use specforge_project::{Environment, SharedRuntime};
use specforge_test::prelude::*;
use specforge_wasm::testing::InProcessRuntime;
use tempfile::TempDir;

const RULES: &str = "@test/rules";

/// `@test/rules`: the `gadget` kind and one custom rule naming
/// `validate__absent`, which it does not export (declared as given,
/// `raw_category`; an unknown export errors as the SDK's guest routing does).
fn rules() -> ContributionsBuilder {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new(RULES, "1.0.0"));
    c.kind("gadget", |k| {
        k.keyword("gadget");
    });
    c.raw_category(
        "validation_rules",
        json!([{
            "code": "E902", "severity": "error", "check": "custom",
            "message_template": "gadget '{id}' failed the absent check",
            "target_kind": "gadget", "wasm_function": "validate__absent"
        }]),
    );
    c
}

/// The codes among `wanted`, in report order.
fn codes_among<'a>(env: &'a Environment, wanted: &[&str]) -> Vec<&'a str> {
    env.diagnostics()
        .map(|d| d.code.as_str())
        .filter(|code| wanted.contains(code))
        .collect()
}

#[specforge_test(
    behavior = "load_extension_manifests",
    verify = "E069 names a mistyped key or a non-string item, and the rest of specforge.json is used"
)]
fn an_environment_reports_its_config_its_load_its_providers_then_its_build() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("specforge.json"),
        json!({
            "name": 5,
            "extensions": [RULES, "@acme/missing"],
            "providers": [{"scheme": "gh", "alias": "w", "extension": RULES}],
        })
        .to_string(),
    )
    .unwrap();
    specforge_installed::testing::install(root, &[RULES]);
    let rt: SharedRuntime = Arc::new(InProcessRuntime::new().with(rules));

    let env = Environment::load(root, Some(rt.clone()));

    let all: Vec<_> = env.diagnostics().collect();
    assert_eq!(
        codes_among(&env, &["E069", "E028", "W118", "W112", "I002"]),
        ["E069", "E028", "W118", "W112"],
        "{all:#?}"
    );
    let w118 = env.diagnostics().find(|d| d.code == "W118").unwrap();
    assert!(
        w118.message.contains("contributes no providers"),
        "{}",
        w118.message
    );
    let w112 = env.diagnostics().find(|d| d.code == "W112").unwrap();
    assert!(
        w112.message.contains("validate__absent"),
        "{}",
        w112.message
    );
    assert!(Arc::ptr_eq(env.runtime.as_ref().unwrap(), &rt));
    assert_eq!(env.spec_root, root);
    let names: Vec<&str> = env.enabled.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, [RULES, "@acme/missing"]);
}

#[specforge_test(
    invariant = "extensions_run_in_their_loading_runtime",
    verify = "an environment holds the runtime it loaded its extensions in, and its session and compiled project read it there"
)]
fn without_a_runtime_nothing_loads_and_each_entry_is_enabled_as_written() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("specforge.json"),
        r#"{"extensions": ["@specforge/product", "x=ext.wasm"], "spec_root": "spec"}"#,
    )
    .unwrap();

    let env = Environment::load(root, None);

    assert!(env.runtime.is_none());
    assert!(env.registries.declarations().is_empty());
    let enabled: Vec<(&str, Option<&str>, bool)> = env
        .enabled
        .iter()
        .map(|e| (e.name.as_str(), e.file.as_deref(), e.failure.is_some()))
        .collect();
    assert_eq!(
        enabled,
        [
            ("@specforge/product", None, false),
            ("x", Some("ext.wasm"), false)
        ]
    );
    let messages: Vec<&str> = env.diagnostics().map(|d| d.message.as_str()).collect();
    assert_eq!(
        messages,
        ["none of the 2 configured extensions loaded — operating in structural-only mode"]
    );
    assert_eq!(env.spec_root, root.join("spec"));
}
