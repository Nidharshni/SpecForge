//! Every builtin declares only codes it owns, at the level the catalog gives
//! them. The builtins write their rule codes as text and do not link the
//! catalog (a guest that did would be rebuilt on every explanation edit,
//! ADR 0026 D4); the registry build checks each rule when it registers it
//! (`check_extension_code`, W150), and this test checks them all, so a
//! mismatch fails a test and not only a user's compile.

use specforge_component::ComponentRuntime;
use specforge_component::builtins::BUILTIN_EXTENSIONS;
use specforge_diagnostics::{Level, check_extension_code};
use specforge_protocol_types::ValidationSeverity;
use specforge_wasm::WasmRuntime as _;
use specforge_wasm::protocol::load_declaration;

fn level(severity: &ValidationSeverity) -> Level {
    match severity {
        ValidationSeverity::Error => Level::Error,
        ValidationSeverity::Warning => Level::Warning,
        ValidationSeverity::Info => Level::Info,
    }
}

#[specforge_test_macros::test(
    behavior = "registry_build_rules",
    verify = "every builtin rule uses a code its extension owns, at the catalogued level"
)]
fn builtin_rules_use_their_own_codes() {
    let runtime = ComponentRuntime::new();
    let mut checked = 0;
    let mut problems = Vec::new();
    for (name, bytes) in BUILTIN_EXTENSIONS {
        runtime.load(name, bytes).expect("builtin loads");
        let declaration = load_declaration(&runtime, name)
            .expect("builtin declares itself")
            .declaration;
        assert_eq!(declaration.name(), *name, "a builtin is named by its blob");
        for rule in &declaration.validation_rules {
            checked += 1;
            if let Err(misuse) =
                check_extension_code(declaration.name(), &rule.code, level(&rule.severity))
            {
                problems.push(format!(
                    "{name}: rule '{}' ({:?}) uses {misuse}",
                    rule.code, rule.severity
                ));
            }
        }
    }
    assert!(
        checked >= 80,
        "only {checked} builtin rules were read; do the builtins still declare their rules?"
    );
    assert!(
        problems.is_empty(),
        "builtin rules use codes their extension may not (W150 on every compile):\n  {}",
        problems.join("\n  ")
    );
}
