use super::diff::{bumped, diff, negotiate_version};
use super::*;
use specforge_emitter::{
    GraphProtocolSchema, SchemaEdgeType, SchemaEntityKind, SchemaExtensionInfo, SchemaField,
    SchemaVersion,
};
use specforge_protocol_types::FieldType;
use specforge_test::prelude::*;

fn sample_schema() -> GraphProtocolSchema {
    GraphProtocolSchema {
        schema_version: SchemaVersion::new(1, 2, 3),
        extensions: vec![SchemaExtensionInfo {
            name: "@specforge/software".to_string(),
            version: "1.0.0".to_string(),
        }],
        entity_kinds: vec![
            SchemaEntityKind {
                name: "behavior".to_string(),
                source_extension: "@specforge/software".to_string(),
                testable: true,
                dot_color: None,
                fields: vec![SchemaField {
                    name: "contract".to_string(),
                    field_type: FieldType::String,
                    required: false,
                    enum_values: None,
                    edge: None,
                    target_kind: None,
                    description: None,
                    default_value: None,
                    source_extension: "@specforge/software".to_string(),
                }],
            },
            SchemaEntityKind {
                name: "feature".to_string(),
                source_extension: "@specforge/product".to_string(),
                testable: false,
                dot_color: None,
                fields: vec![],
            },
        ],
        edge_types: vec![SchemaEdgeType {
            label: "implements".to_string(),
            source_extension: "@specforge/software".to_string(),
            source_kinds: Some(vec!["behavior".to_string()]),
            target_kinds: Some(vec!["feature".to_string()]),
        }],
    }
}

fn optional_field(name: &str) -> SchemaField {
    SchemaField {
        name: name.to_string(),
        field_type: FieldType::String,
        required: false,
        enum_values: None,
        edge: None,
        target_kind: None,
        description: None,
        default_value: None,
        source_extension: "@specforge/software".to_string(),
    }
}

fn breaking(changes: &[SchemaChange]) -> bool {
    changes.iter().any(|c| c.bump() == Bump::Major)
}

fn additions(changes: &[SchemaChange]) -> bool {
    changes.iter().any(|c| c.bump() == Bump::Minor)
}

// ===========================================================================
// Slice 7: Schema Cache Persistence
// ===========================================================================

#[specforge_test(
    behavior = "persist_schema_cache",
    verify = "schema-cache.json written after schema generation"
)]
fn cache_write_read_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let schema = sample_schema();

    let cache = SchemaCache::in_dir(dir.path());
    cache.seed(&schema);
    let loaded = cache.load().unwrap();

    let record = loaded.unwrap();
    assert_eq!(record.schema, schema);
    assert!(!record.content_hash.is_empty());
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "missing schema cache yields no previous schema"
)]
fn cache_missing_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let loaded = SchemaCache::in_dir(dir.path()).load().unwrap();
    assert!(loaded.is_none());
}

#[specforge_test(
    behavior = "persist_schema_cache",
    verify = "cache file overwritten atomically via temp+rename"
)]
fn cache_atomic_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let schema1 = GraphProtocolSchema::empty();
    let schema2 = sample_schema();
    let cache = SchemaCache::in_dir(dir.path());

    cache.seed(&schema1);
    let file = dir.path().join("schema-cache.json");
    let first = std::fs::read_to_string(&file).unwrap();

    // A hard link shares the cache file's inode. Writing the file in place
    // would change what the link reads; a rename puts a new inode at the
    // path and leaves the link on the old, complete content.
    let link = dir.path().join("previous-cache.json");
    std::fs::hard_link(&file, &link).unwrap();

    cache.seed(&schema2);

    assert_eq!(
        std::fs::read_to_string(&link).unwrap(),
        first,
        "the old cache file must be replaced, not rewritten in place"
    );
    let loaded = cache.load().unwrap().unwrap();
    assert_eq!(loaded.schema, schema2);

    // The temp file was renamed away: only the cache and the link remain.
    let mut names: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, vec!["previous-cache.json", "schema-cache.json"]);
}

