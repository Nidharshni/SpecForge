use super::*;
use specforge_extension_sdk::{PassEdge, PassSeverity};

fn entity(id: &str, kind: &str) -> PassEntity {
    PassEntity {
        id: id.to_string(),
        kind: kind.to_string(),
        fields: std::collections::BTreeMap::new(),
        incoming_edge_count: 0,
        outgoing_edge_count: 0,
        span: None,
        testable: false,
        ..Default::default()
    }
}

fn edge(source: &str, target: &str, label: &str) -> PassEdge {
    PassEdge {
        source: source.to_string(),
        target: target.to_string(),
        label: label.to_string(),
    }
}

fn codes(findings: &[PassDiagnostic]) -> Vec<&str> {
    findings.iter().map(|f| f.code.as_str()).collect()
}

/// The two edges a `refinement` entity's `concrete_entity` /
/// `abstract_entity` fields produce in the real graph.
fn refinement(id: &str, concrete: &str, abstract_id: &str) -> [PassEdge; 2] {
    [
        edge(id, concrete, REFINEMENT_CONCRETE_FIELD),
        edge(id, abstract_id, REFINEMENT_ABSTRACT_FIELD),
    ]
}

// C10-02: E031 — refinement ensures set inclusion.
#[test]
fn e031_subset_refinement_fires_and_superset_passes() {
    let mut abstract_entity = entity("abstract", "behavior");
    abstract_entity.fields.insert(
        "ensures".to_string(),
        "config_created, file_created".to_string(),
    );
    let mut good = entity("good_impl", "behavior");
    good.fields.insert(
        "ensures".to_string(),
        "file_created, config_created, extra_guarantee".to_string(),
    );
    let mut bad = entity("bad_impl", "behavior");
    bad.fields
        .insert("ensures".to_string(), "config_created".to_string());

    let entities = vec![
        abstract_entity,
        good,
        bad,
        entity("r_good", "refinement"),
        entity("r_bad", "refinement"),
    ];
    let edges = [
        refinement("r_good", "good_impl", "abstract"),
        refinement("r_bad", "bad_impl", "abstract"),
    ]
    .concat();
    let findings = pass_layering_verify(&PassInput {
        entities,
        edges,
        ..Default::default()
    });

    assert_eq!(
        codes(&findings),
        vec!["E031"],
        "only the weakening refinement fires: {findings:?}"
    );
    let e031 = &findings[0];
    assert!(
        e031.message.contains("file_created")
            && e031.message.contains("bad_impl")
            && e031.message.contains("r_bad"),
        "message names the refinement, the concrete, and the dropped condition: {}",
        e031.message
    );
    assert!(
        !e031.message.contains("good_impl"),
        "superset refinement must pass: {}",
        e031.message
    );
}

// C10-12: the declared rules are the single source for the formal
// rules. Their one_of/matches constraints must survive the same
// parse-time validation the engine applies — otherwise an edit can
// silently register a rule that can never fire (or none at all).
#[test]
fn declared_validation_rules_are_complete_and_well_formed() {
    let rules = specforge_extension_build().declaration().validation_rules;

    let codes: Vec<&str> = rules.iter().map(|r| r.code.as_str()).collect();
    for expected in [
        "W125", "W123", "W126", "W128", "W131", "W134", "W124", "W127", "W129", "W132", "W135",
        "W136", "W133",
    ] {
        assert!(
            codes.contains(&expected),
            "formal rule {expected} is not declared: {codes:?}"
        );
    }

    // Declarative sanity per rule: field_value_constraint rules carry a
    // non-empty constraint; matches patterns are compilable regexes.
    for rule in &rules {
        let code = &rule.code;
        if rule.check == "field_value_constraint" {
            let constraint = rule
                .constraint
                .as_ref()
                .unwrap_or_else(|| panic!("{code}: field_value_constraint without a constraint"));
            if constraint.kind == "matches" {
                let pattern = constraint.pattern.as_deref().expect("matches pattern");
                regex::Regex::new(pattern)
                    .unwrap_or_else(|e| panic!("{code}: malformed regex '{pattern}': {e}"));
            }
            if constraint.kind == "one_of" {
                assert!(
                    !constraint.values.is_empty(),
                    "{code}: one_of with empty values"
                );
            }
        }
    }
}

