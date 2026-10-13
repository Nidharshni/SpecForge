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
  status             TestStatus
}

// The extension protocol's TestStatus (ADR 0069): specforge-test writes
// passed or failed; an #[ignore]d test records nothing.
type TestStatus = passed | failed | skipped

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
