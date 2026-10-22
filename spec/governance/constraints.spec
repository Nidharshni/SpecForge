// Core constraints — non-functional requirements for the compiler engine
//
// Extension-specific constraints live in their respective extension directories
// under spec/extensions/.

use "behaviors/error-reporting"
use "behaviors/extensions"
use "behaviors/formatting"
use "behaviors/graph"
use "behaviors/incremental"
use "behaviors/init"
use "behaviors/lsp"
use "behaviors/output"
use "behaviors/output-schema"
use "behaviors/parsing"
use "behaviors/resolution"
use "behaviors/validation"
use "behaviors/wasm-authoring"
use "behaviors/wasm-extensions"
use "behaviors/wasm-host-functions"
use "behaviors/wasm-lifecycle"
use "behaviors/wasm-sandbox"
use "behaviors/zero-entity-registries"
use "behaviors/zero-entity-validation"
use "extensions/testing/behaviors"
use "invariants/core"
use "invariants/formatting"
use "invariants/validation"
use "invariants/wasm"

constraint incremental_compilation_latency "Incremental Compilation Latency" {
  description "Incremental recompilation after a file change must produce diagnostics within 100ms for projects up to 500 spec files."
  category    performance
  priority    critical
  metric      """
    file_change_to_diagnostics < 100ms
    with up to 500 .spec files in the project
  """
  constrains  [
    watch_file_system_for_changes,
    invalidate_changed_files,
    rebuild_affected_subgraph,
    emit_incremental_diagnostics,
  ]
  protects    [incremental_correctness]
  verify load "benchmark incremental recompile with 500 files, assert < 100ms"
}

constraint memory_usage "Memory Usage" {
  description "Peak memory consumption must stay under 50MB for projects with up to 500 spec files and 2000 entities."
  category    performance
  priority    critical
  metric      """
    peak_memory < 50MB
    for a project with 500 .spec files and approximately 2000 entities
  """
  constrains  [build_in_memory_graph, maintain_mutable_graph]
  protects    [string_interning_consistency]
  verify load "compile 500-file project, measure peak RSS < 50MB"
}

constraint cross_platform_compatibility "Cross-Platform Compatibility" {
  description "CLI and LSP binaries must build and run on Linux, macOS, and Windows with no platform-specific file path handling."
  category    compatibility
  priority    critical
  metric      """
    The CLI and LSP binaries MUST build and run on:
    Linux (x86_64, aarch64), macOS (x86_64, aarch64), Windows (x86_64).
    No platform-specific file path handling.
  """
  constrains  [print_diagnostics_structured, watch_file_system_for_changes]
  protects    [diagnostic_determinism]
  verify integration "CI matrix tests on all 5 platform targets"
}

constraint diagnostic_quality "Diagnostic Quality" {
  description "Every error diagnostic must include file path, line/column numbers, a context snippet, and actionable suggestions when a close match exists."
  category    usability
  priority    critical
  metric      """
    Every error diagnostic MUST include: file path, line number,
    column number, context snippet, and at least one actionable
    suggestion when a close match exists.
  """
  constrains  [
    format_diagnostics_with_source_context,
    provide_did_you_mean_suggestions,
    aggregate_diagnostic_summary,
  ]
  protects    [diagnostic_determinism]
  verify unit "all error diagnostics include required fields"
}

constraint zero_runtime_dependencies "Zero Runtime Dependencies" {
  description "CLI and LSP binaries must be statically linked with zero runtime dependencies beyond the OS, requiring no Node.js, Python, or JVM."
  category    portability
  priority    critical
  metric      """
    The specforge-cli and specforge-lsp binaries MUST be statically
    linked with zero runtime dependencies beyond the OS. No Node.js,
    Python, or JVM required at runtime. The wasmtime runtime is statically
    linked into the binary.
  """
  constrains  [check_mode_for_ci, shared_incremental_pipeline]
  protects    [multi_error_collection]
  verify integration "binary runs on clean OS install without additional packages"
}

