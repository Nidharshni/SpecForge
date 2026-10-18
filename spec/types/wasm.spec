// Wasm component extension runtime types
//
// Extension declarations use ExtensionDeclaration from types/zero-entity-core.spec.
// This file contains supporting types for the Wasm runtime: dependencies,
// host function bindings, sandbox policies, caching, enhancements, and queries.

use "types/core"

type PeerDependency "Peer Dependency" {
  name     string
  // A semver range the installed version must satisfy
  version  string
  // An optional peer that is not installed is fine
  optional boolean @optional
  verify unit "PeerDependency schema is valid"
}

type HostFunctionBinding "Host Function Binding" {
  name          string @readonly
  input_schema  string
  output_schema string
  verify unit "HostFunctionBinding schema is valid"
}

type SandboxPolicy "The limits an extension asks its sandbox to hold it to (ADR 0037). The host grants a component no capability, so the policy declares limits only, each held to the host's ceiling (30000 ms, 512 MB); an undeclared limit is the ceiling. The permission settings the planned host functions would read (allowed paths, domains, output extensions, file and HTTP limits) come with that surface (behaviors/wasm-host-functions.spec)." {
  // Wall-clock budget of one call, in milliseconds
  max_execution_ms integer @optional
  // Ceiling of the instance's linear memory, in MiB
  max_memory_mb    integer @optional
  verify unit "SandboxPolicy schema is valid"
}

// ── Entity Enhancement Types ─────────────────────────────────

type DynamicEdgeType "Dynamic Edge Type" {
  label            string  @readonly
  source_extension string  @readonly
  soft             boolean @optional
  verify unit "DynamicEdgeType schema is valid"
}

// ── Extension Lifecycle Types ─────────────────────────────────

type ExtensionInstallResult "Extension Install Result" {
  extension_name string @readonly
  version        string @readonly
  source         ExtensionSource
  wasm_size      integer
  installed_path string
  verify unit "ExtensionInstallResult schema is valid"
}

type ExtensionSource "Extension Source" = builtin | registry | local | git

type WasmTrapInfo "Wasm Trap Info" {
  kind           string @readonly
  message        string @readonly
  export_name    string @optional
  memory_address string @optional
  extension_name string
  verify unit "WasmTrapInfo schema is valid"
}

// ── Lock File Types ──────────────────────────────────────────

type LockFileEntry "Lock File Entry" {
  extension_name string @readonly
  version        string @readonly
  source         ExtensionSource
  wasm_hash      string @readonly
  resolved_at    string
  key_id         string @optional
  verify unit "LockFileEntry schema is valid"
}

type LockFile "Serialized as JSON (specforge.lock). See P6: standard is the moat." {
  path             string  @readonly
  lockfile_version integer @readonly
  entries          LockFileEntry[]
  verify unit "LockFile schema is valid"
}

// ── Collector Contribution Types ────────────────────────────

type RunnerOutput "Where a collector command's own output goes (ADR 0125)." = inherit
  | stderr
  | discard

type CollectorCommand "One collector command to run to its end (port CollectorMachine)." {
  argv   string[]
  dir    string
  report string
  output RunnerOutput
  verify unit "CollectorCommand schema is valid"
}

type CollectorRun "What running it produced: its exit code and a start time no later than any file it wrote." {
  exit_code integer @optional
  started   string
  verify unit "CollectorRun schema is valid"
}

type CollectorRunFailure "Why a command produced no run: it did not start, or its output could not be captured." = not_run
  | capture

type CommandApproval "One approval: this project may run this extension's collector with this command." {
  project   string
  extension string
  collector string
  run       string[]
  verify unit "CommandApproval schema is valid"
}

type CollectorDescriptor "Collector Descriptor" {
  name          string              @readonly
  input_formats string[]
  auto_detect   CollectorAutoDetect @optional
  export        string
  run           string[]            @optional
  report        string              @optional
  capture       string              @optional
  verify unit "CollectorDescriptor schema is valid"
}

type CollectorAutoDetect "Collector Auto Detect" {
  file_patterns string[]
  env_vars      string[] @optional
  verify unit "CollectorAutoDetect schema is valid"
}

type TestStatus "What a run says of one test: the one status vocabulary of the collect wire, specforge-report.json and a pass's test results (ADR 0069). It is decoded strictly: any other value from a collector is E028, in a report E045." = passed
  | failed
  | skipped

// What an extension package is called: @scope/name, or name alone for a local
// module. Each part is a-z 0-9 . _ - and starts with a letter or digit, so a
// name is always a relative path inside the directory it is joined to (ADR 0036).
type PackageName "Package Name" {
  scope string @optional
  base  string
  verify unit "a package name is @scope/name or a local name, and always a relative path inside its directory"
  verify unit "a package name crosses a registry URL as one segment"
}

type VersionRequirementKind "Version Requirement Kind" = latest | exact | range

// Which version of a package is asked for: latest (or *), one full version,
// or a SemVer requirement read as Cargo reads one (ADR 0036).
type VersionRequirement "Version Requirement" {
  kind VersionRequirementKind
  text string
  verify unit "a version requirement is latest, one version or a SemVer requirement"
  verify unit "one rule picks the version a requirement asks for"
}

