//! `Declared::beyond` over literal deltas, the ones a graph change of each shape produces (ADR 0198).

use super::*;
use specforge_graph::{EdgeChange, ModifiedNodeChange, NodeChange};
use specforge_test_macros::test as specforge_test;

fn modified(id: &str, fields: &[&str], aspects: &[NodeAspect]) -> ModifiedNodeChange {
    ModifiedNodeChange {
        id: id.to_string(),
        changed_fields: fields.iter().map(ToString::to_string).collect(),
        changed_aspects: aspects.to_vec(),
        file: "f.spec".to_string(),
        line: 1,
    }
}

fn node(id: &str) -> NodeChange {
    NodeChange {
        id: id.to_string(),
        kind: "type".to_string(),
        file: "f.spec".to_string(),
        line: 1,
    }
}

fn edge(source: &str, label: &str, target: &str) -> EdgeChange {
    EdgeChange {
        source: source.to_string(),
        target: target.to_string(),
        label: label.to_string(),
    }
}

fn delta(modified: Vec<ModifiedNodeChange>) -> GraphDelta {
    GraphDelta {
        modified_nodes: modified,
        ..GraphDelta::default()
    }
}

fn remove_verify(id: &str) -> Edit {
    Edit::RemoveVerify {
        entity_id: id.to_string(),
        kind: None,
        text: "t".to_string(),
    }
}

fn set(id: &str, field: &str) -> Edit {
    Edit::Set {
        entity_id: id.to_string(),
        field: field.to_string(),
        value: super::super::Input::One("v".to_string()),
        annotations: None,
    }
}

fn declaring(edits: &[Edit]) -> Declared {
    let mut declared = Declared::default();
    edits.iter().for_each(|e| declared.edit(e));
    declared
}

#[specforge_test(
    behavior = "validate_typed_edit",
    verify = "a graph change no edit declares is beyond the declaration"
)]
fn a_remove_verify_that_changes_a_methods_field_leaves_it_beyond() {
    let declared = declaring(&[remove_verify("auth_port")]);
    let changed = delta(vec![modified("auth_port", &["methods"], &[])]);

    let beyond = declared.beyond(changed);

    assert_eq!(beyond.modified_nodes.len(), 1);
    assert_eq!(beyond.modified_nodes[0].changed_fields, ["methods"]);
}

#[test]
fn a_remove_verify_that_changes_the_obligations_is_declared() {
    let declared = declaring(&[remove_verify("auth_port")]);
    let changed = delta(vec![modified("auth_port", &[], &[NodeAspect::Obligations])]);

    assert!(declared.beyond(changed).is_empty());
}

#[test]
fn a_retitle_that_changes_the_variants_leaves_them_beyond() {
    let declared = declaring(&[Edit::Retitle {
        entity_id: "stage".to_string(),
        title: "Stage".to_string(),
    }]);
    let changed = delta(vec![modified("stage", &["variants"], &[])]);

    let beyond = declared.beyond(changed);

    assert_eq!(beyond.modified_nodes[0].changed_fields, ["variants"]);
}

#[test]
fn a_derived_edge_of_a_set_field_is_declared() {
    let declared = declaring(&[set("Account", "owner")]);
    let mut changed = delta(vec![modified("Account", &["owner"], &[NodeAspect::Edges])]);
    changed.added_edges = vec![edge("Account", "composed_types", "User")];

    assert!(declared.beyond(changed).is_empty());
}

#[test]
fn a_block_field_entry_declares_its_field() {
    let declared = declaring(&[set("auth_port", "methods.logout")]);
    let changed = delta(vec![modified("auth_port", &["methods"], &[])]);

    assert!(declared.beyond(changed).is_empty());
}

#[test]
fn a_create_declares_its_node_and_every_edge_to_or_from_it() {
    let declared = declaring(&[Edit::Create {
        kind: "behavior".to_string(),
        id: "judge".to_string(),
        title: "Judge".to_string(),
        file: "f.spec".to_string(),
        fields: BTreeMap::new(),
        verify: Vec::new(),
    }]);
    let mut changed = delta(vec![modified("other", &[], &[NodeAspect::Edges])]);
    changed.added_nodes = vec![node("judge")];
    changed.added_edges = vec![edge("judge", "ports", "Api"), edge("other", "x", "judge")];

    assert!(declared.beyond(changed).is_empty());
}

#[test]
fn a_delete_declares_its_node_and_the_referrers_fields_it_unlinks() {
    let mut declared = declaring(&[Edit::Delete {
        entity_id: "user_auth".to_string(),
        unlink_references: true,
    }]);
    declared.field("core", "features");
    let mut changed = delta(vec![
        modified("core", &["features"], &[NodeAspect::Edges]),
        modified("mvp", &["features"], &[NodeAspect::Edges]),
    ]);
    changed.removed_nodes = vec![node("user_auth")];
    changed.removed_edges = vec![edge("core", "features", "user_auth")];

    let beyond = declared.beyond(changed);

    assert_eq!(beyond.modified_nodes.len(), 1);
    assert_eq!(beyond.modified_nodes[0].id, "mvp");
    assert!(beyond.removed_nodes.is_empty() && beyond.removed_edges.is_empty());
}

#[test]
fn a_list_is_judged_as_a_whole() {
    let link_unlink = |kind: fn(String, String, String) -> Edit| {
        kind("a".to_string(), "invariants".to_string(), "b".to_string())
    };
    let declared = declaring(&[
        link_unlink(|entity_id, field, target| Edit::Link {
            entity_id,
            field,
            target,
        }),
        link_unlink(|entity_id, field, target| Edit::Unlink {
            entity_id,
            field,
            target,
        }),
    ]);
    assert!(declared.beyond(GraphDelta::default()).is_empty());

    let declared = declaring(&[set("a", "x"), set("b", "y")]);
    let beyond = declared.beyond(delta(vec![modified("c", &["z"], &[])]));
    assert_eq!(beyond.modified_nodes[0].id, "c");
}

#[test]
fn an_edge_between_two_undeclared_entities_is_beyond() {
    let declared = declaring(&[Edit::Retitle {
        entity_id: "a".to_string(),
        title: "A".to_string(),
    }]);
    let changed = GraphDelta {
        added_edges: vec![edge("b", "x", "c")],
        ..GraphDelta::default()
    };

    assert_eq!(declared.beyond(changed).added_edges.len(), 1);
}
