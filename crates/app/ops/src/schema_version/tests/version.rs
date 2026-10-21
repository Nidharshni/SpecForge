use super::*;

// ===========================================================================
// Slice 5: Compute Schema Version
// ===========================================================================

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "first compilation without cache produces version 1.0.0"
)]
fn compute_version_no_cache() {
    let dir = tempfile::tempdir().unwrap();

    let versioned = SchemaCache::in_dir(dir.path()).version(sample_schema(), &PlainDisk);

    assert_eq!(versioned.version(), &SchemaVersion::new(1, 0, 0));
}

fn bumped_from(previous: (u32, u32, u32), changes: Vec<SchemaChange>) -> SchemaVersion {
    bumped(
        &SchemaVersion::new(previous.0, previous.1, previous.2),
        &changes,
    )
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "new entity kind triggers minor version bump"
)]
fn compute_version_new_kind_bumps_minor() {
    let version = bumped_from(
        (1, 2, 3),
        vec![SchemaChange::KindAdded("event".to_string())],
    );
    assert_eq!(version, SchemaVersion::new(1, 3, 0));
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "removed entity kind triggers major version bump"
)]
fn compute_version_removed_kind_bumps_major() {
    let version = bumped_from(
        (1, 2, 3),
        vec![SchemaChange::KindRemoved("event".to_string())],
    );
    assert_eq!(version, SchemaVersion::new(2, 0, 0));
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "no changes returns previous"
)]
fn compute_version_no_changes_returns_previous() {
    assert_eq!(bumped_from((3, 1, 4), vec![]), SchemaVersion::new(3, 1, 4));
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "removed entity kind triggers major version bump"
)]
fn compute_version_mixed_changes_major_wins() {
    let version = bumped_from(
        (1, 5, 2),
        vec![
            SchemaChange::KindAdded("event".to_string()),
            SchemaChange::KindRemoved("legacy".to_string()),
        ],
    );
    assert_eq!(version, SchemaVersion::new(2, 0, 0));
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "new edge type triggers minor version bump"
)]
fn compute_version_new_edge_bumps_minor() {
    let version = bumped_from(
        (1, 0, 0),
        vec![SchemaChange::EdgeTypeAdded("enforces".to_string())],
    );
    assert_eq!(version, SchemaVersion::new(1, 1, 0));
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "removed edge type triggers major version bump"
)]
fn compute_version_removed_edge_bumps_major() {
    let version = bumped_from(
        (1, 0, 0),
        vec![SchemaChange::EdgeTypeRemoved("enforces".to_string())],
    );
    assert_eq!(version, SchemaVersion::new(2, 0, 0));
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "added optional field bumps minor"
)]
fn compute_version_added_optional_field_bumps_minor() {
    let version = bumped_from(
        (1, 0, 0),
        vec![SchemaChange::FieldAdded {
            kind: "behavior".to_string(),
            field: "description".to_string(),
            required: false,
        }],
    );
    assert_eq!(version, SchemaVersion::new(1, 1, 0));
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "new required field triggers major version bump"
)]
fn compute_version_added_required_field_bumps_major() {
    let version = bumped_from(
        (1, 0, 0),
        vec![SchemaChange::FieldAdded {
            kind: "behavior".to_string(),
            field: "severity".to_string(),
            required: true,
        }],
    );
    assert_eq!(version, SchemaVersion::new(2, 0, 0));
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "removed field bumps major"
)]
fn compute_version_removed_field_bumps_major() {
    let version = bumped_from(
        (1, 0, 0),
        vec![SchemaChange::FieldRemoved {
            kind: "behavior".to_string(),
            field: "contract".to_string(),
        }],
    );
    assert_eq!(version, SchemaVersion::new(2, 0, 0));
}

// Not linked to the Compute Schema Version contract. `specforge export`
// attaches the computed version (crates/surfaces/cli/tests/schema_cache.rs
// proves it through the CLI), but the CLI has no event sink, so the
// schema_breaking_change_detected_fired and schema_version_computed_emitted
// clauses have nothing to assert. This only checks the version arithmetic.
#[test]
fn compute_version_contract() {
    // ensures: version_auto_computed — major for breaking
    let v = bumped_from((1, 2, 3), vec![SchemaChange::KindRemoved("x".into())]);
    assert_eq!(v.major, 2);

    // ensures: version_auto_computed — minor for additions
    let v = bumped_from((1, 2, 3), vec![SchemaChange::KindAdded("y".into())]);
    assert_eq!(v, SchemaVersion::new(1, 3, 0));
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "field metadata change triggers patch version bump"
)]
fn field_description_change_bumps_the_patch() {
    let old = sample_schema();
    let mut new = sample_schema();
    new.entity_kinds[0].fields[0].description = Some("reworded".to_string());

    let changes = diff(&old, &new);

    assert_eq!(changes.len(), 1, "{changes:?}");
    assert!(!breaking(&changes));
    assert!(!additions(&changes));
    assert_eq!(
        bumped(&SchemaVersion::new(1, 2, 3), &changes),
        SchemaVersion::new(1, 2, 4)
    );
}
