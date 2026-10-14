//! E2E pipeline tests: parse → resolve → graph → validate → emit
//! These tests exercise the full compilation pipeline from spec source files
//! to emitted output, verifying roundtrip integrity.

use specforge_project::testing::TestProject;
use specforge_test::prelude::*;
use std::sync::Arc;

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
fn compile_specs_in(files: &[(&str, &str)]) -> (TestProject, specforge_project::CompiledProject) {
    let dir = TestProject::new().without_config().files(files);
    let compiled = dir.compile_bare();
    (dir, compiled)
}

/// Helper: compile with the builtin runtime for the given extensions (full pipeline).
fn compile_with_builtins(
    extensions: &[&str],
    files: &[(&str, &str)],
) -> specforge_project::CompiledProject {
    let dir = TestProject::new()
        .enabling(extensions)
        .config(|c| c["spec_root"] = "spec".into());
    let dir = files.iter().fold(dir, |dir, (name, content)| {
        dir.file(&format!("spec/{name}"), content)
    });
    let runtime = wasm_runtime_for(extensions);
    dir.compile_in(Arc::new(runtime))
}

// Port method signatures are read by the core grammar: no E001, and none is
// hidden (a kind's has_body_parser flag suppresses nothing).
#[test]
fn port_method_body_reads_without_a_parse_error() {
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

    // Port method signatures are read by the core grammar: no E001, and none
    // is hidden — the `port` entity parses fine (direction, category, verify).
    let e001: Vec<_> = diagnostics.iter().filter(|d| d.code == "E001").collect();
    assert!(
        e001.is_empty(),
        "port method body must not produce E001 parse errors, got: {:?}",
        e001
    );
}

#[test]
fn type_inline_union_field_reads_without_a_parse_error() {
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

    // Inline union field types (`string | string[]`) are read by the core
    // grammar: no E001 — the `type` entity still resolves its other fields.
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

// A parenthesised annotation and an inline object in a `type` body are syntax
// the core grammar does not read. Their E001s are reported like any other: a
// kind's has_body_parser flag hides nothing.
#[test]
fn type_body_syntax_the_grammar_rejects_is_reported() {
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
    assert_eq!(e001.len(), 2, "{e001:?}");
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
    let json =
        specforge_emitter::emit(ctx.graph(), &specforge_emitter::EmitOptions::default()).unwrap();
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
    let dir = TestProject::new().without_config();
    let ctx = dir.compile_bare();

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
    let json =
        specforge_emitter::emit(ctx.graph(), &specforge_emitter::EmitOptions::default()).unwrap();
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

#[specforge_test(
    behavior = "recover_from_syntax_errors",
    verify = "a string or body left unclosed is reported in every entity, whatever its kind"
)]
fn an_unclosed_port_body_reports_its_syntax_error() {
    let ctx = compile_with_builtins(
        &["@specforge/software"],
        &[(
            "main.spec",
            "spec \"t\" { version \"0.1.0\" }\nport pay \"Pay\" {\n  direction inbound\n  method charge(amount: string) -> string\n\nbehavior settle \"Settle\" {\n  contract \"Settles.\"\n}\n",
        )],
    );
    let diagnostics = ctx.diagnostics();
    let e001: Vec<_> = diagnostics.iter().filter(|d| d.code == "E001").collect();
    assert_eq!(e001.len(), 1, "{e001:?}");
    let span = e001[0].span.as_ref().expect("a span");
    assert_eq!((span.start_line, span.start_col), (2, 16));
    assert!(
        e001[0].message.contains("unclosed block"),
        "{}",
        e001[0].message
    );
}

#[specforge_test(
    behavior = "recover_from_syntax_errors",
    verify = "a string or body left unclosed is reported in every entity, whatever its kind"
)]
fn an_unclosed_string_in_a_port_body_reports_its_syntax_error() {
    let ctx = compile_with_builtins(
        &["@specforge/software"],
        &[(
            "main.spec",
            "spec \"t\" { version \"0.1.0\" }\nport pay \"Pay\" {\n  direction inbound\n  description \"Takes payments\n  method charge(amount: string) -> string\n}\n\nbehavior settle \"Settle\" {\n  contract \"Settles.\"\n}\n",
        )],
    );
    let diagnostics = ctx.diagnostics();
    let e001: Vec<_> = diagnostics.iter().filter(|d| d.code == "E001").collect();
    assert_eq!(e001.len(), 1, "{e001:?}");
    let span = e001[0].span.as_ref().expect("a span");
    assert_eq!((span.start_line, span.start_col), (4, 15));
    assert!(
        e001[0].message.contains("unclosed string"),
        "{}",
        e001[0].message
    );
}

