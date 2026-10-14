//! `parse_validation_rule_pattern`: a declared rule becomes a typed rule,
//! or W112 when it cannot work as declared.

use specforge_common::Severity;
use specforge_extension_sdk::prelude::FieldType;
use specforge_protocol_types::{CheckKind, ValidationRuleDescriptor, ValidationSeverity};
use specforge_test_macros::test as spec;

use super::{constraint, entity, one, rule, rules};

#[spec(
    behavior = "parse_validation_rule_pattern",
    verify = "parses no_incoming_edges pattern from manifest"
)]
fn parses_no_incoming_edges_pattern_from_manifest() {
    let built = one(rule("W100", "no_incoming_edges"));
    assert!(built.diagnostics.is_empty(), "{:?}", built.diagnostics);
    let rule = built.rules.iter().next().unwrap();
    assert_eq!(rule.check_kind(), CheckKind::NoIncomingEdges);
    assert_eq!(rule.code(), "W100");
}

#[spec(
    behavior = "parse_validation_rule_pattern",
    verify = "parses missing_field_when_flag_set pattern from manifest"
)]
fn parses_missing_field_when_flag_set_pattern_from_manifest() {
    let mut declared = rule("W101", "missing_field_when_flag_set");
    declared.field = Some("contract".to_string());
    let built = one(declared);
    let rule = built.rules.iter().next().unwrap();
    assert_eq!(rule.check_kind(), CheckKind::MissingFieldWhenFlagSet);
    assert_eq!(rule.describe()["field"], "contract");
}

#[spec(
    behavior = "parse_validation_rule_pattern",
    verify = "unrecognized pattern kind produces warning"
)]
fn unrecognized_pattern_kind_produces_warning() {
    let built = one(rule("W102", "invalid_check_kind"));
    assert!(built.rules.is_empty());
    let w112 = built.coded("W112");
    assert_eq!(w112.len(), 1, "{:?}", built.diagnostics);
    assert_eq!(w112[0].severity, Severity::Warning);
    assert_eq!(
        w112[0].message,
        "extension '@test': unrecognized validation pattern kind 'invalid_check_kind'"
    );
}

#[spec(
    behavior = "parse_validation_rule_pattern",
    verify = "all required fields validated on each rule"
)]
fn misconfigured_one_of_with_empty_values_produces_warning() {
    let mut declared = rule("W107", "field_value_constraint");
    declared.field = Some("status".to_string());
    declared.constraint = Some(constraint("one_of", None, &[]));

    let built = one(declared);

    // The dead rule must not reach execution.
    assert!(built.rules.is_empty());
    assert_eq!(built.diagnostics.len(), 1);
    assert_eq!(
        built.diagnostics[0].message,
        "extension '@test': rule 'W107': one_of constraint has an empty values list — every field value would be flagged as a violation — the rule can never fire and was not registered"
    );
}

#[spec(
    behavior = "parse_validation_rule_pattern",
    verify = "all required fields validated on each rule"
)]
fn field_requiring_check_without_field_produces_warning() {
    for (check, declared_constraint) in [
        ("missing_field_when_flag_set", None),
        ("missing_required_field", None),
        ("file_exists", None),
        (
            "field_value_constraint",
            Some(constraint("one_of", None, &["draft"])),
        ),
        (
            "conditional_field_required",
            Some(constraint("when_field_equals", Some("status"), &["draft"])),
        ),
    ] {
        let mut declared = rule("W108", check);
        declared.constraint = declared_constraint;
        let built = one(declared);
        assert!(built.rules.is_empty(), "{check}");
        assert_eq!(
            built.coded("W112")[0].message,
            format!(
                "extension '@test': rule 'W108': check '{check}' requires a field but none is set — the rule can never fire and was not registered"
            )
        );
    }
}