constraint backward_compatibility "Backward Compatibility" {
  description "The compiler should parse spec files from the previous format version without errors, with deprecation warnings acceptable."
  category    compatibility
  priority    high
  metric      """
    The compiler SHOULD parse .spec files from the previous format version
    without errors. Deprecation warnings are acceptable.
  """
  constrains  [parse_spec_file_to_ast, recover_from_syntax_errors]
  protects    [multi_error_collection, spec_root_singleton]
  verify integration "previous version files compile with current compiler"
}

constraint cli_scaffolding_robustness "CLI Scaffolding Robustness" {
  description "Init and add commands must produce valid specforge.json and starter spec files without overwriting existing files or diverging between interactive and non-interactive modes."
  category    usability
  priority    high
  metric      """
    init/add commands produce valid, parseable specforge.json and a
    starter .spec file; existing files never overwritten without
    consent; non-interactive mode produces identical output to
    interactive mode for the same inputs
  """
  constrains  [
    scaffold_new_project,
    scaffold_starter_spec_file,
    interactive_extension_selection,
    non_interactive_init,
    add_extension_to_existing_project,
    graceful_zero_extension_init,
  ]
  protects    [spec_root_singleton, entity_id_uniqueness, init_config_validity]
  verify integration "scaffold and add commands produce valid spec files"
  verify integration "non-interactive init produces same output as interactive for same inputs"
}

constraint parser_correctness "Parser Correctness" {
  description "The parser must produce correct ASTs for all valid inputs and recover from all syntax errors without crashing."
  category    reliability
  priority    critical
  metric      """
    parser produces correct AST for all valid inputs and recovers
    from all syntax errors without crashing
  """
  constrains  [
    parse_spec_file_to_ast,
    recover_from_syntax_errors,
    parse_use_imports,
    parse_all_block_types,
    parse_triple_quoted_strings,
  ]
  protects    [multi_error_collection]
  verify unit "parser handles all valid inputs and recovers from errors"
}

constraint editor_integration_quality "Editor Integration Quality" {
  description "Tree-sitter query files must load without errors in editors, and extension entities must receive syntax highlighting via the generic entity_block rule."
  category    usability
  priority    high
  metric      """
    highlights.scm, folds.scm, indents.scm load without errors
    in Tree-sitter editors; extension entities receive syntax
    highlighting via generic entity_block rule and extension
    query extensions
  """
  constrains  [
    provide_syntax_highlighting_queries,
    provide_code_folding_queries,
    provide_indentation_queries,
    parse_all_block_types,
  ]
  protects    [multi_error_collection]
  verify integration "all query files load in Tree-sitter editors"
  verify integration "extension entities receive highlighting via generic entity_block"
}

constraint reference_resolution_correctness "Reference Resolution Correctness" {
  description "Every valid reference must resolve exactly once and every invalid reference must produce exactly one diagnostic."
  category    reliability
  priority    critical
  metric      """
    every valid reference resolves exactly once; every invalid
    reference produces exactly one diagnostic
  """
  constrains  [
    resolve_use_imports,
    detect_import_cycles,
    link_entity_references,
    resolve_soft_cross_extension_references,
    resolve_external_ref_declarations,
  ]
  protects    [import_dag, reference_resolution_completeness]
  verify unit "references resolve correctly with no false positives or negatives"
}

constraint validation_pass_correctness "Validation Pass Correctness" {
  description "Each validation code must fire if and only if its specified condition holds, with zero false positives and zero false negatives."
  category    reliability
  priority    critical
  metric      """
    each validation code fires if and only if the specified condition
    holds; zero false positives, zero false negatives
  """
  constrains  [
    detect_dangling_references,
    detect_duplicate_entity_ids,
    detect_import_cycles,
    detect_unreferenced_refs,
    validate_file_reference_paths,
  ]
  protects    [reference_resolution_completeness]
  verify unit "each validation code fires iff its condition holds"
}

