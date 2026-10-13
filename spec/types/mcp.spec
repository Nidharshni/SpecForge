// MCP Server types — Model Context Protocol descriptors and response shapes

use "types/core"
use "types/diagnostics"
use "types/formatting"
use "types/migration"

/// -32002: resource not found, in the handshake revisions (2025-03-26 to
/// 2025-11-25); the 2026-07-28 revision answers -32602.
type JsonRpcErrorCode = -32700 | -32600 | -32601 | -32602 | -32603 | -32002

type McpErrorCode = "invalid_input"
  | "compilation_failed"
  | "entity_not_found"
  | "file_not_found"
  | "extension_not_found"
  | "permission_denied"
  | "timeout"
  | "not_initialized"
  | "schema_mismatch"
  | "internal_error"
  | "conflict"
  | "precondition_failed"

type JsonSchema "JSON Schema Object" {
  type        string
  properties  object   @optional
  required    string[] @optional
  description string   @optional
  verify unit "JSON Schema Object conforms to schema"
}

// A refusal: the content of a failed tool call's isError result (MCP
// 2025-11-25, SEP-1303), and the data of a prompt's or a resource read's
// JSON-RPC error. One definition serializes it everywhere. It carries no
// timestamp: the same failing call returns the same error
// (mcp_tool_idempotency).
type McpError "MCP Structured Error Response" {
  code       McpErrorCode
  message    string
  entity_id  string     @optional
  /// The project file the failure is about (file_not_found).
  file       string     @optional
  tool       string     @optional
  /// The prompt that refused, for a prompts/get answered with an error.
  prompt     string     @optional
  /// The URI of the resource whose read failed (the URI read).
  uri        string     @optional
  /// The argument the tool could not use, for invalid_input.
  argument   string     @optional
  /// The diagnostic behind the failure: its code (E003, E059, ...) is
  /// diagnostic.code, never only text in the message.
  diagnostic Diagnostic @optional
  /// More about the failure: the renderers available, a failed run's report.
  /// An open value: its shape is the failure's own.
  data       object     @optional
  verify unit "MCP Structured Error Response conforms to schema"
}

// What initialize answers (MCP's InitializeResult, 2025-03-26 to
// 2025-11-25), under MCP's wire names. Tools, resources and prompts are
// discovered by their list methods, never by initialize.
type McpInitializeResult "MCP Initialize Result" {
  protocolVersion string
  capabilities    McpServerCapabilities
  serverInfo      McpServerInfo
  /// What the server tells the client's model about itself, naming tools as
  /// tools/list names them; server/discover gives the same text.
  instructions    string
  verify unit "McpInitializeResult schema is valid"
}

// What server/discover answers (MCP 2026-07-28), beside the revision's
// resultType and _meta.
type McpDiscoverResult "MCP Discover Result" {
  supportedVersions string[]
  capabilities      McpServerCapabilities
  instructions      string
  verify unit "McpDiscoverResult schema is valid"
}

type McpServerCapabilities {
  tools     McpListChanged
  resources McpResourcesCapability
  prompts   McpListChanged
  verify unit "McpServerCapabilities schema is valid"
}

type McpListChanged {
  listChanged boolean
  verify unit "McpListChanged schema is valid"
}

type McpResourcesCapability {
  subscribe   boolean
  listChanged boolean
  verify unit "McpResourcesCapability schema is valid"
}

type McpServerInfo {
  name    string @readonly
  version string @readonly
  verify unit "McpServerInfo schema is valid"
}

// A resource as resources/list lists it (MCP Resource).
type McpResourceDescriptor {
  uri         string @readonly
  name        string
  description string @optional
  mimeType    string
  verify unit "McpResourceDescriptor schema is valid"
}

// A templated resource as resources/templates/list lists it (MCP
// ResourceTemplate).
type McpResourceTemplateDescriptor {
  uriTemplate string @readonly
  name        string
  description string @optional
  mimeType    string
  verify unit "McpResourceTemplateDescriptor schema is valid"
}

// A tool as tools/list lists it (MCP Tool, with SpecForge's category and
// source). inputSchema and outputSchema are JSON Schema documents: open
// values. outputSchema is listed from 2025-06-18, for a tool with one.
type McpToolDescriptor {
  name         string @readonly
  description  string
  inputSchema  object
  outputSchema object @optional
  /// The tool's role, never where it comes from.
  category     McpToolCategory
  /// "core" for built-in tools, the extension's name for contributed tools.
  source       string
  /// MCP ToolAnnotations: what the tool does to its environment.
  annotations  McpToolAnnotations
  verify unit "McpToolDescriptor schema is valid"
}

