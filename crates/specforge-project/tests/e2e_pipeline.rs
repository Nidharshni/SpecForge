//! E2E pipeline tests: parse → resolve → graph → validate → emit
//! These tests exercise the full compilation pipeline from spec source files
//! to emitted output, verifying roundtrip integrity.

use specforge_test::prelude::*;
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;

/// `graph` exported as `format`, unscoped and unbudgeted.
fn export(graph: &specforge_graph::Graph, format: specforge_emitter::EmitFormat) -> String {
    specforge_emitter::emit(
        graph,
        &specforge_emitter::EmitOptions {
            format,
            ..specforge_emitter::EmitOptions::default()
        },
    )
    .expect("an unscoped, unbudgeted export cannot fail")
}

/// Build a Wasm runtime for a temp project listing `ext_names`.
fn wasm_runtime_for(ext_names: &[&str]) -> specforge_component::ComponentRuntime {
    let runtime = specforge_component::ComponentRuntime::new();
    let names: Vec<String> = ext_names.iter().map(|name| name.to_string()).collect();
    specforge_component::builtins::load_builtins_for(&runtime, &names).unwrap();
    runtime
}

/// Helper: write spec files to a temp dir and compile them with no extension
/// loaded (structure only).
fn compile_specs(files: &[(&str, &str)]) -> specforge_project::CompiledProject {
    compile_specs_in(files).1
}

/// [`compile_specs`], keeping the directory the files were written to.
fn compile_specs_in(files: &[(&str, &str)]) -> (TempDir, specforge_project::CompiledProject) {
    let dir = TempDir::new().unwrap();
    for (name, content) in files {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, content).unwrap();
    }
    let compiled = specforge_project::CompiledProject::compile(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        None,
    );
    (dir, compiled)
}

/// Helper: compile with the builtin runtime for the given extensions (full pipeline).
fn compile_with_builtins(
    extensions: &[&str],
    files: &[(&str, &str)],
) -> specforge_project::CompiledProject {
    let dir = TempDir::new().unwrap();
    let ext_json = extensions
        .iter()
        .map(|e| format!("\"{e}\""))
        .collect::<Vec<_>>()
        .join(", ");
    fs::write(
        dir.path().join("specforge.json"),
        format!(
            r#"{{ "name": "t", "version": "0.1.0", "spec_root": "spec", "extensions": [{ext_json}] }}"#
        ),
    )
    .unwrap();
    for (name, content) in files {
        let path = dir.path().join("spec").join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, content).unwrap();
    }
    let runtime = wasm_runtime_for(extensions);
    specforge_project::CompiledProject::compile(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Some(Arc::new(runtime)),
    )
}

// B:extension_owned_body_syntax — verify unit "port method body syntax does not surface parse errors"
#[specforge_test(
    behavior = "extension_owned_body_syntax",
    verify = "extension-owned body syntax does not surface E001 parse errors"
)]
fn port_method_body_does_not_surface_parse_errors() {
    let ctx = compile_with_builtins(
        &["@specforge/software"],
        &[(
            "main.spec",
            r#"spec "t" { version "0.1.0" }
type Task "T" { id string }
port TaskRepository "Repo" {
  direction outbound
  category  "persistence/task"
  method create(input: Task) -> Result<Task, never>
  method findById(id: string) -> Result<Task, never>
  verify integration "contract satisfied"
}
"#,
        )],
    );
    let diagnostics = ctx.diagnostics();

    // Port method signatures are extension-owned body syntax. The core grammar
    // cannot parse them, but they must NOT surface as E001 parse errors to the
    // user — the `port` entity itself parses fine (direction, category, verify).
    let e001: Vec<_> = diagnostics.iter().filter(|d| d.code == "E001").collect();
    assert!(
        e001.is_empty(),
        "port method body must not produce E001 parse errors, got: {:?}",
        e001
    );
}

