use specforge_common::Diagnostic;
use specforge_project::testing::TestProject;
use specforge_protocol_types::{DeclaredCategory, EntityKindDescriptor, ExtensionDeclaration};
use specforge_wasm::WasmRuntime;
use specforge_wasm::protocol::load_declaration;

/// A Wasm runtime holding the builtin extensions `ext_names` — the only way
/// extensions exist now (WASM-only migration, Phase 7: the native mirror
/// tier is gone).
fn wasm_runtime_for(ext_names: &[&str]) -> specforge_component::ComponentRuntime {
    let runtime = specforge_component::ComponentRuntime::new();
    let names: Vec<String> = ext_names.iter().map(|name| name.to_string()).collect();
    specforge_component::builtins::load_builtins_for(&runtime, &names).unwrap();
    runtime
}

/// Load an extension through the full protocol pipeline over its real Wasm
/// blob: load_builtins_for → load_declaration → ExtensionDeclaration.
fn load_via_protocol(ext_name: &str) -> ExtensionDeclaration {
    let runtime = wasm_runtime_for(&[ext_name]);
    load_declaration(&runtime, ext_name).unwrap().declaration
}

/// What the registry build reports about `declaration` itself, loaded
/// alone: its identity and shape (E030) and its self-consistency (W021).
/// Its peers are not loaded, so their absence (E027) is not its fault.
fn declaration_diagnostics(declaration: &ExtensionDeclaration) -> Vec<Diagnostic> {
    specforge_registry::build_registries(vec![declaration.clone()])
        .diagnostics()
        .filter(|d| matches!(d.code.as_str(), "E030" | "W021"))
        .cloned()
        .collect()
}

/// The keyword a kind is written with: its declared keyword, else its name.
fn keyword(kind: &EntityKindDescriptor) -> &str {
    kind.keyword.as_deref().unwrap_or(&kind.name)
}

#[test]
fn product_extension_loads_via_protocol() {
    let manifest = load_via_protocol("@specforge/product");
    assert_eq!(manifest.name(), "@specforge/product");
    assert_eq!(manifest.version(), "1.0.0");
    assert_eq!(manifest.entities.len(), 9);
    assert_eq!(manifest.edges.len(), 20);
    assert_eq!(manifest.validation_rules.len(), 50);
    assert_eq!(manifest.shared_fields.len(), 1, "shared fields (tags)");
    assert!(manifest.declares(DeclaredCategory::Entities));
    assert!(manifest.declares(DeclaredCategory::ValidationRules));

    let diags = declaration_diagnostics(&manifest);
    assert!(diags.is_empty(), "declaration errors: {:?}", diags);
}

#[test]
fn product_w093_semver_pattern_is_not_trivial() {
    let manifest = load_via_protocol("@specforge/product");
    let w093 = manifest
        .validation_rules
        .iter()
        .find(|r| r.code == "W093")
        .expect("W093 rule must exist");
    let pattern = w093.constraint.as_ref().unwrap().pattern.as_ref().unwrap();
    assert!(
        pattern.len() > 5,
        "W093 pattern '{}' is too trivial to validate semver",
        pattern
    );
    assert!(
        pattern.contains(r"\d") || pattern.contains("[0-9]"),
        "W093 pattern '{}' must match digits for semver validation",
        pattern
    );
}

#[test]
fn governance_extension_loads_via_protocol() {
    let manifest = load_via_protocol("@specforge/governance");
    assert_eq!(manifest.name(), "@specforge/governance");
    assert_eq!(manifest.version(), "1.0.0");
    assert_eq!(manifest.entities.len(), 3);
    assert_eq!(manifest.edges.len(), 11);
    assert_eq!(manifest.validation_rules.len(), 0);
    assert_eq!(
        manifest.peers().len(),
        2,
        "governance should depend on software AND product"
    );
    let dep_names: Vec<&str> = manifest.peers().iter().map(|p| p.name.as_str()).collect();
    assert!(
        dep_names.contains(&"@specforge/software"),
        "must depend on software"
    );
    assert!(
        dep_names.contains(&"@specforge/product"),
        "must depend on product (declares edges to feature)"
    );
    assert!(manifest.declares(DeclaredCategory::Entities));
    assert!(!manifest.declares(DeclaredCategory::ValidationRules));

    let diags = declaration_diagnostics(&manifest);
    assert!(diags.is_empty(), "declaration errors: {:?}", diags);
}

