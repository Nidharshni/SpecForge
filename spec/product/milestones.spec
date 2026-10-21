// Milestones — delivery phases
//
// Rewritten from scratch with clear priority ordering based on:
// 1. Entity dependency DAG — foundational entities before dependent ones
// 2. Compilation pipeline order — parse > resolve > graph > validate > export
// 3. Vision horizons — H1 (individual value) > H2 (ecosystem) > H3 (standard)
// 4. Each phase ships something usable — no setup-only phases
//
// Phase dependency DAG (enforced via depends_on fields):
// H1: P1 > P2 > P3 > P4 > P5, P4 > P6 > P7, P4 > P8
// H2: P8 > P9 > P10 > P11, P4 > P12 > P13, P11 > P14
// Planned: P10 > P15, P11 > P16, P11 > P17

use "extensions/compliance/features"
use "extensions/embeddings/features"
use "extensions/formal/features"
use "extensions/markdown-renderer/features"
use "extensions/product/features"
use "extensions/software/features"
use "features/extensions"
use "features/formatting"
use "features/incremental"
use "features/lsp"
use "features/mcp"
use "features/migration"
use "features/output"
use "features/parsing"
use "features/project-init"
use "features/validation"
use "features/wasm"
use "features/zero-entity-core"
use "product/features"
use "product/modules"

// ════════════════════════════════════════════════════════════════
// H1: Individual Value — install to first validated output in <60s
// ════════════════════════════════════════════════════════════════

milestone structural_parsing "Phase 1: Structural Parsing" {
  description   "Tree-sitter grammar and parser crate that turns .spec files into typed AST nodes with multi-error recovery."
  status        completed
  start_date    "2025-04-15"
  target_date   "2025-06-01"
  owner         "specforge-team"
  contributors  ["specforge-team"]
  features      [spec_file_parsing, error_recovery_during_parsing, editor_query_files]
  modules       [tree_sitter_specforge, specforge_parser, specforge_common]
  tags          ["h1", "core"]
  exit_criteria [
    "Tree-sitter grammar parses any keyword name { fields } block",
    "Multi-error recovery: N syntax errors produce N diagnostics, not 1",
    "highlights.scm, folds.scm and indents.scm load against the grammar and capture keywords, strings, folds and indents; VS Code highlights through a TextMate grammar and LSP semantic tokens",
    "Generic entity_block rule produces clean AST nodes for any keyword",
  ]
}

milestone resolution_and_graph "Phase 2: Resolution & Graph Construction" {
  description   "Import resolution and mutable entity graph that links all intra-project references across files."
  status        completed
  start_date    "2025-06-01"
  target_date   "2025-07-15"
  owner         "specforge-team"
  contributors  ["specforge-team"]
  depends_on    [structural_parsing]
  features      [reference_resolution, graph_construction]
  modules       [specforge_resolver, specforge_graph]
  tags          ["h1", "core"]
  exit_criteria [
    "All intra-project references linked across files",
    "Import cycles detected and reported as W113 warnings, each cycle once",
    "Cross-extension refs produce I004 info if extension not installed",
    "One node per distinct entity ID (a duplicate is E002 or W060); one edge per distinct source, target and field reference",
    "Mutable graph supports incremental updates",
  ]
}

milestone validation_and_errors "Phase 3: Validation & Error Reporting" {
  description   "Structural and semantic validation with ariadne-powered diagnostic reporting and CI exit codes."
  status        completed
  start_date    "2025-07-15"
  target_date   "2025-08-30"
  owner         "specforge-team"
  contributors  ["specforge-team"]
  depends_on    [resolution_and_graph]
  features      [structural_validation, diagnostic_reporting, ci_integration, product_validation]
  modules       [specforge_common, specforge_cli]
  tags          ["h1", "core"]
  exit_criteria [
    "specforge check passes on SpecForge's own .spec files",
    "Diagnostics include source context with line/column spans",
    "Did-you-mean suggestions for misspelled entity IDs and imports (Levenshtein <= 3 and Jaro-Winkler > 0.85)",
    "Exit code 0 on clean, 1 on errors; --strict promotes warnings to errors",
    "Structured output (JSON) available for CI parsers",
  ]
}