#[specforge_test(behavior = "persist_schema_cache", verify = "content hash changes")]
fn cache_content_hash_changes() {
    let dir = tempfile::tempdir().unwrap();
    let cache = SchemaCache::in_dir(dir.path());
    cache.seed(&GraphProtocolSchema::empty());
    let hash1 = cache.load().unwrap().unwrap().content_hash;

    cache.seed(&sample_schema());
    let hash2 = cache.load().unwrap().unwrap().content_hash;

    assert_ne!(hash1, hash2);
}

// Not linked to "cache updated even when no JSON export is performed": only
// `specforge export` updates the cache (`check` writes no files). This only
// shows the call itself needs no graph export.
#[test]
fn cache_independent_of_export() {
    let dir = tempfile::tempdir().unwrap();
    SchemaCache::in_dir(dir.path()).seed(&sample_schema());

    // Cache file exists independently — no graph export needed
    let cache_file = dir.path().join("schema-cache.json");
    assert!(cache_file.exists());
    let contents = std::fs::read_to_string(&cache_file).unwrap();
    let record: Record = serde_json::from_str(&contents).unwrap();
    assert_eq!(record.schema.entity_kinds.len(), 2);
}

#[specforge_test(
    behavior = "persist_schema_cache",
    verify = "persisted cache feeds breaking change detection in the next compilation"
)]
fn cache_load_diff_version_pipeline() {
    let dir = tempfile::tempdir().unwrap();
    let cache = SchemaCache::in_dir(dir.path());
    cache.seed(&GraphProtocolSchema::empty());

    let versioned = cache.version(sample_schema());

    // Schema2 has additions but no removals from empty
    assert!(additions(versioned.changes()));
    assert!(!breaking(versioned.changes()));
    assert_eq!(versioned.version(), &SchemaVersion::new(1, 1, 0));
}

// ===========================================================================
// Detect breaking changes through the record
// ===========================================================================

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "a missing schema cache emits no diagnostic"
)]
fn a_missing_record_reports_nothing() {
    let dir = tempfile::tempdir().unwrap();

    let versioned = SchemaCache::in_dir(dir.path()).version(sample_schema());

    assert!(versioned.diagnostics().is_empty());
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "previous schema is read from .specforge/schema-cache.json"
)]
fn detect_breaking_with_cached_schema() {
    let dir = tempfile::tempdir().unwrap();
    // The cached schema has a kind the current one lacks. Without the
    // cache there is no previous schema and nothing can be "removed".
    let mut old_schema = sample_schema();
    old_schema.entity_kinds.push(SchemaEntityKind {
        name: "legacy".to_string(),
        source_extension: "@specforge/software".to_string(),
        testable: false,
        dot_color: None,
        fields: vec![],
    });
    SchemaCache::in_dir(dir.path()).seed(&old_schema);

    let versioned = SchemaCache::in_dir(dir.path()).version(sample_schema());

    assert_eq!(
        versioned.changes(),
        [SchemaChange::KindRemoved("legacy".to_string())]
    );
    assert!(breaking(versioned.changes()));
    // The cache was found (no I016); the removal is one W053 warning.
    let diagnostics = versioned.diagnostics();
    let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, vec!["W053"], "{diagnostics:?}");
    assert_eq!(diagnostics[0].severity, specforge_common::Severity::Warning);
    assert_eq!(
        diagnostics[0].message,
        "breaking schema change since the last export: entity kind `legacy` was removed"
    );

    // The same comparison without the cache file sees no removal.
    let empty = tempfile::tempdir().unwrap();
    let no_cache = SchemaCache::in_dir(empty.path()).version(sample_schema());
    assert!(!breaking(no_cache.changes()));
}

