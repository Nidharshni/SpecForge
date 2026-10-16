use super::*;

// ===========================================================================
// Slice 4: Schema Diffing
// ===========================================================================

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "removed entity kind detected as breaking"
)]
fn diff_removed_kind_is_breaking() {
    let old = sample_schema();
    let mut new = sample_schema();
    new.entity_kinds.retain(|k| k.name != "feature");

    let changes = diff(&old, &new);
    assert!(breaking(&changes));
    assert!(
        changes
            .iter()
            .any(|c| matches!(c, SchemaChange::KindRemoved(name) if name == "feature"))
    );
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "new entity kind detected as non-breaking"
)]
fn diff_added_kind_is_non_breaking() {
    let old = sample_schema();
    let mut new = sample_schema();
    new.entity_kinds.push(SchemaEntityKind {
        name: "event".to_string(),
        source_extension: "@specforge/software".to_string(),
        testable: true,
        dot_color: None,
        fields: vec![],
    });

    let changes = diff(&old, &new);
    assert!(!breaking(&changes));
    assert!(additions(&changes));
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "added optional field detected as non-breaking"
)]
fn diff_added_optional_field_non_breaking() {
    let old = sample_schema();
    let mut new = sample_schema();
    new.entity_kinds[0]
        .fields
        .push(optional_field("description"));

    let changes = diff(&old, &new);
    assert!(!breaking(&changes));
    assert!(additions(&changes));
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "new required field detected as breaking"
)]
fn diff_added_required_field_is_breaking() {
    let old = sample_schema();
    let mut new = sample_schema();
    new.entity_kinds[0].fields.push(SchemaField {
        required: true,
        ..optional_field("severity")
    });

    assert!(breaking(&diff(&old, &new)));
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "removed field is breaking"
)]
fn diff_removed_field_is_breaking() {
    let old = sample_schema();
    let mut new = sample_schema();
    new.entity_kinds[0].fields.clear();

    let changes = diff(&old, &new);
    assert!(breaking(&changes));
    assert!(changes.iter().any(|c| matches!(c, SchemaChange::FieldRemoved { kind, field } if kind == "behavior" && field == "contract")));
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "removed edge type detected as breaking"
)]
fn diff_removed_edge_is_breaking() {
    let old = sample_schema();
    let mut new = sample_schema();
    new.edge_types.clear();

    assert!(breaking(&diff(&old, &new)));
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "new edge type detected as non-breaking"
)]
fn diff_added_edge_non_breaking() {
    let old = sample_schema();
    let mut new = sample_schema();
    new.edge_types.push(SchemaEdgeType {
        label: "enforces".to_string(),
        source_extension: "@specforge/software".to_string(),
        source_kinds: None,
        target_kinds: None,
    });

    let changes = diff(&old, &new);
    assert!(!breaking(&changes));
    assert!(additions(&changes));
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "no previous schema treats all changes as non-breaking"
)]
fn diff_no_previous_schema_all_non_breaking() {
    let dir = tempfile::tempdir().unwrap();

    let versioned = SchemaCache::in_dir(dir.path()).version(sample_schema());

    assert!(!breaking(versioned.changes()));
    assert!(versioned.notices().is_empty());
    assert_eq!(versioned.version(), &SchemaVersion::new(1, 0, 0));
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "identical schemas no changes"
)]
fn diff_identical_schemas_no_changes() {
    let changes = diff(&sample_schema(), &sample_schema());
    assert!(changes.is_empty());
    assert!(!breaking(&changes));
    assert!(!additions(&changes));
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "the versioned schema lists each change since the recorded schema"
)]
fn diff_multiple_field_changes() {
    let old = sample_schema();
    let mut new = sample_schema();
    // Remove existing field and add a new one
    new.entity_kinds[0].fields.clear();
    new.entity_kinds[0]
        .fields
        .push(optional_field("description"));

    let changes = diff(&old, &new);
    assert_eq!(
        changes,
        vec![
            SchemaChange::FieldRemoved {
                kind: "behavior".to_string(),
                field: "contract".to_string(),
            },
            SchemaChange::FieldAdded {
                kind: "behavior".to_string(),
                field: "description".to_string(),
                required: false,
            },
        ]
    );
    assert!(breaking(&changes)); // removed "contract"
    assert!(additions(&changes)); // added "description"
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "the versioned schema lists each change since the recorded schema"
)]
fn the_versioned_schema_lists_each_change_since_the_record() {
    let dir = tempfile::tempdir().unwrap();
    let cache = SchemaCache::in_dir(dir.path());
    cache.seed(&GraphProtocolSchema::empty());

    let versioned = cache.version(sample_schema());

    assert_eq!(
        versioned.changes(),
        [
            SchemaChange::ExtensionAdded("@specforge/software".to_string()),
            // A new kind's fields come with it: no separate FieldAdded.
            SchemaChange::KindAdded("behavior".to_string()),
            SchemaChange::KindAdded("feature".to_string()),
            SchemaChange::EdgeTypeAdded("implements".to_string()),
        ]
    );
}

// B:detect_breaking_schema_changes — verify contract "requires/ensures consistency for breaking schema change detection"
#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "Detect Breaking Schema Changes: breaking schema change detection holds — schema_generated_fired, filesystem_available, breaking_changes_classified, nonbreaking_changes_classified, migration_record_emitted, schema_breaking_change_detected_emitted"
)]
fn detect_breaking_contract() {
    let old = sample_schema();

    let mut new_breaking = sample_schema();
    new_breaking.entity_kinds.retain(|k| k.name != "feature");
    let changes = diff(&old, &new_breaking);
    // ensures: breaking_changes_classified
    assert!(breaking(&changes));

    let mut new_nonbreaking = sample_schema();
    new_nonbreaking.entity_kinds[0]
        .fields
        .push(optional_field("notes"));
    let changes2 = diff(&old, &new_nonbreaking);
    // ensures: nonbreaking_changes_classified
    assert!(!breaking(&changes2));
    assert!(additions(&changes2));

    // ensures: migration_record_emitted
    assert!(!changes.is_empty());
}
