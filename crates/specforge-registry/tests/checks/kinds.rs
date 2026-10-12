//! E024: an entity of a kind no loaded extension declares, with the
//! extension that would provide it; and the gate that leaves a project with
//! no entity kind unchecked.

use specforge_common::{SourceSpan, Sym};
use specforge_extension_sdk::prelude::*;
use specforge_registry::entity::EntityRecord;
use specforge_test_macros::test as spec;

use crate::support::{build, check, coded_in, declare, software, span};

#[spec(
    behavior = "detect_unknown_entity_kinds",
    verify = "unregistered keyword produces E024"
)]
fn detect_unknown_kinds_e024() {
    let build = build([software()]);
    let entities = vec![EntityRecord::new("unknown_thing", "u1", span("test.spec"))];
    let diags = check(&build, &entities);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, "E024");
}

#[spec(
    behavior = "detect_unknown_entity_kinds",
    verify = "E024 includes keyword name and source span"
)]
fn detect_unknown_kinds_e024_includes_info() {
    let build = build([software()]);
    let s = SourceSpan {
        file: Sym::new("my/file.spec"),
        start_line: 42,
        start_col: 0,
        end_line: 42,
        end_col: 10,
    };
    let entities = vec![EntityRecord::new("unknown_thing", "u1", &s)];
    let diags = check(&build, &entities);
    assert!(diags[0].message.contains("unknown_thing"));
    assert!(diags[0].message.contains("my/file.spec"));
    assert_eq!(diags[0].span.as_ref().unwrap().start_line, 42);
}

#[spec(
    behavior = "detect_unknown_entity_kinds",
    verify = "registered keyword does not produce E024"
)]
fn detect_unknown_kinds_registered_no_e024() {
    let build = build([software()]);
    let entities = vec![EntityRecord::new("behavior", "b1", span("test.spec"))];
    let diags = check(&build, &entities);
    assert!(diags.is_empty(), "{diags:?}");
}

#[spec(
    behavior = "detect_unknown_entity_kinds",
    verify = "define-block keywords not checked against KindRegistry"
)]
fn detect_unknown_kinds_define_not_checked() {
    // The structural keywords are no extension's kind.
    let entities = vec![
        EntityRecord::new("define", "my_define", span("test.spec")),
        EntityRecord::new("spec", "my_spec", span("test.spec")),
    ];
    let diags = check(&build([software()]), &entities);
    assert!(coded_in(&diags, "E024").is_empty(), "{diags:?}");
    // With no kind registered nothing at all is reported (the gate).
    assert!(check(&build([]), &entities).is_empty());
}

#[spec(
    behavior = "detect_unknown_entity_kinds",
    verify = "Detect Unknown Entity Kinds: unknown entity kind detection holds — registries_populated_fired, structural_parse_ready, unknown_kinds_diagnosed, registered_kinds_accepted"
)]
fn detect_unknown_kinds_contract() {
    let build = build([software()]);
    let unknown = vec![EntityRecord::new("xyzzy", "x1", span("t.spec"))];
    assert!(
        check(&build, &unknown).iter().any(|d| d.code == "E024"),
        "unknown → E024"
    );
    let known = vec![EntityRecord::new("behavior", "b1", span("t.spec"))];
    assert!(check(&build, &known).is_empty(), "registered → no E024");
}

#[spec(
    behavior = "suggest_missing_extensions",
    verify = "E024 for keyword in index suggests the providing extension"
)]
fn suggest_missing_ext_known_keyword() {
    // `feature` is a builtin extension's keyword the bundled index knows.
    let diags = check(
        &build([software()]),
        &[EntityRecord::new("feature", "f1", span("test.spec"))],
    );
    let e024 = coded_in(&diags, "E024");
    assert!(
        e024[0]
            .suggestion
            .as_deref()
            .unwrap()
            .contains("specforge add @specforge/product"),
        "{diags:?}"
    );
}