constraint extension_validation_correctness "Extension Validation Correctness" {
  description "Extension validation rules must run only when the owning extension is installed, with no false positives when the extension is absent."
  category    reliability
  priority    critical
  metric      """
    extension-specific validation rules run only when the owning extension is
    installed; no false positives when extension absent; the core declarative
    validation engine executes ValidationRulePattern entries from extension
    manifests without knowledge of domain-specific entity types
  """
  constrains  [execute_validation_pattern, registry_build_kinds]
  protects    [reference_resolution_completeness]
  verify unit "extension checks activate only when extension is installed"
  verify unit "uninstalled extension rules do not fire"
}

constraint output_format_correctness "Output Format Correctness" {
  description "Every emitter must produce syntactically valid output in its target format and produce deterministic results across runs."
  category    reliability
  priority    critical
  metric      """
    every emitter produces syntactically valid output in its format
    (JSON, DOT); deterministic across runs
  """
  constrains  [
    serialize_json_graph,
    serialize_dot_visualization,
    compute_traceability_chain,
    compute_project_statistics,
    exit_code_reflects_diagnostic_severity,
    serialize_traceability_data,
    deterministic_output,
    export_agent_context_format,
    export_agent_brief_format,
    export_agent_graph_format,
    query_graph_multi_resolution,
  ]
  protects    [diagnostic_determinism]
  verify unit "all emitters produce syntactically valid, deterministic output"
}

constraint lsp_responsiveness "LSP Responsiveness" {
  description "All LSP features including go-to-definition, hover, and completion must respond within 50ms on a 500-file project."
  category    usability
  priority    high
  metric      """
    go-to-def, hover, completion respond within 50ms
    on a 500-file project
  """
  constrains  [
    go_to_definition,
    find_all_references,
    hover_information,
    autocomplete_entity_ids,
    rename_entity_id,
    emit_live_diagnostics,
    code_actions_for_missing_verify,
    outline_view,
    workspace_symbol_search,
    shared_incremental_pipeline,
    provide_semantic_tokens,
    complete_field_names,
    complete_keywords,
    goto_import_definition,
    code_action_create_entity_stub,
    code_action_replace_unresolved,
    incremental_document_sync,
  ]
  protects    [incremental_correctness]
  verify load "LSP features respond within 50ms on 500-file project"
}

constraint traceability_completeness "Traceability Completeness" {
  description "Traceability analysis must detect all broken links in the chain with zero false negatives, including missing tests, non-existent paths, and absent report results."
  category    reliability
  priority    critical
  metric      """
    specforge analyze coverage MUST detect all broken links in the traceability
    chain with zero false negatives: testable entities with verify
    declarations but no recorded test results, and recorded results
    that name unknown entities.
  """
  constrains  [ingest_collector_report, te_coverage_pass, validate_file_reference_paths]
  protects    [traceability_chain_integrity]
  verify unit "all broken traceability links are detected with zero false negatives"
}