// MCP's ToolAnnotations. A core tool's derive from what its tool spec
// declares it does: a read is readOnlyHint and not openWorldHint; a tool
// that writes states all four. An extension tool's say it only reads,
// since the host grants an extension no capability.
type McpToolAnnotations {
  readOnlyHint    boolean
  destructiveHint boolean @optional
  idempotentHint  boolean @optional
  openWorldHint   boolean
  verify unit "McpToolAnnotations schema is valid"
}

// Tool categories: "management" corresponds to the "Project Management Tools"
// feature group (specforge.extensions, specforge.providers, specforge.doctor,
// specforge.collect, specforge.render). The short name is used in the protocol
// for brevity; UIs and documentation may display "project management".
type McpToolCategory = "core" | "navigation" | "mutation" | "management"

type McpPromptDescriptor {
  name        string @readonly
  description string
  arguments   McpPromptArgument[]
  verify unit "McpPromptDescriptor schema is valid"
}

type McpPromptArgument {
  name        string @readonly
  description string
  required    boolean
  verify unit "McpPromptArgument schema is valid"
}

// ── Core tool replies ─────────────────────────────────────────────────────
// Each core tool's reply as its structuredContent and outputSchema carry it
// (ADRs 0048, 0051): a domain type a reply embeds from another spec file
// (Diagnostic, MigrationResult, MigrationDiff, RollbackSummary) is one
// $defs entry the reply's fields refer to; an optional field is absent,
// never null; a mutation's reply
// also carries files_written (string[], absent from a preview), which the
// mutation pipeline adds to every mutation tool, and rename's diagnostics,
// which it adds after a successful rename (ADR 0022). A test holds each
// type below to the schema its tool's reply type derives. Where the CLI
// prints the same operation's outcome under --format json, it prints the same
// document, with only its own keys added (ADR 0052): McpStatsResult,
// McpAddExtensionResult, McpRemoveExtensionResult, McpInitResult,
// McpExtensionsResult, McpMigrateResult, McpAnalyzeResult, McpCollectResult,
// McpProvidersResult, McpDoctorReport, McpInferProgressResult,
// McpInferGapsResult and McpTraceChainResult.

type McpInspectResult {
  entity_id           string     @readonly
  kind                string     @readonly
  /// Absent when the entity declares no title.
  title               string     @optional
  /// The extension that declares the entity's kind; absent when none does.
  source_extension    string     @optional
  /// The entity's kind is testable, the standing the LSP hover shows.
  testable            boolean
  /// The entity declares at least one verify obligation.
  declared            boolean
  /// Testable, but it owes no obligations and declares none: it does not
  /// count toward coverage (specforge.coverage's row says the same).
  exempt              boolean
  /// Its kind must declare obligations (a no_verify_statements rule targets
  /// it): why an exempt entity is exempt.
  obligated           boolean
  source_span         SourceSpan @readonly
  /// The field the entity's extension declares headline and normative (a
  /// behavior's contract); absent when its kind declares none.
  contract            string     @optional
  /// Every field the entity declares, by name.
  fields              object
  /// The entities that reference this one (incoming), distinct, sorted.
  referenced_by       string[]
  /// The entities this one references (outgoing), distinct, sorted.
  refers_to           string[]
  /// Its verify texts; absent when it declares none.
  verify_declarations string[]   @optional
  coverage_status     CoverageStatus
  /// The diagnostics about the entity, as specforge.validate reports them.
  diagnostics         Diagnostic[]
  verify unit "McpInspectResult schema is valid"
}

type McpDefinitionResult {
  entity_id   string     @readonly
  file_path   string     @readonly
  /// The position of the entity's name.
  line        integer    @readonly
  column      integer    @readonly
  /// The entity's block.
  source_span SourceSpan @readonly
  /// The entity's name as written; its block when the name could not be read.
  name_span   SourceSpan @readonly
  /// "token" when name_span is the name as written, else "entity".
  precision   "token" | "entity"
  verify unit "McpDefinitionResult schema is valid"
}

type McpReferenceLocation {
  referencing_entity_id string     @readonly
  referenced_entity_id  string     @readonly
  /// The field naming the referenced entity; absent for its declaration.
  field                 string     @optional
  role                  "declaration" | "reference"
  /// "token" when source_span is the identifier as written, else "entity"
  /// (the text was unreadable or stale: the referencing entity's block).
  precision             "token" | "entity"
  source_span           SourceSpan @readonly
  verify unit "McpReferenceLocation schema is valid"
}

type McpReferenceResult {
  entity_id string @readonly
  /// "incoming", "outgoing" or "both" (navigate::DIRECTION).
  direction "incoming" | "outgoing" | "both"
  locations McpReferenceLocation[]
  verify unit "McpReferenceResult schema is valid"
}

type McpOutlineResult {
  /// The file's entities in line order.
  entries McpOutlineEntry[]
  verify unit "McpOutlineResult schema is valid"
}

