//! `check: "custom"` rules through the real load and compile: registered
//! with the extension's other rules, their `wasm_function` resolved against
//! the extension when it loads, and dispatched to it on compile.

use std::fs;

use specforge_common::{Diagnostic, Severity};
use specforge_extension_sdk::prelude::*;
use specforge_project::{CompiledProject, Environment};
use specforge_registry::CheckKind;
use specforge_test::prelude::*;
use specforge_wasm::testing::InProcessRuntime;
use std::sync::Arc;
use tempfile::TempDir;

fn project(extensions: &[&str], spec: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    let config = serde_json::json!({
        "name": "p", "version": "0.1.0", "extensions": extensions
    });
    fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    specforge_installed::testing::install_configured(dir.path(), &specforge_project::builtins());
    fs::write(dir.path().join("a.spec"), spec).unwrap();
    dir
}

fn w112(diagnostics: &[Diagnostic]) -> Vec<&Diagnostic> {
    diagnostics.iter().filter(|d| d.code == "W112").collect()
}

/// The W112s that mention `part`.
fn w112_naming<'a>(diagnostics: &'a [Diagnostic], part: &str) -> Vec<&'a Diagnostic> {
    w112(diagnostics)
        .into_iter()
        .filter(|d| d.message.contains(part))
        .collect()
}

const EXTENSION: &str = "@test/rules";

/// An extension, in process, that declares the `gadget` kind and four
/// rules on it: a declarative one, a custom one whose `validate__present`
/// it exports (failing the gadget `bad`), a custom one naming
/// `validate__absent`, which it does not export, and a custom one naming no
/// function. The rules are declared as given (`raw_category`): an SDK
/// builder would refuse the last two. An unknown export errors the way the
/// SDK's guest routing does.
fn rules_extension() -> InProcessRuntime {
    fn build() -> ContributionsBuilder {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new(EXTENSION, "1.0.0"));
        c.kind("gadget", |k| {
            k.keyword("gadget");
        });
        c.raw_category(
            "validation_rules",
            serde_json::json!([
                {
                    "code": "W900", "severity": "warning", "check": "no_incoming_edges",
                    "message_template": "gadget '{id}' is unreferenced",
                    "target_kind": "gadget"
                },
                {
                    "code": "E901", "severity": "error", "check": "custom",
                    "message_template": "gadget '{id}' failed {field}",
                    "target_kind": "gadget", "wasm_function": "validate__present"
                },
                {
                    "code": "E902", "severity": "error", "check": "custom",
                    "message_template": "gadget '{id}' failed the absent check",
                    "target_kind": "gadget", "wasm_function": "validate__absent"
                },
                {
                    "code": "E903", "severity": "error", "check": "custom",
                    "message_template": "gadget '{id}' failed a check with no function",
                    "target_kind": "gadget"
                }
            ]),
        );
        c
    }
    /// `validate__present`, the one custom function the extension exports.
    fn present(export: &str, input: &[u8]) -> Option<Result<Vec<u8>, String>> {
        (export == "validate__present").then(|| {
            specforge_extension_sdk::answer_export(export, input, |context: &ValidatorContext| {
                if context.entity.id == "bad" {
                    ValidatorVerdict::Fail {
                        field: Some("present".to_string()),
                        value: None,
                    }
                } else {
                    ValidatorVerdict::Pass
                }
            })
        })
    }
    InProcessRuntime::new().with_handler(build, present)
}

/// The builtin software extension's custom rules come out of the load
/// registered with their `wasm_function` and origin, beside its
/// declarative rules, and every name resolves against its real module.
#[specforge_test(
    behavior = "register_custom_validation_patterns",
    verify = "custom pattern registered with wasm_function reference"
)]
fn a_builtin_custom_rule_is_registered_with_its_wasm_function() {
    let dir = project(&["@specforge/software"], "");
    let runtime = specforge_component::ComponentRuntime::with_user_cache();

    let env = Environment::load(dir.path(), Some(Arc::new(runtime)));

    let w010 = env
        .registries()
        .rules()
        .iter()
        .find(|rule| rule.code() == "W010")
        .expect("W010 is registered");
    assert_eq!(w010.check_kind(), CheckKind::Custom);
    assert_eq!(
        w010.describe()["wasm_function"],
        "validate__type_field_annotations"
    );
    assert_eq!(w010.origin().name(), "@specforge/software");
    assert!(
        env.registries()
            .rules()
            .iter()
            .any(|rule| rule.check_kind() != CheckKind::Custom),
        "declarative rules are registered beside the custom ones"
    );
    let diagnostics: Vec<Diagnostic> = env.diagnostics().cloned().collect();
    assert!(w112(&diagnostics).is_empty(), "{diagnostics:?}");
}