constraint extension_system_integrity "Extension System Integrity" {
  description "Extension install and remove operations must never corrupt specforge.json, and the Wasm sandbox must contain all extension execution with traps producing diagnostics rather than crashes."
  category    reliability
  priority    critical
  metric      """
    extension install/remove never corrupts specforge.json; Wasm sandbox
    contains all extension execution; Wasm traps produce diagnostics, not
    crashes; provider registration survives reload; timeout enforcement
    prevents runaway extensions
  """
  constrains  [
    load_extension_manifests,
    register_extension_entity_types,
    load_provider_configurations,
    validate_provider_refs,
    remove_extension,
    list_installed_extensions,
    list_configured_providers,
    validate_ref_target_format,
    validate_provider_kinds,
    load_wasm_module,
    call_extension_exports,
    initialize_wasm_extension,
    enforce_wasm_sandbox,
    registry_build_peer_dependencies,
    provide_host_function_query_graph,
    provide_host_function_emit_diagnostic,
    provide_host_function_add_graph_node,
    provide_host_function_add_graph_edge,
    reuse_session_runtime,
    registry_build_load_order,
    load_extension_manifest,
    register_entity_enhancements,
    run_doctor_check,
    scaffold_wasm_extension_project,
    build_wasm_extension,
    validate_wasm_extension_locally,
    publish_wasm_extension,
    upgrade_wasm_extension,
    validate_extension_manifest,
    parse_extension_specifier,
    resolve_extension_source,
    write_lock_file,
    read_lock_file,
  ]
  protects    [spec_root_singleton, reference_resolution_completeness, wasm_sandbox_integrity]
  verify integration "extension operations never corrupt state or crash"
  verify integration "Wasm traps produce diagnostics without crashing the compiler"
}

constraint wasm_compile_cache_budget "Wasm Compile Cache Budget" {
  description "Wasm extensions must load from the compile cache in under 50ms, and first compilation must complete in under 500ms for typical components."
  category    performance
  priority    critical
  metric      """
    Each Wasm extension MUST load in under 50ms on a compile-cache hit.
    First compilation (cache cold) SHOULD complete in under 500ms for a
    typical component (<1MB .wasm).
  """
  constrains  [load_wasm_module, compile_wasm_component_with_cache, reuse_session_runtime]
  protects    [extension_load_order_determinism]
  verify load "benchmark compile-cache-hit extension load, assert < 50ms"
  verify load "benchmark first compilation of a 1MB component, assert < 500ms"
}

constraint wasm_memory_limit "Wasm Memory Limit" {
  description "Each Wasm extension instance is limited to the linear memory its handshake declares (max_memory_mb), held to the host ceiling of 512 MB, which is also the limit of an extension declaring none; a growth past it traps the call."
  category    performance
  priority    critical
  metric      """
    Each Wasm extension instance MUST be limited to its declared
    max_memory_mb of linear memory and tables, all its memories counted
    together, at most 512 MB; an extension declaring
    no limit MUST be limited to 512 MB. A growth past the limit MUST trap
    the call (memory_limit_exceeded, reported as E028). No limit is shared
    across extensions: one extension's memory never fails another
    (extension_isolation, ADR 0037).
  """
  constrains  [enforce_wasm_sandbox, load_wasm_module]
  protects    [wasm_sandbox_integrity]
  verify unit "an extension growing past its memory limit traps"
  verify unit "an extension declaring no memory limit is held to the 512 MB ceiling"
  verify unit "the memory limit holds for every memory of an extension together"
  verify unit "an extension cannot grow a shared memory past its memory limit"
  verify unit "an extension cannot grow a table without bound"
}

constraint wasm_sandbox_enforcement "Wasm Sandbox Enforcement" {
  description "Extensions must not access the host filesystem, network, environment variables, arguments, stdin or process control: the host grants a component no WASI capability, whatever its declaration asks for, with zero sandbox escapes in adversarial testing."
  category    security
  priority    critical
  metric      """
    Zero sandbox escapes in adversarial testing. A component's WASI
    context MUST preopen no directory and pass no environment, arguments
    or stdin, and MUST refuse TCP, UDP and name lookup. No declaration
    (sandbox_policy, a surface's sandbox override) grants a capability
    (ADR 0037); host functions that would grant scoped access are planned
    (wasm_host_function_api) and none exists.
  """
  constrains  [
    enforce_wasm_sandbox,
    provide_host_function_emit_file,
    provide_host_function_http_get,
    configure_sandbox_policy,
    enforce_per_call_site_permissions,
  ]
  protects    [wasm_sandbox_integrity]
  verify integration "an extension that tries every capability a component can reach is granted none"
  verify unit "direct filesystem access from Wasm is blocked"
  verify unit "direct network access from Wasm is blocked"
}