#[spec(
    behavior = "parse_validation_rule_pattern",
    verify = "parses field_value_constraint pattern from manifest"
)]
fn valid_one_of_rule_still_parses() {
    let mut declared = rule("W109", "field_value_constraint");
    declared.field = Some("status".to_string());
    declared.constraint = Some(constraint("one_of", None, &["draft", "active"]));
    let built = one(declared);
    assert!(built.diagnostics.is_empty(), "{:?}", built.diagnostics);
    let rule = built.rules.iter().next().unwrap();
    assert_eq!(rule.check_kind(), CheckKind::FieldValueConstraint);
    assert_eq!(
        rule.describe()["constraint"],
        serde_json::json!({ "kind": "one_of", "pattern": null, "values": ["draft", "active"] })
    );
}

#[test]
fn a_rule_keeps_its_declared_head() {
    let built = one(ValidationRuleDescriptor {
        code: "W100".to_string(),
        severity: ValidationSeverity::Error,
        message_template: "test {id}".to_string(),
        check: "no_incoming_edges".to_string(),
        ..Default::default()
    });
    let rule = built.rules.iter().next().unwrap();
    assert_eq!(rule.code(), "W100");
    assert_eq!(rule.severity(), Severity::Error);
    assert_eq!(rule.describe()["message_template"], "test {id}");
    assert_eq!(rule.check_kind(), CheckKind::NoIncomingEdges);
    assert_eq!(rule.target_kind(), None);
}

#[spec(
    behavior = "parse_validation_rule_pattern",
    verify = "Parse Validation Rule Pattern: validation rule parsing holds — manifest_rules_available, patterns_parsed, unrecognized_warned, ignored_warned"
)]
fn parse_validation_rule_pattern_contract() {
    // requires: manifest rules available
    let mut ignoring = rule("W300", "no_edges");
    ignoring.wasm_function = Some("validate".to_string());
    let built = rules(vec![
        rule("W100", "no_incoming_edges"),
        rule("W200", "invalid_kind"),
        ignoring,
    ]);
    // ensures: valid patterns parsed (the one ignoring a property too)
    assert_eq!(built.codes(), ["W100", "W300"]);
    // ensures: unrecognized warned, naming the extension
    let w112 = built.coded("W112");
    assert_eq!(w112.len(), 1);
    assert!(w112[0].message.contains("'@test'"), "{}", w112[0].message);
    // ensures: ignored warned, and the rule registered without it
    let w147 = built.coded("W147");
    assert_eq!(w147.len(), 1);
    assert!(
        w147[0].message.contains("wasm_function"),
        "{}",
        w147[0].message
    );
    assert!(built.rules.iter().nth(1).unwrap().describe()["wasm_function"].is_null());
}

// C14: a malformed regex is rejected when the rule is built and never runs.
#[test]
fn an_invalid_regex_is_w112_and_not_registered() {
    let mut declared = rule("W095", "field_value_constraint");
    declared.field = Some("version".to_string());
    declared.constraint = Some(constraint("matches", Some("(unclosed"), &[]));
    let built = one(declared);
    assert!(built.rules.is_empty());
    let w112 = built.coded("W112");
    assert_eq!(w112.len(), 1);
    assert!(
        w112[0]
            .message
            .starts_with("extension '@test': rule 'W095': invalid regex pattern '(unclosed': "),
        "{}",
        w112[0].message
    );
}

// C6-12: a matches constraint without a pattern can never check anything;
// an unrecognized constraint kind never matches.
#[test]
fn a_constraint_that_can_never_match_is_w112() {
    for (kind, pattern, why) in [
        (
            "matches",
            None,
            "matches constraint has no pattern — no value can ever be checked",
        ),
        (
            "equals",
            None,
            "unknown constraint kind 'equals' for check 'field_value_constraint' (expected non_empty, one_of, or matches)",
        ),
        (
            "when_field_equals",
            Some("status"),
            "unknown constraint kind 'when_field_equals' for check 'field_value_constraint' (expected non_empty, one_of, or matches)",
        ),
    ] {
        let mut declared = rule("W105", "field_value_constraint");
        declared.field = Some("version".to_string());
        declared.constraint = Some(constraint(kind, pattern, &["active"]));
        let built = one(declared);
        assert!(built.rules.is_empty(), "{kind}");
        assert_eq!(
            built.coded("W112")[0].message,
            format!(
                "extension '@test': rule 'W105': {why} — the rule can never fire and was not registered"
            )
        );
    }
    let mut declared = rule("W106", "field_value_constraint");
    declared.field = Some("status".to_string());
    let built = one(declared);
    assert!(
        built.coded("W112")[0]
            .message
            .contains("check 'field_value_constraint' requires a constraint but none is set")
    );
}

