//! `execute_validation_pattern` and `emit_diagnostic_from_pattern`: each
//! declarative check over entity records, and the diagnostic it emits.

use std::path::Path;

use specforge_common::Severity;
use specforge_protocol_types::{ValidationRuleDescriptor, ValidationSeverity};
use specforge_registry::entity::{Direction, EntityRecord, Exemption, RuleInput};
use specforge_registry::rules::NoVerdicts;
use specforge_registry::{Asked, MemoryDisk};
use specforge_test_macros::test as spec;

use super::{check, constraint, declaring, entity, messages, one, over, rule, rules, rules_of};
use crate::support::declare;
use specforge_extension_sdk::prelude::*;

#[spec(
    behavior = "execute_validation_pattern",
    verify = "no_incoming_edges detects unreferenced entities"
)]
fn no_incoming_edges_detects_orphan_entities() {
    let built = one(rule("W100", "no_incoming_edges"));
    let diagnostics = check(
        &built,
        &[
            entity("b1", "behavior", 0, 2), // orphan
            entity("b2", "behavior", 1, 0), // not orphan
        ],
    );
    assert_eq!(messages(&diagnostics), ["orphan behavior 'b1'"]);
}

#[spec(
    behavior = "execute_validation_pattern",
    verify = "no_outgoing_edges detects entities with zero outgoing edges"
)]
fn no_outgoing_edges_detects_entities_with_zero_outgoing_edges() {
    let mut declared = rule("W101", "no_outgoing_edges");
    declared.message_template = "leaf {kind} '{id}'".to_string();
    let diagnostics = check(
        &one(declared),
        &[
            entity("b1", "behavior", 1, 0), // leaf
            entity("b2", "behavior", 1, 3), // not leaf
        ],
    );
    assert_eq!(messages(&diagnostics), ["leaf behavior 'b1'"]);
}

#[test]
fn no_edges_detects_entities_with_no_edge_at_all() {
    let diagnostics = check(
        &one(rule("I010", "no_edges")),
        &[
            entity("b1", "behavior", 0, 0),
            entity("b2", "behavior", 1, 0),
            entity("b3", "behavior", 0, 1),
        ],
    );
    assert_eq!(messages(&diagnostics), ["orphan behavior 'b1'"]);
}

/// `@test` declaring `behavior`, `feature` (only when `with_feature`) and
/// the edge `BehaviorImplementsFeature` between them, plus `rule`.
fn implementing(rule: ValidationRuleDescriptor, with_feature: bool) -> super::Built {
    let mut declaration = declare("@test", |c| {
        c.kind("behavior", |k| {
            k.description("b");
            k.field("features", |f| {
                f.field_type(FieldType::ReferenceList)
                    .edge("BehaviorImplementsFeature")
                    .target_kind("feature");
            });
        });
        if with_feature {
            c.kind("feature", |k| {
                k.description("f");
            });
        }
        c.edge("BehaviorImplementsFeature", |e| {
            e.source_kind("behavior").target_kind("feature");
        });
    });
    declaration.validation_rules = vec![rule];
    rules_of(vec![declaration])
}

#[spec(
    behavior = "execute_validation_pattern",
    verify = "an edge rule counts only edges of its edge type and is dropped when no extension declares the kind at its far end"
)]
fn an_edge_rule_counts_only_its_edge_type() {
    let mut declared = rule("W001", "no_outgoing_edges");
    declared.edge_type = Some("BehaviorImplementsFeature".to_string());
    declared.message_template = "behavior '{id}' does not implement any feature".to_string();

    // b1 references an event but no feature; b2 implements a feature.
    let b1 = entity("b1", "behavior", 0, 0).with_edges(
        Direction::Outgoing,
        Some("BehaviorReferencesEvent"),
        1,
    );
    let b2 = entity("b2", "behavior", 0, 0).with_edges(
        Direction::Outgoing,
        Some("BehaviorImplementsFeature"),
        1,
    );
    let built = implementing(declared.clone(), true);
    assert_eq!(
        built.rules.iter().next().unwrap().describe()["edge_writers"],
        serde_json::json!(["behavior.features"])
    );
    let diagnostics = check(&built, &[b1, b2]);
    assert_eq!(
        messages(&diagnostics),
        ["behavior 'b1' does not implement any feature"]
    );

    // Without an extension declaring `feature`, the rule can't be met: dropped.
    let built = implementing(declared, false);
    assert!(built.rules.is_empty());
    assert!(built.diagnostics.is_empty(), "{:?}", built.diagnostics);
}

