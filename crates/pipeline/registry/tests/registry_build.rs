// `build_registries`: the one call that turns loaded declarations into
// everything the compiler needs from them (architecture plan 05, step R1).
// In-memory declarations, no Wasm.

use specforge_test_macros::test as spec;

use specforge_extension_sdk::prelude::*;
use specforge_graph::{GraphInputs, Reading, Target};
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry::build_registries;

use crate::support::{coded, codes, diagnostics};

/// A builder for the extension `name` in `version`.
fn extension(name: &str, version: &str) -> ContributionsBuilder {
    ContributionsBuilder::new(ExtensionMeta::new(name, version))
}

/// The command `hello`, described as `description`.
fn hello(c: &mut ContributionsBuilder, description: &str) {
    c.command("hello", |cmd| {
        cmd.title("Hello")
            .description(description)
            .handler(|_| CommandOutput::ok("hello"));
    });
}

/// Kinds with fields, a required field, a single reference, a body parser,
/// an edge, a rule and a CLI command.
fn software() -> ExtensionDeclaration {
    let mut c = extension("@test/software", "1.2.0");
    c.kind("Behavior", |k| {
        k.keyword("behavior").testable(true).supports_verify(true);
        k.field("contract", |f| {
            f.field_type(FieldType::String).required();
        });
        k.field("owner", |f| {
            f.field_type(FieldType::Reference)
                .edge("owned_by")
                .target_kind("team");
        });
        k.field("types", |f| {
            f.field_type(FieldType::ReferenceList)
                .edge("uses")
                .target_kind("type");
        });
    });
    c.kind("Type", |k| {
        k.keyword("type").has_body_parser();
    });
    c.edge("uses", |e| {
        e.source_kind("behavior").target_kind("type");
    });
    c.rule("W901", |r| {
        r.severity(ValidationSeverity::Warning)
            .message_template("behavior '{id}' uses nothing")
            .check(CheckKind::NoOutgoingEdges)
            .target_kind("behavior")
            .edge_type("uses");
    });
    hello(&mut c, "hello");
    c.declaration()
}

fn product() -> ExtensionDeclaration {
    let mut c = extension("@test/product", "0.3.0");
    c.kind("Feature", |k| {
        k.keyword("feature");
    });
    hello(&mut c, "hello again");
    c.declaration()
}

#[test]
fn kinds_fields_and_edges_come_from_the_descriptors() {
    let build = build_registries(vec![software(), product()]);

    let kinds: std::collections::BTreeSet<&str> =
        build.kinds().keywords().map(String::as_str).collect();
    assert_eq!(kinds, ["behavior", "feature", "type"].into_iter().collect());
    assert!(build.fields().contains("behavior", "contract"));
    assert!(build.fields().contains("behavior", "owner"));
    assert!(build.edges().contains("uses"));
    assert_eq!(
        build.extension_info().collect::<Vec<_>>(),
        vec![("@test/software", "1.2.0"), ("@test/product", "0.3.0")]
    );
    assert_eq!(build.declarations().len(), 2);
}

#[spec(
    behavior = "load_extension_manifests",
    verify = "two extensions loaded and registries populated without collision"
)]
fn two_extensions_populate_without_collision() {
    let build = build_registries(vec![software(), product()]);

    assert!(build.kinds().contains("behavior"));
    assert!(build.kinds().contains("feature"));
    assert_eq!(
        build.kinds().get("feature").unwrap().source_extension,
        "@test/product"
    );
    // No kind, field or edge collides (no E026, W018): the declarations'
    // own W021s (targets nobody declares); the two `hello`s are two
    // routes, one per short name.
    assert_eq!(codes(&build), ["W021", "W021"]);
}

#[test]
fn rules_include_the_manifest_rules_and_the_required_field_rules() {
    let build = build_registries(vec![software()]);

    let codes: Vec<(&str, &str)> = build
        .rules()
        .iter()
        .map(|rule| (rule.code(), rule.origin().name()))
        .collect();
    // The extension's rule keeps its origin (for custom-rule dispatch);
    // the host-generated E006 rule for `contract` has none.
    assert!(codes.contains(&("W901", "@test/software")), "{codes:?}");
    assert!(codes.contains(&("E006", "")), "{codes:?}");
}