// C6-12: a conditional rule needs a condition field and values.
#[test]
fn a_conditional_rule_that_can_never_trigger_is_w112() {
    for (declared_constraint, why) in [
        (
            None,
            "check 'conditional_field_required' requires a constraint but none is set",
        ),
        (
            Some(constraint("when_field_equals", None, &["deferred"])),
            "conditional_field_required requires constraint.pattern (the condition field) — without it the condition can never be met",
        ),
        (
            Some(constraint("when_field_equals", Some("status"), &[])),
            "conditional_field_required has an empty condition values list — the condition can never be met",
        ),
    ] {
        let mut declared = rule("I059", "conditional_field_required");
        declared.field = Some("reason".to_string());
        declared.constraint = declared_constraint;
        let built = one(declared);
        assert!(built.rules.is_empty());
        assert_eq!(
            built.coded("W112")[0].message,
            format!(
                "extension '@test': rule 'I059': {why} — the rule can never fire and was not registered"
            )
        );
    }
}

#[test]
fn a_conditional_rule_reads_the_condition_field_and_values() {
    let mut declared = rule("I059", "conditional_field_required");
    declared.target_kind = Some("feature".to_string());
    declared.field = Some("reason".to_string());
    declared.constraint = Some(constraint(
        "when_field_equals",
        Some("status"),
        &["deferred"],
    ));
    let built = one(declared);
    let rule = built.rules.iter().next().unwrap();
    assert_eq!(rule.check_kind(), CheckKind::ConditionalFieldRequired);
    assert_eq!(
        rule.describe()["constraint"],
        serde_json::json!({ "kind": "when_field_equals", "pattern": "status", "values": ["deferred"] })
    );
    // It runs: a deferred feature without a reason.
    let deferred = entity("f", "feature", 0, 0).with_field("status", "deferred");
    assert_eq!(super::check(&built, &[deferred]).len(), 1);
}

#[spec(
    behavior = "register_custom_validation_patterns",
    verify = "custom rule without a wasm_function produces warning and is not registered"
)]
fn a_custom_rule_without_a_wasm_function_is_w112() {
    let built = one(rule("E200", "custom"));
    assert!(built.rules.is_empty());
    assert_eq!(
        built.coded("W112")[0].message,
        "extension '@test': rule 'E200': check 'custom' requires a wasm_function but none is set — the rule can never fire and was not registered"
    );
}

#[spec(
    behavior = "register_custom_validation_patterns",
    verify = "custom pattern registered with wasm_function reference"
)]
fn a_custom_rule_is_registered_with_its_wasm_function() {
    let mut declared = rule("E200", "custom");
    declared.wasm_function = Some("validate_custom".to_string());
    let built = one(declared);
    let rule = built.rules.iter().next().unwrap();
    assert_eq!(rule.check_kind(), CheckKind::Custom);
    assert_eq!(rule.describe()["wasm_function"], "validate_custom");
    assert_eq!(rule.origin().name(), "@test");
}

#[spec(
    behavior = "parse_validation_rule_pattern",
    verify = "a verify_kind_allowlist rule without values produces W112 and is not registered"
)]
fn a_verify_kind_allowlist_without_values_is_w112() {
    for declared_constraint in [None, Some(constraint("one_of", None, &[]))] {
        let mut declared = rule("W009", "verify_kind_allowlist");
        declared.constraint = declared_constraint;
        let built = one(declared);
        assert!(built.rules.is_empty());
        assert_eq!(
            built.coded("W112")[0].message,
            "extension '@test': rule 'W009': verify_kind_allowlist requires a constraint with values — every verify kind would be flagged — the rule can never fire and was not registered"
        );
    }
}

