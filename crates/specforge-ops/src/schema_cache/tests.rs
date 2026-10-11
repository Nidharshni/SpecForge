use super::*;
use specforge_emitter::{
    GraphProtocolSchema, SchemaCacheEntry, SchemaEdgeType, SchemaEntityKind, SchemaExtensionInfo,
    SchemaField, SchemaMigrationChange, SchemaVersion, compute_schema_version, diff_schemas,
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

// ===========================================================================
// Slice 7: Schema Cache Persistence
// ===========================================================================

// B:persist_schema_cache — verify unit "write + read round-trip"
#[specforge_test(
    behavior = "persist_schema_cache",
    verify = "schema-cache.json written after schema generation"
)]
fn cache_write_read_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let schema = sample_schema();

    SchemaCache::in_dir(dir.path()).record(&schema).unwrap();
    let loaded = SchemaCache::in_dir(dir.path()).load().unwrap();

    assert!(loaded.is_some());
    let entry = loaded.unwrap();
    assert_eq!(entry.schema, schema);
    assert!(!entry.content_hash.is_empty());
}

// B:persist_schema_cache — verify unit "missing cache returns None"
#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "missing schema cache yields no previous schema"
)]
fn cache_missing_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let loaded = SchemaCache::in_dir(dir.path()).load().unwrap();
    assert!(loaded.is_none());
}

// B:persist_schema_cache — verify unit "atomic overwrite"
#[specforge_test(
    behavior = "persist_schema_cache",
    verify = "cache file overwritten atomically via temp+rename"
)]
fn cache_atomic_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let schema1 = GraphProtocolSchema::empty();
    let schema2 = sample_schema();

    SchemaCache::in_dir(dir.path()).record(&schema1).unwrap();
    let cache = dir.path().join("schema-cache.json");
    let first = std::fs::read_to_string(&cache).unwrap();

    // A hard link shares the cache file's inode. Writing the file in place
    // would change what the link reads; a rename puts a new inode at the
    // path and leaves the link on the old, complete content.
    let link = dir.path().join("previous-cache.json");
    std::fs::hard_link(&cache, &link).unwrap();

    SchemaCache::in_dir(dir.path()).record(&schema2).unwrap();

    assert_eq!(
        std::fs::read_to_string(&link).unwrap(),
        first,
        "the old cache file must be replaced, not rewritten in place"
    );
    let loaded = SchemaCache::in_dir(dir.path()).load().unwrap().unwrap();
    assert_eq!(loaded.schema, schema2);

    // The temp file was renamed away: only the cache and the link remain.
    let mut names: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, vec!["previous-cache.json", "schema-cache.json"]);
}

// B:persist_schema_cache — verify unit "content hash changes with schema"
#[specforge_test(behavior = "persist_schema_cache", verify = "content hash changes")]
fn cache_content_hash_changes() {
    let dir = tempfile::tempdir().unwrap();
    let schema1 = GraphProtocolSchema::empty();
    SchemaCache::in_dir(dir.path()).record(&schema1).unwrap();
    let hash1 = SchemaCache::in_dir(dir.path())
        .load()
        .unwrap()
        .unwrap()
        .content_hash;

    let schema2 = sample_schema();
    SchemaCache::in_dir(dir.path()).record(&schema2).unwrap();
    let hash2 = SchemaCache::in_dir(dir.path())
        .load()
        .unwrap()
        .unwrap()
        .content_hash;

    assert_ne!(hash1, hash2);
}

// Not linked to "cache updated even when no JSON export is performed": only
// `specforge export` updates the cache (`check` writes no files). This only
// shows the call itself needs no graph export.
#[test]
fn cache_independent_of_export() {
    let dir = tempfile::tempdir().unwrap();
    let schema = sample_schema();
    SchemaCache::in_dir(dir.path()).record(&schema).unwrap();

    // Cache file exists independently — no graph export needed
    let cache_file = dir.path().join("schema-cache.json");
    assert!(cache_file.exists());
    let contents = std::fs::read_to_string(&cache_file).unwrap();
    let entry: SchemaCacheEntry = serde_json::from_str(&contents).unwrap();
    assert_eq!(entry.schema.entity_kinds.len(), 2);
}

