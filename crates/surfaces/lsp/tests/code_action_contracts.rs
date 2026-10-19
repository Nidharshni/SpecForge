//! The code-action contracts: the entity stub and the verify stub (ADR 0161).

use crate::contracts::verifiable;
use crate::served::buffers;
use specforge_test_macros::test as specforge_test;

// B:code_action_create_entity_stub — verify contract "requires/ensures consistency for create entity stub"
#[specforge_test(
    behavior = "code_action_create_entity_stub",
    verify = "Code Action: Create Entity Stub: create entity stub holds — graph_available, field_registry_available, stub_created, kind_inferred, no_code_generated"
)]
fn code_action_create_entity_stub_contract() {
    // Requires: an E003 for an id no entity has + the enclosing field's
    // target kind in the FieldRegistry
    // Ensures: a refactoring that appends a bare stub of that kind to the
    // current file; none without a target kind
    let text = "behavior login \"L\" {\n  invariants [missing_inv]\n}\n";
    let state = buffers(&[("/p/auth.spec", text)]);
    let diagnostics = state.session().unwrap().project().diagnostics();
    let fixes_with = |target_kind: Option<&str>| {
        let project = specforge_project::testing::Assembled::over(invariants_field(target_kind))
            .graph(state.graph().clone());
        let texts = project
            .detached()
            .with_source_texts(state.session().unwrap().project().source_texts());
        let view = specforge_ops::view::ProjectView::of(&texts);
        let nav = specforge_ops::read::navigate::Navigator::new(view);
        nav.fixes(
            &diagnostics,
            &specforge_ops::read::navigate::FixQuery::default(),
        )
    };

    let fixes = fixes_with(Some("invariant"));
    let stub = fixes
        .iter()
        .find(|f| f.kind == specforge_ops::read::navigate::FixKind::Refactor)
        .expect("entity stub must be created with target_kind");
    assert_eq!(stub.title, "Create invariant stub for missing_inv");
    let edit = &stub.edits[0];
    assert!(
        edit.new_text.contains("invariant missing_inv"),
        "stub must use correct kind and ID"
    );
    assert_eq!(
        edit.span.file, "/p/auth.spec",
        "stub must target current file"
    );
    assert!(!edit.new_text.contains("fn "), "no application code");

    assert!(
        fixes_with(None)
            .iter()
            .all(|f| f.kind != specforge_ops::read::navigate::FixKind::Refactor),
        "no stub without target_kind"
    );
}

/// `behavior.invariants` as a reference list targeting `target_kind`.
fn invariants_field(target_kind: Option<&str>) -> specforge_registry::RegistryBuild {
    crate::registries::registries("@test/ext", |c| {
        c.kind("behavior", |k| {
            k.field("invariants", |f| {
                f.field_type(specforge_extension_sdk::prelude::FieldType::ReferenceList);
                if let Some(target_kind) = target_kind {
                    f.target_kind(target_kind);
                }
            });
        });
        if let Some(target_kind) = target_kind {
            c.kind(target_kind, |k| {
                k.keyword(target_kind);
            });
        }
    })
}

#[specforge_test(
    behavior = "code_actions_for_missing_verify",
    verify = "Code Actions for Missing Verify: missing verify code actions holds — kind_registry_available, graph_available, quickfix_offered, verify_stubs_produced, no_code_generated"
)]
fn code_actions_for_missing_verify_contract() {
    // Requires: testable entity without verify statements
    // Ensures: quickfix code action with verify stub targeting the .spec file
    let text = "\n\n\n\nbehavior my_behavior \"B\" {\n  contract \"c\"\n}\n";
    let state = buffers(&[("/p/a.spec", text)]);
    let project = specforge_project::testing::Assembled::over(verifiable(&["behavior"], &[]))
        .graph(state.graph().clone());
    let texts = project
        .detached()
        .with_source_texts(state.session().unwrap().project().source_texts());
    let view = specforge_ops::view::ProjectView::of(&texts);
    let nav = specforge_ops::read::navigate::Navigator::new(view);
    let fixes = nav.fixes(&[], &specforge_ops::read::navigate::FixQuery::default());

    assert!(
        !fixes.is_empty(),
        "untested testable entity must produce code action"
    );
    assert_eq!(fixes[0].subject, Some("my_behavior".into()));
    assert_eq!(
        fixes[0].kind,
        specforge_ops::read::navigate::FixKind::QuickFix,
        "must be quickfix action"
    );
    let edit = &fixes[0].edits[0];
    assert!(
        edit.new_text.contains("verify unit"),
        "stub must include verify statement"
    );
    assert!(
        edit.span.file.as_str().ends_with(".spec"),
        "edit must target .spec file"
    );
    assert_eq!(edit.span.start_line, 7, "before the block's closing brace");
}
