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
    changes.iter().any(SchemaChange::is_breaking)
}

fn additions(changes: &[SchemaChange]) -> bool {
    changes.iter().any(SchemaChange::is_addition)
}

fn sorted(mut changes: Vec<SchemaChange>) -> Vec<SchemaChange> {
    changes.sort_by_key(|c| format!("{c:?}"));
    changes
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
    verify = "missing cache without prior exports emits no diagnostic"
)]
fn detect_breaking_missing_cache_no_exports_no_diagnostic() {
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
    let current = specforge_emitter::generate_schema(&build);

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
    verify = "SchemaMigration record emitted on version change"
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
        sorted(changes.clone()),
        vec![
            SchemaChange::FieldAdded {
                kind: "behavior".to_string(),
                field: "description".to_string(),
                required: false,
            },
            SchemaChange::FieldRemoved {
                kind: "behavior".to_string(),
                field: "contract".to_string(),
            },
        ]
    );
    assert!(breaking(&changes)); // removed "contract"
    assert!(additions(&changes)); // added "description"
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "SchemaMigration record emitted on version change"
)]
fn detect_breaking_migration_record_emitted() {
    let old = GraphProtocolSchema::empty();
    let new = sample_schema();

    let changes = diff(&old, &new);

    assert_eq!(
        sorted(changes),
        vec![
            SchemaChange::EdgeAdded("implements".to_string()),
            // A new kind's fields come with it: no separate FieldAdded.
            SchemaChange::KindAdded("behavior".to_string()),
            SchemaChange::KindAdded("feature".to_string()),
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
        vec![SchemaChange::EdgeAdded("enforces".to_string())],
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
        vec![SchemaChange::EdgeRemoved("enforces".to_string())],
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

#[test]
fn compute_version_metadata_only_no_bump() {
    let old = sample_schema();
    let mut new = sample_schema();
    new.extensions[0].version = "2.0.0".to_string();

    let changes = diff(&old, &new);
    assert!(changes.is_empty());
    assert_eq!(
        bumped(&SchemaVersion::new(1, 2, 3), &changes),
        SchemaVersion::new(1, 2, 3),
        "no structural change = no version bump"
    );
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