type McpOutlineEntry {
  entity_id  string            @readonly
  kind       string            @readonly
  /// Absent when the entity declares no title.
  title      string            @optional
  /// The entity's block.
  range      SourceSpan        @readonly
  /// Its name as written: what an editor selects.
  name_range SourceSpan        @readonly
  /// Its method members; absent when it has none.
  children   McpOutlineChild[] @optional
  verify unit "McpOutlineEntry schema is valid"
}

type McpOutlineChild "One method member of an outlined entity" {
  /// <entity>.<method>.
  entity_id  string     @readonly
  kind       "method"   @literal
  /// The method's signature.
  title      string
  range      SourceSpan @readonly
  name_range SourceSpan @readonly
  verify unit "McpOutlineChild schema is valid"
}

type McpFixSuggestions {
  fixes McpFixSuggestion[]
  verify unit "McpFixSuggestions schema is valid"
}

type McpFixSuggestion {
  /// The LSP code action's title, e.g. "Replace with 'session_limit'".
  title           string
  kind            "quickfix" | "refactor"
  /// The code of the diagnostic the fix resolves; absent for a refactoring.
  diagnostic_code string @optional
  /// The entity the fix is about.
  entity_id       string @optional
  /// What applying the fix changes.
  edits           TextEdit[]
  verify unit "McpFixSuggestion schema is valid"
}

type McpStatsResult {
  /// Every entity of the graph.
  entity_count       integer
  /// The entities of each kind, by kind.
  entity_counts      McpEntityCount[]
  edge_count         integer
  /// Entities no edge links to another entity.
  unconnected_count  integer
  /// The entities that count toward coverage.
  testable_count     integer
  /// Testable entities with at least one verify statement.
  declared_count     integer
  /// Entities of any kind with at least one verify statement.
  verified_count     integer
  /// declared_count over testable_count, in percent (0 when nothing is testable).
  declared_pct       float
  /// Testable entities proven, in percent; absent without recorded test results.
  proof_pct          float @optional
  diagnostic_summary McpDiagnosticSummary
  verify unit "McpStatsResult schema is valid"
}

type McpEntityCount {
  kind  string @readonly
  count integer
  verify unit "McpEntityCount schema is valid"
}

type McpDiagnosticSummary {
  errors   integer
  warnings integer
  infos    integer
  verify unit "McpDiagnosticSummary schema is valid"
}

type McpListResult {
  /// The entities, sorted by id and paged.
  entities McpListedEntity[]
  verify unit "McpListResult schema is valid"
}

type McpListedEntity {
  id    string @readonly
  kind  string @readonly
  /// The entity's title; empty when it has none.
  title string
  verify unit "McpListedEntity schema is valid"
}

type McpExtensionsResult {
  extensions            McpExtensionInfo[]
  /// How many extensions are listed.
  count                 integer
  lock_file_entries     McpLockEntry[]
  entity_kinds_in_graph string[]
  verify unit "McpExtensionsResult schema is valid"
}

type McpLockEntry {
  name    string @readonly
  version string
  verify unit "McpLockEntry schema is valid"
}

type McpExtensionInfo {
  name             string @readonly
  /// The loaded version; else an installed extension's locked one; else the
  /// version a legacy name@version entry writes; absent otherwise.
  version          string @optional
  /// "builtin", the lock entry's source ("registry", "local:<path>"), or
  /// "file:<path>" for a .wasm file entry of specforge.json.
  source           string @readonly
  /// The entity kinds the extension contributes.
  entity_kinds     string[]
  /// The project's entities of those kinds.
  entity_count     integer
  validation_rules integer
  /// "loaded"; "not_loaded" (enabled, but not installed or it failed to
  /// load); "not_configured" (installed, but not enabled).
  status           "loaded" | "not_loaded" | "not_configured"
  verify unit "McpExtensionInfo schema is valid"
}

type McpProvidersResult {
  providers   McpProviderInfo[]
  count       integer
  /// W118 (a malformed entry, or an extension that is not loaded or
  /// contributes no providers) and E057 (a scheme declared twice).
  diagnostics Diagnostic[]
  verify unit "McpProvidersResult schema is valid"
}

type McpProviderInfo {
  scheme    string @readonly
  alias     string
  extension string @readonly
  status    "registered" | "extension_not_loaded" | "not_a_provider" | "scheme_taken"
  verify unit "McpProviderInfo schema is valid"
}

type McpDoctorFinding {
  /// What the finding is about: config, lock, binary, load, structural_only, providers,
  /// declaration, peer, registry, rule, surface, conflict, shadowing or toolchain.
  about       "config" | "lock" | "binary" | "load" | "structural_only" | "providers" | "declaration" | "peer" | "registry" | "rule" | "surface" | "conflict" | "shadowing" | "toolchain"
  check       string
  /// The diagnostic's severity; the disk checks' own are error or warn.
  status      "error" | "warn" | "info"
  code        string
  remediation string
  /// A binary finding's issue (missing_binary or stale_hash and its fields).
  issue       object @optional
  /// The keyword a shadowing finding (E013) or a kind conflict (E026) names.
  keyword     string @optional
  verify unit "McpDoctorFinding schema is valid"
}

