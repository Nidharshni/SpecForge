// Compilation events — signals emitted during the compiler pipeline

use "types/core"
use "types/diagnostics"
use "types/graph"
use "types/wasm"

event file_parsed "File Parsed" {
  channel "compiler.file_parsed"
  payload FileParsedPayload
  // recover_from_syntax_errors runs INSIDE the parser (Tree-sitter error
  // recovery), not as a post-parse event consumer.
  verify integration "emits file_parsed with correct entityCount and errorCount after successful parse"
}

type FileParsedPayload {
  filePath    string
  entityCount integer
  errorCount  integer
  timestamp   timestamp
  verify unit "FileParsedPayload schema is valid"
}

event all_files_parsed "All Files Parsed" {
  channel "compiler.all_files_parsed"
  payload AllFilesParsedPayload
  // Phase 1 -> Extension Loader: registries must be populated before resolution.
  // resolve_use_imports is NOT a consumer here — it runs in Phase 2 after
  // registries_populated.
  // Co-consumer ordering: detect_duplicate_entity_ids runs on raw IDs (no registry
  // needed); load_extension_manifests loads extensions; read_lock_file reads
  // specforge.lock, whose hash pins load_wasm_module checks. All three are
  // independent.
  verify integration "emits all_files_parsed after every file in the project has been parsed"
  verify integration "consumer populates the KindRegistry from extension manifests before Phase 2"
}

type AllFilesParsedPayload {
  fileCount   integer
  entityCount integer
  timestamp   timestamp
  verify unit "AllFilesParsedPayload schema is valid"
}

event structural_parse_complete "Structural Parse Complete" {
  // Phase 1 completion signal — all .spec files have been structurally parsed
  // into generic entity blocks. No keyword validation has occurred yet.
  // This event marks the handoff from Phase 1 (structural parsing) to
  // extension loading and registry population.
  channel "compiler.structural_parse_complete"
  payload StructuralParseCompletePayload
  // load_extension_manifests already consumes all_files_parsed. This event
  // is a Phase 1 completion signal for traceability — it does not add a
  // new consumer trigger.
  verify integration "emits structural_parse_complete after all files are structurally parsed"
}

type StructuralParseCompletePayload {
  fileCount   integer
  entityCount integer
  timestamp   timestamp
  verify unit "StructuralParseCompletePayload schema is valid"
}

event extension_manifests_loaded {
  channel internal
  payload ExtensionManifestsLoadedPayload
  verify unit "Extension manifests MUST be loaded before populating the kind registry"
}

type ExtensionManifestsLoadedPayload {
  extensionCount integer
  manifestPaths  string[]
  timestamp      timestamp
  verify unit "ExtensionManifestsLoadedPayload schema is valid"
}

event registries_populated "Registries Populated" {
  // Fires after populate_kind_registry, populate_field_registry, and
  // populate_edge_registry all complete. The kind registry populator
  // orchestrates all three and fires this event last.
  channel "compiler.registries_populated"
  payload RegistriesPopulatedPayload
  // Phase 2 begins: resolution and semantic validation run only after all
  // registries are populated. resolve_use_imports starts reference linking;
  // two_phase_validate_semantic checks keywords against the KindRegistry.
  verify integration "emits registries_populated after KindRegistry, FieldRegistry, and edge type set are fully loaded"
  verify integration "consumer resolve_use_imports starts Phase 2 only after this event"
  verify integration "consumer two_phase_validate_semantic begins keyword and field validation after this event"
}

type RegistriesPopulatedPayload {
  kindCount      integer
  fieldCount     integer
  edgeCount      integer
  extensionCount integer
  timestamp      timestamp
  verify unit "RegistriesPopulatedPayload schema is valid"
}

event resolution_complete "Resolution Complete" {
  channel "compiler.resolution_complete"
  payload ResolutionCompletePayload
  verify integration "emits resolution_complete after all imports resolved"
  verify integration "consumer build_in_memory_graph receives event and constructs graph"
}

type ResolutionCompletePayload {
  fileCount   integer
  entityCount integer
  timestamp   timestamp
  verify unit "ResolutionCompletePayload schema is valid"
}

