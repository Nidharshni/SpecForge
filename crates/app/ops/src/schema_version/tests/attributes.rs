use super::*;

fn edge_type(schema: &mut GraphProtocolSchema) -> &mut SchemaEdgeType {
    &mut schema.edge_types[0]
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| v.to_string()).collect()
}

fn changed(attribute: Scalar, before: Option<&str>, after: Option<&str>) -> AttributeChange {
    AttributeChange::Changed {
        attribute,
        before: before.map(str::to_string),
        after: after.map(str::to_string),
    }
}

type Mutate = fn(&mut GraphProtocolSchema);

struct Row {
    name: &'static str,
    mutate: Mutate,
    change: SchemaChange,
    bump: Bump,
    text: &'static str,
}

fn kind_change(change: AttributeChange) -> SchemaChange {
    SchemaChange::Kind {
        kind: "behavior".to_string(),
        change,
    }
}

fn field_change(name: &str, change: AttributeChange) -> SchemaChange {
    SchemaChange::Field {
        kind: "behavior".to_string(),
        field: name.to_string(),
        change,
    }
}

fn edge_change(change: AttributeChange) -> SchemaChange {
    SchemaChange::EdgeType {
        label: "E".to_string(),
        change,
    }
}

fn rows() -> Vec<Row> {
    vec![
        Row {
            name: "an extension is added",
            mutate: |s| {
                s.extensions.push(SchemaExtensionInfo {
                    name: "@x/new".to_string(),
                    version: "1.0.0".to_string(),
                })
            },
            change: SchemaChange::ExtensionAdded("@x/new".to_string()),
            bump: Bump::Patch,
            text: "extension `@x/new` was added",
        },
        Row {
            name: "an extension is removed",
            mutate: |s| {
                s.extensions.pop();
            },
            change: SchemaChange::ExtensionRemoved("@specforge/product".to_string()),
            bump: Bump::Patch,
            text: "extension `@specforge/product` was removed",
        },
        Row {
            name: "an extension's version changes",
            mutate: |s| s.extensions[0].version = "1.1.0".to_string(),
            change: SchemaChange::ExtensionVersion {
                name: "@specforge/software".to_string(),
                before: "1.0.0".to_string(),
                after: "1.1.0".to_string(),
            },
            bump: Bump::Patch,
            text: "extension `@specforge/software` went from version `1.0.0` to `1.1.0`",
        },
        Row {
            name: "the extensions load in another order",
            mutate: |s| s.extensions.reverse(),
            change: SchemaChange::LoadOrder,
            bump: Bump::Patch,
            text: "the load order of the extensions changed",
        },
        Row {
            name: "a kind is added",
            mutate: |s| {
                s.entity_kinds.push(SchemaEntityKind {
                    name: "event".to_string(),
                    source_extension: "@specforge/software".to_string(),
                    testable: true,
                    dot_color: None,
                    fields: vec![],
                })
            },
            change: SchemaChange::KindAdded("event".to_string()),
            bump: Bump::Minor,
            text: "entity kind `event` was added",
        },
        Row {
            name: "a kind is removed",
            mutate: |s| s.entity_kinds.retain(|k| k.name != "feature"),
            change: SchemaChange::KindRemoved("feature".to_string()),
            bump: Bump::Major,
            text: "entity kind `feature` was removed",
        },
        Row {
            name: "a kind's source extension changes",
            mutate: |s| behavior(s).source_extension = "@specforge/other".to_string(),
            change: kind_change(changed(
                Scalar::SourceExtension,
                Some("@specforge/software"),
                Some("@specforge/other"),
            )),
            bump: Bump::Patch,
            text: "the source extension of entity kind `behavior` changed from `@specforge/software` to `@specforge/other`",
        },
        Row {
            name: "a kind's testable flag changes",
            mutate: |s| behavior(s).testable = false,
            change: kind_change(changed(Scalar::Testable, Some("true"), Some("false"))),
            bump: Bump::Patch,
            text: "the testable flag of entity kind `behavior` changed from true to false",
        },
        Row {
            name: "a kind's DOT colour is dropped",
            mutate: |s| behavior(s).dot_color = None,
            change: kind_change(changed(Scalar::DotColor, Some("#111"), None)),
            bump: Bump::Patch,
            text: "the DOT color of entity kind `behavior` changed from `#111` to none",
        },
        Row {
            name: "an optional field is added",
            mutate: |s| behavior(s).fields.push(optional_field("notes")),
            change: SchemaChange::FieldAdded {
                kind: "behavior".to_string(),
                field: "notes".to_string(),
                required: false,
            },
            bump: Bump::Minor,
            text: "optional field `notes` was added to `behavior`",
        },
        Row {
            name: "a required field is added",
            mutate: |s| {
                behavior(s).fields.push(SchemaField {
                    required: true,
                    ..optional_field("severity")
                })
            },
            change: SchemaChange::FieldAdded {
                kind: "behavior".to_string(),
                field: "severity".to_string(),
                required: true,
            },
            bump: Bump::Major,
            text: "required field `severity` was added to `behavior`",
        },
        Row {
            name: "a field is removed",
            mutate: |s| behavior(s).fields.retain(|f| f.name != "owner"),
            change: SchemaChange::FieldRemoved {
                kind: "behavior".to_string(),
                field: "owner".to_string(),
            },
            bump: Bump::Major,
            text: "field `owner` was removed from `behavior`",
        },
        Row {
            name: "a field's type changes",
            mutate: |s| field(s, "contract").field_type = FieldType::Bool,
            change: field_change(
                "contract",
                changed(Scalar::FieldType, Some("string"), Some("bool")),
            ),
            bump: Bump::Major,
            text: "the type of field `contract` of `behavior` changed from `string` to `bool`",
        },
        Row {
            name: "a field stops being required",
            mutate: |s| field(s, "contract").required = false,
            change: field_change(
                "contract",
                changed(Scalar::Required, Some("true"), Some("false")),
            ),
            bump: Bump::Major,
            text: "the required flag of field `contract` of `behavior` changed from true to false",
        },
        Row {
            name: "an enum field gains a value",
            mutate: |s| {
                field(s, "status")
                    .enum_values
                    .as_mut()
                    .unwrap()
                    .push("c".to_string())
            },
            change: field_change(
                "status",
                AttributeChange::Gained {
                    attribute: Members::EnumValues,
                    members: strings(&["c"]),
                },
            ),
            bump: Bump::Minor,
            text: "the enum values of field `status` of `behavior` gained `c`",
        },
        Row {
            name: "an enum field loses a value",
            mutate: |s| {
                field(s, "status").enum_values.as_mut().unwrap().pop();
            },
            change: field_change(
                "status",
                AttributeChange::Lost {
                    attribute: Members::EnumValues,
                    members: strings(&["b"]),
                },
            ),
            bump: Bump::Major,
            text: "the enum values of field `status` of `behavior` lost `b`",
        },
        Row {
            name: "an enum field's values are reordered",
            mutate: |s| field(s, "status").enum_values.as_mut().unwrap().reverse(),
            change: field_change(
                "status",
                AttributeChange::Reordered {
                    attribute: Members::EnumValues,
                },
            ),
            bump: Bump::Patch,
            text: "the enum values of field `status` of `behavior` were reordered",
        },
        Row {
            name: "a field stops writing its edge type",
            mutate: |s| field(s, "owner").edge = None,
            change: field_change("owner", changed(Scalar::Edge, Some("OwnedBy"), None)),
            bump: Bump::Major,
            text: "the edge type of field `owner` of `behavior` changed from `OwnedBy` to none",
        },
        Row {
            name: "a field's target kind changes",
            mutate: |s| field(s, "owner").target_kind = Some("team".to_string()),
            change: field_change(
                "owner",
                changed(Scalar::TargetKind, Some("person"), Some("team")),
            ),
            bump: Bump::Major,
            text: "the target kind of field `owner` of `behavior` changed from `person` to `team`",
        },
        Row {
            name: "a field's description changes",
            mutate: |s| field(s, "status").description = Some("e".to_string()),
            change: field_change("status", changed(Scalar::Description, Some("d"), Some("e"))),
            bump: Bump::Patch,
            text: "the description of field `status` of `behavior` changed",
        },
        Row {
            name: "a field's default value is dropped",
            mutate: |s| field(s, "status").default_value = None,
            change: field_change("status", changed(Scalar::DefaultValue, Some("a"), None)),
            bump: Bump::Patch,
            text: "the default value of field `status` of `behavior` changed from `a` to none",
        },
        Row {
            name: "a field's source extension changes",
            mutate: |s| field(s, "contract").source_extension = "@specforge/other".to_string(),
            change: field_change(
                "contract",
                changed(
                    Scalar::SourceExtension,
                    Some("@specforge/software"),
                    Some("@specforge/other"),
                ),
            ),
            bump: Bump::Patch,
            text: "the source extension of field `contract` of `behavior` changed from `@specforge/software` to `@specforge/other`",
        },
        Row {
            name: "an edge type is added",
            mutate: |s| {
                s.edge_types.push(SchemaEdgeType {
                    label: "F".to_string(),
                    source_extension: "@specforge/software".to_string(),
                    source_kinds: None,
                    target_kinds: None,
                })
            },
            change: SchemaChange::EdgeTypeAdded("F".to_string()),
            bump: Bump::Minor,
            text: "edge type `F` was added",
        },
        Row {
            name: "an edge type is removed",
            mutate: |s| s.edge_types.clear(),
            change: SchemaChange::EdgeTypeRemoved("E".to_string()),
            bump: Bump::Major,
            text: "edge type `E` was removed",
        },
        Row {
            name: "an edge type gains a target kind",
            mutate: |s| {
                edge_type(s)
                    .target_kinds
                    .as_mut()
                    .unwrap()
                    .push("port".to_string())
            },
            change: edge_change(AttributeChange::Gained {
                attribute: Members::TargetKinds,
                members: strings(&["port"]),
            }),
            bump: Bump::Minor,
            text: "the target kinds of edge type `E` gained `port`",
        },
        Row {
            name: "an edge type loses a target kind",
            mutate: |s| {
                edge_type(s)
                    .target_kinds
                    .as_mut()
                    .unwrap()
                    .retain(|k| k != "behavior")
            },
            change: edge_change(AttributeChange::Lost {
                attribute: Members::TargetKinds,
                members: strings(&["behavior"]),
            }),
            bump: Bump::Major,
            text: "the target kinds of edge type `E` lost `behavior`",
        },
        Row {
            name: "an edge type loses a source kind",
            mutate: |s| {
                edge_type(s)
                    .source_kinds
                    .as_mut()
                    .unwrap()
                    .retain(|k| k != "feature")
            },
            change: edge_change(AttributeChange::Lost {
                attribute: Members::SourceKinds,
                members: strings(&["feature"]),
            }),
            bump: Bump::Major,
            text: "the source kinds of edge type `E` lost `feature`",
        },
        Row {
            name: "an edge type's target kinds are reordered",
            mutate: |s| edge_type(s).target_kinds.as_mut().unwrap().reverse(),
            change: edge_change(AttributeChange::Reordered {
                attribute: Members::TargetKinds,
            }),
            bump: Bump::Patch,
            text: "the target kinds of edge type `E` were reordered",
        },
        Row {
            name: "an edge type's source extension changes",
            mutate: |s| edge_type(s).source_extension = "@specforge/other".to_string(),
            change: edge_change(changed(
                Scalar::SourceExtension,
                Some("@specforge/software"),
                Some("@specforge/other"),
            )),
            bump: Bump::Patch,
            text: "the source extension of edge type `E` changed from `@specforge/software` to `@specforge/other`",
        },
    ]
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "every attribute of the schema is compared: a change to any one moves the version"
)]
fn every_attribute_change_moves_the_version() {
    for row in rows() {
        let recorded = rich_schema();
        let mut generated = rich_schema();
        (row.mutate)(&mut generated);

        let dir = tempfile::tempdir().unwrap();
        let cache = SchemaCache::in_dir(dir.path());
        cache.seed(&recorded);
        let versioned = cache.version(generated);

        assert_eq!(
            versioned.changes(),
            std::slice::from_ref(&row.change),
            "{}",
            row.name
        );
        assert_eq!(row.change.bump(), row.bump, "{}", row.name);
        assert_eq!(row.change.to_string(), row.text, "{}", row.name);
        let expected = match row.bump {
            Bump::Major => SchemaVersion::new(2, 0, 0),
            Bump::Minor => SchemaVersion::new(1, 3, 0),
            Bump::Patch => SchemaVersion::new(1, 2, 4),
        };
        assert_eq!(versioned.version(), &expected, "{}", row.name);
        assert_eq!(
            versioned.notices().len(),
            usize::from(row.bump == Bump::Major),
            "{}: a W053 exactly for a major change",
            row.name
        );
    }
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "identical schemas no changes"
)]
fn a_record_in_another_order_is_no_change() {
    let mut generated = rich_schema();
    generated.edge_types.push(SchemaEdgeType {
        label: "A".to_string(),
        source_extension: "@specforge/software".to_string(),
        source_kinds: None,
        target_kinds: None,
    });
    // The record holds the same kinds, fields and edge types in the reverse
    // order.
    let mut recorded = generated.clone();
    recorded.entity_kinds.reverse();
    for kind in &mut recorded.entity_kinds {
        kind.fields.reverse();
    }
    recorded.edge_types.reverse();
    let dir = tempfile::tempdir().unwrap();
    let cache = SchemaCache::in_dir(dir.path());
    cache.seed(&recorded);

    let versioned = cache.version(generated);

    assert!(versioned.changes().is_empty(), "{:?}", versioned.changes());
    assert_eq!(versioned.version(), &SchemaVersion::new(1, 2, 3));
}