// B:extension_owned_body_syntax — verify unit "type inline-union field syntax does not surface parse errors"
#[specforge_test(
    behavior = "extension_owned_body_syntax",
    verify = "extension-owned body syntax does not surface E001 parse errors"
)]
fn type_inline_union_field_does_not_surface_parse_errors() {
    let ctx = compile_with_builtins(
        &["@specforge/software"],
        &[(
            "main.spec",
            r#"spec "t" { version "0.1.0" }
type Manifest "M" {
  name        string
  query_scope string | string[]  @optional
}
"#,
        )],
    );
    let diagnostics = ctx.diagnostics();

    // Inline union field types (`string | string[]`) are extension-owned body
    // syntax the core grammar does not parse, but they must NOT surface as E001
    // parse errors — the `type` entity itself still resolves its other fields.
    let e001: Vec<_> = diagnostics.iter().filter(|d| d.code == "E001").collect();
    assert!(
        e001.is_empty(),
        "type inline-union field must not produce E001 parse errors, got: {:?}",
        e001
    );
}

// B:se_validate_port_methods — the primitives docs/entities/type.md and
// port.md document (`number`, `integer`, `boolean`, `timestamp`, and
// `never` for an impossible error channel) are valid port method types.
#[specforge_test(
    behavior = "se_validate_port_methods",
    verify = "port method with valid type references passes"
)]
fn port_method_documented_primitives_pass_e004() {
    let ctx = compile_with_builtins(
        &["@specforge/software"],
        &[(
            "main.spec",
            r#"spec "t" { version "0.1.0" }
port Clock "Clock" {
  direction outbound
  category  "time"
  method now() -> Result<timestamp, never>
  method exists(path: string) -> boolean
  method size(path: string) -> Result<integer, never>
  method ratio(a: number, b: number) -> number
  verify integration "contract satisfied"
}
port Broken "Broken" {
  direction outbound
  category  "broken"
  method send(payload: Undeclared) -> void
  verify integration "contract satisfied"
}
"#,
        )],
    );
    let diagnostics = ctx.diagnostics();

    let e004: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == "E004")
        .map(|d| d.message.as_str())
        .collect();
    // The undeclared type still fails, so the rule ran; the primitives don't.
    assert_eq!(
        e004,
        vec!["port 'Broken' method 'send' references unknown type 'Undeclared'"],
        "only the undeclared type is an E004"
    );
}

// Syntax the core grammar really fails on (a parenthesised annotation, a
// literal union, an inline object): the E001s it produces inside a `type`
// body are suppressed because `type` declares a body parser (plan 01, D10:
// the suppression was dead code, filtering a vector that held no E001).
#[specforge_test(
    behavior = "extension_owned_body_syntax",
    verify = "extension-owned body syntax does not surface E001 parse errors"
)]
fn type_body_syntax_the_grammar_rejects_does_not_surface_parse_errors() {
    let ctx = compile_with_builtins(
        &["@specforge/software"],
        &[(
            "main.spec",
            r#"type Money "Money" {
  amount   Decimal @min(0)
  currency "USD" | "EUR"
  meta     { key: string }
}
"#,
        )],
    );
    let diagnostics = ctx.diagnostics();

    let e001: Vec<_> = diagnostics.iter().filter(|d| d.code == "E001").collect();
    assert!(
        e001.is_empty(),
        "a body-parser kind's body must not produce E001 parse errors, got: {e001:?}"
    );
    assert!(
        ctx.graph().node("Money").is_some(),
        "the type itself is still in the graph"
    );
}

