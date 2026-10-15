// Migration types — data shapes for spec file format migration

use "types/graph"

// FormatVersion uses major.minor (no patch) because .spec file format changes
// are always intentional and binary-compatible or not. SchemaVersion (in
// types/graph.spec) uses semver with patch for the Graph Protocol's more
// granular compatibility needs.
type FormatVersion {
  major integer @readonly
  minor integer @readonly
  verify unit "FormatVersion schema is valid"
}

type MigrationDiff {
  /// The file, named from the project root (ADR 0102).
  file_path      string      @readonly
  before_hash    string      @readonly
  after_hash     string      @readonly
  changed_ranges integer[][] @optional
  unified_text   string      @optional
  verify unit "MigrationDiff schema is valid"
}

type MigrationResult {
  /// The file, named from the project root.
  file_path      string        @readonly
  source_version FormatVersion @readonly
  target_version FormatVersion @readonly
  success        boolean
  error          string        @optional
  diff           MigrationDiff @optional
  backup_path    string        @optional
  verify unit "MigrationResult schema is valid"
}

type MigrationBackup {
  /// The file the backup holds the old text of, named from the project root.
  original_path string @readonly
  /// The backup, named from the project root.
  backup_path   string @readonly
  content_hash  string @readonly
  verify unit "MigrationBackup schema is valid"
}

type MigrationSummary {
  migrated_count integer           @readonly
  failed_count   integer           @readonly
  skipped_count  integer           @readonly
  results        MigrationResult[]
  backups        MigrationBackup[] @optional
  verify unit "MigrationSummary schema is valid"
}

type RollbackSummary "What a migration rollback restored" {
  restored_count integer
  skipped_count  integer
  failed_count   integer
  results        MigrationResult[]
  /// One warning per file skipped because its .bak backup is missing.
  warnings       string[]
  verify unit "RollbackSummary schema is valid"
}