#[test]
fn testing_extension_makes_software_kinds_testable() {
    let manifest = load_via_protocol("@specforge/testing");
    assert_eq!(manifest.name(), "@specforge/testing");
    assert!(manifest.entities.is_empty(), "testing owns no kinds");
    let testable: Vec<&str> = manifest
        .enhancements
        .iter()
        .filter(|e| e.verify_kinds.is_some())
        .map(|e| e.target_kind.as_str())
        .collect();
    assert_eq!(
        testable,
        [
            "behavior",
            "invariant",
            "event",
            "type",
            "port",
            "constraint",
            "failure_mode"
        ]
    );
    // Each kind's extension is an optional peer, so an unloaded one is silent.
    let optional_peers: Vec<&str> = manifest
        .peers()
        .iter()
        .filter(|p| p.optional)
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(
        optional_peers,
        ["@specforge/software", "@specforge/governance"]
    );
    let codes: std::collections::BTreeSet<&str> = manifest
        .validation_rules
        .iter()
        .map(|r| r.code.as_str())
        .collect();
    assert_eq!(codes, ["W004", "W009"].into_iter().collect());
}

#[specforge_test_macros::test(
    behavior = "se_declare_manifest",
    verify = "the only peer is @specforge/product, and it is optional"
)]
fn software_extension_loads_via_protocol() {
    let manifest = load_via_protocol("@specforge/software");
    assert_eq!(manifest.name(), "@specforge/software");
    assert_eq!(manifest.version(), "1.0.0");
    assert_eq!(manifest.entities.len(), 5);
    assert_eq!(manifest.edges.len(), 15);
    // W004/W009 moved to @specforge/testing (ADR 0002).
    assert_eq!(manifest.validation_rules.len(), 11);
    assert_eq!(manifest.enhancements.len(), 2);
    assert!(
        manifest
            .entities
            .iter()
            .all(|k| !k.supports_verify && !k.testable),
        "software declares no test vocabulary"
    );
    assert_eq!(manifest.peers().len(), 1);
    assert!(
        manifest.peers()[0].optional,
        "product is an optional peer: software works without it"
    );
    assert!(manifest.handshake.sandbox_policy.is_some());
    assert!(manifest.declares(DeclaredCategory::Entities));
    assert!(manifest.declares(DeclaredCategory::ValidationRules));

    let diags = declaration_diagnostics(&manifest);
    assert!(diags.is_empty(), "declaration errors: {:?}", diags);
}

#[test]
fn formal_extension_loads_via_protocol() {
    let manifest = load_via_protocol("@specforge/formal");
    assert_eq!(manifest.name(), "@specforge/formal");
    assert_eq!(manifest.version(), "1.0.0");
    assert_eq!(manifest.entities.len(), 5);
    assert_eq!(manifest.edges.len(), 10);
    assert!(
        manifest.validation_rules.len() >= 6,
        "formal needs at least 6 validation rules, got {}",
        manifest.validation_rules.len()
    );
    // behavior, event, and invariant (its `expression` claim, ADR 0009).
    assert_eq!(manifest.enhancements.len(), 3);
    assert_eq!(manifest.peers().len(), 1);
    assert!(manifest.declares(DeclaredCategory::Entities));
    assert!(manifest.declares(DeclaredCategory::ValidationRules));

    let property = manifest
        .entities
        .iter()
        .find(|k| keyword(k) == "property")
        .unwrap();
    assert!(
        property.supports_verify,
        "property should support verify (model-checkable)"
    );
    let axiom = manifest
        .entities
        .iter()
        .find(|k| keyword(k) == "axiom")
        .unwrap();
    assert!(
        axiom.supports_verify,
        "axiom should support verify (proof-checkable)"
    );

    let refinement = manifest
        .entities
        .iter()
        .find(|k| keyword(k) == "refinement")
        .unwrap();
    let abstract_f = refinement
        .fields
        .iter()
        .find(|f| f.name == "abstract_entity")
        .unwrap();
    assert!(
        abstract_f.required,
        "refinement.abstract_entity should be required"
    );
    let concrete_f = refinement
        .fields
        .iter()
        .find(|f| f.name == "concrete_entity")
        .unwrap();
    assert!(
        concrete_f.required,
        "refinement.concrete_entity should be required"
    );

    let diags = declaration_diagnostics(&manifest);
    assert!(diags.is_empty(), "declaration errors: {:?}", diags);
}

// ── @specforge/rust ──