event graph_built "Graph Built" {
  channel "compiler.graph_built"
  payload GraphBuiltPayload
  // Core structural validators and the declarative validation engine all
  // run after the graph is built. detect_dangling_references verifies
  // resolver integrity. The remaining checks are core structural checks
  // (unreferenced refs, file reference existence). execute_validation_pattern
  // runs extension-defined declarative rules.
  verify integration "emits graph_built with accurate nodeCount and edgeCount after graph construction"
  verify integration "consumers receive graph_built and trigger structural validation and declarative pattern execution"
}

type GraphBuiltPayload {
  nodeCount integer
  edgeCount integer
  fileCount integer
  timestamp timestamp
  verify unit "GraphBuiltPayload schema is valid"
}

event declarative_validation_executed "Declarative Validation Executed" {
  channel "compiler.declarative_validation_executed"
  payload DeclarativeValidationExecutedPayload
  verify integration "emits declarative_validation_executed with correct ruleCount and violationCount"
  verify integration "consumer aggregate_diagnostic_summary collects pattern diagnostics into final summary"
}

type DeclarativeValidationExecutedPayload {
  ruleCount      integer
  violationCount integer
  extensionCount integer
  timestamp      timestamp
  verify unit "DeclarativeValidationExecutedPayload schema is valid"
}

event validation_complete "Validation Complete" {
  channel "compiler.validation_complete"
  payload ValidationCompletePayload
  // Note: format_diagnostics_with_source_context and provide_did_you_mean_suggestions
  // run INLINE during resolution and validation (each diagnostic is formatted as it's
  // emitted). They are NOT post-validation consumers. This event signals pipeline
  // completion — consumers are output-phase behaviors that need the final diagnostic
  // counts, such as CLI exit code decisions and emitter triggers.
  verify integration "emits validation_complete with correct error, warning, and info counts"
  verify integration "event signals Phase 2 completion — emitter phase may begin"
}

type ValidationCompletePayload {
  errorCount   integer
  warningCount integer
  infoCount    integer
  timestamp    timestamp
  verify unit "ValidationCompletePayload schema is valid"
}

event file_changed "File Changed" {
  channel "watch.file_changed"
  payload FileChangedPayload
  verify integration "emits file_changed with correct filePath and changeType on filesystem modification"
  verify integration "consumer debounce_file_changes receives event and coalesces into batch"
}

type FileChangedPayload {
  filePath   string
  changeType string
  timestamp  timestamp
  verify unit "FileChangedPayload schema is valid"
}

event file_changes_coalesced "File Changes Coalesced" {
  channel "watch.file_changes_coalesced"
  payload FileChangesCoalescedPayload
  verify integration "emits file_changes_coalesced with correct filePaths after debounce window expires"
  verify integration "consumer invalidate_changed_files receives coalesced batch and computes invalidation set"
}

type FileChangesCoalescedPayload {
  filePaths   string[]
  changeCount integer
  timestamp   timestamp
  verify unit "FileChangesCoalescedPayload schema is valid"
}

event subgraph_invalidated "Subgraph Invalidated" {
  channel "watch.subgraph_invalidated"
  payload SubgraphInvalidatedPayload
  verify integration "emits subgraph_invalidated with correct invalidatedFiles list and nodeCount"
  verify integration "consumer rebuild_affected_subgraph receives event and re-parses invalidated files"
  verify integration "consumer resolve_imports_on_update resolves every file's imports again"
}

type SubgraphInvalidatedPayload {
  invalidatedFiles string[]
  nodeCount        integer
  timestamp        timestamp
  verify unit "SubgraphInvalidatedPayload schema is valid"
}

event import_dag_updated "Import DAG Updated" {
  channel "watch.import_dag_updated"
  payload ImportDagUpdatedPayload
  verify integration "emits import_dag_updated after every file's imports are resolved again"
  verify integration "consumer rebuild_affected_subgraph waits for import DAG to be current before rebuilding"
}

type ImportDagUpdatedPayload {
  addedEdges     integer
  removedEdges   integer
  cyclesDetected integer
  timestamp      timestamp
  verify unit "ImportDagUpdatedPayload schema is valid"
}