#[spec(
    behavior = "execute_validation_pattern",
    verify = "an edge rule over an edge type several kinds write without declaring it counts the edges of every writing kind"
)]
fn an_edge_rule_over_an_undeclared_edge_counts_every_writing_kind() {
    // `person` is assigned by `task` (from `@test`) and `ticket` (from
    // `@test/more`), through the `assigned_to` edge neither declares.
    let mut declared = rule("W001", "no_incoming_edges");
    declared.target_kind = Some("person".to_string());
    declared.edge_type = Some("assigned_to".to_string());
    declared.message_template = "person '{id}' is assigned nothing".to_string();
    let mut first = declare("@test", |c| {
        c.kind("task", |k| {
            k.field("assignee", |f| {
                f.field_type(FieldType::Reference)
                    .edge("assigned_to")
                    .target_kind("person");
            });
        });
        c.kind("person", |k| {
            k.description("p");
        });
    });
    first.validation_rules = vec![declared];
    let second = declare("@test/more", |c| {
        c.kind("ticket", |k| {
            k.field("owner", |f| {
                f.field_type(FieldType::Reference)
                    .edge("assigned_to")
                    .target_kind("person");
            });
        });
    });
    let built = rules_of(vec![first, second]);
    assert_eq!(
        built.rules.iter().next().unwrap().describe()["edge_writers"],
        serde_json::json!(["task.assignee", "ticket.owner"])
    );

    let assigned = Some("assigned_to");
    let by_task = entity("p1", "person", 0, 0).with_edges(Direction::Incoming, assigned, 1);
    let by_ticket = entity("p2", "person", 0, 0).with_edges(Direction::Incoming, assigned, 1);
    let by_other = entity("p3", "person", 0, 0).with_edges(Direction::Incoming, Some("owns"), 1);
    let diagnostics = check(&built, &[by_task, by_ticket, by_other]);
    assert_eq!(messages(&diagnostics), ["person 'p3' is assigned nothing"]);
}

#[spec(
    behavior = "execute_validation_pattern",
    verify = "an edge rule no loaded field can satisfy at its target kind is inert"
)]
fn an_edge_rule_no_field_can_satisfy_is_inert() {
    // `behavior.features` is the one field writing BehaviorImplementsFeature:
    // from a behavior, to a feature.
    let mut outgoing = rule("W001", "no_outgoing_edges");
    outgoing.target_kind = Some("feature".to_string());
    outgoing.edge_type = Some("BehaviorImplementsFeature".to_string());
    let mut incoming = rule("W002", "no_incoming_edges");
    incoming.target_kind = Some("behavior".to_string());
    incoming.edge_type = Some("BehaviorImplementsFeature".to_string());

    // No feature writes one, no behavior receives one: nothing can satisfy
    // either rule, so neither is registered (nor flagged).
    for inert in [outgoing, incoming] {
        let built = implementing(inert, true);
        assert!(built.rules.is_empty(), "{:?}", built.rules);
        assert!(built.diagnostics.is_empty(), "{:?}", built.diagnostics);
    }

    // The same rules aimed where the field writes are live.
    let mut outgoing = rule("W001", "no_outgoing_edges");
    outgoing.edge_type = Some("BehaviorImplementsFeature".to_string());
    assert_eq!(implementing(outgoing, true).rules.len(), 1);
    let mut incoming = rule("W002", "no_incoming_edges");
    incoming.target_kind = Some("feature".to_string());
    incoming.edge_type = Some("BehaviorImplementsFeature".to_string());
    assert_eq!(implementing(incoming, true).rules.len(), 1);
}

#[spec(
    behavior = "execute_validation_pattern",
    verify = "missing_field_when_flag_set detects missing specified field on flagged entity"
)]
fn missing_field_when_flag_set_detects_missing_field() {
    let mut declared = rule("W102", "missing_field_when_flag_set");
    declared.field = Some("contract".to_string());
    declared.message_template = "{kind} '{id}' missing field '{field}'".to_string();
    let diagnostics = check(
        &one(declared),
        &[
            entity("b1", "behavior", 1, 0),
            entity("b2", "behavior", 1, 0).with_field("contract", "some text"),
        ],
    );
    assert_eq!(
        messages(&diagnostics),
        ["behavior 'b1' missing field 'contract'"]
    );
}