#[test]
fn rust_extension_loads_via_protocol() {
    let manifest = load_via_protocol("@specforge/rust");
    assert_eq!(manifest.name(), "@specforge/rust");
    assert_eq!(manifest.version(), "1.0.0");
    assert!(manifest.declares(DeclaredCategory::Analyzers));
    assert!(!manifest.declares(DeclaredCategory::Entities));
    assert_eq!(manifest.analyzers.len(), 1);
    let ac = &manifest.analyzers[0];
    assert_eq!(ac.language, "rust");
    assert_eq!(ac.file_extensions, vec![".rs"]);
    assert_eq!(ac.scan_export, "scan__rust");

    let diags = declaration_diagnostics(&manifest);
    assert!(diags.is_empty(), "declaration errors: {:?}", diags);
}

#[test]
fn rust_scanner_produces_same_items_as_fallback() {
    use specforge_protocol_types::{ScanRequest, ScanResponse};

    let source = r#"pub fn hello() {}
pub struct MyConfig {}
pub enum Status {}
pub trait Drawable {}
pub async fn fetch_data() {}
fn private() {}
// pub fn commented() {}
pub const MAX_SIZE: usize = 100;
"#;

    let runtime = wasm_runtime_for(&["@specforge/rust"]);
    let input = serde_json::to_vec(&ScanRequest {
        file_path: "src/lib.rs".into(),
        content: source.into(),
    })
    .unwrap();

    let result = runtime.call_export("@specforge/rust", "scan__rust", &input);
    let bytes = match result {
        specforge_wasm::runtime::WasmCallResult::Ok(b) => b,
        specforge_wasm::runtime::WasmCallResult::Trap(t) => panic!("scan trapped: {:?}", t),
    };
    let resp: ScanResponse = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(resp.items.len(), 6);
    assert_eq!(resp.items[0].name, "hello");
    assert_eq!(resp.items[0].item_kind, "function");
    assert_eq!(resp.items[1].name, "MyConfig");
    assert_eq!(resp.items[1].item_kind, "struct");
    assert_eq!(resp.items[2].name, "Status");
    assert_eq!(resp.items[2].item_kind, "enum");
    assert_eq!(resp.items[3].name, "Drawable");
    assert_eq!(resp.items[3].item_kind, "trait");
    assert_eq!(resp.items[4].name, "fetch_data");
    assert_eq!(resp.items[4].item_kind, "function");
    assert_eq!(resp.items[5].name, "MAX_SIZE");
    assert_eq!(resp.items[5].item_kind, "constant");
}

// ── @specforge/typescript ──

#[test]
fn typescript_extension_loads_via_protocol() {
    let manifest = load_via_protocol("@specforge/typescript");
    assert_eq!(manifest.name(), "@specforge/typescript");
    assert_eq!(manifest.version(), "1.0.0");
    assert!(manifest.declares(DeclaredCategory::Analyzers));
    assert!(!manifest.declares(DeclaredCategory::Entities));
    assert_eq!(manifest.analyzers.len(), 1);
    let ac = &manifest.analyzers[0];
    assert_eq!(ac.language, "typescript");
    assert_eq!(ac.file_extensions, vec![".ts", ".tsx", ".js", ".jsx"]);
    assert_eq!(ac.scan_export, "scan__typescript");

    let diags = declaration_diagnostics(&manifest);
    assert!(diags.is_empty(), "declaration errors: {:?}", diags);
}

#[test]
fn typescript_scanner_finds_exported_symbols() {
    use specforge_protocol_types::{ScanRequest, ScanResponse};

    let source = r#"export function handleRequest() {}
export async function fetchData() {}
export class UserService {}
export interface IRepository {}
export type Config = Record<string, unknown>;
export enum Status { Active, Inactive }
export const MAX_RETRIES = 3;
export let counter = 0;
export abstract class Base {}
export default function main() {}
function privateHelper() {}
// export function commented() {}
export { something } from './other';
export * from './barrel';
"#;

    let runtime = wasm_runtime_for(&["@specforge/typescript"]);
    let input = serde_json::to_vec(&ScanRequest {
        file_path: "src/app.ts".into(),
        content: source.into(),
    })
    .unwrap();

    let result = runtime.call_export("@specforge/typescript", "scan__typescript", &input);
    let bytes = match result {
        specforge_wasm::runtime::WasmCallResult::Ok(b) => b,
        specforge_wasm::runtime::WasmCallResult::Trap(t) => panic!("scan trapped: {:?}", t),
    };
    let resp: ScanResponse = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(resp.items.len(), 10);
    assert_eq!(resp.items[0].name, "handleRequest");
    assert_eq!(resp.items[0].item_kind, "function");
    assert_eq!(resp.items[1].name, "fetchData");
    assert_eq!(resp.items[1].item_kind, "function");
    assert_eq!(resp.items[2].name, "UserService");
    assert_eq!(resp.items[2].item_kind, "class");
    assert_eq!(resp.items[3].name, "IRepository");
    assert_eq!(resp.items[3].item_kind, "interface");
    assert_eq!(resp.items[4].name, "Config");
    assert_eq!(resp.items[4].item_kind, "type_alias");
    assert_eq!(resp.items[5].name, "Status");
    assert_eq!(resp.items[5].item_kind, "enum");
    assert_eq!(resp.items[6].name, "MAX_RETRIES");
    assert_eq!(resp.items[6].item_kind, "constant");
    assert_eq!(resp.items[7].name, "counter");
    assert_eq!(resp.items[7].item_kind, "variable");
    assert_eq!(resp.items[8].name, "Base");
    assert_eq!(resp.items[8].item_kind, "class");
    assert_eq!(resp.items[9].name, "main");
    assert_eq!(resp.items[9].item_kind, "function");
    assert_eq!(resp.items[9].visibility.as_deref(), Some("default"));
}