// B:parse_all_block_types — verify unit "a domain field named `kind` on a type is not flagged as an invalid meta-kind"
#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "a type field named kind is an ordinary field"
)]
fn type_domain_kind_field_is_an_ordinary_field() {
    let ctx = compile_with_builtins(
        &["@specforge/software"],
        &[(
            "main.spec",
            r#"spec "t" { version "0.1.0" }
type CodeActionKind = quick_fix | refactor | source
type CodeAction "Code Action" {
  title string
  kind  CodeActionKind
}
"#,
        )],
    );
    let diagnostics = ctx.diagnostics();

    // `kind` here is an ordinary struct field (of type CodeActionKind), not the
    // struct meta-attribute. It must NOT trip the type-kind enum constraint.
    let node = ctx.graph().node("CodeAction").expect("CodeAction compiled");
    assert_eq!(
        node.kind.raw.as_str(),
        "type",
        "the block keyword is still `type`"
    );
    assert!(
        matches!(
            node.fields.get("kind"),
            Some(specforge_parser::FieldValue::Identifier(t)) if t == "CodeActionKind"
        ),
        "`kind` is an ordinary field of the type: {:?}",
        node.fields
    );
    let about_kind: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.message.contains("'kind'"))
        .collect();
    assert!(
        about_kind.is_empty(),
        "a domain field named `kind` must not be diagnosed, got: {about_kind:?}"
    );
}

// B:build_in_memory_graph — verify unit "single entity roundtrip: parse→graph→json"
#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "graph contains one node per entity"
)]
fn single_entity_roundtrip() {
    let ctx = compile_specs(&[(
        "core.spec",
        r#"behavior login "User Login" {
    status planned
    contract "The system MUST authenticate users"
}
"#,
    )]);

    // Graph should contain exactly one node
    assert_eq!(
        ctx.graph().nodes().len(),
        1,
        "expected 1 node, got {}",
        ctx.graph().nodes().len()
    );
    let node = &ctx.graph().nodes()[0];
    assert_eq!(node.id.raw.as_str(), "login");
    assert_eq!(node.kind.raw.as_str(), "behavior");
    assert_eq!(node.title.as_deref(), Some("User Login"));

    // Emit as JSON — should produce valid JSON
    let json = specforge_emitter::json::emit_json(ctx.graph());
    let parsed: serde_json::Value =
        serde_json::from_str(&json).expect("emitted JSON must be valid");
    let nodes = parsed["nodes"].as_array().expect("must have nodes array");
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["id"].as_str().unwrap(), "login");
}

// B:resolve_use_imports — verify unit "multi-file resolution with imports"
#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "a use path resolves to the project source it names"
)]
fn multi_file_with_imports() {
    let (_dir, ctx) = compile_specs_in(&[
        (
            "types.spec",
            r#"type user_id "User ID" {
    format "UUID"
}
"#,
        ),
        (
            "behaviors.spec",
            r#"use "types"

behavior login "User Login" {
    status planned
    types [user_id]
}
"#,
        ),
    ]);
    let diagnostics = ctx.diagnostics();

    // Should have 2 nodes
    assert_eq!(ctx.graph().nodes().len(), 2, "expected 2 nodes");
    // Should have resolved the import (no E025 file-not-found errors)
    let file_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == "E025").collect();
    assert!(
        file_errors.is_empty(),
        "should not have file errors: {:?}",
        file_errors
    );

    // `use "types"` resolved to the file types.spec (extension appended,
    // path relative to the spec root), and only that.
    assert_eq!(
        specforge_resolver::import_target(&ctx.source_keys(), "behaviors.spec", "types"),
        Some("types.spec")
    );
    // The import's types edge links the two files' entities.
    assert!(
        ctx.graph()
            .edges()
            .iter()
            .any(|e| e.source == "login" && e.target == "user_id"),
        "{:?}",
        ctx.graph().edges()
    );

    // The path is looked up on disk: a `use` naming no file is E025.
    let missing = compile_specs(&[("main.spec", "use \"no_such_file\"\n")]).diagnostics();
    let e025: Vec<_> = missing.iter().filter(|d| d.code == "E025").collect();
    assert_eq!(e025.len(), 1, "{missing:?}");
    assert!(e025[0].message.contains("no_such_file"), "{:?}", e025[0]);
}

