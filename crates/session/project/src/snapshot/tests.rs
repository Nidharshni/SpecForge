use super::*;
use specforge_test_macros::test as specforge_test;

/// Every field of `source`'s first entity: key, variant name, text.
fn texts(source: &str) -> Vec<(String, &'static str, String)> {
    let parsed = specforge_parser::parse(source, "t.spec");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    parsed.entities[0]
        .fields
        .entries()
        .iter()
        .map(|e| {
            let variant = match &e.value {
                FieldValue::String(_) => "String",
                FieldValue::Identifier(_) => "Identifier",
                FieldValue::Date(_) => "Date",
                FieldValue::Integer(_) => "Integer",
                FieldValue::Boolean(_) => "Boolean",
                FieldValue::List(_) => "List",
                FieldValue::VariantList(_) => "VariantList",
                FieldValue::TypeUnion(_) => "TypeUnion",
                FieldValue::Expression(_) => "Expression",
                FieldValue::Block(_) => "Block",
            };
            // The text is the items joined, for every list-shaped value.
            if let Some(items) = field_items(&e.value) {
                let joiner = match variant {
                    "VariantList" | "TypeUnion" => " | ",
                    _ => ", ",
                };
                assert_eq!(items.join(joiner), field_text(&e.value), "{}", e.key);
            }
            (e.key.to_string(), variant, field_text(&e.value))
        })
        .collect()
}

#[specforge_test(
    behavior = "snapshot_entities_once",
    verify = "every written field keeps its value shape and value span beside its text"
)]
fn every_variant_has_a_value_shape() {
    let source = r#"item x "X" {
  title "a title"
  owner alice
  due 2026-10-06
  count 42
  active true
  labels ["a, b", "c"]
  needs [y, z]
  values [low, high]
  shape string | string[]
  mix [1, true, two]
  metric expr { latency < 10ms, load > 5 }
  ensures {
    done "it is done"
  }
  verify unit "x works"
}
"#;
    let (graph, _) = specforge_graph::build_graph(&[specforge_parser::parse(source, "t.spec")]);
    let snapshot = EntitySnapshot::of(&graph, &RegistryBuild::default(), Path::new("."));
    let record = snapshot.records().iter().find(|r| r.id == "x").unwrap();
    let shapes: Vec<(&str, ValueShape)> = record
        .fields
        .iter()
        .map(|f| (f.key.as_str(), f.shape))
        .collect();
    assert_eq!(
        shapes,
        [
            ("title", ValueShape::String),
            ("owner", ValueShape::Identifier),
            ("due", ValueShape::Date),
            ("count", ValueShape::Integer),
            ("active", ValueShape::Boolean),
            ("labels", ValueShape::List),
            ("needs", ValueShape::List),
            ("values", ValueShape::List),
            ("shape", ValueShape::TypeUnion),
            ("mix", ValueShape::List),
            ("metric", ValueShape::Expression),
            ("ensures", ValueShape::Block),
        ]
    );
    for shape in [ValueShape::List, ValueShape::Variants] {
        assert!(shape.is_list());
    }
    assert!(!ValueShape::Block.is_list());

    // Each value carries the span it is written at: its own line.
    let lines: Vec<(&str, Option<usize>)> = record
        .fields
        .iter()
        .map(|f| (f.key.as_str(), f.value_span.as_ref().map(|s| s.start_line)))
        .collect();
    for (key, line) in [("title", 2), ("count", 5), ("labels", 7), ("needs", 8)] {
        assert_eq!(
            lines.iter().find(|(k, _)| *k == key).map(|(_, l)| *l),
            Some(Some(line)),
            "{key}: {lines:?}"
        );
    }
}

#[specforge_test(
    behavior = "snapshot_entities_once",
    verify = "a variant list or type union is its members joined by ' | ', a list or expression group its items joined by ', '"
)]
fn field_text_of_every_variant() {
    let source = r#"item x "X" {
  title "a title"
  owner alice
  due 2026-10-06
  count 42
  active true
  labels ["a, b", "c"]
  needs [y, z]
  values [low, high]
  shape string | string[]
  mix [1, true, two]
  metric expr { latency < 10ms, load > 5 }
  ensures {
    done "it is done"
    kept "it is kept"
  }
  verify unit "x works"
  verify "x holds"
}
"#;
    let expected = [
        ("title", "String", "a title"),
        ("owner", "Identifier", "alice"),
        ("due", "Date", "2026-10-06"),
        ("count", "Integer", "42"),
        ("active", "Boolean", "true"),
        ("labels", "List", "a, b, c"),
        ("needs", "List", "y, z"),
        ("values", "List", "low, high"),
        ("shape", "TypeUnion", "string | string[]"),
        ("mix", "List", "1, true, two"),
        ("metric", "Expression", "latency < 10ms, load > 5"),
        ("ensures", "Block", "done, kept"),
    ];
    let actual = texts(source);
    assert_eq!(
        actual,
        expected
            .iter()
            .map(|(k, v, t)| (k.to_string(), *v, t.to_string()))
            .collect::<Vec<_>>()
    );

    // A list keeps its items unjoined: the joined text of `labels`
    // cannot tell "a, b" from "a" and "b"; its items can.
    let parsed = specforge_parser::parse(source, "t.spec");
    let items: Vec<(String, Option<Vec<String>>)> = parsed.entities[0]
        .fields
        .entries()
        .iter()
        .map(|e| (e.key.to_string(), field_items(&e.value)))
        .collect();
    let of = |key: &str| {
        items
            .iter()
            .find(|(k, _)| k == key)
            .and_then(|(_, items)| items.clone())
    };
    let owned = |list: &[&str]| Some(list.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    assert_eq!(of("labels"), owned(&["a, b", "c"]));
    assert_eq!(of("values"), owned(&["low", "high"]));
    assert_eq!(of("shape"), owned(&["string", "string[]"]));
    assert_eq!(of("mix"), owned(&["1", "true", "two"]));
    assert_eq!(of("metric"), owned(&["latency < 10ms", "load > 5"]));
    assert_eq!(of("ensures"), owned(&["done", "kept"]));
    for scalar in ["title", "owner", "due", "count", "active"] {
        assert_eq!(of(scalar), None, "{scalar}");
    }
}