event incremental_rebuild_complete "Incremental Rebuild Complete" {
  channel "watch.incremental_rebuild_complete"
  payload IncrementalRebuildCompletePayload
  verify integration "emits incremental_rebuild_complete with correct rebuiltFiles and node counts"
  verify integration "consumer emit_incremental_diagnostics refreshes diagnostics for rebuilt files"
  verify integration "consumer shared_incremental_pipeline updates LSP graph state"
  verify integration "consumer compute_graph_delta diffs previous and new graph states to produce GraphDelta"
}

type IncrementalRebuildCompletePayload {
  rebuiltFiles string[]
  addedNodes   integer
  removedNodes integer
  elapsedMs    integer
  timestamp    timestamp
  verify unit "IncrementalRebuildCompletePayload schema is valid"
}

event render_complete "Render Complete" {
  channel "compiler.render_complete"
  payload RenderCompletePayload
  verify integration "emits render_complete with correct outputFileCount after rendering"
}

type RenderCompletePayload {
  outputFileCount integer
  format          string
  timestamp       timestamp
  verify unit "RenderCompletePayload schema is valid"
}

event export_complete "Export Complete" {
  channel "compiler.export_complete"
  payload ExportCompletePayload
  verify integration "emits export_complete with correct format and entityCount after agent export"
}

type ExportCompletePayload {
  format         string
  entityCount    integer
  scopedToEntity string
  timestamp      timestamp
  verify unit "ExportCompletePayload schema is valid"
}

event format_complete "Format Complete" {
  channel "compiler.format_complete"
  payload FormatCompletePayload
  verify integration "emits format_complete with correct filesChecked and filesChanged"
}

type FormatCompletePayload {
  filesChecked integer
  filesChanged integer
  timestamp    timestamp
  verify unit "FormatCompletePayload schema is valid"
}

event project_initialized "Project Initialized" {
  // Triggers are mutually exclusive — exactly one fires per init invocation depending on the code path.
  // interactive_extension_selection is a sub-step of scaffold_new_project, not an independent trigger.
  channel "cli.project_initialized"
  payload ProjectInitializedPayload
  verify integration "emits project_initialized with correct projectName and extensionCount"
  verify integration "specFilePath refers to the starter entity spec file, not specforge.json"
}

type ProjectInitializedPayload {
  projectName    string
  extensionCount integer
  specFilePath   string // Path to the starter entity spec file created by init (e.g., hello.spec)
  timestamp      timestamp
  verify unit "ProjectInitializedPayload schema is valid"
}

event extension_added "Extension Added" {
  channel "cli.extension_added"
  payload ExtensionAddedPayload
  verify integration "emits extension_added with correct extensionSpecifier after specforge add"
  verify unit "wasDuplicate is true when extension was already installed"
}

type ExtensionAddedPayload {
  extensionSpecifier string
  totalExtensions    integer
  wasDuplicate       boolean
  timestamp          timestamp
  verify unit "ExtensionAddedPayload schema is valid"
}

event extension_loading_failed {
  channel "compilation.extension_loading_failed"
  payload ExtensionLoadingFailedPayload
  verify integration "manifest parse error emits extension_loading_failed event"
}

type ExtensionLoadingFailedPayload {
  extension_name string
  error_kind     string
  message        string
  verify unit "ExtensionLoadingFailedPayload schema is valid"
}

// ── LSP Lifecycle Events ─────────────────────────────────────

event lsp_initialized "LSP Initialized" {
  channel "lsp.initialized"
  payload LspInitializedPayload
  verify integration "emits lsp_initialized after LSP server completes initialization"
}

type LspInitializedPayload {
  extensionCount integer
  kindCount      integer
  timestamp      timestamp
  verify unit "LspInitializedPayload schema is valid"
}

event lsp_shutdown_complete "LSP Shutdown Complete" {
  channel "lsp.shutdown_complete"
  payload LspShutdownCompletePayload
  verify integration "emits lsp_shutdown_complete after LSP server shuts down cleanly"
}

type LspShutdownCompletePayload {
  timestamp timestamp
  verify unit "LspShutdownCompletePayload schema is valid"
}

// ── Migration Events ──────────────────────────────────────────

// migration_started fires after the backup is created but before transforms run.
// Contrast with migration_starting, which fires before any file I/O begins.
event migration_started "Migration Started" {
  channel "compiler.migration_started"
  payload MigrationStartedPayload
  verify integration "emits migration_started with correct fileCount and targetVersion"
}