milestone output_and_export "Phase 4: Output & Agent Export" {
  description   "Graph serialization to JSON, DOT, and agent-optimized formats with multi-resolution queries and deterministic output."
  status        completed
  start_date    "2025-08-30"
  target_date   "2025-10-15"
  owner         "specforge-team"
  contributors  ["specforge-team"]
  depends_on    [validation_and_errors]
  features      [json_and_dot_render, traceability_serialization, agent_export, product_graph_rendering]
  modules       [specforge_emitter]
  tags          ["h1", "core"]
  exit_criteria [
    "specforge schema --publish emits the Graph Protocol as JSON Schema (draft 2020-12); graph exports carry format_version 2.0 and a computed schema_version",
    "specforge export --format=context produces token-optimized output",
    "specforge export --format=graph produces complete entity graph JSON",
    "specforge export --format=brief produces id, kind and title per entity, plus edges",
    "Multi-resolution queries: export --scope <id> and query <id> --depth N [--kind K] return subgraphs",
    "specforge trace prints full traceability chains",
    "specforge stats reports accurate entity/edge/unreferenced counts",
    "Output is deterministic: same input always produces same bytes",
  ]
}

milestone project_init "Phase 5: Project Initialization" {
  description   "Project scaffolding via specforge init with interactive extension selection and specforge.json configuration."
  status        completed
  start_date    "2025-10-15"
  target_date   "2025-11-01"
  owner         "specforge-team"
  contributors  ["specforge-team"]
  depends_on    [output_and_export]
  features      [project_initialization]
  modules       [specforge_cli]
  tags          ["h1", "platform"]
  exit_criteria [
    "specforge init creates specforge.json and starter .spec file",
    "Full init > check > export pipeline completes in under 60 seconds",
    "Zero-extension project is valid and produces a graph",
    "Non-interactive mode works for CI: --name and --extensions flags",
    "specforge add @specforge/software adds extension to existing project",
  ]
}

milestone ms_incremental_compilation "Phase 6: Incremental Compilation" {
  description   "File watching with debounced incremental rebuild, graph deltas, and minimal invalidation (only changed files re-parsed)."
  status        completed
  start_date    "2025-10-15"
  target_date   "2025-11-15"
  owner         "specforge-team"
  contributors  ["specforge-team"]
  depends_on    [output_and_export]
  features      [incremental_compilation, incremental_graph_deltas]
  modules       [specforge_watch]
  tags          ["h1", "core"]
  exit_criteria [
    "A file change is detected within 100ms plus the debounce window, and an incremental update of a small project yields diagnostics in under 100ms",
    "Incremental rebuild matches a cold rebuild, checked by --verify-incremental on every fixture and by seeded random update sequences",
    "Graph delta contains only added/removed/modified nodes and edges",
    "File change debouncing prevents redundant rebuilds",
    "Only changed files are re-parsed; imports are resolved again on every rebuild",
  ]
}

milestone lsp_server "Phase 7: LSP Server" {
  description   "Full Language Server Protocol implementation with navigation, completion, refactoring, live diagnostics, and semantic tokens."
  status        completed
  start_date    "2025-11-15"
  target_date   "2025-12-15"
  owner         "specforge-team"
  contributors  ["specforge-team"]
  depends_on    [ms_incremental_compilation]
  features      [
    lsp_lifecycle,
    go_to_definition_and_references,
    hover_and_autocomplete,
    rename_refactoring,
    live_diagnostics,
    semantic_tokens,
    code_actions,
    outline_and_symbol_search,
  ]
  modules       [specforge_lsp]
  tags          ["h1", "platform"]
  exit_criteria [
    "Go-to-definition and find-references work across files",
    "Hover shows entity details, contract text, and reference count",
    "Autocomplete suggests entity IDs, field names, and keywords",
    "Rename updates declaration and all references atomically",
    "Live diagnostics appear within 100ms via shared incremental pipeline",
    "Semantic tokens classify entity keywords from extensions",
    "Code actions: replace an unresolved ID or import with its close match, create an entity stub, add a verify stub",
    "Outline view and workspace symbol search work for all entity types",
  ]
}

