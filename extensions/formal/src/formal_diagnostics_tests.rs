use super::*;
use specforge_extension_sdk::PassEdge;

fn entity(id: &str, kind: &str, fields: &[(&str, &str)]) -> PassEntity {
    PassEntity {
        id: id.to_string(),
        kind: kind.to_string(),
        fields: fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
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

fn of_code<'a>(findings: &'a [PassDiagnostic], code: &str) -> Vec<&'a str> {
    findings
        .iter()
        .filter(|f| f.code == code)
        .map(|f| f.message.as_str())
        .collect()
}

/// a produces e1, b consumes e1 and produces e2, a consumes e2.
fn two_behavior_cycle(sync_on: Option<&str>) -> PassInput {
    let field = |id: &str| -> Vec<(&str, &str)> {
        if sync_on == Some(id) {
            vec![("sync", "timeout 5s")]
        } else {
            vec![]
        }
    };
    PassInput {
        entities: vec![
            entity("a", "behavior", &field("a")),
            entity("b", "behavior", &field("b")),
            entity("e1", "event", &field("e1")),
            entity("e2", "event", &field("e2")),
        ],
        edges: vec![
            edge("a", "e1", "produces"),
            edge("b", "e1", "consumes"),
            edge("b", "e2", "produces"),
            edge("a", "e2", "consumes"),
        ],
        ..Default::default()
    }
}

#[test]
fn an_unmitigated_cycle_is_e034_with_its_path() {
    let findings = pass_event_graph_analyze(&two_behavior_cycle(None));
    assert_eq!(
            of_code(&findings, "E034"),
            ["unmitigated event cycle: a -> e1 -> b -> e2 -> a (no behavior or event in it declares sync)"]
        );
    assert!(of_code(&findings, "I009").is_empty(), "{findings:?}");
    let e034 = findings.iter().find(|f| f.code == "E034").unwrap();
    assert!(matches!(e034.severity, PassSeverity::Error));
    assert!(e034
        .suggestion
        .as_deref()
        .unwrap()
        .contains("declare `sync`"));
}

#[test]
fn sync_on_any_member_mitigates_the_cycle() {
    for member in ["a", "e2"] {
        let findings = pass_event_graph_analyze(&two_behavior_cycle(Some(member)));
        assert!(
            of_code(&findings, "E034").is_empty(),
            "{member}: {findings:?}"
        );
        assert_eq!(
            of_code(&findings, "I009").len(),
            1,
            "{member}: {findings:?}"
        );
    }
}

#[test]
fn a_chain_is_no_cycle_and_earns_i009() {
    let input = PassInput {
        entities: vec![
            entity("a", "behavior", &[]),
            entity("b", "behavior", &[]),
            entity("e", "event", &[("sync", "timeout 1s")]),
        ],
        edges: vec![edge("a", "e", "produces"), edge("b", "e", "consumes")],
        ..Default::default()
    };
    let findings = pass_event_graph_analyze(&input);
    assert_eq!(
        codes(&findings),
        ["I009"],
        "a chain with a bounded event: {findings:?}"
    );
}

fn codes(findings: &[PassDiagnostic]) -> Vec<&str> {
    findings.iter().map(|f| f.code.as_str()).collect()
}

#[test]
fn a_behavior_re_producing_what_it_consumes_is_w032_unless_synced() {
    let retry = |sync: &[(&str, &str)]| PassInput {
        entities: vec![entity("r", "behavior", sync), entity("job", "event", &[])],
        edges: vec![edge("r", "job", "consumes"), edge("r", "job", "produces")],
        ..Default::default()
    };
    let findings = pass_event_graph_analyze(&retry(&[]));
    assert_eq!(
            of_code(&findings, "W032"),
            ["behavior 'r' consumes event 'job' and produces it again, with no sync on either (an unmitigated retry cycle)"]
        );
    assert!(
        of_code(&findings, "E034").is_empty(),
        "one behavior is a retry, not E034"
    );
    assert!(of_code(&findings, "I009").is_empty());
    let synced = pass_event_graph_analyze(&retry(&[("sync", "backoff 2s")]));
    assert!(of_code(&synced, "W032").is_empty(), "{synced:?}");
}

