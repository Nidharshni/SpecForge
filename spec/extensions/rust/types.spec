// @specforge/rust extension types — Rust test traceability

type TestGuard "Test Guard" {
  entity_kind string  @readonly
  entity_id   string  @readonly
  test_name   string  @readonly
  module_path string  @readonly
  file        string  @readonly
  line        integer @readonly
  verify unit "TestGuard schema is valid"
}

type TestRegistry "Test Registry" {
  entries EntityMappingEntry[]
  verify unit "TestRegistry schema is valid"
}

type EntityMappingEntry "Entity Mapping Entry" {
  entity_id  string  @readonly
  test_name  string
  file       string
  line       integer @optional
  resolution MappingResolutionLevel
  verify unit "EntityMappingEntry schema is valid"
}

type MappingResolutionLevel "Mapping Resolution Level" = proc_macro | convention

type RustFrameworkSupport "Rust Framework Support" {
  framework RustFramework
  support   RustSupportLevel
  mechanism string
  verify unit "RustFrameworkSupport schema is valid"
}

type RustFramework "Rust Framework" = builtin
  | nextest
  | proptest
  | criterion
  | tokio
  | rstest
  | trybuild

type RustSupportLevel "Rust Support Level" = full | partial | unsupported