#[spec(
    behavior = "execute_validation_pattern",
    verify = "field_value_constraint rejects invalid field value"
)]
fn field_value_constraint_rejects_invalid_field_value() {
    let mut declared = rule("W103", "field_value_constraint");
    declared.message_template = "{kind} '{id}' has invalid {field}='{value}'".to_string();
    declared.field = Some("status".to_string());
    declared.constraint = Some(constraint(
        "one_of",
        None,
        &["draft", "active", "deprecated"],
    ));
    let diagnostics = check(
        &one(declared),
        &[
            entity("b1", "behavior", 1, 0).with_field("status", "invalid_status"),
            entity("b2", "behavior", 1, 0).with_field("status", "active"),
            // A field that is not written breaks no constraint.
            entity("b3", "behavior", 1, 0),
        ],
    );
    assert_eq!(
        messages(&diagnostics),
        ["behavior 'b1' has invalid status='invalid_status'"]
    );
}

#[test]
fn a_non_empty_constraint_flags_an_empty_value() {
    let mut declared = rule("I068", "field_value_constraint");
    declared.field = Some("tags".to_string());
    declared.constraint = Some(constraint("non_empty", None, &[]));
    let diagnostics = check(
        &one(declared),
        &[
            entity("b1", "behavior", 1, 0).with_field("tags", ""),
            entity("b2", "behavior", 1, 0).with_field("tags", "x"),
        ],
    );
    assert_eq!(messages(&diagnostics), ["orphan behavior 'b1'"]);
}

/// A `matches` rule on `release.version` with `pattern`.
fn matches(code: &str, pattern: &str) -> ValidationRuleDescriptor {
    ValidationRuleDescriptor {
        code: code.to_string(),
        severity: ValidationSeverity::Warning,
        message_template: "{kind} '{id}' has invalid {field}='{value}'".to_string(),
        check: "field_value_constraint".to_string(),
        target_kind: Some("release".to_string()),
        field: Some("version".to_string()),
        constraint: Some(constraint("matches", Some(pattern), &[])),
        ..Default::default()
    }
}

const SEMVER: &str = r"^\d+\.\d+\.\d+(-[a-zA-Z0-9.]+)?(\+[a-zA-Z0-9.]+)?$";

#[test]
fn matches_accepts_a_value_satisfying_the_regex_and_flags_one_violating_it() {
    let built = one(matches("W093", SEMVER));
    let release = |version: &str| entity("r1", "release", 1, 0).with_field("version", version);
    assert!(check(&built, &[release("1.0.0")]).is_empty());
    assert_eq!(
        messages(&check(&built, &[release("v1.2")])),
        ["release 'r1' has invalid version='v1.2'"]
    );
    // Anchored: a valid semver followed by junk is no match.
    assert_eq!(check(&built, &[release("1.0.0-not valid")]).len(), 1);
}

// C14: the regex compiles once, when the rule is built, and checks every
// entity.
#[test]
fn matches_compiles_once_and_checks_every_entity() {
    let built = one(matches("W094", r"^v\d+$"));
    let entities: Vec<EntityRecord> = (0..50)
        .map(|i| {
            let version = if i % 2 == 0 { "v1" } else { "bad" };
            entity(&format!("r{i}"), "release", 1, 0).with_field("version", version)
        })
        .collect();
    let diagnostics = check(&built, &entities);
    assert_eq!(diagnostics.len(), 25, "exactly the non-matching values");
    assert!(
        diagnostics
            .iter()
            .all(|d| d.message.contains("version='bad'"))
    );
}

/// A `conditional_field_required` rule: a deferred feature needs a reason.
fn deferred_needs_reason() -> super::Built {
    one(ValidationRuleDescriptor {
        code: "I059".to_string(),
        severity: ValidationSeverity::Info,
        message_template: "feature '{id}' has status 'deferred' but no reason".to_string(),
        check: "conditional_field_required".to_string(),
        target_kind: Some("feature".to_string()),
        field: Some("reason".to_string()),
        constraint: Some(constraint(
            "when_field_equals",
            Some("status"),
            &["deferred"],
        )),
        ..Default::default()
    })
}