type ExtensionSpecifier "Parsed Extension Specifier" {
  raw         string
  format      ExtensionSource
  builtin     string             @optional
  name        PackageName        @optional
  requirement VersionRequirement @optional
  path        string             @optional
  git_url     string             @optional
  verify unit "Parsed Extension Specifier conforms to schema"
}

// ── Extension Call Payloads ─────────────────────────────────
// What the host sends each export it calls on a loaded extension, and
// what the export answers: one specforge_protocol_types type each, shared
// by the host and the SDK (ADR 0013). An optional field the host leaves
// unset is absent from the wire, never null.

type CollectInput "What a collect__<name> export receives: the runner's report files and, when the collector captures it, the command's standard output." {
  reports CollectReportFile[]
  stdout  string @optional
  verify unit "CollectInput schema is valid"
}

type CollectReportFile "Collect Report File" {
  path    string
  content string
  verify unit "CollectReportFile schema is valid"
}

type CollectOutput "What a collect__<name> export answers: tests grouped by the entity each proves, and the tests the report links to none (the host links them by naming convention)." {
  entity_results CollectEntityResult[]
  unlinked       CollectUnlinkedTest[] @optional
  verify unit "CollectOutput schema is valid"
}

type CollectEntityResult "Collect Entity Result" {
  entity_id    string
  test_results TestResult[]
  verify unit "CollectEntityResult schema is valid"
}

type TestResult "One test: as a collector answers it, as specforge-report.json keeps it (with the runner that recorded it) and as a pass reads it." {
  name        string
  status      TestStatus
  verify      string @optional
  duration_ms float  @optional
  verify unit "TestResult schema is valid"
}

type CollectUnlinkedTest "Collect Unlinked Test" {
  name   string
  // The name's path segments, the test's own name last.
  path   string[]
  status TestStatus
  verify unit "CollectUnlinkedTest schema is valid"
}

type PassInput "What a __pass_<name> export receives: the entity snapshot, the resolved references, and the recorded test results, the claims the prove pass entailed and (check-phase passes) the previous build's statuses when the host has them." {
  entities      PassEntity[]
  edges         PassEdge[]
  // The recorded test report, without each test's runner.
  test_results  TestResults @optional
  proved_claims string[]    @optional
  // The build cache's statuses: per entity id, its kind and status.
  previous      object      @optional
  verify unit "PassInput schema is valid"
}

type TestResults "The recorded tests a pass reads: results maps each entity id to {tests: TestResult[]}; runner names the collectors, joined \", \"." {
  runner  string @optional
  results object
  verify unit "TestResults schema is valid"
}

type PassEntity "One entity of the snapshot. testable: its kind's flag; exempt: it owes no obligations of its own (ADR 0004, D2-b): decided by the host's one obligation rule (ADR 0019)." {
  id                  string
  kind                string
  // Every field it writes, by name, as its field text (ADR 0019); a name written twice keeps its last text.
  fields              object
  incoming_edge_count integer
  outgoing_edge_count integer
  span                SourceSpan @optional
  testable            bool
  exempt              bool
  verify_kinds        string[]
  verify_texts        string[]
  verify unit "PassEntity schema is valid"
}

type PassEdge "Pass Edge" {
  source string
  target string
  label  string
  verify unit "PassEdge schema is valid"
}

type PassDiagnostic "A diagnostic a pass reports. Naming an entity and carrying no span, it gets that entity's span from the host." {
  code       string
  severity   Severity
  message    string
  span       SourceSpan @optional
  suggestion string     @optional
  entity     string     @optional
  verify unit "PassDiagnostic schema is valid"
}

type PassOutput "What a pass answers: its diagnostics bare (an array), or with a summary whose keys join the host's report of the pass (an object)." {
  diagnostics PassDiagnostic[]
  summary     object @optional
  verify unit "PassOutput schema is valid"
}

type ValidatorContext "What a custom rule's wasm_function receives for one entity: the entity, the resolution of its references, the declared types and the host's primitive types. The entity's field values are their field texts (ADR 0019), always strings." {
  entity         object
  referenced     object[]
  declared_types string[]
  primitives     string[]
  verify unit "ValidatorContext schema is valid"
}

type ValidatorVerdict "{\"verdict\": \"pass\"} or {\"verdict\": \"fail\", \"field\"?, \"value\"?}." {
  verdict ValidatorVerdictKind
  field   string @optional
  value   string @optional
  verify unit "ValidatorVerdict schema is valid"
}

type ValidatorVerdictKind "Validator Verdict Kind" = pass | fail

type ScanRequest "What an analyzer's scan export receives: one source file." {
  file_path string
  content   string
  verify unit "ScanRequest schema is valid"
}

type ScanResponse "The public items the scan found." {
  items    ScannedItem[]
  language string @optional
  verify unit "ScanResponse schema is valid"
}

type ScannedItem "Scanned Item" {
  name       string
  item_kind  string
  line       integer
  visibility string @optional
  signature  string @optional
  verify unit "ScannedItem schema is valid"
}

type McpResourceRequest "What an MCP resource's export receives, and answers." {
  uri string
  verify unit "McpResourceRequest schema is valid"
}

type McpResourceContent "Mcp Resource Content" {
  content   string
  mime_type string
  verify unit "McpResourceContent schema is valid"
}

type MigrationInput "What the migration hook receives after specforge migrate rewrote the project's files. Its answer is not read." {
  from  string
  to    string
  files string[]
  verify unit "MigrationInput schema is valid"
}