// B:persist_schema_cache — verify integration "cache → load → diff → version"
#[specforge_test(
    behavior = "persist_schema_cache",
    verify = "persisted cache feeds breaking change detection in the next compilation"
)]
fn cache_load_diff_version_pipeline() {
    let dir = tempfile::tempdir().unwrap();

    // Save initial schema
    let schema1 = GraphProtocolSchema::empty();
    SchemaCache::in_dir(dir.path()).record(&schema1).unwrap();

    // Load and diff with new schema
    let cached = SchemaCache::in_dir(dir.path()).load().unwrap().unwrap();
    let schema2 = sample_schema();
    let migration = diff_schemas(&cached.schema, &schema2);

    // Schema2 has additions but no removals from empty
    assert!(migration.has_additions());

    let version = compute_schema_version(&migration, Some(&SchemaVersion::new(1, 0, 0)));
    assert_eq!(version, SchemaVersion::new(1, 1, 0));
}

// B:negotiate_schema_version — verify unit "no version requested defaults to latest"
// ===========================================================================
// Gap coverage: Diagnostic integration (I016)
// ===========================================================================

// B:detect_breaking_schema_changes — verify unit "missing cache with prior exports emits I016 info diagnostic"
#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "missing cache with prior exports emits I016 info diagnostic"
)]
fn detect_breaking_missing_cache_emits_i016() {
    let dir = tempfile::tempdir().unwrap();
    let current = sample_schema();

    let (migration, diagnostics) = SchemaCache::in_dir(dir.path()).detect_breaking(&current, true);

    assert!(!migration.has_breaking_changes());
    assert!(migration.has_additions());

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "I016");
    assert_eq!(diagnostics[0].severity, specforge_common::Severity::Info);
    assert!(diagnostics[0].message.contains("Schema cache not found"));
}

// B:detect_breaking_schema_changes — verify unit "missing cache without prior exports emits no diagnostic"
#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "missing cache without prior exports emits no diagnostic"
)]
fn detect_breaking_missing_cache_no_exports_no_diagnostic() {
    let dir = tempfile::tempdir().unwrap();
    let current = sample_schema();

    let (_migration, diagnostics) =
        SchemaCache::in_dir(dir.path()).detect_breaking(&current, false);

    assert!(diagnostics.is_empty());
}

// B:detect_breaking_schema_changes — verify unit "cached schema used for diff"
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
    SchemaCache::in_dir(dir.path()).record(&old_schema).unwrap();

    let current = sample_schema();
    let (migration, diagnostics) = SchemaCache::in_dir(dir.path()).detect_breaking(&current, true);

    assert_eq!(
        migration.changes,
        vec![SchemaMigrationChange::KindRemoved("legacy".to_string())]
    );
    assert!(migration.has_breaking_changes());
    // The cache was found (no I016); the removal is one W053 warning.
    let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, vec!["W053"], "{diagnostics:?}");
    assert_eq!(diagnostics[0].severity, specforge_common::Severity::Warning);
    assert_eq!(
        diagnostics[0].message,
        "breaking schema change since the last export: entity kind `legacy` was removed"
    );

    // The same comparison without the cache file sees no removal.
    let empty = tempfile::tempdir().unwrap();
    let (no_cache, _) = SchemaCache::in_dir(empty.path()).detect_breaking(&current, false);
    assert!(!no_cache.has_breaking_changes());
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

    SchemaCache::in_dir(dir.path()).record(&schema).unwrap();
    // ensures: cache_written_atomically
    assert!(!dir.path().join(".schema-cache.tmp").exists());
    assert!(dir.path().join("schema-cache.json").exists());

    let loaded = SchemaCache::in_dir(dir.path()).load().unwrap().unwrap();
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
    SchemaCache::of_root(root).record(&cached).unwrap();
    assert_eq!(
        view_at(root).schema_cache().unwrap().dir(),
        root.join(".specforge")
    );
    assert_eq!(
        view_at(root).versioned_schema().schema_version,
        SchemaVersion::new(2, 0, 0)
    );

    // Rooted at the sub-path: no cache there, a first version.
    assert_eq!(
        view_at(&sub).versioned_schema().schema_version,
        SchemaVersion::new(1, 0, 0)
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
    let mut current =
        specforge_emitter::generate_schema(build.kinds(), build.edges(), build.fields(), &[]);

    let cache = SchemaCache::in_dir(dir.path());
    let (migration, diagnostics) = cache.detect_breaking(&current, true);
    assert!(migration.changes.is_empty(), "{:?}", migration.changes);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    cache.version(&mut current);
    assert_eq!(current.schema_version, SchemaVersion::new(1, 0, 0));
}
