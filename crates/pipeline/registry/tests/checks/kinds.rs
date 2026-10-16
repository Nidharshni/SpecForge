//! E024: an entity of a kind no loaded extension declares, with the
//! extension that would provide it; and the gate that leaves a project with
//! no entity kind unchecked. Every test starts from `.spec` text (ADR 0115).

use specforge_extension_sdk::prelude::*;
use specforge_test_macros::test as spec;

use crate::support::written::checked;
use crate::support::{build, candidates, coded_in, declare, product, software};

/// An entity of `kind` named `id`, with an empty body.
fn entity(kind: &str, id: &str) -> String {
    format!("{kind} {id} \"E\" {{\n}}\n\n")
}

#[spec(
    behavior = "detect_unknown_entity_kinds",
    verify = "unregistered keyword produces E024"
)]
fn detect_unknown_kinds_e024() {
    let build = build([software()]);
    let diags = checked(&build, &[("test.spec", &entity("unknown_thing", "u1"))]);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, "E024");
}

#[spec(
    behavior = "detect_unknown_entity_kinds",
    verify = "E024 includes keyword name and source span"
)]
fn detect_unknown_kinds_e024_includes_info() {
    let build = build([software()]);
    let text = format!("{}{}", "\n".repeat(41), entity("unknown_thing", "u1"));
    let diags = checked(&build, &[("my/file.spec", &text)]);
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
    let diags = checked(&build, &[("test.spec", &entity("behavior", "b1"))]);
    assert!(diags.is_empty(), "{diags:?}");
}

#[spec(
    behavior = "detect_unknown_entity_kinds",
    verify = "define-block keywords not checked against KindRegistry"
)]
fn detect_unknown_kinds_define_not_checked() {
    // A `define` block is no node (the parser reports W143), and a `spec` block is no extension's
    // kind: neither is E024.
    let text = "define my_define {\n}\n\nspec \"my_spec\" {\n}\n";
    let diags = checked(&build([software()]), &[("test.spec", text)]);
    assert!(coded_in(&diags, "E024").is_empty(), "{diags:?}");
    // With no kind registered nothing at all is reported (the gate).
    assert!(checked(&build([]), &[("test.spec", text)]).is_empty());
}

#[spec(
    behavior = "detect_unknown_entity_kinds",
    verify = "Detect Unknown Entity Kinds: unknown entity kind detection holds — registries_populated_fired, structural_parse_ready, unknown_kinds_diagnosed, registered_kinds_accepted"
)]
fn detect_unknown_kinds_contract() {
    let build = build([software()]);
    let unknown = checked(&build, &[("t.spec", &entity("xyzzy", "x1"))]);
    assert!(unknown.iter().any(|d| d.code == "E024"), "unknown → E024");
    let known = checked(&build, &[("t.spec", &entity("behavior", "b1"))]);
    assert!(known.is_empty(), "registered → no E024");
}