milestone ms_code_formatting "Phase 8: Code Formatting" {
  description   "Idempotent code formatter with CST-preserving comment handling, CLI check mode, and LSP formatting integration."
  status        completed
  start_date    "2025-10-15"
  target_date   "2025-11-30"
  owner         "specforge-team"
  contributors  ["specforge-team"]
  depends_on    [output_and_export]
  features      [code_formatting, lsp_formatting]
  modules       [specforge_formatter]
  tags          ["h1", "tooling"]
  exit_criteria [
    "format(format(x)) == format(x) on a representative input set and through stdin",
    "All comments preserved after formatting",
    "A 50-entity file formats in under 50ms",
    "specforge format --check exits 1 on unformatted files",
    "LSP textDocument/formatting produces same result as CLI",
    "Range formatting matches full formatting for affected blocks",
    "Files with parse errors are partially formatted without data loss",
  ]
}

// ════════════════════════════════════════════════════════════════
// H2: Ecosystem — zero domain knowledge in core, extensions for all
// ════════════════════════════════════════════════════════════════

milestone zero_entity_core "Phase 9: Zero-Entity Core Architecture" {
  description   "Core compiler refactored to have zero hardcoded entity types. All domain vocabulary comes from extensions via ExtensionDeclaration declarations."
  status        completed
  start_date    "2025-12-01"
  target_date   "2026-01-15"
  owner         "specforge-team"
  contributors  ["specforge-team"]
  depends_on    [ms_code_formatting]
  features      [
    declarative_validation_rules,
    extension_manifest,
    dynamic_entity_registration,
    extension_driven_lsp,
    extension_driven_visualization,
    zero_entity_bootstrap,
    zero_entity_validation,
    entity_enhancement,
    product_entity_registration,
    extension_driven_code_actions,
    extension_driven_coverage,
  ]
  modules       [specforge_wasm]
  tags          ["h2", "architecture"]
  exit_criteria [
    "Core compiler has zero hardcoded entity types — all from extensions",
    "KindRegistry boots empty and is populated only from the loaded extensions' ExtensionDeclarations",
    "ExtensionDeclaration declares entity kinds (with testability), edge types, enhancements, validation rules, surfaces, collectors, analyzers and passes",
    "@specforge/software (5), product (9) and governance (3) declare the 17 domain kinds SpecForge's own specs use",
    "Two-phase compilation separates structural parsing from semantic validation",
    "E024 diagnostics suggest which extension provides unknown keywords",
    "Graceful degradation with zero extensions produces I002 info",
    "Declarative validation patterns interpreted by core, not hardcoded passes",
    "LSP highlights, completes, and navigates extension-defined entity types",
    "Enhancements from several extensions add their fields to a kind; a field two extensions add resolves to the first in specforge.json order, and never overrides the kind's own field",
    "Third-party domain extensions work end-to-end",
  ]
}

milestone wasm_runtime "Phase 10: Wasm Extension Runtime" {
  description   "Wasm component runtime with compile caching, sandbox enforcement, peer dependency validation, and surface contribution dispatch. The host-function import surface is not part of it: it is planned as Phase 15 (wasm_host_functions)."
  status        completed
  start_date    "2026-01-15"
  target_date   "2026-02-01"
  owner         "specforge-team"
  contributors  ["specforge-team"]
  depends_on    [zero_entity_core]
  features      [
    wasm_extension_runtime,
    wasm_performance_optimization,
    entity_kind_conflict_prevention,
    provider_based_ref_validation,
    contribution_based_extensions,
    surface_contributions,
    product_graph_queries,
    product_surface_access,
    product_health_metric,
    product_impact_and_whatif,
  ]
  modules       [specforge_wasm, specforge_provider_gh]
  tags          ["h2", "runtime"]
  exit_criteria [
    "Wasm extensions load, initialize, and validate without errors",
    "Compiled components are cached on disk; a second runtime reuses the cache, and an unwritable cache degrades to no cache with a warning",
    "Sandbox enforcement blocks unauthorized filesystem and network access",
    "Peer dependency validation catches missing or incompatible extensions",
    "Wasm traps produce structured diagnostics without crashing the compiler",
    "Entity kind conflicts between extensions detected and reported",
    "Configured providers register ref schemes: a ref whose scheme no provider registers is I005, and a scheme two providers claim is E057",
    "Contribution-based dispatch routes to correct exports per contribution type",
    "Surface contributions registered from the declaration's surfaces field",
    "CLI commands auto-promoted to MCP tools with matching schemas",
  ]
}

