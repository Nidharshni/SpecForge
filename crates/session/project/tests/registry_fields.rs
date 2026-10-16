/// Tests link themselves to entities by annotation (ADR 0002): no extension
/// registers a spec-side `tests` field any more.
#[test]
fn no_spec_side_tests_field_is_registered() {
    let exts = vec![
        "@specforge/software".to_string(),
        "@specforge/testing".to_string(),
        "@specforge/formal".to_string(),
    ];
    let runtime = specforge_component::ComponentRuntime::new();
    let builtins = specforge_installed::Builtins(specforge_component::builtins::BUILTIN_EXTENSIONS);
    let declarations = specforge_installed::Installed::none()
        .load(&exts, &builtins, &runtime, &specforge_registry::PlainDisk)
        .declarations;
    let build = specforge_registry::build_registries(declarations);
    let field_reg = build.fields();
    assert!(field_reg.contains("behavior", "invariants"));
    assert!(!field_reg.contains("behavior", "tests"));
    assert!(!field_reg.contains("invariant", "tests"));
}