#[spec(
    behavior = "registry_build_graph_inputs",
    verify = "the registry build derives every graph input from the declarations"
)]
fn derived_graph_inputs_match_the_manifests() {
    let build = build_registries(vec![software(), product()]);
    let inputs = build.graph_inputs();

    assert!(
        inputs
            .field_forms
            .reading("behavior", "owner")
            .is_single_reference()
    );
    assert!(
        !inputs
            .field_forms
            .reading("behavior", "types")
            .is_single_reference(),
        "a reference list is not a single reference"
    );
    // `team` is declared by no loaded extension.
    assert_eq!(
        inputs.field_forms.target("behavior", "owner"),
        Some(Target::Absent("team"))
    );
    assert_eq!(inputs.target_owners.get("team"), None, "nothing owns it");
    // A single reference is read as written; a reference list is a list.
    let form = |field: &str| match inputs.field_forms.reading("behavior", field) {
        Reading::Declared(form) => Some(form.field_type()),
        _ => None,
    };
    assert_eq!(form("owner"), Some(FieldType::Reference));
    assert_eq!(form("contract"), Some(FieldType::String));
    assert_eq!(form("types"), Some(FieldType::ReferenceList));
}

#[test]
fn surface_conflicts_come_last_in_the_builds_diagnostics() {
    // Routed under `@test/software`'s short name: its `hello` is E039.
    let mut product = product();
    product.handshake.ext_short = Some("software".into());
    let build = build_registries(vec![software(), product]);

    let reported = codes(&build);
    assert_eq!(
        reported.last(),
        Some(&"E039"),
        "E039 comes last, after the registries' diagnostics: {reported:?}"
    );
    assert_eq!(reported.iter().filter(|c| **c == "E039").count(), 1);
    assert_eq!(build.commands().routed().len(), 1, "first route wins");
    assert_eq!(build.commands().shadowed().len(), 1);
    assert_eq!(
        build
            .declarations()
            .iter()
            .map(|d| d.surfaces.commands.len())
            .sum::<usize>(),
        2,
        "each declaration keeps its own surfaces"
    );
}

/// Two extensions declaring one rule code: the build warns (W023), after
/// the rule-parse diagnostics, and keeps both rules.
#[spec(
    behavior = "registry_build_rules",
    verify = "a rule code two extensions declare is W023 and both rules are kept"
)]
fn a_rule_code_declared_by_two_extensions_warns() {
    let rule = |name: &str| {
        let mut c = extension(name, "1.0.0");
        c.rule("W900", |r| {
            r.severity(ValidationSeverity::Warning)
                .message_template("m")
                .check(CheckKind::NoIncomingEdges);
        });
        c.declaration()
    };

    let build = build_registries(vec![rule("@ext/a"), rule("@ext/b")]);

    assert_eq!(codes(&build), ["W023"]);
    let message = &diagnostics(&build)[0].message;
    for part in ["'W900'", "'@ext/a'", "'@ext/b'"] {
        assert!(message.contains(part), "{message}");
    }
    assert_eq!(
        build.rules().iter().filter(|r| r.code() == "W900").count(),
        2
    );
}

/// With no extension loaded the host knows no kind, field or edge: every
/// registry starts empty and stays so.
#[spec(
    behavior = "build_registries_from_declarations",
    verify = "a build of no declarations has empty registries, no rules and no diagnostics"
)]
fn no_declarations_build_empty_registries() {
    let build = build_registries(Vec::new());

    assert!(build.kinds().is_empty());
    assert_eq!(build.kinds().len(), 0);
    assert_eq!(build.kinds().keywords().count(), 0);
    assert!(build.kinds().get("behavior").is_none());
    assert!(build.fields().is_empty());
    assert!(build.fields().get("behavior", "contract").is_none());
    assert!(build.fields().fields_for_kind("behavior").is_empty());
    assert!(build.edges().is_empty());
    assert!(build.edges().get("enforces").is_none());
    assert_eq!(build.edges().iter().count(), 0);
    assert!(build.rules().is_empty());
    assert!(build.commands().routed().is_empty());
    assert!(build.passes().is_empty());
    assert_eq!(build.graph_inputs(), &GraphInputs::default());
    assert!(build.declarations().is_empty());
    assert!(build.diagnostics().next().is_none());
}

/// An extension `name` whose one rule `code` reports at `severity`; the
/// rule's kind and edge are its own, so nothing else about it warns.
fn one_rule(name: &str, code: &str, severity: ValidationSeverity) -> ExtensionDeclaration {
    let mut c = extension(name, "1.0.0");
    c.kind("Thing", |k| {
        k.keyword("thing");
        k.field("uses", |f| {
            f.field_type(FieldType::ReferenceList)
                .edge("uses")
                .target_kind("thing");
        });
    });
    c.edge("uses", |e| {
        e.source_kind("thing").target_kind("thing");
    });
    c.rule(code, |r| {
        r.severity(severity)
            .message_template("thing '{id}' uses nothing")
            .check(CheckKind::NoOutgoingEdges)
            .target_kind("thing")
            .edge_type("uses");
    });
    c.declaration()
}