#[test]
fn a_produced_event_without_sync_is_w034() {
    let input = PassInput {
        entities: vec![
            entity("p", "behavior", &[]),
            entity("open", "event", &[]),
            entity("bounded", "event", &[("sync", "timeout 1s")]),
            entity("idle", "event", &[]),
        ],
        edges: vec![
            edge("p", "open", "produces"),
            edge("p", "bounded", "produces"),
        ],
        ..Default::default()
    };
    let findings = pass_event_graph_analyze(&input);
    assert_eq!(
        of_code(&findings, "W034"),
        ["event 'open' is produced but declares no sync, so nothing bounds its channel"]
    );
}

fn port_input(incoming: &[(&str, usize)]) -> PassInput {
    let mut entities = vec![entity("store", "port", &[])];
    let mut edges = Vec::new();
    for (id, count) in incoming {
        let mut b = entity(id, "behavior", &[]);
        b.incoming_edge_count = *count;
        entities.push(b);
        edges.push(edge(id, "store", "ports"));
    }
    PassInput {
        entities,
        edges,
        ..Default::default()
    }
}

#[test]
fn a_port_with_unbalanced_users_is_w033() {
    let findings = pass_event_graph_analyze(&port_input(&[("hot", 7), ("cold", 2)]));
    assert_eq!(
            of_code(&findings, "W033"),
            ["port 'store' is used by 2 behaviors with unbalanced connectivity: 'hot' has 7 incoming edge(s), 'cold' 2 (more than 3:1)"]
        );
    let w033 = findings.iter().find(|f| f.code == "W033").unwrap();
    assert!(w033.suggestion.as_deref().unwrap().contains("review"));
}

#[test]
fn a_balanced_or_single_use_port_passes() {
    for incoming in [
        &[("a", 3), ("b", 1)][..],
        &[("a", 3), ("b", 0)][..],
        &[("only", 9)][..],
    ] {
        let findings = pass_event_graph_analyze(&port_input(incoming));
        assert!(
            of_code(&findings, "W033").is_empty(),
            "{incoming:?}: {findings:?}"
        );
    }
}

#[test]
fn ensures_without_requires_is_i011_and_prose_invariants_are_w040() {
    let input = PassInput {
        entities: vec![
            entity("guaranteeing", "behavior", &[("ensures", "done")]),
            entity(
                "contracted",
                "behavior",
                &[("requires", "ready"), ("ensures", "done")],
            ),
            entity("prose", "invariant", &[("guarantee", "ids are unique")]),
            entity(
                "formal",
                "invariant",
                &[
                    ("guarantee", "ids are unique"),
                    ("expression", "count(ids) == count(distinct(ids))"),
                ],
            ),
        ],
        ..Default::default()
    };
    let findings = pass_condition_check(&input);
    assert_eq!(
        of_code(&findings, "I011"),
        ["behavior 'guaranteeing' declares ensures but no requires"]
    );
    assert_eq!(
        of_code(&findings, "W040"),
        ["invariant 'prose' states its guarantee in prose only (no expression)"]
    );
    assert!(of_code(&findings, "W096").is_empty());
}

#[test]
fn a_requires_naming_a_condition_twice_is_w039() {
    let input = PassInput {
        entities: vec![
            entity(
                "twice",
                "behavior",
                &[("requires", "ready, open, ready"), ("ensures", "done")],
            ),
            entity(
                "once",
                "behavior",
                &[("requires", "ready, open"), ("ensures", "done")],
            ),
        ],
        ..Default::default()
    };
    let findings = pass_condition_check(&input);
    assert_eq!(
        of_code(&findings, "W039"),
        ["behavior 'twice' requires 'ready' more than once (the repeat is redundant)"]
    );
}

