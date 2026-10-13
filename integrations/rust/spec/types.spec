// specforge-test crate types
// These types define the runtime data structures for the specforge-test
// and specforge-test-macros crates. They have ZERO dependency on the
// SpecForge compiler — the only coupling is the specforge-report.json schema.

type TestGuard {
  entity_kind string  @readonly
  entity_id   string  @readonly
  module_path string  @readonly
  test_name   string  @readonly
  file        string  @readonly
  line        integer @readonly
}

type TestRecordEntry {
  entity_kind        string  @readonly
  entity_id          string  @readonly
  test_name          string  @readonly
  module_path        string  @optional
  file               string  @readonly
  // serialized as `verify` (reserved statement keyword in .spec files)
  verify_description string  @optional
  duration_ms        integer @readonly
  status             TestOutcome
}

type TestOutcome = pass | fail | skipped

type EntityMappingEntry {
  entity_kind string
  entity_id   string
  test_name   string
  file        string
  line        integer
  resolution  MappingResolutionLevel
}

type MappingResolutionLevel = proc_macro | convention

// Registry holds all collected test results for a single binary.
// Written to target/specforge/<binary-name>.json at process exit.
type BinaryReport {
  schema_version string @readonly
  binary_name    string @readonly
  entries        TestRecordEntry[]
}

// The graph export that build.rs fetches from specforge.
// This is a SUBSET of the full graph — only what the crate needs.
type GraphExport {
  entities  ExportedEntity[]
  // Older exports stamped the build time; current ones don't.
  timestamp string @optional
}

// `obligations` is serialized as `verify` (a reserved statement keyword in
// .spec files, so it can't be a field name here).
type ExportedEntity {
  id          string   @readonly
  kind        string   @readonly
  obligations ExportedVerify[]
  testable    boolean  @readonly
  gherkin     string[] @optional
}

type ExportedVerify {
  kind        string @readonly
  description string @readonly
  slug        string @readonly
}

// Coverage diff computed at test exit (Phase 3).
type CoverageDiff {
  entity_id   string @readonly
  entity_kind string @readonly
  expected    integer
  covered     integer
  passing     integer
  status      CoverageDiffStatus
}

type CoverageDiffStatus = fully_covered
  | covered_with_failures
  | partially_covered
  | uncovered
  | no_intent