type McpDoctorExtension {
  name              string @readonly
  /// The version the extensions listing gives it; absent when it has none.
  version           string @optional
  /// Where it comes from: builtin, the lock entry's source, file:<path>, or unknown.
  source            string
  /// "loaded"; "not_loaded" (enabled, but not installed or it failed to
  /// load); "not_configured" (installed, but not enabled).
  status            "loaded" | "not_loaded" | "not_configured"
  enhancement_count integer
  verify unit "McpDoctorExtension schema is valid"
}

type McpDoctorReport {
  /// The report's verdict: no error-level finding (specforge doctor exits 1 without it).
  ok              boolean
  /// No error-level finding about an installed binary, an extension load or a peer requirement.
  extensions_ok   boolean
  /// The check of each conflict finding: two contributions claiming one name (E026, W018, W023, E039, E057).
  conflicts       string[]
  /// "stale" when an installed binary is missing or its hash drifted.
  cache_status    "ok" | "stale"
  installed_count integer
  /// Whether the SMT solver z3 runs here: false exactly when a toolchain finding (W098) says why not.
  z3_available    boolean
  extensions      McpDoctorExtension[]
  /// Entity enhancements keyed by the entity kind they target.
  enhancements    object
  findings        McpDoctorFinding[]
  verify unit "McpDoctorReport schema is valid"
}

type McpInitResult {
  /// The new project's root, absolute.
  project_root         string @readonly
  name                 string
  version              string
  /// specforge.json, relative to project_root.
  config_file          string @readonly
  /// The starter spec file, relative to project_root.
  starter_file         string @readonly
  /// The extensions enabled: builtins by name, local installs by the name they declare.
  extensions_installed string[]
  verify unit "McpInitResult schema is valid"
}

type McpFormatResult {
  changed_files string[]
  total_checked integer
  /// The verdict specforge format exits by (0 when true): every file read and
  /// written, no region left unformatted, specforge.json used as written (no
  /// E069), and under check no file that would change.
  ok            boolean
  /// True only when every file was read and is in canonical form: no change,
  /// no failure, no region left unformatted.
  all_clean     boolean
  /// The call did not write (check or diff).
  check_only    boolean
  /// E069 per problem in a specforge.json the run went around, then W141 per
  /// configuration file used, W142 per region kept verbatim.
  diagnostics   Diagnostic[]
  /// The files that would change, before and after.
  diffs         FormatDiff[]       @optional
  /// Why the call failed; a failed call's data holds this reply.
  message       string             @optional
  /// The files the call could not read or write (the call failed).
  failed_files  string[]           @optional
  failures      McpFormatFailure[] @optional
  verify unit "McpFormatResult schema is valid"
}

type McpFormatFailure "One file a format could not read or write" {
  file      string @readonly
  operation string
  /// The McpErrorCode of the file's failure.
  code      string
  message   string
  verify unit "McpFormatFailure schema is valid"
}

type McpSearchResults {
  /// The hits, best first.
  results McpSearchResult[]
  verify unit "McpSearchResults schema is valid"
}

type McpSearchResult {
  entity_id     string  @readonly
  kind          string  @readonly
  /// Absent when the entity declares no title.
  title         string  @optional
  file_path     string  @readonly
  line          integer @readonly
  /// The rank band: 1.0 exact, 0.9 prefix, 0.8 substring, 0.7 field text,
  /// 0.6 × similarity for a fuzzy match.
  score         float
  /// What matched: "id", "title" or a string field's name; absent for an
  /// empty query.
  match_field   string  @optional
  /// For a field-text match, the field's text around the match.
  match_snippet string  @optional
  verify unit "McpSearchResult schema is valid"
}

type CoverageStatus = "covered" | "uncovered" | "partial"

// P2 compliance: CoverageStatus is a structural computation, not domain vocabulary.
// An obligation is proven when a passing recorded test names its verify text,
// the same rule `analyze coverage` applies (A015), computable without extension input:
//   covered   = every obligation proven AND no recorded test fails
//   partial   = not covered, AND some obligation proven OR a recorded test fails
//   uncovered = no obligation proven and no recorded test fails (incl. no verify declarations)
// Extensions may overlay domain-specific labels (pass/fail, conformant/non-conformant)
// via extension-contributed metadata fields on their entity kinds.
type McpCoverageResults {
  /// One row per entity that counts toward coverage (or the one named).
  entities McpCoverageResult[]
  verify unit "McpCoverageResults schema is valid"
}