constraint wasm_binary_size_limit "Wasm Binary Size Limit" {
  description "Extension Wasm binaries should be under 5MB, with a warning diagnostic for binaries exceeding 10MB to ensure reasonable download and compilation times."
  category    portability
  priority    high
  metric      """
    Each extension .wasm binary SHOULD be under 5MB. Binaries exceeding 10MB
    MUST produce a warning diagnostic at install time. This ensures
    reasonable download times and first-compilation latency.
  """
  constrains  [install_wasm_extension, build_wasm_extension, compile_wasm_component_with_cache]
  protects    [extension_load_order_determinism]
  verify unit "extension under 5MB installs without warning"
  verify unit "extension over 10MB produces warning at install"
}

constraint extension_count_limit "Extension Count Limit" {
  description "The system should support up to 20 extensions without degradation, warn at 21-50, and refuse more than 50 with a hard error."
  category    performance
  priority    high
  metric      """
    The system SHOULD support up to 20 extensions without performance
    degradation. 21-50 extensions MUST produce a warning diagnostic.
    More than 50 extensions MUST be refused with a hard error.
  """
  constrains  [load_wasm_module, initialize_wasm_extension, registry_build_load_order]
  protects    [wasm_sandbox_integrity]
  verify unit "20 extensions load without warning"
  verify unit "21+ extensions produce warning"
  verify unit "51+ extensions produce hard error"
}

constraint wasm_compile_cache_size_limit "Wasm Compile Cache Size Limit" {
  description "The wasmtime compile cache directory must stay under 500MB total; the engine's built-in size-based cleanup removes least-recently-used entries beyond the soft limit."
  category    portability
  priority    high
  metric      """
    The compile cache directory SHOULD stay under 500MB total. The
    runtime configures wasmtime's cache with a 500MB files-total-size
    soft limit; when exceeded, the engine's cleanup removes
    least-recently-used entries on subsequent runs. No info diagnostic
    is required — cleanup is engine-internal.
  """
  constrains  [compile_wasm_component_with_cache]
  protects    [wasm_compile_cache_integrity]
  verify unit "cache under 500MB operates normally"
  verify unit "cache configured with 500MB files-total-size soft limit"
}

constraint formatting_correctness "Formatting Correctness" {
  description "Formatter output must be idempotent, preserve all comments, produce an identical compiler graph, and complete within 50ms per file."
  category    reliability
  priority    critical
  metric      """
    formatter output is idempotent (format(format(x)) == format(x));
    all comments preserved; formatted files produce identical compiler graph;
    single file formatted in under 50ms
  """
  constrains  [
    format_spec_files,
    preserve_comments,
    check_formatting,
    show_formatting_diff,
    format_from_stdin,
    load_format_config,
    apply_format_rules,
    maintain_format_idempotency,
    lsp_format_document,
    lsp_format_range,
    lsp_respect_editor_config,
  ]
  protects    [formatting_idempotency, comment_preservation, formatting_consistency]
  verify property "format(format(x)) == format(x) for all valid inputs"
  verify unit "all comments preserved after formatting round-trip"
}

constraint schema_publication_accessibility "Schema Publication Accessibility" {
  description "The Graph Protocol JSON Schema must be publicly accessible without authentication at a URL that remains stable across patch versions."
  category    interoperability
  priority    critical
  metric      """
    The Graph Protocol schema MUST be publicly accessible as a standalone
    JSON Schema document. The published schema MUST be downloadable without
    authentication. The schema URL MUST be stable across patch versions.
  """
  constrains  [publish_schema_specification, embed_schema_in_export, serve_schema_resource]
  protects    [graph_schema_completeness, schema_version_backward_compatibility]
  verify integration "published schema is accessible without authentication"
  verify unit "schema URL is stable across patch versions"
}