#[test]
fn conditional_field_required_fires_only_when_its_condition_holds_and_the_field_is_missing() {
    let built = deferred_needs_reason();
    let feature = |id: &str| entity(id, "feature", 1, 0);
    let diagnostics = check(
        &built,
        &[
            // condition met, field missing: fires
            feature("f1").with_field("status", "deferred"),
            // condition met, field empty: fires
            feature("f2")
                .with_field("status", "deferred")
                .with_field("reason", ""),
            // condition not met
            feature("f3").with_field("status", "active"),
            // field present
            feature("f4")
                .with_field("status", "deferred")
                .with_field("reason", "Waiting for upstream"),
            // condition field absent
            feature("f5"),
        ],
    );
    assert_eq!(
        messages(&diagnostics),
        [
            "feature 'f1' has status 'deferred' but no reason",
            "feature 'f2' has status 'deferred' but no reason",
        ]
    );
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code == "I059" && d.severity == Severity::Info)
    );
}

#[test]
fn missing_required_field_fires_on_its_kind_when_the_field_is_absent() {
    let mut declared = rule("E006", "missing_required_field");
    declared.severity = ValidationSeverity::Error;
    declared.field = Some("contract".to_string());
    declared.message_template = "behavior '{id}' is missing required field 'contract'".to_string();
    let built = one(declared);
    let diagnostics = check(
        &built,
        &[
            entity("my_beh", "behavior", 1, 0),
            entity("ok_beh", "behavior", 1, 0).with_field("contract", "Handles user login"),
            // another kind: skipped
            entity("my_evt", "event", 1, 0),
        ],
    );
    assert_eq!(
        messages(&diagnostics),
        ["behavior 'my_beh' is missing required field 'contract'"]
    );
    assert_eq!(diagnostics[0].severity, Severity::Error);
}

#[spec(
    behavior = "execute_validation_pattern",
    verify = "file_exists reports missing file-reference field targets"
)]
fn file_exists_reports_missing_file_reference_field_targets() {
    let mut declared = rule("E101", "file_exists");
    declared.severity = ValidationSeverity::Error;
    declared.message_template = "{kind} '{id}' references missing file".to_string();
    declared.field = Some("gherkin".to_string());
    let built = one(declared);
    let disk = MemoryDisk::new().with_file("/spec/features/login.feature", b"");
    let naming = |id: &str, path: &str| entity(id, "behavior", 1, 0).with_field("gherkin", path);
    let entities = [
        naming("b1", "features/login.feature"),
        naming("b2", "features/logout.feature"),
        naming("b3", "/nonexistent/file.feature"),
        naming("b4", "/spec/features/login.feature"),
    ];

    // Relative paths are the spec root's; absolute ones are checked as
    // written.
    let input = RuleInput {
        entities: &entities,
        edges: &[],
        spec_root: Path::new("/spec"),
        disk: &disk,
    };
    let diagnostics = built.rules.check(&input, &NoVerdicts);
    assert_eq!(
        messages(&diagnostics),
        [
            "behavior 'b2' references missing file",
            "behavior 'b3' references missing file",
        ]
    );
    assert!(diagnostics.iter().all(|d| d.code == "E101"));
}

#[spec(
    behavior = "execute_validation_pattern",
    verify = "file_exists resolves a relative path against the spec root, never the working directory"
)]
fn file_exists_resolves_against_the_spec_root_not_the_working_directory() {
    let mut declared = rule("E101", "file_exists");
    declared.field = Some("doc".to_string());
    declared.message_template = "{id}: missing '{value}'".to_string();
    let built = one(declared);
    let entities = [entity("b1", "behavior", 0, 0).with_field("doc", "Cargo.toml")];
    let beside = MemoryDisk::new().with_file("Cargo.toml", b"");
    let input = RuleInput {
        entities: &entities,
        edges: &[],
        spec_root: Path::new("/spec"),
        disk: &beside,
    };
    assert_eq!(
        messages(&built.rules.check(&input, &NoVerdicts)),
        ["b1: missing 'Cargo.toml'"]
    );
    // The rule asks for the spec root's file, never the working directory's.
    assert_eq!(beside.asked(), [Asked::Exists("/spec/Cargo.toml".into())]);
}

