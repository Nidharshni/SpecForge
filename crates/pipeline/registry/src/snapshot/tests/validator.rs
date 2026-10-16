use std::path::Path;

use specforge_protocol_types::{EntityKindDescriptor, ExtensionDeclaration, FieldDescriptor};

use crate::compilation::build_registries;
use crate::snapshot::EntitySnapshot;

/// A custom validator is handed the ids an entity references, once each, in written order; a
/// dangling one has no kind (E022 is the linker's, so the record keeps no list of its own).
#[test]
fn the_validator_context_names_each_reference_once_dangling_included() {
    let mut declaration = ExtensionDeclaration {
        entities: vec![EntityKindDescriptor {
            name: "node".into(),
            keyword: Some("node".into()),
            fields: vec![FieldDescriptor {
                name: "parts".into(),
                field_type: "reference_list".into(),
                target_kind: Some("node".into()),
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    declaration.handshake.name = "@specforge/testing".into();
    declaration.handshake.version = "1.0.0".into();
    let registries = build_registries(vec![declaration]);
    let source = "node a \"A\" {\n  parts [a2, ghost, a2]\n}\nnode a2 \"A2\" {\n}\n";
    let snapshot = EntitySnapshot::of(
        &super::graph_of(source, &registries),
        &registries,
        Path::new(""),
    );

    let context = snapshot.validator_context("a").expect("a is an entity");
    let referenced: Vec<(&str, Option<&str>)> = context
        .referenced
        .iter()
        .map(|r| (r.id.as_str(), r.kind.as_deref()))
        .collect();
    assert_eq!(referenced, [("a2", Some("node")), ("ghost", None)]);
}
