use super::*;

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
// the cache (crates/surfaces/cli/tests/schema_cache.rs proves the write and
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
    use crate::testing::Viewed;
    use specforge_project::testing::Assembled;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let sub = root.join("sub");
    std::fs::create_dir(&sub).unwrap();
    let (at_root, at_sub) = (Assembled::at(root), Assembled::at(&sub));

    // The project's cache holds an older schema at 1.2.3 with a kind the
    // project no longer has: a breaking change since.
    let mut cached = sample_schema();
    cached.schema_version = SchemaVersion::new(1, 2, 3);
    SchemaCache::of_root(root).seed(&cached);
    assert_eq!(
        at_root.view().schema_cache().unwrap().dir(),
        root.join(".specforge")
    );
    assert_eq!(
        at_root.view().versioned_schema().version(),
        &SchemaVersion::new(2, 0, 0)
    );

    // Rooted at the sub-path: no cache there, a first version.
    assert_eq!(
        at_sub.view().versioned_schema().version(),
        &SchemaVersion::new(1, 0, 0)
    );
    assert_eq!(
        at_sub.view().schema_cache().unwrap().dir(),
        sub.join(".specforge")
    );

    // Without a root there is no cache.
    let rootless = at_root.rootless_view();
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
