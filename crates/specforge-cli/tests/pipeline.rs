use std::fs;
use std::sync::Arc;
use tempfile::TempDir;

/// Compile through the real Wasm runtime (post-migration equivalent of the
/// removed `specforge_emitter::compile` convenience).
fn compile(dir: &TempDir) -> specforge_project::CompiledProject {
    let runtime = specforge_component::ComponentRuntime::with_user_cache();
    specforge_project::CompiledProject::compile(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Some(Arc::new(runtime)),
    )
}
fn setup_project_with_extension(spec_content: &str) -> TempDir {
    let dir = TempDir::new().unwrap();

    let config = serde_json::json!({
        "name": "test-project",
        "version": "0.1.0",
        "extensions": ["@specforge/product"]
    });
    fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();

    fs::write(dir.path().join("test.spec"), spec_content).unwrap();

    dir
}

fn setup_project_without_extension(spec_content: &str) -> TempDir {
    let dir = TempDir::new().unwrap();

    let config = serde_json::json!({
        "name": "test-project",
        "version": "0.1.0",
        "extensions": []
    });
    fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    fs::write(dir.path().join("test.spec"), spec_content).unwrap();

    dir
}

#[test]
fn test_pipeline_with_product_extension_recognizes_feature() {
    let dir = setup_project_with_extension(
        r#"feature my_feature "Test Feature" {
    status proposed
    priority high
}"#,
    );

    let ctx = compile(&dir);
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
    let dir = setup_project_with_extension(
        r#"feature bad_feature "Bad Feature" {
    status invalid_status
}"#,
    );

    let ctx = compile(&dir);
    let diagnostics = ctx.diagnostics();

    // Should have W077 for invalid feature status
    let w077_diags: Vec<_> = diagnostics.iter().filter(|d| d.code == "W077").collect();
    assert!(
        !w077_diags.is_empty(),
        "expected W077 for invalid status, diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn test_pipeline_with_product_extension_validates_priority() {
    let dir = setup_project_with_extension(
        r#"feature f1 "Feature" {
    priority invalid_priority
}"#,
    );

    let ctx = compile(&dir);
    let diagnostics = ctx.diagnostics();

    // W078 should fire for invalid priority
    let w078_diags: Vec<_> = diagnostics.iter().filter(|d| d.code == "W078").collect();
    assert!(
        !w078_diags.is_empty(),
        "expected W078 for invalid priority, diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn test_pipeline_with_product_extension_detects_orphans() {
    let dir = setup_project_with_extension(
        r#"feature orphan_feature "Orphan" {
    status proposed
    priority high
}"#,
    );

    let ctx = compile(&dir);
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
    let dir = setup_project_without_extension(
        r#"feature my_feature "Test Feature" {
    status proposed
}"#,
    );

    let ctx = compile(&dir);
    let diagnostics = ctx.diagnostics();

    // No W077/W041 since no extension is loaded to provide those rules
    let extension_diags: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.starts_with("W0") && d.code.len() == 4)
        .collect();
    // Extension-specific warnings should not fire without the extension
    let product_codes: Vec<_> = extension_diags
        .iter()
        .filter(|d| {
            matches!(
                d.code.as_str(),
                "W041" | "W042" | "W044" | "W077" | "W078" | "W079" | "W080"
            )
        })
        .collect();
    assert!(
        product_codes.is_empty(),
        "expected no product extension diagnostics without extension, got: {:?}",
        product_codes
    );
}

#[test]
fn test_pipeline_multiple_entity_kinds() {
    let dir = setup_project_with_extension(
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

    let ctx = compile(&dir);
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
    let dir = setup_project_with_extension(r#"feature f1 "Test" { status proposed }"#);

    let ctx = compile(&dir);

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
    let dir = setup_project_with_extension(
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

    let ctx = compile(&dir);
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
    let dir = setup_project_with_extension(
        r#"module mod_a "Module A" {
    depends_on [mod_b]
}

module mod_b "Module B" {
    depends_on [mod_a]
}
"#,
    );

    let ctx = compile(&dir);
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
    let dir = setup_project_with_extension(
        r#"module mod_a "Module A" {
    depends_on [mod_b]
}

module mod_b "Module B" {}

module mod_c "Module C" {
    depends_on [mod_b]
}
"#,
    );

    let ctx = compile(&dir);
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
    let dir = setup_project_with_extension(
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

    let ctx = compile(&dir);
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
    let dir = setup_project_with_extension(
        r#"feature f1 "Deferred Feature" {
    status deferred
    priority low
}"#,
    );

    let ctx = compile(&dir);
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
    let dir = setup_project_with_extension(
        r#"feature f1 "Deferred Feature" {
    status deferred
    priority low
    reason "Postponed to v2"
}"#,
    );

    let ctx = compile(&dir);
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
    let dir = setup_project_with_extension(
        r#"feature f1 "Feature" {
    status done
    priority high
}

milestone m1 "Done Milestone" {
    status completed
    features [f1]
}"#,
    );

    let ctx = compile(&dir);
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
    let dir = setup_project_with_extension(
        r#"feature broken_feat "Missing problem" {
    status proposed
    priority high
}

journey broken_journey "Missing flow" {
    description "no flow field"
}
"#,
    );

    let ctx = compile(&dir);
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
    let dir = setup_project_with_extension(
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

    let ctx = compile(&dir);
    let diagnostics = ctx.diagnostics();

    let e006_diags: Vec<_> = diagnostics.iter().filter(|d| d.code == "E006").collect();
    assert!(
        e006_diags.is_empty(),
        "Expected no E006 when required fields present, got: {:?}",
        e006_diags
    );
}