type McpCoverageResult {
  entity_id          string @readonly
  kind               string @readonly
  status             CoverageStatus
  declared           boolean
  linked             boolean
  /// Whether evidence has been collected from an external report (test results, audit findings, review logs).
  evidence_collected boolean
  /// The entity's verify obligations.
  obligations        integer
  /// Obligations a passing recorded test names.
  proven             integer
  /// Verify texts no passing recorded test names, in declaration order.
  unproven           string[]
  /// A testable-kind entity that owes no obligations and declares none (W004 exempts it).
  exempt             boolean
  verify unit "McpCoverageResult schema is valid"
}

type McpRenameResult {
  old_name       string       @readonly
  new_name       string       @readonly
  /// The files the rename edits, relative to the spec root.
  affected_files string[]
  edits          McpRenameEdit[]
  /// Present (true) for a preview.
  dry_run        boolean      @optional
  /// What specforge check reports for the project once the edits are made;
  /// absent from a preview.
  diagnostics    Diagnostic[] @optional
  verify unit "McpRenameResult schema is valid"
}

type McpRenameEdit "One occurrence of the old identifier a rename replaces" {
  /// The file, relative to the spec root.
  file      string @readonly
  /// 1-based line.
  line      integer
  /// Byte columns of the occurrence on its line.
  start_col integer
  end_col   integer
  new_text  string
  verify unit "McpRenameEdit schema is valid"
}

type McpTraceResult = McpTraceChainResult | McpTracePlanResult

type McpTraceChainResult "Trace tool response when entity_id is provided" {
  schema_version string
  entity_id      string @readonly
  entity_kind    string @readonly
  upstream       McpTraceLink[]
  downstream     McpTraceLink[]
  /// Expected edges the traced entity does not have.
  missing        McpMissingLink[]
  verify unit "Trace tool response when entity_id is provided conforms to schema"
}

type McpTraceLink {
  entity_id   string @readonly
  entity_kind string @readonly
  edge_label  string
  depth       integer
  status      "resolved" | "missing"
  verify unit "McpTraceLink schema is valid"
}

type McpMissingLink "An edge the registries lead an entity to have, which the graph does not instantiate" {
  from          string @readonly
  from_kind     string @readonly
  /// The field that would declare the edge.
  edge_label    string
  /// The registered edge type the field instantiates; absent when it names none.
  edge_type     string @optional
  expected_kind string
  depth         integer
  required      boolean
  status        "resolved" | "missing"
  verify unit "McpMissingLink schema is valid"
}

type McpTracePlanResult "Trace tool response when plan parameter is provided" {
  /// Plan entries that name an entity in the graph, in plan order.
  affected_entities string[]
  /// Traceability gaps between plan items and the graph.
  gaps              McpTraceGap[]
  verify unit "Trace tool response when plan parameter is provided conforms to schema"
}

type McpAnalyzeResult {
  /// The run's verdict is passed.
  ok            boolean
  passes        McpAnalyzePass[]
  /// Where the proof-coverage gate landed; absent without min.
  gate          McpAnalyzeGate   @optional
  /// Stray test records; absent when there are none.
  stray_records McpStrayRecord[] @optional
  verify unit "McpAnalyzeResult schema is valid"
}

type McpAnalyzePass {
  pass     string @readonly
  findings Diagnostic[]
  /// What the pass summarizes: the pass's own.
  summary  object
  verify unit "McpAnalyzePass schema is valid"
}

type McpAnalyzeGate = McpGateMet | McpGateBelow | McpGateUnjudged

type McpGateMet {
  status "met" @literal
  min    float
  pct    float
  proven integer
  total  integer
  verify unit "McpGateMet schema is valid"
}

type McpGateBelow {
  status "below" @literal
  min    float
  pct    float
  proven integer
  total  integer
  verify unit "McpGateBelow schema is valid"
}

type McpGateUnjudged {
  status "unjudged" @literal
  min    float
  reason string
  verify unit "McpGateUnjudged schema is valid"
}

type McpStrayRecord "A recorded test naming an entity the graph does not have (W097)" {
  entity_id string @readonly
  /// The closest known id; absent when none is near.
  near      string @optional
  verify unit "McpStrayRecord schema is valid"
}

type McpExplainResult = McpExplainEntry | McpExplainRetired

type McpExplainEntry {
  code        string @readonly
  title       string
  owner       string
  level       string
  explanation string
  /// Absent when the code has no docs page.
  docs        string @optional
  retired     boolean
  verify unit "McpExplainEntry schema is valid"
}

type McpExplainRetired {
  code        string                @readonly
  retired     boolean
  /// The entry that replaced the code; absent when none did.
  replaced_by McpExplainReplacement @optional
  verify unit "McpExplainRetired schema is valid"
}

type McpExplainReplacement {
  code        string @readonly
  title       string
  owner       string
  level       string
  explanation string
  docs        string @optional
  verify unit "McpExplainReplacement schema is valid"
}