// ── The pipeline over projects the fixture builds (moved from the CLI's tests) ──

/// A project enabling `@specforge/product`, its one spec in `test.spec`.
fn product_project(spec_content: &str) -> TestProject {
    TestProject::new()
        .config(|c| c["name"] = "test-project".into())
        .enabling(&["@specforge/product"])
        .file("test.spec", spec_content)
}

/// A project enabling no extension, its one spec in `test.spec`.
fn bare_project(spec_content: &str) -> TestProject {
    TestProject::new()
        .config(|c| c["name"] = "test-project".into())
        .file("test.spec", spec_content)
}

#[test]
fn test_pipeline_with_product_extension_recognizes_feature() {
    let dir = product_project(
        r#"feature my_feature "Test Feature" {
    status proposed
    priority high
}"#,
    );

    let ctx = dir.compile();
    let diagnostics = ctx.diagnostics();

    // Should NOT have I004 warnings about unrecognized keyword
    let i004_diags: Vec<_> = diagnostics.iter().filter(|d| d.code == "I004").collect();
    assert!(
        i004_diags.is_empty(),
        "expected no I004 for 'feature', got: {:?}",
        i004_diags
    );

    // Graph should contain the feature node
    assert!(ctx.graph().node("my_feature").is_some());
    assert_eq!(ctx.graph().node("my_feature").unwrap().kind.raw, "feature");
}

#[test]
fn test_pipeline_with_product_extension_runs_validation() {
    let dir = product_project(
        r#"feature bad_feature "Bad Feature" {
    status invalid_status
}"#,
    );

    let ctx = dir.compile();
    let diagnostics = ctx.diagnostics();

    // The closed vocabulary is checked by the host: one E061, no retired W077
    let e061: Vec<_> = diagnostics.iter().filter(|d| d.code == "E061").collect();
    assert_eq!(
        e061.len(),
        1,
        "expected one E061, diagnostics: {diagnostics:?}"
    );
    assert!(
        e061[0].message.contains("'status'") && e061[0].message.contains("invalid_status"),
        "{:?}",
        e061[0]
    );
    assert!(
        diagnostics.iter().all(|d| d.code != "W077"),
        "{diagnostics:?}"
    );
}

#[test]
fn test_pipeline_with_product_extension_validates_priority() {
    let dir = product_project(
        r#"feature f1 "Feature" {
    priority invalid_priority
}"#,
    );

    let ctx = dir.compile();
    let diagnostics = ctx.diagnostics();

    // The closed vocabulary is checked by the host: one E061, no retired W078
    let e061: Vec<_> = diagnostics.iter().filter(|d| d.code == "E061").collect();
    assert_eq!(
        e061.len(),
        1,
        "expected one E061, diagnostics: {diagnostics:?}"
    );
    assert!(
        e061[0].message.contains("'priority'") && e061[0].message.contains("invalid_priority"),
        "{:?}",
        e061[0]
    );
    assert!(
        diagnostics.iter().all(|d| d.code != "W078"),
        "{diagnostics:?}"
    );
}

#[test]
fn test_pipeline_with_product_extension_detects_orphans() {
    let dir = product_project(
        r#"feature orphan_feature "Orphan" {
    status proposed
    priority high
}"#,
    );

    let ctx = dir.compile();
    let diagnostics = ctx.diagnostics();

    // W041 should fire for orphan feature (no incoming edges)
    let w041_diags: Vec<_> = diagnostics.iter().filter(|d| d.code == "W041").collect();
    assert!(
        !w041_diags.is_empty(),
        "expected W041 for orphan feature, diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn test_pipeline_without_extension_no_extension_validation() {
    let dir = bare_project(
        r#"feature my_feature "Test Feature" {
    status proposed
}"#,
    );

    let ctx = dir.compile();
    let diagnostics = ctx.diagnostics();

    // No W041 since no extension is loaded to provide those rules
    let extension_diags: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.starts_with("W0") && d.code.len() == 4)
        .collect();
    // Extension-specific warnings should not fire without the extension
    let product_codes: Vec<_> = extension_diags
        .iter()
        .filter(|d| matches!(d.code.as_str(), "W041" | "W042" | "W044"))
        .collect();
    assert!(
        product_codes.is_empty(),
        "expected no product extension diagnostics without extension, got: {:?}",
        product_codes
    );
}