#[spec(
    behavior = "execute_validation_pattern",
    verify = "file_exists checks each item of a list field as its own path"
)]
fn file_exists_checks_each_item_of_a_list_field() {
    let mut declared = rule("E102", "file_exists");
    declared.field = Some("docs".to_string());
    declared.message_template = "{id}: missing '{value}'".to_string();
    let built = one(declared);
    let root = Path::new("/spec");
    let disk = MemoryDisk::new().with_file("/spec/a.md", b"");
    let entities = [
        entity("b1", "behavior", 0, 0).with_list("docs", &["a.md", "b.md"]),
        // An empty list names no path.
        entity("b2", "behavior", 0, 0).with_list("docs", &[]),
        // A scalar is one path, as written.
        entity("b3", "behavior", 0, 0).with_field("docs", "a.md, b.md"),
    ];
    let input = RuleInput {
        entities: &entities,
        edges: &[],
        spec_root: root,
        disk: &disk,
    };

    let diagnostics = built.rules.check(&input, &NoVerdicts);

    assert_eq!(
        messages(&diagnostics),
        ["b1: missing 'b.md'", "b3: missing 'a.md, b.md'"]
    );
    // The files the rule reads: every item, once, against the spec root.
    assert_eq!(
        built.rules.files(&input),
        [
            root.join("a.md"),
            root.join("a.md, b.md"),
            root.join("b.md"),
        ]
    );
}

/// W009-like: a `verify_kind_allowlist` rule on `target` allowing `allowed`.
fn allowlist(target: &str, allowed: &[&str]) -> super::Built {
    one(ValidationRuleDescriptor {
        code: "W009".to_string(),
        severity: ValidationSeverity::Warning,
        message_template: "entity '{id}' has verify kind '{value}' not in allowed set {allowed}"
            .to_string(),
        check: "verify_kind_allowlist".to_string(),
        target_kind: Some(target.to_string()),
        constraint: Some(constraint("one_of", None, allowed)),
        ..Default::default()
    })
}

fn verifying(id: &str, kinds: &[&str]) -> EntityRecord {
    kinds.iter().fold(entity(id, "invariant", 1, 1), |e, k| {
        e.with_obligation(k, "")
    })
}

#[test]
fn verify_kind_allowlist_flags_the_first_offending_kind() {
    let built = allowlist("invariant", &["property", "unit"]);
    let diagnostics = check(&built, &[verifying("inv", &["load"])]);
    assert_eq!(
        messages(&diagnostics),
        ["entity 'inv' has verify kind 'load' not in allowed set property, unit"]
    );
}

#[test]
fn verify_kind_allowlist_passes_allowed_kinds_and_a_bare_verify() {
    let built = allowlist("invariant", &["property", "unit", "mutation"]);
    assert!(check(&built, &[verifying("inv", &["unit", "mutation"])]).is_empty());
    // A bare `verify "..."` (empty kind) is exempt.
    assert!(check(&built, &[verifying("inv2", &[""])]).is_empty());
}

/// A `no_verify_statements` rule `code` on `target` (every kind when
/// `None`), reading `field`.
fn obligation_rule(code: &str, target: Option<&str>, field: &str) -> ValidationRuleDescriptor {
    ValidationRuleDescriptor {
        code: code.to_string(),
        severity: ValidationSeverity::Warning,
        message_template: "{kind} '{id}' has no verify".to_string(),
        check: "no_verify_statements".to_string(),
        target_kind: target.map(str::to_string),
        field: Some(field.to_string()),
        ..Default::default()
    }
}

#[spec(
    behavior = "te_validate_unverified_testable",
    verify = "a field named like an obligation or an exemption exempts nothing from W004"
)]
fn w004_reads_statements_and_the_exemption_not_field_names() {
    let built = one(obligation_rule("W004", Some("behavior"), "verify"));
    assert_eq!(check(&built, &[entity("b1", "behavior", 1, 1)]).len(), 1);
    let verified = entity("b2", "behavior", 1, 1).with_obligation("unit", "it works");
    assert!(check(&built, &[verified]).is_empty());

    // Members named like a statement or an exemption are fields: none of
    // them stands in for an obligation or exempts the entity.
    for (name, value) in [
        ("verify", "string"),
        ("gherkin", "string"),
        ("abstract", "true"),
        ("variants", "open | done"),
    ] {
        let named = entity("b3", "behavior", 1, 1).with_field(name, value);
        assert_eq!(
            check(&built, &[named]).len(),
            1,
            "a field named {name} exempts nothing"
        );
    }

    let exempt = entity("b4", "behavior", 1, 1).exempt(Exemption::Union);
    assert!(check(&built, &[exempt]).is_empty());
}