/// The proof role of `kind`'s `field` as the protocol bridge reads it,
/// looking at the declaration's enhancements of `kind` too.
fn proof_role(manifest: &ExtensionDeclaration, kind: &str, field: &str) -> Option<String> {
    let own = manifest
        .entities
        .iter()
        .filter(|k| keyword(k) == kind)
        .flat_map(|k| &k.fields);
    let enhanced = manifest
        .enhancements
        .iter()
        .filter(|e| e.target_kind == kind)
        .flat_map(|e| &e.fields);
    own.chain(enhanced)
        .find(|f| f.name == field)
        .unwrap_or_else(|| panic!("{} declares no {kind}.{field}", manifest.name()))
        .proof_role
        .clone()
}

#[specforge_test_macros::test(
    behavior = "ge_register_field_definitions",
    verify = "constraint metric field declares the bound proof role"
)]
fn governance_constraint_metric_is_a_bound() {
    let manifest = load_via_protocol("@specforge/governance");
    assert_eq!(
        proof_role(&manifest, "constraint", "metric").as_deref(),
        Some("bound")
    );
    assert_eq!(proof_role(&manifest, "constraint", "threshold"), None);
}

#[specforge_test_macros::test(
    behavior = "fa_declare_manifest",
    verify = "property and invariant expressions are claims and axiom expressions bounds"
)]
fn formal_expressions_declare_their_proof_roles() {
    let manifest = load_via_protocol("@specforge/formal");
    let role = |kind| proof_role(&manifest, kind, "expression");
    assert_eq!(role("property").as_deref(), Some("claim"));
    assert_eq!(role("axiom").as_deref(), Some("bound"));
    assert_eq!(role("invariant").as_deref(), Some("claim"));
}

#[specforge_test_macros::test(
    behavior = "fa_declare_manifest",
    verify = "manifest declares the 12 formal validation rules"
)]
fn formal_declares_its_validation_rules() {
    let manifest = load_via_protocol("@specforge/formal");
    let mut codes: Vec<&str> = manifest
        .validation_rules
        .iter()
        .map(|r| r.code.as_str())
        .collect();
    codes.sort_unstable();
    assert_eq!(
        codes,
        [
            "W123", "W124", "W126", "W127", "W128", "W129", "W131", "W132", "W133", "W134", "W135",
            "W136"
        ]
    );
}

#[specforge_test_macros::test(
    behavior = "fa_declare_manifest",
    verify = "manifest declares 5 passes in dependency order"
)]
fn formal_declares_four_analyze_passes_and_one_check_pass() {
    let manifest = load_via_protocol("@specforge/formal");
    let mut passes: Vec<(&str, Option<&str>, Option<&str>)> = manifest
        .passes
        .iter()
        .map(|p| (p.name.as_str(), p.after.as_deref(), p.phase.as_deref()))
        .collect();
    passes.sort_unstable();
    assert_eq!(
        passes,
        [
            ("analysis_available", Some("resolve"), Some("check")),
            ("condition_check", Some("resolve"), None),
            ("coverage_tracking", Some("event_graph_analyze"), None),
            ("event_graph_analyze", Some("layering_verify"), None),
            ("layering_verify", Some("condition_check"), None),
        ]
    );
}

#[specforge_test_macros::test(
    behavior = "pe_declare_manifest",
    verify = "the six lifecycle kinds declare status as their lifecycle field"
)]
fn product_lifecycle_kinds_declare_status() {
    let manifest = load_via_protocol("@specforge/product");
    let lifecycle: Vec<(&str, Option<&str>)> = manifest
        .entities
        .iter()
        .map(|k| (keyword(k), k.lifecycle_field.as_deref()))
        .collect();
    for (kind, field) in lifecycle {
        let expected = matches!(
            kind,
            "feature" | "milestone" | "deliverable" | "persona" | "channel" | "release"
        )
        .then_some("status");
        assert_eq!(field, expected, "{kind}'s lifecycle field");
    }
}