#[spec(
    behavior = "suggest_missing_extensions",
    verify = "E024 for keyword not in index suggests specforge search"
)]
fn suggest_missing_ext_unknown_keyword() {
    let diags = check(
        &build([software()]),
        &[EntityRecord::new("xyzzy", "x1", span("test.spec"))],
    );
    let e024 = coded_in(&diags, "E024");
    assert!(
        e024[0]
            .suggestion
            .as_deref()
            .unwrap()
            .contains("specforge search xyzzy"),
        "{diags:?}"
    );
}

#[spec(
    behavior = "suggest_missing_extensions",
    verify = "Suggest Missing Extensions: missing extension suggestions holds — e024_diagnostic_emitted, suggestion_provided, lazy_loading_enforced"
)]
fn suggest_missing_ext_contract() {
    let build = build([software()]);
    // ensures: a keyword the index knows gets the extension suggestion.
    let d1 = check(
        &build,
        &[EntityRecord::new("feature", "f1", span("test.spec"))],
    );
    assert!(
        d1[0]
            .suggestion
            .as_ref()
            .unwrap()
            .contains("@specforge/product")
    );
    // ensures: one it does not gets the search suggestion.
    let d2 = check(
        &build,
        &[EntityRecord::new("xyzzy", "x1", span("test.spec"))],
    );
    assert!(
        d2[0]
            .suggestion
            .as_ref()
            .unwrap()
            .contains("specforge search")
    );
}

#[spec(
    behavior = "two_phase_validate_semantic",
    verify = "known keyword passes semantic validation"
)]
fn a_known_keyword_passes_semantic_validation() {
    let diags = check(
        &build([software()]),
        &[EntityRecord::new("behavior", "my_beh", span("test.spec"))],
    );
    assert!(diags.is_empty(), "{diags:?}");
}

#[spec(
    behavior = "two_phase_validate_semantic",
    verify = "unknown keyword produces E024"
)]
fn an_unknown_keyword_produces_e024() {
    let diags = check(
        &build([software()]),
        &[EntityRecord::new("xyzzy", "my_xyz", span("test.spec"))],
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == "E024" && d.message.contains("xyzzy")),
        "{diags:?}"
    );
}

#[spec(
    behavior = "two_phase_validate_semantic",
    verify = "Phase 2 starts only after registries populated"
)]
fn phase_2_starts_only_after_registries_populated() {
    let entities = vec![EntityRecord::new("behavior", "my_beh", span("test.spec"))];
    // Before any kind is registered the kind checks do not run: nothing is
    // reported, not even E024 for every keyword.
    assert!(check(&build([]), &entities).is_empty());
    // Populated by an extension that does not declare `behavior`, the
    // registry makes it unknown.
    let other = declare("@test/other", |c| {
        c.kind("Thing", |k| {
            k.keyword("thing");
        });
    });
    assert!(
        check(&build([other]), &entities)
            .iter()
            .any(|d| d.code == "E024")
    );
    // Populated by the software extension, it passes.
    assert!(check(&build([software()]), &entities).is_empty());
}

#[spec(
    behavior = "two_phase_validate_semantic",
    verify = "Two-Phase Validate: Semantic: semantic validation holds — unknown_keywords_diagnosed"
)]
fn two_phase_validate_semantic_contract() {
    let build = build([software()]);
    // ensures: all blocks checked — known passes, unknown diagnosed.
    let entities = vec![
        EntityRecord::new("behavior", "b1", span("a.spec")),
        EntityRecord::new("xyzzy", "x1", span("b.spec")),
    ];
    let diags = check(&build, &entities);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, "E024");
    // ensures: fields validated.
    let with_bad_field = vec![
        EntityRecord::new("behavior", "b1", span("a.spec")).with_fields(&["contract", "bad_field"]),
    ];
    assert!(
        check(&build, &with_bad_field)
            .iter()
            .any(|d| d.code == "W020")
    );
}