/// The rule `code` as registered: its severity and declaring extension.
fn registered(
    build: &specforge_registry::RegistryBuild,
    code: &str,
) -> Vec<(specforge_common::Severity, String)> {
    build
        .rules()
        .iter()
        .filter(|rule| rule.code() == code)
        .map(|rule| (rule.severity(), rule.origin().name().to_string()))
        .collect()
}

/// A rule whose code its extension may not use is still registered, as
/// declared, and reported (W150) with the code, why, and the extension.
#[spec(
    behavior = "registry_build_rules",
    verify = "a rule whose code the extension may not use is reported (W150) and still registered"
)]
fn a_rule_with_a_code_it_may_not_use_is_reported() {
    let build = build_registries(vec![one_rule(
        "@acme/squat",
        "E001",
        ValidationSeverity::Info,
    )]);

    assert_eq!(
        registered(&build, "E001"),
        [(specforge_common::Severity::Info, "@acme/squat".to_string())]
    );
    let all = diagnostics(&build);
    let [w150] = all.as_slice() else {
        panic!("one W150 expected: {all:?}");
    };
    assert_eq!(w150.code, "W150");
    assert_eq!(w150.severity, specforge_common::Severity::Warning);
    assert!(w150.message.contains("'@acme/squat'"), "{}", w150.message);
    assert!(w150.message.contains("rule 'E001'"), "{}", w150.message);
    assert!(w150.message.contains("core owns"), "{}", w150.message);
    assert!(w150.message.contains("Parse error"), "{}", w150.message);
}

/// The same code declared for several target kinds is one W150 per
/// severity, and every rule is still registered.
#[spec(
    behavior = "registry_build_rules",
    verify = "a rule whose code the extension may not use is reported (W150) and still registered"
)]
fn a_misused_code_declared_for_several_kinds_is_reported_once_per_severity() {
    let mut c = extension("@acme/squat", "1.0.0");
    for (name, severity) in [
        ("a", ValidationSeverity::Warning),
        ("b", ValidationSeverity::Warning),
        ("c", ValidationSeverity::Error),
    ] {
        c.kind(name, |k| {
            k.keyword(name);
        });
        c.rule("W001", |r| {
            r.severity(severity)
                .message_template("m")
                .check(CheckKind::NoEdges)
                .target_kind(name);
        });
    }
    let build = build_registries(vec![c.declaration()]);

    let w150 = coded(&build, "W150");
    // W001 at Warning (declared twice) and at Error.
    assert_eq!(w150.len(), 2, "{w150:?}");
    assert_eq!(registered(&build, "W001").len(), 3);
}

/// A first-party extension's own catalogued rule at its catalogued level
/// is not reported; at another level, or uncatalogued, it is.
#[spec(
    behavior = "registry_build_rules",
    verify = "a rule whose code the extension may not use is reported (W150) and still registered"
)]
fn a_first_party_rule_is_checked_against_the_catalog() {
    let own = build_registries(vec![one_rule(
        "@specforge/product",
        "W041",
        ValidationSeverity::Warning,
    )]);
    assert!(
        own.diagnostics().next().is_none(),
        "{:?}",
        diagnostics(&own)
    );

    let wrong_level = build_registries(vec![one_rule(
        "@specforge/product",
        "W041",
        ValidationSeverity::Error,
    )]);
    let all = diagnostics(&wrong_level);
    let [w150] = all.as_slice() else {
        panic!("one W150 expected: {all:?}");
    };
    assert_eq!(w150.code, "W150");
    assert!(w150.message.contains("another level"), "{}", w150.message);

    let uncatalogued = build_registries(vec![one_rule(
        "@specforge/rust",
        "W500",
        ValidationSeverity::Warning,
    )]);
    assert_eq!(codes(&uncatalogued), ["W150"]);
}

#[test]
fn third_party_rule_codes_in_their_range_register_silently() {
    let build = build_registries(vec![one_rule(
        "@acme/x",
        "W950",
        ValidationSeverity::Warning,
    )]);

    assert_eq!(
        registered(&build, "W950"),
        [(specforge_common::Severity::Warning, "@acme/x".to_string())]
    );
    assert!(
        build.diagnostics().next().is_none(),
        "{:?}",
        diagnostics(&build)
    );
}