milestone extension_ecosystem "Phase 11: Extension Ecosystem" {
  description   "Full extension lifecycle: install, upgrade, remove, author, build, test, publish. Registry integration and lock management."
  status        completed
  start_date    "2026-02-01"
  target_date   "2026-02-28"
  owner         "specforge-team"
  contributors  ["specforge-team"]
  depends_on    [wasm_runtime]
  features      [
    extension_management,
    wasm_extension_installation,
    wasm_lock_management,
    wasm_extension_maintenance,
    wasm_extension_authoring,
    extension_registry,
    registry_authentication,
    test_result_collection,
    pe_planning_insights,
    pe_external_blockers,
  ]
  modules       [
    specforge_package_formal,
    specforge_package_product,
    specforge_package_governance,
    specforge_package_testing,
    specforge_package_cargo_test,
    specforge_package_vitest,
    specforge_test_lib,
    specforge_test_macros_lib,
    specforge_registry_server,
  ]
  tags          ["h2", "ecosystem"]
  exit_criteria [
    "Full install/upgrade/remove lifecycle for Wasm extensions",
    "specforge.lock pins exact versions with SHA256 integrity hashes",
    "Extension authoring: init > build > test > publish works e2e",
    "Collectors produce specforge-report.json from test frameworks",
    "Registry search, resolve and publish work over the SpecForge HTTP package registry",
    "specforge doctor reports conflicts, cache health, and extension status",
    "Private registry authentication: login keeps a registry credential under its registry root, logout removes it",
    "Surface commands dispatched via cmd__{id} Wasm exports with sandbox enforcement",
    "Surface MCP tools/resources dispatched via mcp__{name} Wasm exports",
  ]
}

milestone software_extension_v1 "Phase 11a: @specforge/software Extension v1" {
  description   "First-party domain extension implementing behavior, invariant, event, type, and port entity kinds for software engineering specifications."
  status        completed
  start_date    "2026-03-01"
  target_date   "2026-04-15"
  owner         "specforge-team"
  contributors  ["specforge-team"]
  depends_on    [zero_entity_core, wasm_runtime]
  features      [se_core_entity_kinds, se_validation_suite, se_gherkin_bridge]
  modules       [specforge_package_software]
  tags          ["h2", "extension"]
  exit_criteria [
    "The declaration declares 5 entity kinds (behavior, invariant, event, type, port) with fields and LSP metadata; @specforge/testing makes them testable",
    "The declaration declares 15 edge types with source/target constraints",
    "Rules W001-W003, W005-W008, W010, E004 and E010 (three of them custom Wasm checks) fire correctly",
    "Entity enhancements add ports and behaviors fields to product entities",
    "specforge check on SpecForge's own specs reports 0 errors, and every software warning is a genuine finding",
    "Every testable software entity declares verify obligations; linked tests prove them, and specforge stats and milestone-completion report how many are proven",
  ]
}

milestone schema_versioning "Phase 12: Graph Protocol Schema Versioning" {
  description   "Self-describing graph protocol schema embedded in exports with version auto-computation, breaking change detection, and schema negotiation."
  status        completed
  start_date    "2025-10-15"
  target_date   "2025-11-30"
  owner         "specforge-team"
  contributors  ["specforge-team"]
  depends_on    [output_and_export]
  features      [self_describing_graph_protocol, graph_protocol_versioning]
  modules       [specforge_emitter, specforge_cli]
  tags          ["h2", "schema"]
  exit_criteria [
    "The schema is embedded in graph exports by default (opt out with --no-schema; opt in for context, brief or budgeted exports with --with-schema)",
    "Schema version is computed by diffing every attribute against the last export's cached schema (major for breaking, minor for additions, patch for documentation and provenance; 1.0.0 with no cache)",
    "Breaking changes warn W053 with the versions they move between",
    "--schema-version admits a version whose readers read the current schema and refuses others; the export is never relabelled",
    "specforge schema --publish emits a JSON Schema (draft 2020-12) for graph, context and brief exports",
  ]
}