type MigrationStartedPayload {
  fileCount     integer
  targetVersion string
  timestamp     timestamp
  verify unit "MigrationStartedPayload schema is valid"
}

// migration_starting fires before any file I/O begins (pre-backup).
// Contrast with migration_started, which fires after the backup is created.
event migration_starting "Migration Starting" {
  channel "compiler.migration_starting"
  payload MigrationStartingPayload
  verify integration "emits migration_starting before any migration transforms are applied"
}

type MigrationStartingPayload {
  fileCount     integer
  targetVersion string
  timestamp     timestamp
  verify unit "MigrationStartingPayload schema is valid"
}

event pre_migration_snapshot_captured "Pre-Migration Snapshot Captured" {
  channel "compiler.pre_migration_snapshot_captured"
  payload PreMigrationSnapshotCapturedPayload
  verify integration "emits pre_migration_snapshot_captured after schema snapshot is taken"
}

type PreMigrationSnapshotCapturedPayload {
  nodeKindCount integer
  edgeTypeCount integer
  fieldCount    integer
  timestamp     timestamp
  verify unit "PreMigrationSnapshotCapturedPayload schema is valid"
}

event migration_complete "Migration Complete" {
  channel "compiler.migration_complete"
  payload MigrationCompletePayload
  verify integration "emits migration_complete with correct migratedCount, failedCount, and skippedCount"
  verify integration "consumer invoke_extension_migration_hooks receives event to run extension hooks"
}

type MigrationCompletePayload {
  migratedCount integer
  failedCount   integer
  // skippedCount: files already at the target version (idempotency).
  // A skipped file is one whose format_version already matches the
  // migration target — no transforms are applied and no backup is created.
  skippedCount  integer
  timestamp     timestamp
  verify unit "MigrationCompletePayload schema is valid"
}

event migration_diff_generated "Migration Diff Generated" {
  channel "compiler.migration_diff_generated"
  payload MigrationDiffGeneratedPayload
  verify integration "emits migration_diff_generated with correct fileCount and totalDiffLines after dry-run"
}

type MigrationDiffGeneratedPayload {
  fileCount      integer
  totalDiffLines integer
  timestamp      timestamp
  verify unit "MigrationDiffGeneratedPayload schema is valid"
}

event extension_migration_hooks_complete "Extension Migration Hooks Complete" {
  channel "compiler.extension_migration_hooks_complete"
  payload ExtensionMigrationHooksCompletePayload
  verify integration "emits extension_migration_hooks_complete after all extension hooks finish"
  verify integration "consumer validate_post_migration_integrity receives event"
}

type ExtensionMigrationHooksCompletePayload {
  extensionsInvoked integer
  extensionsFailed  integer
  extensionsSkipped integer
  timestamp         timestamp
  verify unit "ExtensionMigrationHooksCompletePayload schema is valid"
}

event migration_validation_complete "Migration Validation Complete" {
  channel "compiler.migration_validation_complete"
  payload MigrationValidationCompletePayload
  verify integration "emits migration_validation_complete after post-migration validation finishes"
}

type MigrationValidationCompletePayload {
  structuralDifferences integer
  newDiagnostics        integer
  timestamp             timestamp
  verify unit "MigrationValidationCompletePayload schema is valid"
}

event migration_rolled_back "Migration Rolled Back" {
  channel "compiler.migration_rolled_back"
  payload MigrationRolledBackPayload
  verify integration "emits migration_rolled_back with correct restored, skipped, and failed counts"
}

type MigrationRolledBackPayload {
  restoredCount integer
  skippedCount  integer
  failedCount   integer
  timestamp     timestamp
  verify unit "MigrationRolledBackPayload schema is valid"
}

// ── Schema Events ────────────────────────────────────────────

event schema_generated "Schema Generated" {
  channel "compiler.schema_generated"
  payload SchemaGeneratedPayload
  // embed_schema_in_export now waits for schema_version_computed (via the
  // schema pipeline), not schema_generated directly.
  verify integration "emits schema_generated with correct entityKindCount and edgeTypeCount"
  verify integration "consumer persist_schema_cache receives event"
  verify integration "consumer detect_breaking_schema_changes receives event"
}

