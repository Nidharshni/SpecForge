// Extension authoring workflow events

use "types/wasm"

event extension_project_scaffolded "Extension Project Scaffolded" {
  channel "wasm.extension_project_scaffolded"
  payload ExtensionProjectScaffoldedPayload
  verify integration "emits extension_project_scaffolded with correct projectPath and extensionName"
  verify integration "consumer build_wasm_extension receives event after scaffold"
}

type ExtensionProjectScaffoldedPayload {
  projectPath        string
  extensionName      string
  manifestCreated    boolean
  buildScriptCreated boolean
  verify unit "ExtensionProjectScaffoldedPayload schema is valid"
}

event extension_built "Extension Built" {
  channel "wasm.extension_built"
  payload ExtensionBuiltPayload
  verify integration "emits extension_built with correct wasmSize and buildTimeMs"
  verify integration "consumer validate_wasm_extension_locally receives event after build"
}

type ExtensionBuiltPayload {
  extensionName string
  wasmSize      integer
  buildTimeMs   integer
  success       boolean
  verify unit "ExtensionBuiltPayload schema is valid"
}

event extension_fixtures_validated "Extension Fixtures Validated" {
  channel "wasm.extension_fixtures_validated"
  payload ExtensionFixturesValidatedPayload
  verify integration "emits extension_fixtures_validated with correct passed_fixtures and failed_fixtures"
  verify integration "consumer publish_wasm_extension receives event after fixture validation"
}

type ExtensionFixturesValidatedPayload {
  extensionName      string
  fixtureCount       integer
  passed_fixtures    integer
  failed_fixtures    integer
  validation_time_ms integer
  verify unit "ExtensionFixturesValidatedPayload schema is valid"
}

event extension_published "Extension Published" {
  channel "wasm.extension_published"
  payload ExtensionPublishedPayload
  verify integration "emits extension_published with correct extensionName, version, and registry"
}

type ExtensionPublishedPayload {
  extensionName string
  version       string
  registry      string
  publishTimeMs integer
  verify unit "ExtensionPublishedPayload schema is valid"
}