#[spec(
    behavior = "execute_validation_pattern",
    verify = "pattern violation produces diagnostic with configured code and severity"
)]
fn applies_to_reads_target_kind_once() {
    let built = rules(vec![
        obligation_rule("W004", Some("behavior"), "verify"),
        obligation_rule("P300", None, "verify"),
    ]);
    let (p300, w004) = {
        let mut iter = built.rules.iter();
        (iter.next().unwrap(), iter.next().unwrap())
    };
    assert_eq!((p300.code(), w004.code()), ("P300", "W004"));
    assert!(w004.applies_to("behavior"));
    assert!(!w004.applies_to("event"));
    assert!(p300.applies_to("behavior") && p300.applies_to("event"));

    // The rules read it the same way: an untargeted rule runs on every
    // kind, a targeted one on its own.
    let entities = [entity("b", "behavior", 0, 0), entity("e", "event", 0, 0)];
    let diagnostics = check(&built, &entities);
    let reported: Vec<(&str, &str)> = diagnostics
        .iter()
        .map(|d| (d.code.as_str(), d.message.as_str()))
        .collect();
    assert_eq!(
        reported,
        [
            ("P300", "behavior 'b' has no verify"),
            ("P300", "event 'e' has no verify"),
            ("W004", "behavior 'b' has no verify"),
        ]
    );
    assert!(diagnostics.iter().all(|d| d.severity == Severity::Warning));

    // The verify rule for a kind is the first by code that applies; rules
    // of other checks oblige nothing.
    let mut other = obligation_rule("A000", None, "verify");
    other.check = "missing_required_field".to_string();
    let built = rules(vec![
        other,
        obligation_rule("W004", Some("behavior"), "verify"),
        obligation_rule("P300", None, "verify"),
        obligation_rule("W009", Some("event"), "verify"),
    ]);
    let code = |kind| built.rules.verify_rule_for(kind).map(|r| r.code());
    assert_eq!(code("behavior"), Some("P300"));
    assert_eq!(code("event"), Some("P300"));
    assert_eq!(code("type"), Some("P300"));
    assert!(built.rules.obligates("type"));
    let only_w004 = one(obligation_rule("W004", Some("behavior"), "verify"));
    assert_eq!(
        only_w004.rules.verify_rule_for("behavior").unwrap().code(),
        "W004"
    );
    assert!(only_w004.rules.verify_rule_for("type").is_none());
    assert!(!only_w004.rules.obligates("type"));
}

#[spec(
    behavior = "snapshot_entities_once",
    verify = "a kind that accepts no verify statements owes no obligations, whatever rule applies to it"
)]
fn a_kind_without_verify_owes_no_statements_but_still_owes_other_fields() {
    let memo = entity("m", "memo", 0, 0).exempt(Exemption::NoVerify);
    // Statement obligations: exempt.
    let statements = one(obligation_rule("P300", None, "verify"));
    assert!(check(&statements, std::slice::from_ref(&memo)).is_empty());
    let mut flagged = obligation_rule("P301", None, "verify");
    flagged.check = "missing_field_when_flag_set".to_string();
    assert!(check(&one(flagged), std::slice::from_ref(&memo)).is_empty());
    // An obligation declared in another field: not exempt.
    let gherkin = one(obligation_rule("P302", None, "gherkin"));
    assert_eq!(check(&gherkin, std::slice::from_ref(&memo)).len(), 1);
    // A union body exempts from both.
    let union = entity("u", "memo", 0, 0).exempt(Exemption::Union);
    assert!(check(&gherkin, &[union]).is_empty());
}

