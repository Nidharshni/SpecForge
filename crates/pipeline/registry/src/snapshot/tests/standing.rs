use specforge_test_macros::test as specforge_test;

use super::{declared, snapshot, w004, w004_ids};
use crate::entity::{EntityRecord, Exemption};
use crate::snapshot::Standing;
use crate::snapshot::wire::pass_entity;
use specforge_protocol_types::ValidationRuleDescriptor;

#[specforge_test(
    behavior = "te_validate_unverified_testable",
    verify = "a union type never produces W004"
)]
fn a_union_type_owes_no_obligations() {
    let ids = w004_ids(
        "type Status = active | inactive\n\ntype Plain \"Plain\" {\n  id string\n}\n",
        false,
    );
    assert_eq!(ids, ["Plain"]);
}

#[test]
fn an_enum_values_list_is_not_a_union() {
    let ids = w004_ids(
        "type Priority \"Priority\" {\n  values [high, low]\n}\n",
        false,
    );
    assert_eq!(ids, ["Priority"]);
}

#[specforge_test(
    behavior = "te_validate_unverified_testable",
    verify = "an abstract entity never produces W004"
)]
fn an_abstract_entity_owes_no_obligations_when_its_kind_declares_the_flag() {
    let source = "behavior base \"Base\" {\n  contract \"The system MUST work\"\n  abstract true\n}\n\n\
                      behavior concrete \"Concrete\" {\n  contract \"The system MUST work\"\n  abstract false\n}\n";
    assert_eq!(w004_ids(source, true), ["concrete"]);
    // Without a registry entry declaring it, `abstract` is just a name.
    assert_eq!(w004_ids(source, false), ["base", "concrete"]);
}

/// Production coerces a quoted `"false"` on a `bool` flag to `false` (the graph
/// build's field forms), so it exempts nothing. Needs the graph built with the
/// registry build's own inputs; the default-config graph keeps the string.
#[test]
#[ignore = "flipped by T4"]
fn a_quoted_false_flag_exempts_nothing() {
    let source = "behavior q \"Q\" {\n  abstract \"false\"\n}\n\nbehavior t \"T\" {\n  abstract \"true\"\n}\n";
    let registries = declared(&[("behavior", true, true)], true, vec![w004("behavior")]);
    let snapshot = snapshot(source, &registries);
    assert_eq!(snapshot.standing("q").unwrap().exemption, None);
    assert_eq!(w004_ids(source, true), ["q"]);
    assert_eq!(
        snapshot.standing("t").unwrap().exemption,
        Some(Exemption::Flag {
            field: "abstract".into()
        })
    );
}

/// The ids of `source`'s entities whose kind is testable, under `kinds`
/// as `(name, testable, supports_verify)`.
fn testable_ids(source: &str, kinds: &[(&str, bool, bool)]) -> Vec<String> {
    let snapshot = snapshot(source, &declared(kinds, false, Vec::new()));
    snapshot
        .iter()
        .filter(|(_, standing)| standing.testable)
        .map(|(record, _)| record.id.clone())
        .collect()
}

#[specforge_test(
    invariant = "testable_entity_classification",
    verify = "no default testability assumed by core"
)]
fn no_kind_is_testable_unless_an_extension_says_so() {
    let source = "behavior login \"Login\" {\n}\n";
    assert!(testable_ids(source, &[]).is_empty());
    assert!(testable_ids(source, &[("behavior", false, false)]).is_empty());
}

#[specforge_test(
    invariant = "testable_entity_classification",
    verify = "testable=false entity excluded from coverage"
)]
fn only_kinds_declared_testable_count() {
    let kinds = [
        ("behavior", true, true),
        ("type", true, true),
        // Accepts verify statements but does not count toward coverage.
        ("property", false, true),
        ("feature", false, false),
    ];
    let source = "behavior b \"B\" {\n}\n\ntype t \"T\" {\n}\n\n\
                      property p \"P\" {\n  verify unit \"holds\"\n}\n\nfeature f \"F\" {\n}\n";
    assert_eq!(testable_ids(source, &kinds), ["b", "t"]);
}