type McpCollectResult {
  /// "collected".
  status      string @readonly
  /// One entry per runner whose results were collected.
  runners     McpCollectRunner[]
  /// What collecting reported (W115: a test names an unknown entity).
  diagnostics Diagnostic[]
  /// The path of the written specforge-report.json.
  report      string @readonly
  verify unit "McpCollectResult schema is valid"
}

type McpCollectRunner "What one runner's collection found" {
  /// The collector's name, e.g. cargo-test.
  name          string  @readonly
  /// The extension contributing the collector.
  extension     string  @readonly
  /// Whether the runner's command ran (run=true), or its report was read.
  ran           boolean
  exit_code     integer @optional
  /// Report files read.
  files         integer
  /// Entities the results name.
  entities      integer
  passed        integer
  failed        integer
  skipped       integer
  /// Tests linked by naming convention rather than by the report.
  by_convention integer
  verify unit "McpCollectRunner schema is valid"
}

type McpRenderResult {
  format       "graph" | "context" | "brief" | "dot" @readonly
  /// The rendering itself, when no out_dir was given.
  output       string                                @optional
  /// The files written into out_dir, absolute.
  output_files string[]
  verify unit "McpRenderResult schema is valid"
}

type McpAddExtensionResult "What an add did for each of its specifiers" {
  /// One entry per specifier, in the order given.
  added McpAddedExtension[]
  /// Whether the server serves the extensions from the next call on;
  /// absent for a preview (MCP only).
  note  string @optional
  verify unit "McpAddExtensionResult schema is valid"
}

type McpAddedExtension = McpAddExtensionEnabled
  | McpAddExtensionInstalled
  | McpAddExtensionPresent
  | McpAddExtensionPlanned
  | McpAddExtensionDuplicate

/// How a registry package's publisher was accepted: unsigned (allowed),
/// pinned (its key matched the pin), pinned_now (pinned on first use) or
/// repinned (a key change accepted with consent).
type PublisherAcceptance = "unsigned" | "pinned" | "pinned_now" | "repinned"

type McpAddExtensionEnabled "A builtin extension enabled" {
  action        "enabled" @literal
  /// The specifier as the call gave it.
  specifier     string
  extension     string    @readonly
  source        "builtin" @literal
  /// False when the builtin was already enabled: nothing changed.
  changed       boolean
  /// The required builtins the add enabled with it, dependencies first.
  peers_enabled string[]
  verify unit "McpAddExtensionEnabled schema is valid"
}

type McpAddExtensionInstalled "A package installed and locked" {
  action        "installed"         @literal
  /// The specifier as the call gave it.
  specifier     string
  extension     string              @readonly
  version       string
  sha256        string
  /// "registry", or "local:<path>" for a local file.
  source        string
  /// The publisher key id; absent for an unsigned package or a local file.
  key_id        string              @optional
  /// Absent for a local file.
  publisher     PublisherAcceptance @optional
  /// The required builtins the add enabled with it, dependencies first.
  peers_enabled string[]
  verify unit "McpAddExtensionInstalled schema is valid"
}

type McpAddExtensionPresent "An extension already installed and enabled: nothing changed" {
  action    "already_present" @literal
  /// The specifier as the call gave it.
  specifier string
  extension string            @readonly
  version   string
  verify unit "McpAddExtensionPresent schema is valid"
}

type McpAddExtensionPlanned "What a dry run would install or enable" {
  action        "planned" @literal
  /// The specifier as the call gave it.
  specifier     string
  extension     string    @readonly
  /// Absent for a builtin, which tracks the binary.
  version       string    @optional
  /// "builtin", "registry", or "local:<path>".
  source        string
  /// The required builtins the add would enable with it, dependencies first.
  peers_enabled string[]
  verify unit "McpAddExtensionPlanned schema is valid"
}

type McpAddExtensionDuplicate "A specifier naming an extension an earlier specifier of the same call adds, the same way: added once" {
  action       "duplicate" @literal
  /// The specifier as the call gave it.
  specifier    string
  extension    string      @readonly
  /// The earlier specifier that adds it.
  duplicate_of string
  verify unit "McpAddExtensionDuplicate schema is valid"
}

type McpRemoveExtensionResult {
  /// "planned" for a dry run, which changes nothing.
  action    "removed" | "planned"
  extension string @readonly
  /// builtin, the lock entry's source (registry, local:<path>), file:<path> or unknown.
  source    string
  /// The locked version, or a builtin's loaded one; absent when neither is known.
  version   string @optional
  /// The entities of the removed extension's kinds, which the project still holds.
  stranded  McpStrandedEntity[]
  verify unit "McpRemoveExtensionResult schema is valid"
}

type McpStrandedEntity "An entity a removal strands" {
  entity_id string @readonly
  kind      string @readonly
  verify unit "McpStrandedEntity schema is valid"
}