#[spec(
    behavior = "execute_validation_pattern",
    verify = "Execute Validation Pattern: declarative validation holds — all_entities_matched, violations_diagnosed, deterministic_order"
)]
fn execute_validation_pattern_contract() {
    let built = rules(vec![
        rule("W200", "no_outgoing_edges"),
        rule("W100", "no_incoming_edges"),
    ]);
    // Entities of other kinds are skipped by the target kind filter.
    let entities = [
        entity("b1", "behavior", 0, 0),
        entity("b2", "behavior", 2, 0),
        entity("f1", "feature", 0, 0),
    ];
    let diagnostics = check(&built, &entities);
    let reported: Vec<(&str, &str)> = diagnostics
        .iter()
        .map(|d| (d.code.as_str(), d.message.as_str()))
        .collect();
    // Rules in code order, each rule's entities in id order.
    assert_eq!(
        reported,
        [
            ("W100", "orphan behavior 'b1'"),
            ("W200", "orphan behavior 'b1'"),
            ("W200", "orphan behavior 'b2'"),
        ]
    );
    // Identical across runs.
    assert_eq!(diagnostics, check(&built, &entities));
}

// -- emit_diagnostic_from_pattern --

#[spec(
    behavior = "emit_diagnostic_from_pattern",
    verify = "message template interpolates {id} and {kind}"
)]
fn message_template_interpolates_id_and_kind() {
    let built = one(rule("W100", "no_incoming_edges"));
    let diagnostics = check(&built, &[entity("my_beh", "behavior", 0, 0)]);
    assert_eq!(messages(&diagnostics), ["orphan behavior 'my_beh'"]);
    assert!(diagnostics[0].span.is_some());
}

#[spec(
    behavior = "emit_diagnostic_from_pattern",
    verify = "message template interpolates {field} and {value}"
)]
fn message_template_interpolates_field_and_value() {
    let mut declared = rule("W103", "field_value_constraint");
    declared.message_template = "{kind} '{id}' has {field}='{value}'".to_string();
    declared.field = Some("status".to_string());
    declared.constraint = Some(constraint("one_of", None, &["active"]));
    let diagnostics = check(
        &one(declared),
        &[entity("b1", "behavior", 0, 0).with_field("status", "invalid")],
    );
    assert_eq!(
        messages(&diagnostics),
        ["behavior 'b1' has status='invalid'"]
    );
}

#[spec(
    behavior = "emit_diagnostic_from_pattern",
    verify = "message template interpolates {allowed} with a one_of rule's values"
)]
fn a_one_of_rule_fills_allowed_with_its_values() {
    let mut declared = rule("W900", "field_value_constraint");
    declared.target_kind = Some("behavior".to_string());
    declared.field = Some("state".to_string());
    declared.message_template = "{id} {value} not in {allowed}".to_string();
    declared.constraint = Some(constraint("one_of", None, &["open", "closed"]));
    let diagnostics = check(
        &one(declared),
        &[entity("b1", "behavior", 0, 0).with_field("state", "shipped")],
    );
    assert_eq!(messages(&diagnostics), ["b1 shipped not in open, closed"]);

    // The allowlist rule fills it already.
    let built = allowlist("invariant", &["property", "unit"]);
    assert_eq!(
        messages(&check(&built, &[verifying("inv", &["load"])])),
        ["entity 'inv' has verify kind 'load' not in allowed set property, unit"]
    );
}

#[spec(
    behavior = "emit_diagnostic_from_pattern",
    verify = "diagnostic code matches pattern code"
)]
fn diagnostic_code_matches_pattern_code() {
    let mut declared = rule("E999", "no_incoming_edges");
    declared.message_template = "test".to_string();
    let diagnostics = check(&one(declared), &[entity("b1", "behavior", 0, 0)]);
    assert_eq!(diagnostics[0].code, "E999");
}

#[spec(
    behavior = "emit_diagnostic_from_pattern",
    verify = "diagnostic severity matches pattern severity"
)]
fn diagnostic_severity_matches_pattern_severity() {
    for (severity, expected) in [
        (ValidationSeverity::Error, Severity::Error),
        (ValidationSeverity::Warning, Severity::Warning),
        (ValidationSeverity::Info, Severity::Info),
    ] {
        let mut declared = rule("X001", "no_incoming_edges");
        declared.severity = severity.clone();
        let diagnostics = check(&one(declared), &[entity("b1", "behavior", 0, 0)]);
        assert_eq!(diagnostics[0].severity, expected, "{severity:?}");
    }
}