#[test]
fn test_pipeline_multiple_entity_kinds() {
    let dir = product_project(
        r#"feature f1 "Feature One" {
    status proposed
    priority high
}

milestone m1 "Milestone One" {
    status planned
    features [f1]
}

module mod1 "Module One" {
    features [f1]
}
"#,
    );

    let ctx = dir.compile();
    let diagnostics = ctx.diagnostics();

    // All 3 entities should be in the graph
    assert!(ctx.graph().node("f1").is_some());
    assert!(ctx.graph().node("m1").is_some());
    assert!(ctx.graph().node("mod1").is_some());

    // Edges should exist
    let edges = ctx.graph().edges_from("m1");
    assert!(!edges.is_empty(), "milestone m1 should have edges to f1");

    // f1 is referenced by m1 and mod1, so W041 (orphan feature) should NOT fire
    let w041_diags: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == "W041" && d.message.contains("f1"))
        .collect();
    assert!(
        w041_diags.is_empty(),
        "f1 is referenced, should not be orphan: {:?}",
        w041_diags
    );
}

#[test]
fn test_pipeline_registries_populated() {
    let dir = product_project(r#"feature f1 "Test" { status proposed }"#);

    let ctx = dir.compile();

    // Kind registry should have all 9 product entity kinds
    assert!(ctx.environment().registries().kinds().contains("feature"));
    assert!(ctx.environment().registries().kinds().contains("journey"));
    assert!(
        ctx.environment()
            .registries()
            .kinds()
            .contains("deliverable")
    );
    assert!(ctx.environment().registries().kinds().contains("milestone"));
    assert!(ctx.environment().registries().kinds().contains("module"));
    assert!(ctx.environment().registries().kinds().contains("term"));
    assert!(ctx.environment().registries().kinds().contains("persona"));
    assert!(ctx.environment().registries().kinds().contains("channel"));
    assert!(ctx.environment().registries().kinds().contains("release"));
}

#[test]
fn test_pipeline_valid_project_zero_errors() {
    let dir = product_project(
        r#"persona dev "Developer" {
    description "A software developer"
    status active
}

channel cli "CLI" {
    description "Command-line interface"
    status active
}

feature f1 "Core Feature" {
    problem "Users need a core feature"
    status proposed
    priority high
}

journey j1 "Dev Flow" {
    persona dev
    channels [cli]
    features [f1]
    description "Developer uses CLI"
    flow ["Open CLI", "Run command", "See output"]
}

module mod1 "Core Module" {
    features [f1]
}

milestone m1 "Launch" {
    status planned
    features [f1]
    modules [mod1]
}

deliverable d1 "CLI App" {
    artifact_type cli
    status draft
    journeys [j1]
    modules [mod1]
    milestones [m1]
}

term specification "Specification" {
    definition "A formal description of system behavior"
}
"#,
    );

    let ctx = dir.compile();
    let diagnostics = ctx.diagnostics();

    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == specforge_common::Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "expected zero errors for valid project, got: {:?}",
        errors
    );
}

#[test]
fn test_pipeline_detects_module_dependency_cycle() {
    let dir = product_project(
        r#"module mod_a "Module A" {
    depends_on [mod_b]
}

module mod_b "Module B" {
    depends_on [mod_a]
}
"#,
    );

    let ctx = dir.compile();
    let diagnostics = ctx.diagnostics();

    let e007_diags: Vec<_> = diagnostics.iter().filter(|d| d.code == "E007").collect();
    assert!(
        !e007_diags.is_empty(),
        "expected E007 for module cycle, diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn test_pipeline_no_cycle_for_linear_deps() {
    let dir = product_project(
        r#"module mod_a "Module A" {
    depends_on [mod_b]
}

module mod_b "Module B" {}

module mod_c "Module C" {
    depends_on [mod_b]
}
"#,
    );

    let ctx = dir.compile();
    let diagnostics = ctx.diagnostics();

    let cycle_diags: Vec<_> = diagnostics.iter().filter(|d| d.code == "E007").collect();
    assert!(
        cycle_diags.is_empty(),
        "expected no E007 for linear deps, got: {:?}",
        cycle_diags
    );
}