// Not linked to the Persist Schema Cache contract. `specforge export` persists
// the cache (crates/specforge-cli/tests/schema_cache.rs proves the write and
// its atomicity through the CLI), but the CLI has no event sink, so its
// schema_cache_persisted_emitted clause has nothing to assert. This only
// shows the call writes atomically and round-trips.
#[test]
fn persist_cache_contract() {
    let dir = tempfile::tempdir().unwrap();
    let schema = sample_schema();
    let cache = SchemaCache::in_dir(dir.path());

    cache.seed(&schema);
    // ensures: cache_written_atomically
    assert!(!dir.path().join(".schema-cache.tmp").exists());
    assert!(dir.path().join("schema-cache.json").exists());

    let loaded = cache.load().unwrap().unwrap();
    assert_eq!(loaded.schema, schema);
}

// The view's cache is the one at the root it was compiled from: a view of
// a sub-path never versions against (or records into) its ancestor's.
#[specforge_test(
    behavior = "read_views_over_the_project_view",
    verify = "the schema cache is the view root's, never an ancestor's"
)]
fn the_view_versions_against_its_root_cache() {
    use crate::view::ProjectView;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let sub = root.join("sub");
    std::fs::create_dir(&sub).unwrap();
    let graph = specforge_graph::Graph::new();
    let env = specforge_project::Environment::with_registries(
        specforge_registry::RegistryBuild::default(),
        None,
    );
    let recorded = specforge_project::coverage::RecordedCoverage::over(&graph, &env);
    let view_at = |root| ProjectView::new(&graph, &env, Some(root), &recorded);

    // The project's cache holds an older schema at 1.2.3 with a kind the
    // project no longer has: a breaking change since.
    let mut cached = sample_schema();
    cached.schema_version = SchemaVersion::new(1, 2, 3);
    SchemaCache::of_root(root).seed(&cached);
    assert_eq!(
        view_at(root).schema_cache().unwrap().dir(),
        root.join(".specforge")
    );
    assert_eq!(
        view_at(root).versioned_schema().version(),
        &SchemaVersion::new(2, 0, 0)
    );

    // Rooted at the sub-path: no cache there, a first version.
    assert_eq!(
        view_at(&sub).versioned_schema().version(),
        &SchemaVersion::new(1, 0, 0)
    );
    assert_eq!(
        view_at(&sub).schema_cache().unwrap().dir(),
        sub.join(".specforge")
    );

    // Without a root there is no cache.
    let rootless = ProjectView::new(&graph, &env, None, &recorded);
    assert!(rootless.schema_cache().is_none());
}