/// The nine builtin extensions loaded together, as a project enabling all
/// of them does: their rules register with no W112 or W147, the custom
/// ones' functions answer the load probe, and every rule's target kind and
/// edge type is its extension's or a declared peer's (no W021).
#[specforge_test_macros::test(
    behavior = "execute_validation_pattern",
    verify = "the builtin extensions' rules register with no W112, W147 or W021"
)]
fn the_builtin_extensions_rules_register_cleanly() {
    let names: Vec<&str> = specforge_component::builtins::BUILTIN_EXTENSIONS
        .iter()
        .map(|(name, _)| *name)
        .collect();
    assert_eq!(names.len(), 9, "{names:?}");
    let dir = TestProject::new()
        .config(|c| c["name"] = "all".into())
        .enabling(&names);

    let env = specforge_project::Environment::load(&dir.project_root(), Some(dir.runtime()));

    assert_eq!(env.registries().declarations().len(), 9);
    assert!(
        env.registries().rules().len() > 80,
        "{}",
        env.registries().rules().len()
    );
    let unworkable: Vec<&Diagnostic> = env
        .diagnostics()
        .filter(|d| ["W112", "W147", "W021"].contains(&d.code.as_str()))
        .collect();
    assert!(unworkable.is_empty(), "{unworkable:?}");
}

#[test]
fn product_and_governance_build_clean_alone_and_together() {
    let product = load_via_protocol("@specforge/product");
    let governance = load_via_protocol("@specforge/governance");
    // No builtin rule names a peer extension any more: the closed values are
    // enum vocabularies the host checks (E061), not cross-extension rules.
    for m in [&product, &governance] {
        assert!(
            m.validation_rules
                .iter()
                .all(|r| r.target_extension.is_none()),
            "{}: a rule names a target_extension",
            m.name()
        );
    }

    // Alone: no W021, and the field checks still run, so a typo'd kind in
    // product would still be W021.
    let alone = specforge_registry::build_registries(vec![product.clone()]);
    assert!(
        alone.diagnostics().all(|d| d.code != "W021"),
        "{:?}",
        alone.diagnostics().collect::<Vec<_>>()
    );
    let mut typo = product.clone();
    typo.entities[0].fields[0].target_kind = Some("featur".to_string());
    let broken = specforge_registry::build_registries(vec![typo]);
    assert_eq!(broken.diagnostics().filter(|d| d.code == "W021").count(), 1);

    // Together: governance declares `constraint`, so nothing is wrong either.
    let both = specforge_registry::build_registries(vec![governance, product]);
    assert!(
        both.diagnostics().all(|d| d.code != "W021"),
        "{:?}",
        both.diagnostics().collect::<Vec<_>>()
    );
}

#[specforge_test_macros::test(
    behavior = "pe_declare_manifest",
    verify = "passes declares lifecycle in the check phase and delivery_evidence under analyze"
)]
fn product_declares_its_lifecycle_passes() {
    let manifest = load_via_protocol("@specforge/product");
    let passes: Vec<(&str, Option<&str>, Option<&str>)> = manifest
        .passes
        .iter()
        .map(|p| (p.name.as_str(), p.after.as_deref(), p.phase.as_deref()))
        .collect();
    assert_eq!(
        passes,
        [
            ("lifecycle", Some("resolve"), Some("check")),
            ("delivery_evidence", Some("resolve"), None),
        ]
    );
}

#[specforge_test_macros::test(
    behavior = "se_validate_unused_invariants",
    verify = "invariant nothing references produces W003"
)]
#[specforge_test_macros::test(
    behavior = "se_validate_unused_invariants",
    verify = "invariant with incoming reference edge passes"
)]
fn check_reports_an_unreferenced_invariant_as_w003() {
    let compiled = TestProject::new()
        .enabling(&["@specforge/software"])
        .file(
            "main.spec",
            concat!(
                "invariant held \"Held\" {\n  guarantee \"g\"\n}\n",
                "invariant orphan \"Orphan\" {\n  guarantee \"g\"\n}\n",
                "behavior keeper \"Keeper\" {\n  contract \"c\"\n  invariants [held]\n}\n",
            ),
        )
        .compile();
    let diagnostics = compiled.diagnostics();
    let w003: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.code == "W003")
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(w003.len(), 1, "{diagnostics:?}");
    assert!(w003[0].contains("'orphan'"), "{w003:?}");
}