#[test]
fn layering_detects_refinement_cycles() {
    let input = PassInput {
        entities: vec![
            entity("a", "behavior"),
            entity("b", "behavior"),
            entity("c", "behavior"),
            entity("r1", "refinement"),
            entity("r2", "refinement"),
            entity("r3", "refinement"),
        ],
        edges: [
            refinement("r1", "a", "b"),
            refinement("r2", "b", "c"),
            refinement("r3", "c", "a"),
        ]
        .concat(),
        ..Default::default()
    };
    let findings = pass_layering_verify(&input);
    assert_eq!(codes(&findings), vec!["E041"]);
    assert!(matches!(findings[0].severity, PassSeverity::Error));
}

#[test]
fn layering_flags_deep_chains() {
    let mut entities: Vec<PassEntity> = (0..=5)
        .map(|l| entity(&format!("l{l}"), "behavior"))
        .collect();
    let mut edges = Vec::new();
    for w in 0..5 {
        entities.push(entity(&format!("r{w}"), "refinement"));
        edges.extend(refinement(
            &format!("r{w}"),
            &format!("l{w}"),
            &format!("l{}", w + 1),
        ));
    }
    let input = PassInput {
        entities,
        edges,
        ..Default::default()
    };
    let findings = pass_layering_verify(&input);
    assert_eq!(
        codes(&findings),
        vec!["W031"],
        "depth-5 chain: {:?}",
        findings
    );
    assert!(matches!(findings[0].severity, PassSeverity::Warning));
}

#[test]
fn layering_accepts_shallow_acyclic_chains() {
    let input = PassInput {
        entities: vec![
            entity("a", "behavior"),
            entity("b", "behavior"),
            entity("r", "refinement"),
        ],
        edges: refinement("r", "a", "b").to_vec(),
        ..Default::default()
    };
    assert!(pass_layering_verify(&input).is_empty());
}

/// Labels no declared field produces (the pre-fix markers) must not be
/// mistaken for refinements, nor may a `refines` edge from a non-behavior.
#[test]
fn layering_ignores_edges_that_are_not_refinement_fields() {
    let input = PassInput {
        entities: vec![
            entity("a", "behavior"),
            entity("b", "behavior"),
            entity("t", "type"),
        ],
        edges: vec![
            edge("a", "b", "RefinesTo"),
            edge("a", "b", "RefinementChainLink"),
            edge("t", "a", REFINES_FIELD),
        ],
        ..Default::default()
    };
    assert!(pass_layering_verify(&input).is_empty());
}

fn behavior(id: &str, ensures: &str, is_abstract: bool) -> PassEntity {
    let mut e = entity(id, "behavior");
    if !ensures.is_empty() {
        e.fields.insert("ensures".to_string(), ensures.to_string());
    }
    if is_abstract {
        e.fields
            .insert(ABSTRACT_FLAG_FIELD.to_string(), "true".to_string());
    }
    e
}

#[test]
fn refines_field_is_checked_like_a_refinement_entity() {
    let input = PassInput {
        entities: vec![
            behavior("spec", "a, b", true),
            behavior("keeps", "a, b, c", false),
            behavior("drops", "a", false),
        ],
        edges: vec![
            edge("keeps", "spec", REFINES_FIELD),
            edge("drops", "spec", REFINES_FIELD),
        ],
        ..Default::default()
    };
    let findings = pass_layering_verify(&input);
    assert_eq!(codes(&findings), vec!["E031"], "{findings:?}");
    assert!(
        findings[0].message.contains("'drops' refines 'spec'")
            && findings[0].message.contains("[b]"),
        "{}",
        findings[0].message
    );
}