#[spec(
    behavior = "parse_validation_rule_pattern",
    verify = "a rule that reads verify statements on a kind that accepts none produces W112 and is not registered"
)]
fn a_rule_reading_verify_statements_on_a_kind_without_verify_is_w112() {
    use crate::support::declare;

    // `memo` accepts no verify statements; `note` does.
    let kinds = |rules: Vec<ValidationRuleDescriptor>| {
        let mut declaration = declare("@test", |c| {
            c.kind("memo", |k| {
                k.description("m");
            });
            c.kind("note", |k| {
                k.description("n").supports_verify(true);
            });
        });
        declaration.validation_rules = rules;
        super::rules_of(vec![declaration])
    };
    let on = |code: &str, check: &str, target: &str, field: Option<&str>| {
        let mut declared = rule(code, check);
        declared.target_kind = Some(target.to_string());
        declared.field = field.map(str::to_string);
        declared.constraint =
            (check == "verify_kind_allowlist").then(|| constraint("one_of", None, &["unit"]));
        declared
    };

    let built = kinds(vec![
        on("W004", "no_verify_statements", "memo", Some("verify")),
        on("W009", "verify_kind_allowlist", "memo", None),
        // Obligations declared in another field: memo can write it.
        on("W010", "no_verify_statements", "memo", Some("gherkin")),
        // A kind that accepts verify statements.
        on("W011", "no_verify_statements", "note", None),
        // A kind no loaded extension declares: not registered (inert), not
        // W112.
        on("W012", "no_verify_statements", "ghost", None),
    ]);

    let messages: Vec<&str> = built
        .coded("W112")
        .iter()
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(
        messages,
        [
            "extension '@test': rule 'W004': check 'no_verify_statements' reads verify statements, which kind 'memo' does not accept — the rule can never fire and was not registered",
            "extension '@test': rule 'W009': check 'verify_kind_allowlist' reads verify statements, which kind 'memo' does not accept — the rule can never fire and was not registered",
        ]
    );
    assert_eq!(built.codes(), ["W010", "W011"]);
}

/// (check, edge_type, constraint, wasm_function, the properties W147 names)
type UnreadCase = (
    &'static str,
    Option<&'static str>,
    Option<specforge_protocol_types::FieldConstraintDescriptor>,
    Option<&'static str>,
    &'static [&'static str],
);

