// Migration-specific invariants

invariant migration_idempotency "Migration Idempotency" {
  guarantee """
    Running specforge migrate twice on the same .spec files MUST produce
    identical output. A file already at the target version MUST be left
    unchanged. The migration transform MUST be a pure function of the
    source content and version pair — no external state may influence
    the result.
  """
  risk      medium
  verify property "running migrate twice produces identical files"
  verify unit "file already at target version is unchanged"
}

invariant migration_backup_safety "Migration Backup Safety" {
  guarantee """
    Before any in-place migration modifies a .spec file, the system MUST
    create a backup copy with a .bak extension unless --no-backup is
    explicitly specified. The backup MUST be a byte-for-byte copy of the
    original file. If backup creation fails (e.g., disk full, permission
    denied), the migration MUST abort without modifying any file.
  """
  risk      high
  verify unit "backup created before file modification"
  verify unit "backup is byte-for-byte copy of original"
  verify unit "failed backup aborts migration for that file"
}

invariant migration_atomicity "Migration Atomicity" {
  guarantee """
    Each .spec file MUST be written atomically during migration: the system
    MUST write the migrated content to a temporary file in the same directory,
    then rename the temporary file to the target path, keeping the file's
    permissions. If the rename fails,
    the original file MUST remain unmodified. This guarantees that no
    partially written file can exist on disk, even if the process is
    interrupted mid-write.
  """
  risk      high
  verify unit "migration writes to temporary file then renames"
  verify unit "interrupted write leaves original file intact"
  verify unit "failed rename does not corrupt original file"
}

invariant migration_event_ordering "Migration Event Ordering" {
  guarantee """
    Migration events MUST be emitted in strict temporal order:
    migration_starting → migration_started → migration_complete →
    extension_migration_hooks_complete. No event in this sequence MAY
    be emitted before its predecessor has completed. Consumers that
    depend on this ordering (e.g., pre-migration snapshot capture)
    MUST receive events in the guaranteed sequence.
  """
  risk      high
  verify property "migration_starting always precedes migration_started"
  verify property "migration_started always precedes migration_complete"
  verify property "migration_complete always precedes extension_migration_hooks_complete"
  verify unit "no event emitted before its predecessor completes"
}

invariant migration_semantic_preservation "Migration Semantic Preservation" {
  guarantee """
    Migration transforms MUST preserve the entity graph structure. After
    migration, the set of nodes (entity IDs, and each node's kind, title,
    field values and methods) and edges (reference relationships) MUST be
    identical to the pre-migration graph, source positions aside: the
    graph delta between them is empty, except a title a migration step
    adds to a block that had none or a blank one (add_missing_titles); every other node
    and edge MUST be identical. Only syntax-level changes
    (whitespace, keyword spelling, block ordering) are permitted. Any
    migration that adds, removes, or alters a node or edge is a breaking
    change and MUST be flagged by validate_post_migration_integrity.
  """
  risk      high
  verify property "pre-migration and post-migration entity graphs are structurally identical"
  verify unit "migration that only changes formatting preserves graph structure"
}

invariant migration_cross_extension_stability "Migration Cross-Extension Reference Stability" {
  guarantee """
    Migration MUST preserve cross-extension reference resolution.
    References that resolved before migration MUST resolve identically
    after migration. References to entities in uninstalled extensions
    MUST retain their soft-resolution status (I004).
  """
  risk      medium
  verify integration "cross-extension references resolve identically after migration"
  verify unit "soft-resolution I004 references preserved after migration"
}