#[test]
fn entity_and_field_declaring_one_pair_report_once_via_the_entity() {
    let mut entities = vec![behavior("spec", "a, b", true), behavior("impl", "a", false)];
    entities.push(entity("r", "refinement"));
    let mut edges = refinement("r", "impl", "spec").to_vec();
    edges.push(edge("impl", "spec", REFINES_FIELD));
    let findings = pass_layering_verify(&PassInput {
        entities,
        edges,
        ..Default::default()
    });
    assert_eq!(codes(&findings), vec!["E031"], "{findings:?}");
    assert!(
        findings[0].message.starts_with("refinement 'r'"),
        "{}",
        findings[0].message
    );
}

#[test]
fn refines_on_a_non_abstract_behavior_is_w110_but_entities_are_exempt() {
    let field = PassInput {
        entities: vec![behavior("base", "", false), behavior("derived", "", false)],
        edges: vec![edge("derived", "base", REFINES_FIELD)],
        ..Default::default()
    };
    let findings = pass_layering_verify(&field);
    assert_eq!(codes(&findings), vec!["W110"], "{findings:?}");
    assert!(matches!(findings[0].severity, PassSeverity::Warning));

    let entity_based = PassInput {
        entities: vec![
            behavior("base", "", false),
            behavior("derived", "", false),
            entity("r", "refinement"),
        ],
        edges: refinement("r", "derived", "base").to_vec(),
        ..Default::default()
    };
    assert!(pass_layering_verify(&entity_based).is_empty());
}

#[test]
fn abstract_behavior_without_a_refinement_is_w030() {
    let lonely = PassInput {
        entities: vec![behavior("spec", "a", true)],
        edges: vec![],
        ..Default::default()
    };
    let findings = pass_layering_verify(&lonely);
    assert_eq!(codes(&findings), vec!["W030"], "{findings:?}");
    assert!(findings[0].message.contains("'spec'"));

    for edges in [
        vec![edge("impl", "spec", REFINES_FIELD)],
        refinement("r", "impl", "spec").to_vec(),
    ] {
        let input = PassInput {
            entities: vec![
                behavior("spec", "a", true),
                behavior("impl", "a", false),
                entity("r", "refinement"),
            ],
            edges,
            ..Default::default()
        };
        assert!(
            pass_layering_verify(&input).is_empty(),
            "refined either way -> complete"
        );
    }
}

#[test]
fn event_graph_flags_only_unconsumed_producers() {
    let input = PassInput {
        entities: vec![
            entity("tick", "event"),
            entity("done", "event"),
            entity("ticker", "behavior"),
            entity("finisher", "behavior"),
        ],
        edges: vec![
            edge("ticker", "tick", "produces"),
            edge("finisher", "done", "produces"),
            edge("handler", "done", "consumes"),
        ],
        ..Default::default()
    };
    let findings: Vec<PassDiagnostic> = pass_event_graph_analyze(&input)
        .into_iter()
        .filter(|f| f.code == "W029")
        .collect();
    assert_eq!(codes(&findings), vec!["W029"]);
    assert!(findings[0].message.contains("tick"));
    assert!(
        !findings[0].message.contains("done"),
        "consumed event spared"
    );
}

/// Pinned by plan 01 (fixed by T8): a property's type is a required enum of
/// its three values, so another value is the core's E061 and no rule of the
/// extension (W125) says so.
#[specforge_test::prelude::specforge_test(
    behavior = "fa_validate_property_type",
    verify = "property_type is a declared enum of safety, liveness, fairness"
)]
#[ignore = "fixed by 01-T8"]
fn a_propertys_type_is_a_declared_vocabulary() {
    let declaration = specforge_extension_build().declaration();
    let field = declaration
        .entities
        .iter()
        .find(|k| k.keyword.as_deref() == Some("property"))
        .and_then(|k| k.fields.iter().find(|f| f.name == "property_type"))
        .expect("property.property_type is declared");
    assert_eq!(field.field_type, "enum");
    assert_eq!(field.enum_values, ["safety", "liveness", "fairness"]);
    assert!(field.required);
    assert!(
        declaration
            .validation_rules
            .iter()
            .all(|r| r.code != "W125"),
        "no rule has code W125"
    );
}