#[specforge_test(
    behavior = "snapshot_entities_once",
    verify = "an empty list or block is written, with empty text, never left out or null"
)]
fn an_empty_value_has_empty_text() {
    let actual = texts("item x \"X\" {\n  values []\n  tags []\n  requires {\n  }\n}\n");
    assert_eq!(
        actual,
        [
            ("values".to_string(), "List", String::new()),
            ("tags".to_string(), "List", String::new()),
            ("requires".to_string(), "Block", String::new()),
        ]
    );
}

// ── The obligation rule (moved from coverage.rs; links kept) ──

use specforge_protocol_types::{
    EntityKindDescriptor, ExtensionDeclaration, FieldDescriptor, ValidationRuleDescriptor,
    ValidationSeverity,
};
use specforge_registry::rules::NoVerdicts;
use specforge_registry::{RegistryBuild, build_registries};

fn graph_of(source: &str) -> Graph {
    let (graph, _) = specforge_graph::build_graph(&[specforge_parser::parse(source, "test.spec")]);
    graph
}

fn w004(kind: &str) -> ValidationRuleDescriptor {
    ValidationRuleDescriptor {
        code: "W004".into(),
        severity: ValidationSeverity::Warning,
        message_template: "{kind} '{id}' is testable but declares no verify obligations".into(),
        check: "no_verify_statements".into(),
        target_kind: Some(kind.into()),
        field: Some("verify".into()),
        ..Default::default()
    }
}

/// The registry build of one extension declaring `kinds` as
/// `(name, testable, supports_verify)`, the `abstract` flag on
/// `behavior` (as @specforge/formal does) when `abstract_behaviors`, and
/// `rules`.
fn declared(
    kinds: &[(&str, bool, bool)],
    abstract_behaviors: bool,
    rules: Vec<ValidationRuleDescriptor>,
) -> RegistryBuild {
    let mut declaration = ExtensionDeclaration {
        validation_rules: rules,
        entities: kinds
            .iter()
            .map(|(name, testable, supports_verify)| EntityKindDescriptor {
                name: name.to_string(),
                keyword: Some(name.to_string()),
                testable: *testable,
                supports_verify: *supports_verify,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    declaration.handshake.name = "@specforge/testing".into();
    declaration.handshake.version = "1.0.0".into();
    if abstract_behaviors {
        declaration
            .entities
            .iter_mut()
            .find(|kind| kind.name == "behavior")
            .expect("a behavior kind")
            .fields
            .push(FieldDescriptor {
                name: "abstract".into(),
                field_type: "bool".into(),
                exempts_obligations: true,
                ..Default::default()
            });
    }
    let build = build_registries(vec![declaration]);
    assert!(
        build.diagnostics().next().is_none(),
        "{:?}",
        build.diagnostics().collect::<Vec<_>>()
    );
    build
}

fn snapshot(source: &str, registries: &RegistryBuild) -> EntitySnapshot {
    EntitySnapshot::of(&graph_of(source), registries, Path::new(""))
}

/// The ids W004 reports on `source` (rules on `behavior` and `type`).
fn w004_ids(source: &str, abstract_behaviors: bool) -> Vec<String> {
    // A rule for a kind nobody declares is inert (ADR 0020 D5): declare
    // the two the rules target.
    let registries = declared(
        &[("behavior", true, true), ("type", true, true)],
        abstract_behaviors,
        vec![w004("behavior"), w004("type")],
    );
    let snapshot = snapshot(source, &registries);
    let mut ids: Vec<String> = registries
        .rules()
        .check(&snapshot.rule_input(), &NoVerdicts)
        .into_iter()
        .map(|d| d.message.split('\'').nth(1).unwrap().to_string())
        .collect();
    ids.sort();
    ids
}

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