type McpMigrateResult "A migration: what it ran or previewed, or that nothing was pending" {
  /// No file failed, nothing was rolled back and specforge.json is used as written:
  /// the verdict specforge migrate exits by.
  ok                       boolean
  /// Some file was behind to_version; when false nothing else ran.
  pending                  boolean
  /// The oldest format version among the project's files.
  from_version             string
  to_version               string
  /// One E069 per problem in specforge.json the run went around; any makes ok false.
  diagnostics              Diagnostic[]
  /// The migrated files are on disk now.
  migrated                 boolean
  dry_run                  boolean
  files_migrated           integer
  files_skipped            integer
  files_failed             integer
  results                  MigrationResult[]
  diffs                    MigrationDiff[]
  /// The .bak copy of each migrated file (none with no_backup or a dry run).
  backups                  MigrationBackup[]
  /// extension:hook for each migration hook that ran.
  hooks_invoked            string[]
  /// Why each failing hook failed.
  hook_failures            string[]
  /// How the migrated graph differs from the one before (W054).
  structural_differences   Diagnostic[]
  rolled_back              boolean
  /// The restore, when the migration was rolled back.
  rollback                 RollbackSummary @optional
  post_migration_validated boolean
  /// What compiling the migrated project reported as errors.
  post_migration_errors    Diagnostic[]
  verify unit "McpMigrateResult schema is valid"
}

type McpInferProgressResult {
  summary    McpInferSummary
  unanalyzed string[]
  stale      string[]
  deleted    string[]
  sessions   McpInferSessionRow[]
  verify unit "McpInferProgressResult schema is valid"
}

type McpInferSummary {
  files_total       integer
  files_analyzed    integer
  entities_produced integer
  verify unit "McpInferSummary schema is valid"
}

type McpInferSessionRow {
  session_id string @readonly
  agent      string
  status     string
  started_at string
  ended_at   string @optional
  verify unit "McpInferSessionRow schema is valid"
}

type McpInferGapsResult {
  total_pub_items integer
  covered_items   integer
  gap_count       integer
  approximate     boolean
  scanners_used   string[]
  scan_failures   McpScanFailure[]
  by_directory    McpDirectoryGaps[]
  verify unit "McpInferGapsResult schema is valid"
}

type McpScanFailure {
  file    string @readonly
  code    string
  message string
  verify unit "McpScanFailure schema is valid"
}

type McpDirectoryGaps {
  directory string @readonly
  count     integer
  items     McpGapItem[]
  verify unit "McpDirectoryGaps schema is valid"
}

type McpGapItem {
  name      string
  item_kind string
  file      string
  line      integer
  verify unit "McpGapItem schema is valid"
}

type McpInferSessionResult {
  /// "active" (started), "recorded" (a file marked) or the status the session ended with.
  status            string
  session_id        string   @optional
  source_file       string   @optional
  entities_produced string[] @optional
  verify unit "McpInferSessionResult schema is valid"
}

type McpImplementationResult {
  entity_id       string @readonly
  implementations McpImplementation[]
  count           integer
  verify unit "McpImplementationResult schema is valid"
}

type McpImplementation {
  file        string
  line        integer
  symbol_name string
  item_kind   string
  scanner     string
  verify unit "McpImplementation schema is valid"
}

type McpSpecForSourceResult {
  file_path  string @readonly
  match_mode "exact" | "directory" | "suffix_path" | "none"
  entities   McpSpecForSourceEntity[]
  count      integer
  verify unit "McpSpecForSourceResult schema is valid"
}

type McpSpecForSourceEntity {
  entity_id   string @readonly
  /// The entity's kind; absent when the graph does not have it.
  kind        string @optional
  file        string
  line        integer
  symbol_name string
  item_kind   string
  /// The anchor's confidence; absent when the manifest gives none.
  confidence  float  @optional
  verify unit "McpSpecForSourceEntity schema is valid"
}

// The context prompt's payload: structured context for an agent, NOT
// generated code or instructions ("SpecForge provides context, agents
// produce output").
type McpContextPromptResult "Context Prompt Result" {
  entity_id                      string @readonly
  kind                           string @readonly
  /// The field the extension declares headline and normative (a behavior's
  /// contract); empty when the entity's kind declares none.
  contract_text                  string
  /// Every field the entity declares, by name (an open value: an
  /// extension's fields).
  fields                         object
  upstream_entities              string[]
  downstream_entities            string[]
  verify_expectations            string[]
  /// The entity IDs the request named as structural constraints.
  structural_constraints         string[]
  /// Each of them, whether or not an edge connects it to entity_id.
  structural_constraint_entities McpContextConstraint[]
  verify unit "Context Prompt Result conforms to schema"
}

type McpContextConstraint "An entity a context request names as a structural constraint" {
  entity_id string @readonly
  kind      string @readonly
  title     string
  /// Every field it declares, by name (an open value).
  fields    object
  verify unit "McpContextConstraint schema is valid"
}