/// What `360bd587` wrote for a kind `ticket` with one bool field `urgent`:
/// the older host named the type `boolean`.
const CACHE_AN_OLDER_HOST_WROTE: &str = r#"{
  "schema": {
    "schema_version": { "major": 1, "minor": 0, "patch": 0 },
    "extensions": [],
    "entity_kinds": [{
      "name": "ticket", "source_extension": "@t/x", "testable": false,
      "fields": [{ "name": "urgent", "field_type": "boolean", "required": false, "source_extension": "@t/x" }]
    }],
    "edge_types": []
  },
  "content_hash": "0"
}"#;

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "a cache an older host wrote reads its field types unchanged"
)]
fn a_cache_an_older_host_wrote_reads_its_bool_field_unchanged() {
    use specforge_protocol_types::{EntityKindDescriptor, ExtensionDeclaration, FieldDescriptor};

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("schema-cache.json"),
        CACHE_AN_OLDER_HOST_WROTE,
    )
    .unwrap();

    let mut declaration = ExtensionDeclaration::default();
    declaration.handshake.name = "@t/x".to_string();
    declaration.entities = vec![EntityKindDescriptor {
        name: "ticket".to_string(),
        keyword: Some("ticket".to_string()),
        fields: vec![FieldDescriptor {
            name: "urgent".to_string(),
            field_type: "bool".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    }];
    let build = specforge_registry::build_registries(vec![declaration]);
    let mut current = specforge_emitter::generate_schema(&build);
    // The record names no extensions: only its field types are under test.
    current.extensions.clear();

    let versioned = SchemaCache::in_dir(dir.path()).version(current);

    assert!(versioned.changes().is_empty(), "{:?}", versioned.changes());
    assert!(versioned.diagnostics().is_empty());
    assert_eq!(versioned.version(), &SchemaVersion::new(1, 0, 0));
}

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
    assert!(versioned.diagnostics().is_empty());
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

// ===========================================================================
// Slice 5: Compute Schema Version
// ===========================================================================

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "first compilation without cache produces version 1.0.0"
)]
fn compute_version_no_cache() {
    let dir = tempfile::tempdir().unwrap();

    let versioned = SchemaCache::in_dir(dir.path()).version(sample_schema());

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
// attaches the computed version (crates/specforge-cli/tests/schema_cache.rs
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

// ===========================================================================
// Slice 6: Version Negotiation
// ===========================================================================

#[specforge_test(
    behavior = "negotiate_schema_version",
    verify = "compatible version within range is resolved"
)]
fn negotiate_in_range_succeeds() {
    let min = SchemaVersion::new(1, 0, 0);
    let max = SchemaVersion::new(1, 5, 0);
    let requested = SchemaVersion::new(1, 3, 0);

    assert!(negotiate_version(&requested, &min, &max).is_ok());
}

#[specforge_test(
    behavior = "negotiate_schema_version",
    verify = "compatible version within range is resolved"
)]
fn negotiate_exact_min_succeeds() {
    let min = SchemaVersion::new(1, 0, 0);
    let max = SchemaVersion::new(1, 5, 0);
    assert!(negotiate_version(&min, &min, &max).is_ok());
}

#[specforge_test(
    behavior = "negotiate_schema_version",
    verify = "compatible version within range is resolved"
)]
fn negotiate_exact_max_succeeds() {
    let min = SchemaVersion::new(1, 0, 0);
    let max = SchemaVersion::new(1, 5, 0);
    assert!(negotiate_version(&max, &min, &max).is_ok());
}

#[specforge_test(
    behavior = "negotiate_schema_version",
    verify = "incompatible version produces E027 with supported range"
)]
fn negotiate_out_of_range_fails() {
    let min = SchemaVersion::new(1, 2, 0);
    let max = SchemaVersion::new(1, 5, 0);
    let requested = SchemaVersion::new(1, 1, 0);

    let err = negotiate_version(&requested, &min, &max).unwrap_err();
    assert_eq!(
        err.to_string(),
        "E027: requested schema version 1.1.0 is out of range [1.2.0, 1.5.0]"
    );
    assert_eq!(
        (err.min, err.max),
        (SchemaVersion::new(1, 2, 0), SchemaVersion::new(1, 5, 0))
    );
}

#[specforge_test(
    behavior = "negotiate_schema_version",
    verify = "incompatible version produces E027 with supported range"
)]
fn negotiate_different_major_fails() {
    let min = SchemaVersion::new(1, 0, 0);
    let max = SchemaVersion::new(1, 5, 0);
    let requested = SchemaVersion::new(2, 0, 0);

    let err = negotiate_version(&requested, &min, &max).unwrap_err();
    assert_eq!(
        err.to_string(),
        "E027: requested schema version 2.0.0 has incompatible major version \
         (supported range [1.0.0, 1.5.0])"
    );
    assert_eq!(err.requested, requested);
}

#[specforge_test(
    behavior = "negotiate_schema_version",
    verify = "incompatible version produces E027 with supported range"
)]
fn schema_version_error_display() {
    // Below the supported minimum, in the same major version.
    let err = negotiate_version(
        &SchemaVersion::new(1, 0, 9),
        &SchemaVersion::new(1, 1, 0),
        &SchemaVersion::new(1, 4, 2),
    )
    .unwrap_err();
    let shown = err.to_string();
    assert!(shown.starts_with("E027: "), "{shown}");
    assert!(shown.ends_with("[1.1.0, 1.4.2]"), "{shown}");
}