fn depth_input(proven: bool) -> PassInput {
    let mut connected = entity("connected", "behavior", &[]);
    connected.outgoing_edge_count = 1;
    let mut entities = vec![
        entity("bare", "behavior", &[]),
        connected,
        entity("conditioned", "behavior", &[("ensures", "done")]),
        entity(
            "kept",
            "behavior",
            &[("ensures", "done"), ("maintains", "unique_ids")],
        ),
    ];
    entities[3].testable = true;
    entities[3].verify_kinds = vec!["unit".into()];
    entities[3].verify_texts = vec!["works".into()];
    let results = if proven {
        serde_json::json!({"results": {"kept": {"tests": [{"name": "t", "status": "pass", "verify": "works"}]}}})
    } else {
        serde_json::json!({"results": {}})
    };
    PassInput {
        entities,
        test_results: Some(serde_json::from_value(results).unwrap()),
        ..Default::default()
    }
}

#[test]
fn specification_depth_is_reported_from_level_two() {
    let findings = pass_coverage_tracking(&depth_input(false));
    assert_eq!(
        of_code(&findings, "I014"),
        [
            "behavior 'conditioned' is at specification depth 'conditions' (level 2 of 4)",
            "behavior 'kept' is at specification depth 'invariants' (level 3 of 4)",
        ]
    );
    let proven = pass_coverage_tracking(&depth_input(true));
    assert!(
        of_code(&proven, "I014")
            .contains(&"behavior 'kept' is at specification depth 'proofs' (level 4 of 4)"),
        "{proven:?}"
    );
}

#[test]
fn many_shallow_behaviors_earn_the_adoption_note() {
    let mut input = depth_input(false);
    for i in 0..5 {
        input
            .entities
            .push(entity(&format!("prose{i}"), "behavior", &[]));
    }
    let findings = pass_coverage_tracking(&input);
    let notes: Vec<&PassDiagnostic> = findings
        .iter()
        .filter(|f| f.code == "I014" && f.entity.is_none())
        .collect();
    assert_eq!(notes.len(), 1, "{findings:?}");
    assert_eq!(
            notes[0].message,
            "7 behavior(s) are at specification depth 'prose' or 'entity_graph' (no requires or ensures)"
        );
    assert!(notes[0]
        .suggestion
        .as_deref()
        .unwrap()
        .contains("requires/ensures"));
    let few = pass_coverage_tracking(&depth_input(false));
    assert!(few
        .iter()
        .all(|f| !(f.code == "I014" && f.entity.is_none())));
}

#[test]
fn conditioned_behaviors_earn_one_i015() {
    let input = PassInput {
        entities: vec![
            entity("a", "behavior", &[("requires", "ready")]),
            entity("b", "behavior", &[("ensures", "done")]),
            entity("c", "behavior", &[]),
        ],
        ..Default::default()
    };
    let findings = pass_analysis_available(&input);
    assert_eq!(
        of_code(&findings, "I015"),
        ["2 behavior(s) declare requires/ensures; `specforge analyze` checks them"]
    );
    let none = PassInput {
        entities: vec![entity("c", "behavior", &[])],
        ..Default::default()
    };
    assert!(pass_analysis_available(&none).is_empty());
}

#[test]
fn a_refinement_without_invariant_deltas_fails_w133() {
    use specforge_extension_sdk::{
        ValidatorContext, ValidatorEntity, ValidatorField, ValidatorVerdict,
    };
    let context = |fields: Vec<(&str, &str)>| ValidatorContext {
        entity: ValidatorEntity {
            id: "r".into(),
            kind: "refinement".into(),
            fields: fields
                .into_iter()
                .map(|(k, v)| ValidatorField {
                    key: k.into(),
                    value: serde_json::Value::String(v.into()),
                    annotations: Vec::new(),
                })
                .collect(),
            methods: Vec::new(),
        },
        referenced: Vec::new(),
        declared_types: Vec::new(),
        primitives: Vec::new(),
    };
    let fails = |fields| {
        matches!(
            declaration::refinement_declares_deltas(&context(fields)),
            ValidatorVerdict::Fail { .. }
        )
    };
    assert!(fails(vec![]), "absent");
    assert!(fails(vec![("invariant_deltas", "")]), "written empty");
    assert!(!fails(vec![("invariant_deltas", "adds retry_bound")]));
}