#[test]
fn test_pipeline_detects_feature_dependency_cycle() {
    let dir = product_project(
        r#"feature f1 "Feature 1" {
    depends_on [f2]
    status proposed
    priority high
}

feature f2 "Feature 2" {
    depends_on [f1]
    status proposed
    priority high
}
"#,
    );

    let ctx = dir.compile();
    let diagnostics = ctx.diagnostics();

    let w045_diags: Vec<_> = diagnostics.iter().filter(|d| d.code == "W045").collect();
    assert!(
        !w045_diags.is_empty(),
        "expected W045 for feature cycle, diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn test_pipeline_conditional_deferred_feature_without_reason() {
    let dir = product_project(
        r#"feature f1 "Deferred Feature" {
    status deferred
    priority low
}"#,
    );

    let ctx = dir.compile();
    let diagnostics = ctx.diagnostics();

    let i059_diags: Vec<_> = diagnostics.iter().filter(|d| d.code == "I059").collect();
    assert!(
        !i059_diags.is_empty(),
        "expected I059 for deferred feature without reason, diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn test_pipeline_conditional_deferred_feature_with_reason_no_warning() {
    let dir = product_project(
        r#"feature f1 "Deferred Feature" {
    status deferred
    priority low
    reason "Postponed to v2"
}"#,
    );

    let ctx = dir.compile();
    let diagnostics = ctx.diagnostics();

    let i059_diags: Vec<_> = diagnostics.iter().filter(|d| d.code == "I059").collect();
    assert!(
        i059_diags.is_empty(),
        "expected no I059 when reason is present, got: {:?}",
        i059_diags
    );
}

#[test]
fn test_pipeline_completed_milestone_without_exit_criteria() {
    let dir = product_project(
        r#"feature f1 "Feature" {
    status done
    priority high
}

milestone m1 "Done Milestone" {
    status completed
    features [f1]
}"#,
    );

    let ctx = dir.compile();
    let diagnostics = ctx.diagnostics();

    let w057_diags: Vec<_> = diagnostics.iter().filter(|d| d.code == "W057").collect();
    assert!(
        !w057_diags.is_empty(),
        "expected W057 for completed milestone without exit_criteria, diagnostics: {:?}",
        diagnostics
    );
}
#[test]
fn test_pipeline_e006_fires_for_missing_required_fields() {
    let dir = product_project(
        r#"feature broken_feat "Missing problem" {
    status proposed
    priority high
}

journey broken_journey "Missing flow" {
    description "no flow field"
}
"#,
    );

    let ctx = dir.compile();
    let diagnostics = ctx.diagnostics();

    let e006_diags: Vec<_> = diagnostics.iter().filter(|d| d.code == "E006").collect();

    assert!(
        e006_diags.iter().any(|d| d.message.contains("problem")),
        "Expected E006 for feature missing 'problem', got: {:?}",
        e006_diags
    );
    assert!(
        e006_diags.iter().any(|d| d.message.contains("flow")),
        "Expected E006 for journey missing 'flow', got: {:?}",
        e006_diags
    );
}

#[test]
fn test_pipeline_e006_silent_when_required_fields_present() {
    let dir = product_project(
        r#"feature f1 "Complete" {
    problem "Users need this"
    status proposed
    priority high
}

journey j1 "Complete" {
    flow ["step 1", "step 2"]
    features [f1]
}
"#,
    );

    let ctx = dir.compile();
    let diagnostics = ctx.diagnostics();

    let e006_diags: Vec<_> = diagnostics.iter().filter(|d| d.code == "E006").collect();
    assert!(
        e006_diags.is_empty(),
        "Expected no E006 when required fields present, got: {:?}",
        e006_diags
    );
}