type SchemaGeneratedPayload {
  entityKindCount integer
  edgeTypeCount   integer
  extensionCount  integer
  timestamp       timestamp
  verify unit "SchemaGeneratedPayload schema is valid"
}

event schema_cache_persisted "Schema Cache Persisted" {
  channel "compiler.schema_cache_persisted"
  payload SchemaCachePersistedPayload
  verify integration "emits schema_cache_persisted after writing .specforge/schema-cache.json"
}

type SchemaCachePersistedPayload {
  schemaVersion string
  hash          string
  timestamp     timestamp
  verify unit "SchemaCachePersistedPayload schema is valid"
}

event schema_version_computed "Schema Version Computed" {
  channel "compiler.schema_version_computed"
  payload SchemaVersionComputedPayload
  verify integration "emits schema_version_computed with correct major, minor, patch after schema comparison"
}

type SchemaVersionComputedPayload {
  major     integer
  minor     integer
  patch     integer
  bumpType  string
  timestamp timestamp
  verify unit "SchemaVersionComputedPayload schema is valid"
}

// Entity embedding events moved to spec/extensions/embeddings/events.spec

// ── Agent & Schema Events ─────────────────────────────────────

event plan_validated "Agent Plan Validated" {
  payload PlanValidationResult
  channel compilation
  verify integration "Agent Plan Validated emitted correctly"
}

event token_budget_applied "Token Budget Applied" {
  payload ExportResult
  channel compilation
  verify integration "Token Budget Applied emitted correctly"
}

event schema_version_negotiated "Schema Version Negotiated" {
  payload SchemaCompatibility
  channel compilation
  verify integration "Schema Version Negotiated emitted correctly"
}

// ── Output Query Events ──────────────────────────────────────

event graph_queried "Graph Queried" {
  channel "output"
  payload GraphQueriedPayload
  verify integration "emits graph_queried with correct entityId, scope, and resultNodeCount"
  verify integration "payload includes depth and scope used for the query"
}

type GraphQueriedPayload {
  entityId        string
  scope           string
  depth           integer
  resultNodeCount integer
  timestamp       timestamp
  verify unit "GraphQueriedPayload schema is valid"
}

// entity_search_performed moved to spec/extensions/embeddings/events.spec

// ── Incremental Pipeline Terminal Events ──────────────────────
// Terminal events (consumers []) are intentionally leaf events for
// observability, audit trails, and CLI output. Not every event requires
// a behavioral consumer — these events serve as integration points for
// external tooling, logging, and traceability without mandating an
// in-process consumer.

event incremental_diagnostics_complete "Incremental Diagnostics Complete" {
  channel "watch.incremental_diagnostics_complete"
  payload IncrementalDiagnosticsCompletePayload
  verify integration "emits incremental_diagnostics_complete after incremental diagnostic pipeline finishes"
}

type IncrementalDiagnosticsCompletePayload {
  errorCount    integer
  warningCount  integer
  infoCount     integer
  affectedFiles integer
  timestamp     timestamp
  verify unit "IncrementalDiagnosticsCompletePayload schema is valid"
}

event delta_subscribers_notified "Delta Subscribers Notified" {
  channel "watch.delta_subscribers_notified"
  payload DeltaSubscribersNotifiedPayload
  verify integration "emits delta_subscribers_notified after all subscribers receive delta"
}

type DeltaSubscribersNotifiedPayload {
  subscriberCount integer
  timestamp       timestamp
  verify unit "DeltaSubscribersNotifiedPayload schema is valid"
}

// ── Rename Events ───────────────────────────────────────────

event entity_renamed "Entity Renamed" {
  channel "lsp.entity_renamed"
  payload EntityRenamedPayload
  verify integration "emits entity_renamed with correct oldId, newId, and affectedFiles"
}

type EntityRenamedPayload {
  oldId         string
  newId         string
  affectedFiles integer
  timestamp     timestamp
  verify unit "EntityRenamedPayload schema is valid"
}

// ── Graph Delta Events ──────────────────────────────────────

event graph_delta_computed "Graph Delta Computed" {
  channel "watch.graph_delta_computed"
  payload GraphDeltaComputedPayload
  verify integration "emits graph_delta_computed with correct node and edge counts"
  verify integration "consumer dispatch_incremental_validators receives delta event"
  verify integration "consumer notify_delta_subscribers receives delta event"
  verify integration "consumer validate_delta_correctness receives delta event in debug mode"
}