// B:link_entity_references — verify unit "cross-entity references produce edges"
#[specforge_test(
    behavior = "link_entity_references",
    verify = "reference list IDs create graph edges"
)]
fn cross_entity_references_produce_edges() {
    let ctx = compile_specs(&[(
        "core.spec",
        r#"invariant data_integrity "Data Integrity" {
    contract "Data MUST be consistent"
}

behavior save_record "Save Record" {
    status planned
    invariants [data_integrity]
}
"#,
    )]);

    assert_eq!(ctx.graph().nodes().len(), 2);
    assert!(
        !ctx.graph().edges().is_empty(),
        "should have at least one edge from behavior to invariant"
    );

    // Check edge connects the right nodes
    let edge = &ctx.graph().edges()[0];
    assert_eq!(edge.source.as_str(), "save_record");
    assert_eq!(edge.target.as_str(), "data_integrity");
}

// B:link_entity_references — verify unit "validation diagnostics surface through pipeline"
#[specforge_test(
    behavior = "link_entity_references",
    verify = "unresolvable reference produces E003"
)]
fn validation_diagnostics_surface() {
    let ctx = compile_specs(&[(
        "core.spec",
        r#"behavior broken "Broken" {
    status planned
    invariants [nonexistent_invariant]
}
"#,
    )]);
    let diagnostics = ctx.diagnostics();

    // Exactly one E003, naming the unresolved reference.
    let e003: Vec<_> = diagnostics.iter().filter(|d| d.code == "E003").collect();
    assert_eq!(
        e003.len(),
        1,
        "expected one E003 for the unresolved reference, got: {:?}",
        diagnostics
    );
    assert_eq!(e003[0].severity, specforge_common::Severity::Error);
    assert!(
        e003[0].message.contains("nonexistent_invariant"),
        "{:?}",
        e003[0]
    );
    // And no edge to the missing entity.
    assert!(ctx.graph().edges().is_empty(), "{:?}", ctx.graph().edges());
}

// B:build_in_memory_graph — verify unit "empty project produces empty graph"
#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "graph contains one node per entity"
)]
fn empty_project_produces_empty_graph() {
    let dir = TempDir::new().unwrap();
    let ctx = specforge_project::CompiledProject::compile(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        None,
    );

    assert!(
        ctx.graph().nodes().is_empty(),
        "empty project should have no nodes"
    );
    assert!(
        ctx.graph().edges().is_empty(),
        "empty project should have no edges"
    );
}

// B:serialize_json_graph — verify unit "all emit formats work on pipeline output"
#[specforge_test(behavior = "serialize_json_graph", verify = "output is valid JSON")]
fn all_emit_formats_work() {
    let ctx = compile_specs(&[(
        "core.spec",
        r#"behavior auth "Authentication" {
    status planned
    contract "Users MUST be authenticated"

    verify unit "credentials are validated"
}

invariant security "Security Invariant" {
    contract "All endpoints MUST be authenticated"
}
"#,
    )]);

    // JSON format
    let json = specforge_emitter::json::emit_json(ctx.graph());
    assert!(
        serde_json::from_str::<serde_json::Value>(&json).is_ok(),
        "JSON must be valid"
    );

    // Brief format
    let brief = export(ctx.graph(), specforge_emitter::EmitFormat::Brief);
    assert!(!brief.is_empty(), "brief must not be empty");
    assert!(brief.contains("auth"), "brief must mention entity");

    // Context format
    let context = export(ctx.graph(), specforge_emitter::EmitFormat::Context);
    assert!(!context.is_empty(), "context must not be empty");

    // DOT format
    let dot =
        specforge_emitter::dot::emit_dot(ctx.graph(), &specforge_emitter::DotOptions::default());
    assert!(dot.contains("digraph"), "DOT must contain digraph");

    assert!(
        ctx.graph().node_count() >= 2,
        "should have at least 2 entities"
    );
}