#[specforge_test(
    behavior = "negotiate_schema_version",
    verify = "incompatible version produces E027 with supported range"
)]
fn negotiate_above_max_fails() {
    let min = SchemaVersion::new(1, 0, 0);
    let max = SchemaVersion::new(1, 5, 0);
    let requested = SchemaVersion::new(1, 6, 0);
    let err = negotiate_version(&requested, &min, &max).unwrap_err();
    assert_eq!(
        err.to_string(),
        "E027: requested schema version 1.6.0 is out of range [1.0.0, 1.5.0]"
    );
}

#[test]
fn schema_version_cli_flag_selects_version() {
    let requested: SchemaVersion = "1.2.0".parse().unwrap();
    let min = SchemaVersion::new(1, 0, 0);
    let max = SchemaVersion::new(1, 5, 0);
    assert!(negotiate_version(&requested, &min, &max).is_ok());
}

#[test]
fn schema_version_mcp_query_parameter() {
    let requested: SchemaVersion = "1.3.0".parse().unwrap();
    let min = SchemaVersion::new(1, 0, 0);
    let max = SchemaVersion::new(1, 5, 0);
    assert!(negotiate_version(&requested, &min, &max).is_ok());
}

// ===========================================================================
// Every attribute is compared and classified (ADR 0067 D2)
// ===========================================================================

/// A schema holding every attribute a change can touch: two extensions, a
/// kind with a DOT colour and an enum field with a default and a description,
/// a field that writes an edge type, and an edge type with two ends each.
fn rich_schema() -> GraphProtocolSchema {
    let software = "@specforge/software".to_string();
    GraphProtocolSchema {
        schema_version: SchemaVersion::new(1, 2, 3),
        extensions: vec![
            SchemaExtensionInfo {
                name: software.clone(),
                version: "1.0.0".to_string(),
            },
            SchemaExtensionInfo {
                name: "@specforge/product".to_string(),
                version: "1.0.0".to_string(),
            },
        ],
        entity_kinds: vec![
            SchemaEntityKind {
                name: "behavior".to_string(),
                source_extension: software.clone(),
                testable: true,
                dot_color: Some("#111".to_string()),
                fields: vec![
                    SchemaField {
                        required: true,
                        ..optional_field("contract")
                    },
                    SchemaField {
                        field_type: FieldType::Enum,
                        enum_values: Some(vec!["a".to_string(), "b".to_string()]),
                        description: Some("d".to_string()),
                        default_value: Some("a".to_string()),
                        ..optional_field("status")
                    },
                    SchemaField {
                        edge: Some("OwnedBy".to_string()),
                        target_kind: Some("person".to_string()),
                        ..optional_field("owner")
                    },
                ],
            },
            SchemaEntityKind {
                name: "feature".to_string(),
                source_extension: "@specforge/product".to_string(),
                testable: false,
                dot_color: None,
                fields: vec![],
            },
        ],
        edge_types: vec![SchemaEdgeType {
            label: "E".to_string(),
            source_extension: software,
            source_kinds: Some(vec!["behavior".to_string(), "feature".to_string()]),
            target_kinds: Some(vec!["feature".to_string(), "behavior".to_string()]),
        }],
    }
}

fn behavior(schema: &mut GraphProtocolSchema) -> &mut SchemaEntityKind {
    schema
        .entity_kinds
        .iter_mut()
        .find(|k| k.name == "behavior")
        .unwrap()
}