#[spec(
    behavior = "suggest_missing_extensions",
    verify = "E024 for keyword in index suggests the providing extension"
)]
fn suggest_missing_ext_known_keyword() {
    // `feature` is a keyword the vocabulary the build is handed knows.
    let diags = checked(
        &build([software()]).with_candidates(candidates(&[product()])),
        &[("test.spec", &entity("feature", "f1"))],
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
    let diags = checked(
        &build([software()]),
        &[("test.spec", &entity("xyzzy", "x1"))],
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
    verify = "Suggest Missing Extensions: missing extension suggestions holds — e024_diagnostic_emitted, suggestion_provided, no_network"
)]
fn suggest_missing_ext_contract() {
    let build = build([software()]).with_candidates(candidates(&[product()]));
    // ensures: a keyword the vocabulary knows gets the extension suggestion.
    let d1 = checked(&build, &[("test.spec", &entity("feature", "f1"))]);
    assert!(
        d1[0]
            .suggestion
            .as_ref()
            .unwrap()
            .contains("@specforge/product")
    );
    // ensures: one it does not gets the search suggestion.
    let d2 = checked(&build, &[("test.spec", &entity("xyzzy", "x1"))]);
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
    let diags = checked(
        &build([software()]),
        &[("test.spec", &entity("behavior", "my_beh"))],
    );
    assert!(diags.is_empty(), "{diags:?}");
}

#[spec(
    behavior = "two_phase_validate_semantic",
    verify = "unknown keyword produces E024"
)]
fn an_unknown_keyword_produces_e024() {
    let diags = checked(
        &build([software()]),
        &[("test.spec", &entity("xyzzy", "my_xyz"))],
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
    let files = [("test.spec", entity("behavior", "my_beh"))];
    let files: Vec<(&str, &str)> = files.iter().map(|(p, t)| (*p, t.as_str())).collect();
    // Before any kind is registered the kind checks do not run: nothing is
    // reported, not even E024 for every keyword.
    assert!(checked(&build([]), &files).is_empty());
    // Populated by an extension that does not declare `behavior`, the
    // registry makes it unknown.
    let other = declare("@test/other", |c| {
        c.kind("Thing", |k| {
            k.keyword("thing");
        });
    });
    assert!(
        checked(&build([other]), &files)
            .iter()
            .any(|d| d.code == "E024")
    );
    // Populated by the software extension, it passes.
    assert!(checked(&build([software()]), &files).is_empty());
}

#[spec(
    behavior = "two_phase_validate_semantic",
    verify = "Two-Phase Validate: Semantic: semantic validation holds — unknown_keywords_diagnosed"
)]
fn two_phase_validate_semantic_contract() {
    let build = build([software()]);
    // ensures: all blocks checked — known passes, unknown diagnosed.
    let diags = checked(
        &build,
        &[
            ("a.spec", &entity("behavior", "b1")),
            ("b.spec", &entity("xyzzy", "x1")),
        ],
    );
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, "E024");
    // ensures: fields validated.
    let with_bad_field = "behavior b1 \"B\" {\n  contract {\n  }\n  bad_field \"\"\n}\n";
    assert!(
        checked(&build, &[("a.spec", with_bad_field)])
            .iter()
            .any(|d| d.code == "W020")
    );
}

/// With no entity kind registered, E024, E013, E014, W020, W158 and E061 do
/// not run; W012 and the rules still do (and W151 says what is unchecked). (E016 is a rule over a
/// declared kind's `file_reference` field, so a build with no kind has none.)
#[spec(
    behavior = "check_entities_in_one_order",
    verify = "with no entity kind registered, E024, E013, E014, W020, W158 and E061 do not run, and W012 and the rules still do"
)]
fn a_build_with_no_kind_checks_no_kind() {
    let records = "wibble x \"X\" {\n  bogus \"\"\n}\n\nbehavior define \"B\" {\n}\n\n";
    // Nothing declared: nothing reported.
    assert!(checked(&build([]), &[("t.spec", records)]).is_empty());
    // A ref nobody references is still W012.
    let with_ref = format!("{records}ref gh.issue:1 \"R\"\n");
    let diags = checked(&build([]), &[("t.spec", &with_ref)]);
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
    let diags = checked(&build, &[("t.spec", &with_ref)]);
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
    let kindless =
        build([declare("@test/kindless", |_| {})]).with_candidates(candidates(&[software()]));
    assert!(kindless.structural_only());

    // Two non-structural entities: one W151 naming how many and which kinds.
    let text = format!(
        "{}{}{}spec \"s\" {{\n}}\n",
        entity("wibble", "wb"),
        entity("behavior", "b1"),
        entity("wibble", "wb2")
    );
    let diags = checked(&kindless, &[("t.spec", &text)]);
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
    let one = checked(&kindless, &[("t.spec", &entity("xyzzy", "x1"))]);
    assert_eq!(
        one[0].message,
        "the loaded extensions declare no entity kind: 1 entity (kinds: xyzzy) is not checked against kinds, fields or identifiers"
    );
    assert_eq!(
        one[0].suggestion.as_deref(),
        Some("enable the extension that declares them; search with: specforge search xyzzy")
    );

    // Seven kinds: five named, then "…".
    let many: String = ["k1", "k2", "k3", "k4", "k5", "k6", "k7"]
        .iter()
        .map(|kind| entity(kind, &format!("{kind}_e")))
        .collect();
    let diags = checked(&kindless, &[("t.spec", &many)]);
    assert!(
        diags[0]
            .message
            .contains("7 entities (kinds: k1, k2, k3, k4, k5, …)"),
        "{diags:?}"
    );

    // Only structural entities, or none: nothing is left unchecked.
    assert!(checked(&kindless, &[("t.spec", "spec \"s\" {\n}\n")]).is_empty());
    assert!(checked(&kindless, &[]).is_empty());

    // No extension loaded: I002 says so, not W151.
    let none = checked(&build([]), &[("t.spec", &entity("wibble", "wb"))]);
    assert!(none.is_empty(), "{none:?}");
}