type GraphDeltaComputedPayload {
  addedNodes    integer
  removedNodes  integer
  modifiedNodes integer
  addedEdges    integer
  removedEdges  integer
  affectedFiles integer
  timestamp     timestamp
  verify unit "GraphDeltaComputedPayload schema is valid"
}

event incremental_validators_dispatched "Incremental Validators Dispatched" {
  channel "watch.incremental_validators_dispatched"
  payload IncrementalValidatorsDispatchedPayload
  verify integration "emits incremental_validators_dispatched with correct extension counts"
  verify integration "payload distinguishes incremental vs full-rebuild extension counts"
}

type IncrementalValidatorsDispatchedPayload {
  incrementalExtensions integer
  fullRebuildExtensions integer
  deltaNodeCount        integer
  timestamp             timestamp
  verify unit "IncrementalValidatorsDispatchedPayload schema is valid"
}

// ── Delta Validation Events ──────────────────────────────────

event delta_validation_passed "Delta Validation Passed" {
  // Success-path counterpart to delta_validation_failed — emitted when
  // validate_delta_correctness confirms incremental and full-rebuild
  // graph states are consistent.
  channel "watch.delta_validation_passed"
  payload DeltaValidationPassedPayload
  verify integration "emits delta_validation_passed when incremental rebuild matches full rebuild"
}

type DeltaValidationPassedPayload {
  nodeCount integer
  edgeCount integer
  verify unit "DeltaValidationPassedPayload schema is valid"
}

event trace_chain_computed "Trace Chain Computed" {
  channel "output.trace_chain_computed"
  payload TraceChainComputedPayload
  verify integration "emits trace_chain_computed with correct entityId, chainDepth, and linkCount"
}

type TraceChainComputedPayload {
  entityId   string
  chainDepth integer
  linkCount  integer
  verify unit "TraceChainComputedPayload schema is valid"
}

event delta_validation_failed "Delta Validation Failed" {
  // Debug-only event — emitted when validate_delta_correctness detects
  // a discrepancy between the incremental and full-rebuild graph states.
  channel "watch.delta_validation_failed"
  payload DeltaValidationFailedPayload
  verify unit "emits delta_validation_failed when incremental rebuild diverges from full rebuild"
}

type DeltaValidationFailedPayload {
  inconsistentNodes integer
  inconsistentEdges integer
  timestamp         timestamp
  verify unit "DeltaValidationFailedPayload schema is valid"
}

// ── MCP Graph Resource Events ─────────────────────────────────

event graph_resource_served "Graph Resource Served" {
  channel "mcp"
  payload GraphResourceServedPayload
  verify integration "emits graph_resource_served with correct resourceUri and format after MCP resource read"
}

type GraphResourceServedPayload {
  resourceUri    string
  format         string
  entityCount    integer
  scopedToEntity string
  timestamp      timestamp
  verify unit "GraphResourceServedPayload schema is valid"
}

// ── Schema Breaking Change Events ─────────────────────────────

event schema_breaking_change_detected "Schema Breaking Change Detected" {
  channel "internal"
  payload SchemaBreakingChangeDetectedPayload
  verify integration "emits schema_breaking_change_detected with correct previousVersion and currentVersion"
  verify integration "consumer negotiate_schema_version receives event for version compatibility checks"
}

type SchemaBreakingChangeDetectedPayload {
  previousVersion  string
  currentVersion   string
  breakingCount    integer
  nonBreakingCount integer
  timestamp        timestamp
  verify unit "SchemaBreakingChangeDetectedPayload schema is valid"
}

event graph_protocol_compatibility_verified "Graph Protocol Compatibility Verified" {
  channel "compiler.graph_protocol_compatibility_verified"
  payload GraphProtocolCompatibilityVerifiedPayload
  verify integration "emits graph_protocol_compatibility_verified after post-migration schema comparison"
}

type GraphProtocolCompatibilityVerifiedPayload {
  breakingChanges    integer
  nonBreakingChanges integer
  timestamp          timestamp
  verify unit "GraphProtocolCompatibilityVerifiedPayload schema is valid"
}
