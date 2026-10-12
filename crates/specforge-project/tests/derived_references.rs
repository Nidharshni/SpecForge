//! Derived reference fields through the real load and compile: a field's
//! `derived_from`, declared by an extension, gives the compiled graph edges
//! from the type names entities write in their field types and method
//! signatures, and the extension's own rules see them.

use std::fs;

use specforge_common::Diagnostic;
use specforge_extension_sdk::prelude::*;
use specforge_project::CompiledProject;
use specforge_test::prelude::*;
use specforge_wasm::testing::InProcessRuntime;
use std::sync::Arc;
use tempfile::TempDir;

const EXTENSION: &str = "@test/shapes";

/// An extension, in process, with two kinds: `shape`, whose `parts` field
/// derives from its field types, and `iface`, whose `uses` field derives
/// from its method signatures; both point at `shape`. A rule reports a
/// shape nothing references.
fn shapes_extension() -> InProcessRuntime {
    InProcessRuntime::new().with(|| {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new(EXTENSION, "1.0.0"));
        c.kind("Shape", |k| {
            k.keyword("shape").open_fields(true);
            k.field("parts", |f| {
                f.field_type(FieldType::ReferenceList)
                    .target_kind("shape")
                    .derived_from("type_expressions");
            });
        });
        c.kind("Iface", |k| {
            k.keyword("iface").open_fields(true);
            k.field("uses", |f| {
                f.field_type(FieldType::ReferenceList)
                    .target_kind("shape")
                    .derived_from("method_signatures");
            });
        });
        c.rule("W950", |r| {
            r.check(CheckKind::NoIncomingEdges)
                .message_template("shape '{id}' is not referenced")
                .target_kind("shape");
        });
        c
    })
}

fn project(spec: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    let config = serde_json::json!({
        "name": "p", "version": "0.1.0", "extensions": [EXTENSION]
    });
    fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    specforge_installed::testing::install_configured(dir.path(), &specforge_project::builtins());
    fs::write(dir.path().join("a.spec"), spec).unwrap();
    dir
}

/// The (target, label) of every edge out of `id`, sorted.
fn edges_from(compiled: &CompiledProject, id: &str) -> Vec<(String, String)> {
    let mut edges: Vec<(String, String)> = compiled
        .graph()
        .edges_from(id)
        .iter()
        .map(|e| (e.target.to_string(), e.label.to_string()))
        .collect();
    edges.sort();
    edges
}

#[specforge_test(
    behavior = "link_derived_references",
    verify = "a field's derived_from reaches the graph from the extension's manifest"
)]
fn a_derived_from_declared_by_an_extension_links_the_compiled_graph() {
    let dir = project(
        r#"
shape Wheel {
  size number
}
shape Config {
  root string
}
shape Car {
  wheels Wheel[]
}
shape Spare {
  size number
}
iface Garage {
  method park(config: Config) -> Result<Car, Error>
}
"#,
    );

    let compiled = CompiledProject::compile(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Some(Arc::new(shapes_extension())),
    );

    let edge = |target: &str, label: &str| (target.to_string(), label.to_string());
    assert_eq!(edges_from(&compiled, "Car"), vec![edge("Wheel", "parts")]);
    assert_eq!(
        edges_from(&compiled, "Garage"),
        vec![edge("Car", "uses"), edge("Config", "uses")]
    );
    // Only the shape nothing names is unreferenced.
    let diagnostics: Vec<Diagnostic> = compiled.diagnostics();
    let unreferenced: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.code == "W950")
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(
        unreferenced,
        ["shape 'Spare' is not referenced"],
        "{diagnostics:?}"
    );
}

/// `spec` compiled with the builtin @specforge/software.
fn compile_with_software(spec: &str) -> CompiledProject {
    let dir = TempDir::new().unwrap();
    let config = serde_json::json!({
        "name": "p", "version": "0.1.0", "extensions": ["@specforge/software"]
    });
    fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    specforge_installed::testing::install_configured(dir.path(), &specforge_project::builtins());
    fs::write(dir.path().join("a.spec"), spec).unwrap();
    let runtime = specforge_component::ComponentRuntime::with_user_cache();
    CompiledProject::compile(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Some(Arc::new(runtime)),
    )
}

/// The W002 messages, sorted.
fn w002(compiled: &CompiledProject) -> Vec<String> {
    let mut messages: Vec<String> = compiled
        .diagnostics()
        .into_iter()
        .filter(|d| d.code == "W002")
        .map(|d| d.message)
        .collect();
    messages.sort();
    messages
}

#[specforge_test(
    behavior = "se_validate_unreferenced_types",
    verify = "a type named in another type's field type is referenced"
)]
fn a_type_named_in_a_field_type_is_referenced() {
    let compiled = compile_with_software(
        r#"
type Member {
  name string
}
type Kind = method | property
type Holder {
  members Member[]
  kind    Kind
}
"#,
    );

    // Holder itself is named by nothing.
    assert_eq!(
        w002(&compiled),
        ["type 'Holder' is not referenced by any behavior, port, or type"]
    );
}

#[specforge_test(
    behavior = "se_validate_unreferenced_types",
    verify = "a type named in a port method signature is referenced"
)]
fn a_type_named_in_a_port_method_signature_is_referenced() {
    let compiled = compile_with_software(
        r#"
type ScanConfig {
  root string
}
type Project {
  name string
}
type ScanError {
  message string
}
port Scanner {
  direction outbound
  method detect(config: ScanConfig) -> Result<Project, ScanError>
}
"#,
    );

    assert!(w002(&compiled).is_empty(), "{:?}", w002(&compiled));
}

#[specforge_test(
    behavior = "se_validate_unreferenced_types",
    verify = "a primitive or generic wrapper name references no type"
)]
fn a_primitive_or_generic_wrapper_references_no_type() {
    let compiled = compile_with_software(
        r#"
type Counter {
  count Option<number>[]
  label string
}
port Store {
  direction outbound
  method load(key: string) -> Result<Option<string>, number>
}
"#,
    );

    assert!(compiled.graph().edges_from("Counter").is_empty());
    assert!(compiled.graph().edges_from("Store").is_empty());
    assert_eq!(
        w002(&compiled),
        ["type 'Counter' is not referenced by any behavior, port, or type"]
    );
}