/// With no entity kind registered, E024, E013, E014, W020, E022 and E061 do
/// not run; W012 and the rules still do (and W151 says what is unchecked). (E016 needs a declared kind to hold
/// its `file_reference` field.)
#[spec(
    behavior = "check_entities_in_one_order",
    verify = "with no entity kind registered, E024, E013, E014, W020, E022 and E061 do not run, and W012, E016 and the rules still do"
)]
fn a_build_with_no_kind_checks_no_kind() {
    let records = vec![
        EntityRecord::new("wibble", "x", span("t.spec")).with_fields(&["bogus"]),
        EntityRecord::new("behavior", "define", span("t.spec")),
    ];
    // Nothing declared: nothing reported.
    assert!(check(&build([]), &records).is_empty());
    // A ref nobody references is still W012.
    let with_ref = [
        records.clone(),
        vec![EntityRecord::new("ref", "gh.issue:1", span("t.spec"))],
    ]
    .concat();
    let diags = check(&build([]), &with_ref);
    let codes: Vec<&str> = diags.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["W012"], "{diags:?}");

    // One declaration with a rule and no kind: the rule and the notice report.
    let kindless = declare("@test/kindless", |c| {
        c.rule("W902", |r| {
            r.check(CheckKind::NoIncomingEdges)
                .severity(ValidationSeverity::Warning)
                .message_template("{kind} '{id}' is not referenced");
        });
    });
    let build = build([kindless]);
    assert!(build.structural_only());
    let diags = check(&build, &with_ref);
    let codes: Vec<&str> = diags.iter().map(|d| d.code.as_str()).collect();
    // W012, then the one notice that the kinds are unchecked (W151), then
    // the rule.
    assert_eq!(codes, ["W012", "W151", "W902", "W902", "W902"], "{diags:?}");
}

#[spec(
    behavior = "check_entities_in_one_order",
    verify = "extensions that load but declare no entity kind report one W151 naming the unchecked entities"
)]
fn kindless_extensions_report_one_w151() {
    let kindless = build([declare("@test/kindless", |_| {})]);
    assert!(kindless.structural_only());

    // Two non-structural entities: one W151 naming how many and which kinds.
    let diags = check(
        &kindless,
        &[
            EntityRecord::new("wibble", "wb", span("t.spec")),
            EntityRecord::new("behavior", "b1", span("t.spec")),
            EntityRecord::new("wibble", "wb2", span("t.spec")),
            EntityRecord::new("spec", "s", span("t.spec")),
        ],
    );
    let w151 = coded_in(&diags, "W151");
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(
        w151[0].message,
        "the loaded extensions declare no entity kind: 3 entities (kinds: behavior, wibble) are not checked against kinds, fields or identifiers"
    );
    assert_eq!(
        w151[0].suggestion.as_deref(),
        Some("install it with: specforge add @specforge/software")
    );
    assert!(w151[0].span.is_none(), "it is about the project");

    // One entity: singular; a kind no builtin declares: search for it.
    let one = check(
        &kindless,
        &[EntityRecord::new("xyzzy", "x1", span("t.spec"))],
    );
    assert_eq!(
        one[0].message,
        "the loaded extensions declare no entity kind: 1 entity (kinds: xyzzy) is not checked against kinds, fields or identifiers"
    );
    assert_eq!(
        one[0].suggestion.as_deref(),
        Some("enable the extension that declares them; search with: specforge search xyzzy")
    );

    // Seven kinds: five named, then "…".
    let many: Vec<EntityRecord> = ["k1", "k2", "k3", "k4", "k5", "k6", "k7"]
        .iter()
        .map(|kind| EntityRecord::new(kind, &format!("{kind}_e"), span("t.spec")))
        .collect();
    let diags = check(&kindless, &many);
    assert!(
        diags[0]
            .message
            .contains("7 entities (kinds: k1, k2, k3, k4, k5, …)"),
        "{diags:?}"
    );

    // Only structural entities, or none: nothing is left unchecked.
    let structural = [
        EntityRecord::new("ref", "gh.issue:1", span("t.spec")).with_edges(
            specforge_registry::entity::Direction::Incoming,
            Some("References"),
            1,
        ),
        EntityRecord::new("spec", "s", span("t.spec")),
    ];
    assert!(check(&kindless, &structural).is_empty());
    assert!(check(&kindless, &[]).is_empty());

    // No extension loaded: I002 says so, not W151.
    let none = check(
        &build([]),
        &[EntityRecord::new("wibble", "wb", span("t.spec"))],
    );
    assert!(none.is_empty(), "{none:?}");
}