/// A custom rule naming an export the extension does not have is W112 when
/// the extension loads, naming the function and the extension; the rule
/// naming a real export is not.
#[specforge_test(
    behavior = "register_custom_validation_patterns",
    verify = "unresolvable wasm_function produces warning"
)]
fn an_unresolvable_wasm_function_is_w112_on_load() {
    let dir = project(&[EXTENSION], "");

    let env = Environment::load(dir.path(), Some(Arc::new(rules_extension())));

    let diagnostics: Vec<Diagnostic> = env.diagnostics().cloned().collect();
    let warnings = w112_naming(&diagnostics, "'validate__absent'");
    assert_eq!(warnings.len(), 1, "{diagnostics:?}");
    assert_eq!(warnings[0].severity, Severity::Warning);
    for part in ["'@test/rules'", "'E902'"] {
        assert!(
            warnings[0].message.contains(part),
            "{}",
            warnings[0].message
        );
    }
    assert!(
        w112_naming(&diagnostics, "'validate__present'").is_empty(),
        "{diagnostics:?}"
    );
}

/// A custom rule that names no `wasm_function` has nothing to dispatch to:
/// it is W112 when the extension loads and is not registered.
#[specforge_test(
    behavior = "register_custom_validation_patterns",
    verify = "custom rule without a wasm_function produces warning and is not registered"
)]
fn a_custom_rule_without_a_wasm_function_is_w112_and_not_registered() {
    let dir = project(&[EXTENSION], "");

    let env = Environment::load(dir.path(), Some(Arc::new(rules_extension())));

    let diagnostics: Vec<Diagnostic> = env.diagnostics().cloned().collect();
    let warnings = w112_naming(&diagnostics, "'E903'");
    assert_eq!(warnings.len(), 1, "{diagnostics:?}");
    assert!(
        warnings[0].message.contains("requires a wasm_function"),
        "{}",
        warnings[0].message
    );
    assert!(
        !env.registries()
            .rules()
            .iter()
            .any(|rule| rule.code() == "E903"),
        "a rule with nothing to dispatch to is not registered"
    );
}

/// The whole registration through a compile: the extension's manifests
/// load in a runtime, both custom rules are registered with the
/// declarative one, the unresolvable name is warned about once, and the
/// resolvable rule is dispatched to the extension and reports with its
/// configured code and severity.
#[specforge_test(
    behavior = "register_custom_validation_patterns",
    verify = "Register Custom Validation Patterns: custom validation pattern registration holds — extension_manifests_loaded_fired, wasm_runtime_available, custom_patterns_registered, wasm_functions_resolved"
)]
fn custom_rules_register_resolve_and_dispatch_through_a_compile() {
    let dir = project(
        &[EXTENSION],
        "gadget good \"Good\" {\n}\n\ngadget bad \"Bad\" {\n}\n",
    );

    let compiled = CompiledProject::compile(dir.path(), Some(Arc::new(rules_extension())));

    // extension_manifests_loaded_fired + custom_patterns_registered
    assert_eq!(compiled.environment().registries().declarations().len(), 1);
    let mut registered: Vec<(&str, &str)> = compiled
        .environment()
        .registries()
        .rules()
        .iter()
        .map(|rule| (rule.code(), rule.origin().name()))
        .collect();
    registered.sort();
    assert_eq!(
        registered,
        [
            ("E901", EXTENSION),
            ("E902", EXTENSION),
            ("W900", EXTENSION)
        ]
    );

    let diagnostics = compiled.diagnostics();
    // wasm_functions_resolved: the absent export is warned about, once,
    // and the present one is not.
    assert_eq!(
        w112_naming(&diagnostics, "'validate__absent'").len(),
        1,
        "{diagnostics:?}"
    );
    assert!(
        w112_naming(&diagnostics, "'validate__present'").is_empty(),
        "{diagnostics:?}"
    );
    // wasm_runtime_available: the resolvable rule ran in the extension.
    let failures: Vec<&Diagnostic> = diagnostics.iter().filter(|d| d.code == "E901").collect();
    assert_eq!(failures.len(), 1, "{diagnostics:?}");
    assert_eq!(failures[0].severity, Severity::Error);
    assert_eq!(failures[0].message, "gadget 'bad' failed present");
    assert!(
        !diagnostics.iter().any(|d| d.code == "E902"),
        "{diagnostics:?}"
    );
}