milestone mcp_server "Phase 13: MCP Server" {
  description   "Model Context Protocol server exposing graph resources, core/navigation/mutation/project tools, delta notifications, and guided prompts."
  status        completed
  start_date    "2025-12-15"
  target_date   "2026-01-31"
  owner         "specforge-team"
  contributors  ["specforge-team"]
  depends_on    [schema_versioning]
  modules       [specforge_mcp]
  features      [
    mcp_lifecycle,
    mcp_resource_exposure,
    mcp_core_tools,
    mcp_navigation_tools,
    mcp_mutation_tools,
    mcp_project_management_tools,
    mcp_delta_notifications,
    mcp_prompts,
    mcp_protocol_compliance,
    mcp_discovery,
  ]
  tags          ["h2", "platform"]
  exit_criteria [
    "MCP server initializes and shuts down cleanly per protocol spec",
    "All 8 core resources (5 fixed, 3 templates) registered and return current graph state",
    "All 22 core and navigation tools respond with correct results",
    "All 12 mutation and management tools execute operations successfully",
    "Subscribed clients get graph and diagnostics delta notifications when a request finds the project changed and the incremental update is applied",
    "All 5 prompts (context, review, trace, explore, infer) return pre-composed workflows",
    "Protocol errors produce JSON-RPC error responses, not crashes",
    "Agents consume graph without CLI invocation",
  ]
}

milestone migration "Phase 14: Migration" {
  description   "Spec file migration with dry-run preview, backup, post-migration validation, rollback, and extension migration hook invocation."
  status        completed
  start_date    "2026-05-01"
  target_date   "2026-06-30"
  owner         "specforge-team"
  contributors  ["specforge-team"]
  depends_on    [extension_ecosystem]
  features      [spec_file_migration]
  modules       [specforge_cli, specforge_wasm]
  tags          ["h3", "tooling"]
  exit_criteria [
    "specforge migrate --dry-run shows unified diff of all proposed changes",
    "Backup created before in-place transformation",
    "Post-migration validation confirms graph structural equivalence",
    "When a migration hook fails or the migrated graph differs structurally, migrated files are restored from their .spec.bak backups; --rollback restores them on demand",
    "Extension migration hooks invoked in topological order",
  ]
}

milestone wasm_host_functions "Phase 15: Wasm Host Functions" {
  description   "The host-import surface extensions call into: graph queries, diagnostic emission, graph node and edge registration, scoped file reads, non-code file output and allowlisted HTTP, each granted per contribution call site. Planned: the component runtime's bridge world imports nothing today (its one export is call), so an extension reaches the host only through the typed extension calls."
  status        planned
  owner         "specforge-team"
  contributors  ["specforge-team"]
  depends_on    [wasm_runtime]
  features      [wasm_host_function_api]
  modules       [specforge_wasm]
  tags          ["h2", "runtime"]
  exit_criteria [
    "The bridge world imports the seven host functions (query_graph, emit_diagnostic, add_graph_node, add_graph_edge, read_file, emit_file, http_get) and an SDK guest calls each",
    "Per-call-site permissions enforce least-privilege for each contribution export",
    "An unauthorized host call is rejected and reported, without failing the compile",
  ]
}

milestone extension_catalog "Phase 16: Compliance, Embeddings and Markdown Extensions" {
  description   "The first-party extensions specified beside the builtins and not built yet: @specforge/compliance (its entity kinds, validation and reporting), @specforge/embeddings (entity embedding search) and @specforge/markdown-renderer (documentation generation). Their specs are under spec/extensions/; no extension source exists."
  status        planned
  owner         "specforge-team"
  contributors  ["specforge-team"]
  depends_on    [extension_ecosystem]
  features      [
    ce_core_entity_kinds,
    compliance_validation,
    compliance_reporting,
    entity_embedding_search,
    markdown_documentation_generation,
  ]
  tags          ["h2", "ecosystem"]
  exit_criteria [
    "Each extension builds as a wasip2 component, is vendored as a builtin or installable, and declares its kinds and rules",
    "Every feature listed here is done and proven by recorded tests",
  ]
}

milestone ms_followups "Phase 17: Product Graph Diff and Progressive Formal Warnings" {
  description   "Two features once listed in completed milestones that were never built: comparing product graph snapshots between builds (product_graph_diff) and the formal extension's warning levels (fa_progressive_warnings)."
  status        planned
  owner         "specforge-team"
  contributors  ["specforge-team"]
  depends_on    [extension_ecosystem]
  features      [product_graph_diff, fa_progressive_warnings]
  tags          ["h2"]
  exit_criteria [
    "A product command compares two recorded graph snapshots and reports structural and status changes",
    "warning_level (onboarding, standard, strict) in specforge.json gates the formal warnings",
  ]
}
