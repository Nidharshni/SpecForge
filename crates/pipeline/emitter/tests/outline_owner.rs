//! An enhancement of a kind no loaded declaration declares has no owner (ADR 0096).

use serde_json::Value;
use specforge_emitter::outline::{
    DependencyDepth, OutlineDetail, OutlineFormat, OutlineOptions, export,
};
use specforge_wasm::protocol::load_declaration;

fn outline_of_software_alone(format: OutlineFormat) -> String {
    let runtime = specforge_component::ComponentRuntime::new();
    let names = vec!["@specforge/software".to_string()];
    specforge_component::builtins::load_builtins_for(&runtime, &names).unwrap();
    let declaration = load_declaration(&runtime, "@specforge/software")
        .unwrap()
        .declaration;
    export(
        &[declaration],
        &OutlineOptions {
            format,
            detail: OutlineDetail::All,
            deps: DependencyDepth::Full,
        },
    )
}

#[specforge_test_macros::test(
    behavior = "render_extension_outline",
    verify = "an enhancement of a kind no extension declares names no owner"
)]
fn an_enhancement_of_a_kind_no_declaration_declares_has_no_owner() {
    let json: Value =
        serde_json::from_str(&outline_of_software_alone(OutlineFormat::Json)).unwrap();
    let module = json["enhancements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["target_kind"] == "module")
        .expect("software enhances module");
    assert!(module["owner"].is_null(), "{module}");

    let markdown = outline_of_software_alone(OutlineFormat::Markdown);
    assert!(
        markdown.contains("enhances module (no enabled extension declares it)"),
        "{markdown}"
    );
    let mermaid = outline_of_software_alone(OutlineFormat::Mermaid);
    assert!(!mermaid.contains("-.->|\"enhances module\"|"), "{mermaid}");
}