#[specforge_test(
    behavior = "snapshot_entities_once",
    verify = "an entity owes obligations when a no_verify_statements rule applies to its kind and neither a union body nor an exempting flag exempts it"
)]
fn an_entity_owes_obligations_when_a_rule_applies_and_nothing_exempts_it() {
    let kinds = [
        ("behavior", true, true),
        ("type", true, true),
        ("memo", false, false),
    ];
    let source = "behavior open \"Open\" {\n}\n\nbehavior base \"Base\" {\n  abstract true\n}\n\n\
                      type Status = active | inactive\n\ntype Plain \"Plain\" {\n  id string\n}\n\n\
                      memo note \"Note\" {\n}\n";
    let owes = |rules: Vec<ValidationRuleDescriptor>| -> Vec<String> {
        let registries = declared(&kinds, true, rules);
        snapshot(source, &registries)
            .iter()
            .filter(|(_, standing)| standing.owes_obligations())
            .map(|(record, _)| record.id.clone())
            .collect()
    };
    // What an entity declares is its own, rule or not.
    for (record, standing) in snapshot(source, &declared(&kinds, true, Vec::new())).iter() {
        assert_eq!(
            standing.declares_obligations(),
            !record.obligations.is_empty(),
            "{}",
            record.id
        );
    }
    // No rule: nobody owes anything.
    assert!(owes(Vec::new()).is_empty());
    // A rule on `behavior`: its entities owe, unless a flag exempts.
    assert_eq!(owes(vec![w004("behavior")]), ["open"]);
    // A rule without a target kind applies to every kind; a union body,
    // a flag and a kind without `verify` still exempt.
    let mut untargeted = w004("behavior");
    untargeted.target_kind = None;
    assert_eq!(owes(vec![untargeted]), ["Plain", "open"]);
    // What exempts each, decided once.
    let registries = declared(&kinds, true, Vec::new());
    let exemptions: Vec<(String, Option<Exemption>)> = snapshot(source, &registries)
        .iter()
        .map(|(record, standing)| {
            assert_eq!(record.exemption, standing.exemption);
            (record.id.clone(), standing.exemption.clone())
        })
        .collect();
    assert_eq!(
        exemptions,
        [
            ("Plain".to_string(), None),
            ("Status".to_string(), Some(Exemption::Union)),
            (
                "base".to_string(),
                Some(Exemption::Flag {
                    field: "abstract".to_string()
                })
            ),
            ("note".to_string(), Some(Exemption::NoVerify)),
            ("open".to_string(), None),
        ]
    );
}

#[specforge_test(
    behavior = "snapshot_entities_once",
    verify = "the standing counts an entity toward coverage exactly as the coverage rule counts the pass entity it sends"
)]
fn standing_counts_as_the_coverage_rule_does() {
    let span = specforge_common::SourceSpan {
        file: specforge_common::Sym::new("t.spec"),
        start_line: 1,
        start_col: 1,
        end_line: 1,
        end_col: 1,
    };
    let exemptions = [
        None,
        Some(Exemption::Union),
        Some(Exemption::Flag {
            field: "abstract".into(),
        }),
        Some(Exemption::NoVerify),
    ];
    let mut combinations = 0;
    for testable in [false, true] {
        for rule in [None, Some("W004".to_string())] {
            for exemption in &exemptions {
                for declared in [0, 1] {
                    let mut record = EntityRecord::new("item", "a", &span);
                    record.exemption = exemption.clone();
                    if declared == 1 {
                        record = record.with_obligation("unit", "works");
                    }
                    let standing = Standing {
                        testable,
                        rule: rule.clone(),
                        exemption: exemption.clone(),
                        declared,
                    };
                    let entity = specforge_coverage::Entity::from(&pass_entity(&record, &standing));
                    assert_eq!(standing.counting(), entity.counting(), "{standing:?}");
                    assert_eq!(entity.owes_none, !standing.owes_obligations());
                    assert_eq!(standing.exempt(), testable && !standing.counts());
                    combinations += 1;
                }
            }
        }
    }
    assert_eq!(combinations, 32);
}

#[specforge_test(
    behavior = "snapshot_entities_once",
    verify = "the coverage rule reads each entity as the pass input carries it"
)]
fn coverage_entities_are_the_pass_entities_as_the_rule_reads_them() {
    let registries = declared(
        &[
            ("behavior", true, true),
            ("type", true, true),
            ("memo", false, false),
        ],
        true,
        vec![w004("behavior"), w004("type")],
    );
    let source = "behavior open \"Open\" {\n  features [base]\n  verify unit \"works\"\n}\n\n\
                      behavior other \"Other\" {\n  features [base]\n}\n\n\
                      behavior base \"Base\" {\n  abstract true\n}\n\n\
                      type Status = active | inactive\n\ntype Plain \"Plain\" {\n  id string\n}\n\n\
                      memo note \"Note\" {\n}\n";
    let snapshot = snapshot(source, &registries);
    let passed = snapshot.pass_entities();
    let covered = snapshot.coverage_entities();
    assert_eq!(passed.len(), 6);
    assert_eq!(covered.len(), passed.len());
    // The reading the rule makes of a pass entity, written out.
    for (entity, pass) in covered.iter().zip(&passed) {
        assert_eq!(entity.id, pass.id);
        assert_eq!(entity.kind, pass.kind);
        assert_eq!(entity.testable, pass.testable, "{}", pass.id);
        assert_eq!(entity.owes_none, pass.owes_none, "{}", pass.id);
        assert_eq!(entity.verify_kinds, pass.verify_kinds, "{}", pass.id);
        assert_eq!(entity.verify_texts, pass.verify_texts, "{}", pass.id);
        assert_eq!(entity.risk, None, "{}", pass.id);
        assert_eq!(
            entity.referenced,
            pass.incoming_edge_count > 0,
            "{}",
            pass.id
        );
    }
    // The cases are all there: a testable entity that owes, one that is
    // exempt, one nothing refers to and one that something does.
    let by_id = |id: &str| covered.iter().find(|e| e.id == id).unwrap();
    assert!(by_id("open").testable && !by_id("open").owes_none);
    assert!(by_id("base").owes_none);
    assert!(by_id("Status").owes_none);
    assert!(!by_id("note").testable && by_id("note").owes_none);
    assert!(by_id("base").referenced);
    assert!(!by_id("open").referenced);
}