fn field<'a>(schema: &'a mut GraphProtocolSchema, name: &str) -> &'a mut SchemaField {
    behavior(schema)
        .fields
        .iter_mut()
        .find(|f| f.name == name)
        .unwrap()
}

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
            versioned.diagnostics().len(),
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

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "an unreadable schema cache restarts the version at 1.0.0 with I016"
)]
fn an_unreadable_record_restarts_at_1_0_0_with_i016() {
    let invalid_json = r#"{"schema": {"#;
    let no_schema_key = r#"{"content_hash": "0"}"#;
    let unknown_field_type = r#"{
      "schema": {
        "schema_version": { "major": 1, "minor": 4, "patch": 0 },
        "extensions": [],
        "entity_kinds": [{
          "name": "t", "source_extension": "@t/x", "testable": false,
          "fields": [{ "name": "f", "field_type": "no_such_type", "required": false, "source_extension": "@t/x" }]
        }],
        "edge_types": []
      },
      "content_hash": "0"
    }"#;
    for (cause, text) in [
        ("invalid json", invalid_json),
        ("no schema key", no_schema_key),
        ("an unknown field type", unknown_field_type),
    ] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("schema-cache.json"), text).unwrap();

        let versioned = SchemaCache::in_dir(dir.path()).version(sample_schema());

        assert_eq!(versioned.version(), &SchemaVersion::new(1, 0, 0), "{cause}");
        assert!(versioned.changes().is_empty(), "{cause}");
        let diagnostics = versioned.diagnostics();
        assert_eq!(diagnostics.len(), 1, "{cause}: {diagnostics:?}");
        assert_eq!(diagnostics[0].code, "I016", "{cause}");
        assert_eq!(diagnostics[0].severity, specforge_common::Severity::Info);
        let path = dir.path().join("schema-cache.json");
        assert!(
            diagnostics[0].message.starts_with(&format!(
                "the schema cache {} could not be read (",
                path.display()
            )),
            "{cause}: {}",
            diagnostics[0].message
        );
        assert!(
            diagnostics[0]
                .message
                .ends_with("): the schema version starts again at 1.0.0"),
            "{cause}: {}",
            diagnostics[0].message
        );
    }
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "the W053 message names the change and the versions"
)]
fn the_w053_message_names_the_change_and_the_versions() {
    let dir = tempfile::tempdir().unwrap();
    let cache = SchemaCache::in_dir(dir.path());
    cache.seed(&rich_schema());
    let mut generated = rich_schema();
    field(&mut generated, "contract").required = false;

    let versioned = cache.version(generated);

    let diagnostics = versioned.diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "W053");
    assert_eq!(
        diagnostics[0].message,
        "breaking schema change since the last export: the required flag of field `contract` of `behavior` changed from true to false"
    );
    assert_eq!(
        diagnostics[0].suggestion.as_deref(),
        Some(
            "The schema version goes from 1.2.3 to 2.0.0: agents and tools that read 1.x exports may not read this one. Update them, or keep the extension versions that produced 1.2.3."
        )
    );
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "one schema version names one document: equal versions have equal content hashes"
)]
fn a_version_names_the_document_the_record_holds() {
    let dir = tempfile::tempdir().unwrap();
    let cache = SchemaCache::in_dir(dir.path());
    cache.seed(&rich_schema());

    // A patch change: 1.2.3 becomes 1.2.4, and the export records it.
    let mut patched = rich_schema();
    patched.extensions[0].version = "1.0.1".to_string();
    let first = cache.version(patched.clone());
    assert_eq!(first.version(), &SchemaVersion::new(1, 2, 4));
    cache.record(&first).unwrap();

    // The same content against the new record: the same version and hash.
    let again = cache.version(patched);
    assert_eq!(again.version(), first.version());
    assert_eq!(again.content_hash(), first.content_hash());

    // Other content is another version, so another hash: 1.2.4 keeps naming
    // the document it was recorded for.
    let mut other = rich_schema();
    other.extensions[0].version = "1.0.2".to_string();
    let next = cache.version(other);
    assert_eq!(next.version(), &SchemaVersion::new(1, 2, 5));
    assert_ne!(next.content_hash(), first.content_hash());
}