#[test]
fn the_changes_come_in_schema_order() {
    let recorded = rich_schema();
    let mut generated = rich_schema();
    // Written against the order: an edge type, a field, a kind, an extension.
    generated.edge_types.clear();
    field(&mut generated, "owner").target_kind = Some("team".to_string());
    behavior(&mut generated).testable = false;
    generated.entity_kinds.retain(|k| k.name != "feature");
    generated.extensions[0].version = "2.0.0".to_string();

    let changes = diff(&recorded, &generated);

    assert_eq!(
        changes
            .iter()
            .map(|change| change.to_string())
            .collect::<Vec<_>>(),
        [
            "extension `@specforge/software` went from version `1.0.0` to `2.0.0`",
            "the testable flag of entity kind `behavior` changed from true to false",
            "the target kind of field `owner` of `behavior` changed from `person` to `team`",
            "entity kind `feature` was removed",
            "edge type `E` was removed",
        ]
    );
}

#[test]
fn a_type_change_does_not_also_report_the_enum_values() {
    let recorded = rich_schema();
    let mut generated = rich_schema();
    let status = field(&mut generated, "status");
    status.field_type = FieldType::String;
    status.enum_values = None;

    let changes = diff(&recorded, &generated);

    assert_eq!(
        changes,
        [field_change(
            "status",
            changed(Scalar::FieldType, Some("enum"), Some("string"))
        )]
    );
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "an enum field losing a value is breaking; gaining one is an addition"
)]
fn an_enum_field_losing_a_value_is_breaking_and_gaining_one_is_an_addition() {
    let recorded = rich_schema();

    let mut lost = rich_schema();
    field(&mut lost, "status").enum_values = Some(strings(&["a"]));
    let lost = diff(&recorded, &lost);
    assert!(breaking(&lost), "{lost:?}");

    let mut gained = rich_schema();
    field(&mut gained, "status").enum_values = Some(strings(&["a", "b", "c"]));
    let gained = diff(&recorded, &gained);
    assert!(!breaking(&gained) && additions(&gained), "{gained:?}");

    // One value swapped for another loses one and gains one: breaking.
    let mut swapped = rich_schema();
    field(&mut swapped, "status").enum_values = Some(strings(&["a", "c"]));
    let swapped = diff(&recorded, &swapped);
    assert_eq!(swapped.len(), 2, "{swapped:?}");
    assert!(breaking(&swapped));
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "a field's edge type or target kind changing is breaking"
)]
fn a_fields_edge_type_or_target_kind_changing_is_breaking() {
    let recorded = rich_schema();

    let mut edge = rich_schema();
    field(&mut edge, "owner").edge = Some("Other".to_string());
    assert!(breaking(&diff(&recorded, &edge)));

    let mut target = rich_schema();
    field(&mut target, "owner").target_kind = None;
    assert!(breaking(&diff(&recorded, &target)));

    // Setting one on a field that had none is a change too.
    let mut set = rich_schema();
    field(&mut set, "contract").edge = Some("Writes".to_string());
    assert!(breaking(&diff(&recorded, &set)));
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "a field's required flag changing either way is breaking"
)]
fn a_required_flag_changing_either_way_is_breaking() {
    let recorded = rich_schema();

    let mut optional = rich_schema();
    field(&mut optional, "contract").required = false;
    assert!(breaking(&diff(&recorded, &optional)));

    let mut required = rich_schema();
    field(&mut required, "owner").required = true;
    assert!(breaking(&diff(&recorded, &required)));
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "an edge type losing a source or target kind is breaking; gaining one is an addition"
)]
fn an_edge_type_losing_an_end_is_breaking_and_gaining_one_is_an_addition() {
    let recorded = rich_schema();

    let mut lost = rich_schema();
    edge_type(&mut lost).target_kinds = Some(strings(&["feature"]));
    assert!(breaking(&diff(&recorded, &lost)));

    let mut gained = rich_schema();
    edge_type(&mut gained).source_kinds = None;
    let none = diff(&recorded, &gained);
    assert!(breaking(&none), "no ends is every end lost: {none:?}");

    let mut open = rich_schema();
    open.edge_types[0].source_kinds = None;
    let mut closed = rich_schema();
    closed.edge_types[0].source_kinds = Some(strings(&["behavior"]));
    // From no ends to some is a gain.
    let changes = diff(&open, &closed);
    assert!(!breaking(&changes) && additions(&changes), "{changes:?}");
}