#[spec(
    behavior = "emit_diagnostic_from_pattern",
    verify = "Emit Diagnostic From Pattern: pattern diagnostic emission holds — violation_detected, pattern_configured, diagnostic_emitted, template_interpolated"
)]
fn emit_diagnostic_from_pattern_contract() {
    // requires: violation detected, pattern configured
    let mut declared = rule("W100", "no_incoming_edges");
    declared.message_template = "{kind} '{id}' orphan, {field} {value} {allowed}".to_string();
    let diagnostics = check(&one(declared), &[entity("b1", "behavior", 0, 0)]);
    // ensures: template interpolated (a placeholder with nothing to put in
    // stays as written), code and severity the rule's
    assert_eq!(
        messages(&diagnostics),
        ["behavior 'b1' orphan, {field} {value} {allowed}"]
    );
    assert_eq!(diagnostics[0].code, "W100");
    assert_eq!(diagnostics[0].severity, Severity::Warning);
}

#[spec(
    behavior = "registry_build_rules",
    verify = "a rule targeting a kind no loaded extension declares reports nothing"
)]
fn a_rule_for_an_unloaded_kind_reports_nothing() {
    let mut ghost = rule("W100", "no_incoming_edges");
    ghost.target_kind = Some("ghost".to_string());
    let built = rules_of(vec![crate::support::software(), declaring(vec![ghost])]);
    assert!(built.diagnostics.is_empty(), "{:?}", built.diagnostics);
    let orphans = [
        entity("b1", "behavior", 0, 0),
        entity("b2", "behavior", 0, 0),
        // The rule's own target, written though nobody declares it.
        entity("g1", "ghost", 0, 0),
    ];
    assert!(built.rules.check(&over(&orphans), &NoVerdicts).is_empty());

    // The same rule on a loaded kind does fire: it is inert, not broken.
    let built = one(rule("W100", "no_incoming_edges"));
    assert_eq!(check(&built, &orphans).len(), 2);
}

/// The rule for a kind nobody declares is not registered at all (ADR 0020
/// D5), whether the entities of that keyword are there or not.
#[test]
fn a_rule_for_an_unloaded_kind_is_not_registered() {
    let mut ghost = rule("W100", "no_incoming_edges");
    ghost.target_kind = Some("ghost".to_string());
    let built = rules_of(vec![crate::support::software(), declaring(vec![ghost])]);
    assert!(built.rules.is_empty(), "{:?}", built.rules.len());

    // A structural target is always there: its rule is registered.
    let mut refs = rule("W101", "no_incoming_edges");
    refs.target_kind = Some("ref".to_string());
    let built = rules_of(vec![crate::support::software(), declaring(vec![refs])]);
    assert_eq!(built.rules.len(), 1);
}

#[spec(
    behavior = "registry_build_rules",
    verify = "a rule whose edge type no loaded extension declares reports nothing"
)]
fn a_rule_whose_edge_type_no_loaded_extension_declares_reports_nothing() {
    let mut edge_rule = rule("X005", "no_outgoing_edges");
    edge_rule.edge_type = Some("NoSuchEdge".to_string());
    let mut cycle_rule = rule("X003", "cycle_detection");
    cycle_rule.edge_type = Some("NoSuchEdge".to_string());
    let built = rules(vec![edge_rule, cycle_rule]);

    // Inert: not registered, so no entity is reported, edges or not.
    assert!(built.rules.is_empty());
    assert!(built.diagnostics.is_empty(), "{:?}", built.diagnostics);
    let entities = [
        entity("b1", "behavior", 0, 0),
        entity("b2", "behavior", 0, 0).with_edges(Direction::Outgoing, Some("Emits"), 1),
    ];
    assert!(check(&built, &entities).is_empty());
}

#[test]
fn an_sdk_declared_rule_runs_as_declared() {
    let declaration = declare("@test/sdk", |c| {
        c.kind("Behavior", |k| {
            k.keyword("behavior").testable(true).supports_verify(true);
        });
        c.rule("W300", |r| {
            r.check(CheckKind::MissingFieldWhenFlagSet)
                .severity(ValidationSeverity::Warning)
                .target_kind("behavior")
                .field("verify")
                .message_template("{kind} '{id}' has no {field}");
        });
    });
    let built = rules_of(vec![declaration]);
    // `verify` on an entity that owes no statements is not missing.
    let diagnostics = check(
        &built,
        &[
            entity("b1", "behavior", 0, 0),
            entity("b2", "behavior", 0, 0).exempt(Exemption::Union),
        ],
    );
    assert_eq!(messages(&diagnostics), ["behavior 'b1' has no verify"]);
}