type McpReviewPromptResult "Review Prompt Result" {
  entity_id        string @readonly
  findings         McpReviewFinding[]
  /// The coverage view's rows of the reviewed entities, as specforge.coverage lists them.
  coverage_summary McpCoverageResult[]
  verify unit "Review Prompt Result conforms to schema"
}

type McpReviewFinding {
  entity_id string @readonly
  severity  string
  message   string
  verify unit "McpReviewFinding schema is valid"
}

type McpTracePromptResult "Trace Prompt Result" {
  /// Plan gaps for a plan; the traced entities' missing links for an entity.
  coverage_gaps       McpTraceGap[]
  /// Entity IDs the trace reaches that count toward coverage and are not proven.
  unverified_entities string[]
  /// Entity IDs that the plan touches directly or transitively via graph edges.
  affected_entities   string[]
  verify unit "Trace Prompt Result conforms to schema"
}

type McpTraceGap {
  source_entity     string @readonly
  target_entity     string @readonly
  missing_link_type string
  gap_context       string
  verify unit "McpTraceGap schema is valid"
}

type McpExplorePromptResult "Explore Prompt Result" {
  /// The selected entities: those entity_id reaches within depth (every
  /// entity without it), of kind when given, in id order.
  matching_entities  string[]
  relationship_paths McpRelationshipPath[]
  /// The selected connected entities with the highest lead, at most five.
  starting_points    string[]
  /// The selected connected entities with the most edges to other entities,
  /// at most ten.
  high_connectivity  string[]
  /// The selected entities no edge links to another entity.
  unconnected        string[]
  /// I020 for a kind the project does not know.
  notices            Diagnostic[]
  verify unit "Explore Prompt Result conforms to schema"
}

type McpRelationshipPath {
  from_entity string @readonly
  to_entity   string @readonly
  edge_types  string[]
  path_length integer
  verify unit "McpRelationshipPath schema is valid"
}

// The infer prompt's payload, one per scope, tagged by it.
type McpInferPromptResult = McpInferOverview
  | McpInferKindGuide
  | McpInferFileGuide
  | McpInferPlan
  | McpInferWorkflow

type McpInferOverview "Infer prompt, no scope: every kind's guide" {
  scope                "overview" @literal
  installed_extensions string[]
  /// Entities per kind, every kind entities are written with.
  existing_entities    object
  kinds                McpInferGuideKind[]
  project_conventions  string
  spec_directory       string
  output_format        string
  validation           string
  verify unit "Infer Prompt Result conforms to schema"
}

type McpInferGuideKind {
  kind            string @readonly
  extension       string @readonly
  description     string @optional
  /// Field names; a required one is followed by *.
  fields          string[]
  inference_guide string
  verify unit "McpInferGuideKind schema is valid"
}

type McpInferKindGuide "Infer prompt, scope kind:<name>: one kind's guide" {
  scope               "kind" @literal
  kind                string @readonly
  extension           string @readonly
  description         string @optional
  existing_entity_ids string[]
  fields              McpInferGuideField[]
  inference_guide     string
  example             string
  validation          string
  verify unit "McpInferKindGuide schema is valid"
}

type McpInferGuideField {
  name        string @readonly
  type        string
  required    boolean
  description string @optional
  verify unit "McpInferGuideField schema is valid"
}

type McpInferFileGuide "Infer prompt, scope file:<path>: what a source file already has" {
  scope                              "file" @literal
  file                               string @readonly
  match_mode                         "exact" | "directory" | "suffix_path" | "none"
  existing_entities_referencing_file McpSpecForSourceEntity[]
  kinds                              McpInferFileKind[]
  project_conventions                string
  validation                         string
  verify unit "McpInferFileGuide schema is valid"
}

type McpInferFileKind {
  kind            string @readonly
  inference_guide string
  verify unit "McpInferFileKind schema is valid"
}

type McpInferPlan "Infer prompt, scope plan: the prioritized inference plan" {
  scope                 "plan"  @literal
  target_spec_directory string
  progress              McpInferSummary
  cursor                integer
  /// The cursor of the next page of either file list; absent on the last.
  next_cursor           integer @optional
  /// At most 50 unanalyzed files from cursor; files only.
  unanalyzed_files      string[]
  unanalyzed_total      integer
  stale_files           string[]
  stale_total           integer
  kind_priorities       McpInferKindPriority[]
  verify unit "McpInferPlan schema is valid"
}

type McpInferKindPriority "One kind in the inference plan's order" {
  kind           string @readonly
  extension      string @readonly
  existing_count integer
  verify unit "McpInferKindPriority schema is valid"
}

type McpInferWorkflow "Infer prompt, scope workflow: the tools the protocol names" {
  scope           "workflow" @literal
  /// The tools the protocol names, as tools/list names them.
  tools           string[]
  installed_kinds string[]
  verify unit "McpInferWorkflow schema is valid"
}