#[spec(
    behavior = "parse_validation_rule_pattern",
    verify = "a property its check does not read produces W147 and the rule is registered without it"
)]
fn a_property_its_check_does_not_read_is_w147() {
    // (check, edge_type, constraint, wasm_function, the properties W147 names)
    let edge = Some("enforces");
    let any = || Some(constraint("one_of", None, &["x"]));
    let cases: Vec<UnreadCase> = vec![
        (
            "no_incoming_edges",
            None,
            any(),
            Some("f"),
            &["constraint", "wasm_function"],
        ),
        ("no_outgoing_edges", None, any(), None, &["constraint"]),
        (
            "no_edges",
            edge,
            any(),
            Some("f"),
            &["edge_type", "constraint", "wasm_function"],
        ),
        (
            "missing_field_when_flag_set",
            edge,
            any(),
            Some("f"),
            &["edge_type", "constraint", "wasm_function"],
        ),
        ("missing_required_field", edge, None, None, &["edge_type"]),
        ("file_exists", None, any(), None, &["constraint"]),
        (
            "field_value_constraint",
            edge,
            any(),
            Some("f"),
            &["edge_type", "wasm_function"],
        ),
        (
            "field_value_constraint",
            None,
            Some(constraint("non_empty", Some("x"), &["a"])),
            None,
            &["constraint.pattern", "constraint.values"],
        ),
        (
            "field_value_constraint",
            None,
            Some(constraint("one_of", Some("x"), &["a"])),
            None,
            &["constraint.pattern"],
        ),
        (
            "field_value_constraint",
            None,
            Some(constraint("matches", Some("^x$"), &["a"])),
            None,
            &["constraint.values"],
        ),
        (
            "conditional_field_required",
            edge,
            Some(constraint("when_field_equals", Some("status"), &["a"])),
            Some("f"),
            &["edge_type", "wasm_function"],
        ),
        (
            "cycle_detection",
            edge,
            any(),
            Some("f"),
            &["constraint", "wasm_function"],
        ),
        (
            "verify_kind_allowlist",
            edge,
            Some(constraint("one_of", Some("x"), &["unit"])),
            Some("f"),
            &["edge_type", "wasm_function", "constraint.pattern"],
        ),
        (
            "no_verify_statements",
            edge,
            any(),
            Some("f"),
            &["edge_type", "constraint", "wasm_function"],
        ),
        (
            "custom",
            edge,
            any(),
            Some("f"),
            &["edge_type", "constraint"],
        ),
    ];
    for (check, edge_type, declared_constraint, function, ignored) in cases {
        let mut declared = rule("W500", check);
        declared.field = Some("status".to_string());
        declared.edge_type = edge_type.map(str::to_string);
        declared.constraint = declared_constraint;
        declared.wasm_function = function.map(str::to_string);

        // `@test` declares the edge type, so an edge rule resolves.
        let mut declaration = crate::support::declare("@test", |c| {
            c.kind("Behavior", |k| {
                k.keyword("behavior").supports_verify(true);
                k.field("enforcing", |f| {
                    f.field_type(FieldType::ReferenceList)
                        .edge("enforces")
                        .target_kind("behavior");
                });
            });
            c.edge("enforces", |e| {
                e.description("e");
            });
        });
        declaration.validation_rules = vec![declared];
        let built = super::rules_of(vec![declaration]);

        assert!(
            built.coded("W112").is_empty(),
            "{check}: {:?}",
            built.diagnostics
        );
        let messages: Vec<&str> = built
            .coded("W147")
            .iter()
            .map(|d| d.message.as_str())
            .collect();
        let expected: Vec<String> = ignored
            .iter()
            .map(|property| {
                format!(
                    "extension '@test': rule 'W500': {property} is not read by check '{check}' — the rule was registered without it"
                )
            })
            .collect();
        assert_eq!(messages, expected, "{check}");
        // Registered, without what it ignores; `field` is always kept.
        let described = built.rules.iter().next().expect(check).describe();
        assert_eq!(described["field"], "status", "{check}");
        for property in ignored {
            let gone = match *property {
                "constraint.pattern" => described["constraint"]["pattern"].is_null(),
                "constraint.values" => described["constraint"]["values"] == serde_json::json!([]),
                property => described[property].is_null(),
            };
            assert!(gone, "{check}: {property} in {described}");
        }
    }
}

#[spec(
    behavior = "parse_validation_rule_pattern",
    verify = "a conditional_field_required constraint of another kind produces W147 and is read as when_field_equals"
)]
fn a_conditional_constraint_of_another_kind_is_w147_and_read_as_when_field_equals() {
    let mut declared = rule("I905", "conditional_field_required");
    declared.field = Some("reason".to_string());
    declared.message_template = "{id} is deferred with no reason".to_string();
    declared.constraint = Some(constraint("matches", Some("status"), &["deferred"]));

    let built = one(declared);

    assert_eq!(
        built.coded("W147")[0].message,
        "extension '@test': rule 'I905': constraint kind 'matches' is not read by check 'conditional_field_required' (it reads when_field_equals) — read as when_field_equals"
    );
    let registered = built.rules.iter().next().unwrap();
    assert_eq!(
        registered.describe()["constraint"]["kind"],
        "when_field_equals"
    );
    // It fires as before: `pattern` names the condition field.
    let deferred = entity("a", "behavior", 0, 0).with_field("status", "deferred");
    assert_eq!(
        super::messages(&super::check(&built, &[deferred])),
        ["a is deferred with no reason"]
    );

    // An allowlist reads one_of; any other kind is read as one_of.
    let mut allowlist = rule("W009", "verify_kind_allowlist");
    allowlist.constraint = Some(constraint("non_empty", None, &["unit"]));
    let built = one(allowlist);
    assert_eq!(
        built.coded("W147")[0].message,
        "extension '@test': rule 'W009': constraint kind 'non_empty' is not read by check 'verify_kind_allowlist' (it reads one_of) — read as one_of"
    );
    assert_eq!(built.codes(), ["W009"]);
}
