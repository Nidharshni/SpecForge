use super::*;
use crate::layering::{REFINEMENT_ABSTRACT_FIELD, REFINEMENT_CONCRETE_FIELD};
use specforge_extension_sdk::PassEdge;

fn codes(findings: &[PassDiagnostic]) -> Vec<&str> {
    findings.iter().map(|f| f.code.as_str()).collect()
}

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

#[test]
fn process_composition_cycle_is_e042() {
    let input = PassInput {
        entities: vec![
            entity("p1", "process"),
            entity("p2", "process"),
            entity("p3", "process"),
        ],
        edges: vec![
            edge("p1", "p2", "ProcessComposesProcess"),
            edge("p2", "p3", "ProcessComposesProcess"),
            edge("p3", "p1", "ProcessComposesProcess"),
        ],
        ..Default::default()
    };
    let findings = pass_event_graph_analyze(&input);
    assert_eq!(codes(&findings), vec!["E042"]);
    assert!(findings[0].message.contains("p1 -> p2 -> p3"));
}

#[test]
fn acyclic_process_composition_is_clean() {
    let input = PassInput {
        entities: vec![entity("p1", "process"), entity("p2", "process")],
        edges: vec![edge("p1", "p2", "ProcessComposesProcess")],
        ..Default::default()
    };
    let findings = pass_event_graph_analyze(&input);
    assert!(findings.is_empty(), "acyclic composition: {findings:?}");
}

#[test]
fn participation_counts_as_usage_for_w029() {
    // event produced by a behavior AND participating in a process: the
    // participation is usage, so W029 must not fire.
    let input = PassInput {
        entities: vec![
            entity("evt", "event"),
            entity("proc", "process"),
            entity("b", "behavior"),
        ],
        edges: vec![
            edge("b", "evt", "produces"),
            edge("evt", "proc", "EventParticipatesInProcess"),
        ],
        ..Default::default()
    };
    let findings = pass_event_graph_analyze(&input);
    assert!(
        !findings.iter().any(|f| f.code == "W029"),
        "participation counts as usage: {findings:?}"
    );
}

#[test]
fn field_labeled_composition_edges_are_interpreted() {
    // The host labels process edges with the field name (sub_processes).
    let input = PassInput {
        entities: vec![entity("monitor", "process"), entity("scheduler", "process")],
        edges: vec![
            edge("monitor", "scheduler", "sub_processes"),
            edge("scheduler", "monitor", "sub_processes"),
        ],
        ..Default::default()
    };
    let findings = pass_event_graph_analyze(&input);
    assert_eq!(codes(&findings), vec!["E042"]);
}

#[test]
fn self_cycle_in_process_composition_is_e042() {
    let input = PassInput {
        entities: vec![entity("p", "process")],
        edges: vec![edge("p", "p", "ProcessComposesProcess")],
        ..Default::default()
    };
    let findings = pass_event_graph_analyze(&input);
    assert_eq!(codes(&findings), vec!["E042"]);
}

#[test]
fn layering_parallel_edges_report_once() {
    // two refinements naming the same b -> a pair: E041 must not dup.
    let mut entities = vec![entity("a", "behavior"), entity("b", "behavior")];
    let mut edges = Vec::new();
    for (r, concrete, abstract_id) in [("r1", "a", "b"), ("r2", "b", "a"), ("r3", "b", "a")] {
        entities.push(entity(r, "refinement"));
        edges.push(edge(r, concrete, REFINEMENT_CONCRETE_FIELD));
        edges.push(edge(r, abstract_id, REFINEMENT_ABSTRACT_FIELD));
    }
    let input = PassInput {
        entities,
        edges,
        ..Default::default()
    };
    let findings = pass_layering_verify(&input);
    let e041 = findings.iter().filter(|f| f.code == "E041").count();
    assert_eq!(e041, 1, "parallel edges yield one cycle diagnostic");
}

#[test]
fn cycle_e042_names_every_process_of_a_tangle_of_cycles() {
    // delta -> beta closes a cycle through beta, which the walk had finished.
    let input = PassInput {
        entities: ["alpha", "beta", "gamma", "delta"]
            .map(|id| entity(id, "process"))
            .into(),
        edges: [
            ("alpha", "beta"),
            ("alpha", "delta"),
            ("beta", "gamma"),
            ("gamma", "alpha"),
            ("delta", "beta"),
        ]
        .map(|(s, t)| edge(s, t, "sub_processes"))
        .into(),
        ..Default::default()
    };

    let findings = pass_event_graph_analyze(&input);

    let e042: Vec<&str> = findings
        .iter()
        .filter(|f| f.code == "E042")
        .map(|f| f.message.as_str())
        .collect();
    assert_eq!(
        e042,
        ["process composition cycle: alpha -> beta -> gamma -> alpha -> delta -> beta -> gamma -> alpha"]
    );
}

#[test]
fn cycle_e041_names_the_same_behavior_on_every_run() {
    let input = PassInput {
        entities: ["alpha", "beta"].map(|id| entity(id, "behavior")).into(),
        edges: vec![
            edge("alpha", "beta", crate::layering::REFINES_FIELD),
            edge("beta", "alpha", crate::layering::REFINES_FIELD),
        ],
        ..Default::default()
    };

    // `refines` is a fresh HashMap with new random keys on each call.
    for _ in 0..32 {
        let findings = pass_layering_verify(&input);
        let e041: Vec<&str> = findings
            .iter()
            .filter(|f| f.code == "E041")
            .map(|f| f.message.as_str())
            .collect();
        assert_eq!(e041, ["refinement chain cycle through 'alpha'"]);
    }
}
