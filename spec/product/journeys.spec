// Journeys — UX flows mapping personas to features
// SpecForge is the structured context standard for AI agents.
// The graph is the product — consumed by any agent for any task.

use "extensions/cargo-test/features"
use "extensions/testing/features"
use "extensions/vitest/features"
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
use "product/channels"
use "product/features"
use "product/personas"

defaults journey {
  channels [cli]
}

// ── Developer + CLI ──────────────────────────────────────────

journey initialize_a_new_spec_project "Initialize a New Spec Project" {
  persona  developer
  priority critical
  tags     ["developer", "cli"]
  features [project_initialization]
  flow     """
    1. Developer runs specforge init in an empty directory
    2. System prompts for project name (version defaults to 0.1.0)
    3. System offers interactive extension selection
    4. Developer selects desired extensions (or none — zero extensions is valid)
    5. System generates specforge.json with configuration
    6. System creates a starter .spec file using structural syntax (extensions MAY contribute templates, but core generates only generic entity blocks)
    7. Developer runs specforge check — validates immediately
    8. Developer runs specforge export — produces Graph Protocol JSON
    9. Success: project is ready for .spec file authoring in under 60 seconds via three commands (init, check, export)
  """
}

journey initialize_project_non_interactively "Initialize a Project Non-Interactively" {
  persona  ci
  priority high
  tags     ["ci", "cli"]
  features [project_initialization]
  flow     """
    1. CI script runs specforge init --name myproject --extensions @specforge/software
    2. System creates specforge.json with specified configuration (no prompts)
    3. System creates a starter .spec file
    4. CI script runs specforge check — validates with zero errors
    5. CI script runs specforge export — produces Graph Protocol JSON
    6. Success: project is ready for automated spec validation via three commands (init, check, export)
  """
}

journey initialize_project_as_agent "Initialize a Project as an AI Agent" {
  persona  agent
  priority high
  tags     ["agent", "cli"]
  features [project_initialization]
  flow     """
    1. AI agent runs specforge init --name myproject --extensions @specforge/software,@specforge/product
    2. System creates specforge.json with specified configuration (no prompts, no TTY required)
    3. System creates a starter .spec file using structural syntax
    4. Agent runs specforge check to validate the project
    5. Agent runs specforge export --format=context to get the initial graph
    6. Agent uses the graph as structured context for subsequent tasks
    7. Success: agent has a consumable graph in under 60 seconds
  """
}

journey validate_spec_files "Validate Spec Files" {
  persona  developer
  priority critical
  tags     ["developer", "cli"]
  features [
    spec_file_parsing,
    error_recovery_during_parsing,
    reference_resolution,
    graph_construction,
    structural_validation,
    diagnostic_reporting,
    declarative_validation_rules,
    zero_entity_bootstrap,
  ]
  flow     """
    1. Developer runs specforge check
    2. System parses all .spec files
    3. System resolves imports and links references
    4. System validates the graph
    5. Success: diagnostics printed, exit code 0 if clean
    6. Failure: errors printed in rustc style with suggestions
  """
}

journey watch_for_changes "Watch for Changes" {
  persona  developer
  priority critical
  tags     ["developer", "cli"]
  features [incremental_compilation]
  flow     """
    1. Developer runs specforge watch
    2. System performs initial full compilation
    3. Developer edits a .spec file
    4. System detects change within 100ms
    5. System incrementally recompiles affected files
    6. Updated diagnostics are printed
    7. Cycle repeats on each file change
  """
}

journey generate_documentation "Generate Documentation" {
  persona  developer
  priority high
  tags     ["developer", "cli"]
  features [json_and_dot_render]
  flow     """
    1. Developer installs a renderer extension (e.g., @specforge/markdown-renderer)
    2. Developer runs specforge render markdown ./docs/
    3. Renderer extension traverses the graph via specforge.query_graph host function
    4. Renderer produces human-readable documentation files via specforge.emit_file
    5. Developer reviews generated documentation
    6. Success: documentation reflects current spec state
    7. Note: SpecForge renders documentation, not code — code generation is the agent's job
  """
}

journey export_graph_formats "Export Graph Formats" {
  persona  developer
  priority high
  tags     ["developer", "cli"]
  features [json_and_dot_render, traceability_serialization]
  flow     """
    1. Developer runs specforge render json ./output/ or specforge graph
    2. System traverses the graph and serializes in the requested format
    3. JSON export produces full Graph Protocol output; DOT produces Graphviz visualization
    4. Success: machine-readable graph written to output directory
  """
}

journey trace_requirements "Trace Requirements" {
  persona  developer
  priority high
  tags     ["developer", "cli"]
  features [traceability_serialization]
  flow     """
    1. Developer runs specforge trace scaffold_new_project
    2. System traverses upstream following all registered edge types
    3. System traverses downstream following all registered edge types
    4. Trace chain is printed showing all links
    5. Missing links are flagged
  """
}

// specforge export is a separate command from specforge render — export writes to stdout in agent-optimized formats, render writes files

journey export_graph_for_agents "Export Graph for AI Agents" {
  persona  developer
  channels [cli, ch_graph_protocol]
  priority critical
  tags     ["developer", "cli"]
  features [json_and_dot_render, agent_export]
  flow     """
    1. Developer runs specforge export --format=context
    2. System compiles all .spec files into the entity graph
    3. System serializes graph in token-optimized format for AI agent consumption
    4. Developer feeds output to any AI agent (coding, PM, compliance, docs, security)
    5. Agent performs task correctly on first attempt using structured context
    6. Alternative formats: --format=graph (full JSON), --format=brief (IDs + contracts only)
  """
}

journey check_test_coverage "Check Test Coverage" {
  persona  developer
  priority high
  tags     ["developer", "cli"]
  features [test_result_collection, te_coverage_analysis]
  flow     """
    1. Developer runs specforge collect
    2. System runs each detected runner extension's approved test command
       (e.g. vitest, cargo test) and records results in specforge-report.json
    3. Developer runs specforge analyze coverage
    4. System scores intent, enforcement and proof per entity
    5. Success: coverage findings and summary printed
    6. Developer sets --min threshold for CI gating
  """
}

journey trace_test_coverage "Trace Test Coverage" {
  persona  developer
  priority high
  tags     ["developer", "cli"]
  features [te_coverage_analysis]
  flow     """
    1. Developer runs specforge analyze coverage after specforge collect
    2. System reads the recorded results from specforge-report.json
    3. System reports testable entities without obligations (A001), with
       failing tests (A014), and the discharge funnel of proven entities
    4. Developer reviews the findings to identify gaps
    5. Developer fills gaps by adding tests annotated with the entity they prove
  """
}

journey diagnose_extension_issues "Diagnose Extension Issues" {
  persona  developer
  priority medium
  tags     ["developer", "cli"]
  features [entity_enhancement, extension_management, wasm_extension_runtime, extension_manifest]
  flow     """
    1. Developer runs specforge doctor
    2. System loads all extension manifests
    3. System builds KindRegistry and FieldRegistry from extension declarations
    4. System detects entity kind conflicts and enhancement conflicts
    5. System checks Wasm compile cache health
    6. System produces a report listing installed extensions, entity kinds,
       enhancements, conflicts with actionable resolution suggestions, and cache status
    7. Developer resolves issues based on report
  """
}

journey manage_extensions "Manage Extensions" {
  persona  developer
  priority medium
  tags     ["developer", "cli"]
  features [extension_management, wasm_extension_runtime, entity_enhancement, extension_manifest]
  flow     """
    1. Developer runs specforge add @specforge/governance
    2. System downloads the .wasm extension binary
    3. System verifies the binary SHA256 and places it atomically under .specforge/extensions/
    4. System adds extension to specforge.json
    5. New entity kinds become available from the extension's domain vocabulary
    6. Developer runs specforge extensions to verify installed extensions
    7. Developer can later run specforge remove to uninstall
  """
}

journey install_domain_extensions "Install Domain Extensions" {
  persona  developer
  priority medium
  tags     ["developer", "cli"]
  features [extension_management, wasm_extension_runtime, dynamic_entity_registration]
  flow     """
    1. Developer runs specforge add @specforge/atomic-design
    2. System resolves extension from registry
    3. System downloads .wasm binary and validates manifest
    4. System verifies the binary SHA256 and places it atomically under .specforge/extensions/
    5. System adds extension to specforge.json extensions list
    6. New entity kinds (atom, molecule, organism, template, page) become available
    7. Developer writes .spec files using the new domain vocabulary
    8. All validation, LSP, and graph export work with new entity types
  """
}

journey configure_ref_providers "Configure Ref Providers" {
  persona  developer
  priority medium
  tags     ["developer", "cli"]
  features [provider_based_ref_validation]
  flow     """
    1. Developer adds a providers block to specforge.json
    2. Developer configures one or more provider instances with aliases
    3. Developer writes ref entities using registered schemes (e.g., gh.issue:42)
    4. Compiler delegates validation to the matching provider instance
    5. Developer runs specforge providers to verify configured instances
    6. Success: valid refs are resolved, invalid identifiers produce diagnostics
    7. Developer can configure multiple instances of the same provider for different repos
  """
}

journey view_project_statistics "View Project Statistics" {
  persona  developer
  priority medium
  tags     ["developer", "cli"]
  features [ci_integration]
  flow     """
    1. Developer runs specforge stats
    2. System computes entity counts, coverage %, and unreferenced count
    3. Summary table is printed
    4. Developer uses this to track project health
  """
}

journey j_query_graph_multi_resolution "Query Graph at Multiple Resolutions" {
  persona  developer
  priority medium
  tags     ["developer", "cli"]
  features [agent_export]
  flow     """
    1. Developer runs specforge query --scope=verify auth_login
    2. System returns only the verify declarations for the specified behavior
    3. Developer runs specforge query --depth=1 auth_login for adjacent entities
    4. Developer runs specforge query --scope=deep auth_feature for full subgraph
    5. Agent receives exactly the context slice it needs — no more, no less
    6. Works for any agent type: coding, PM, compliance, documentation
  """
}

journey j_format_spec_files "Format Spec Files" {
  persona  developer
  priority high
  tags     ["developer", "cli"]
  features [code_formatting]
  flow     """
    1. Developer runs specforge format
    2. System discovers all .spec files in the project
    3. System parses each file into a CST via tree-sitter
    4. System applies formatting rules and writes formatted output
    5. Success: formatted file names and summary count printed
    6. Developer optionally runs specforge format --diff to preview changes
  """
}

journey check_formatting_in_ci "Check Formatting in CI" {
  persona  ci
  channels [ci_surface]
  priority high
  tags     ["ci", "automation"]
  features [code_formatting]
  flow     """
    1. CI pipeline runs specforge format --check
    2. System compares formatted output against existing files
    3. No changes needed: exit 0
    4. Changes needed: exit 1 with list of unformatted files
    5. Developer must run specforge format and commit
  """
}

journey format_on_save "Format on Save in IDE" {
  persona  developer
  channels [ide]
  priority medium
  tags     ["developer", "ide"]
  features [lsp_formatting]
  flow     """
    1. Developer enables format-on-save in their editor
    2. Developer edits a .spec file
    3. Developer saves the file
    4. LSP receives textDocument/formatting request
    5. LSP formats the document and returns TextEdit operations
    6. Editor applies edits atomically
    7. File is saved with canonical formatting
  """
}

// ── Developer + IDE ──────────────────────────────────────────

journey navigate_to_entity_definitions "Navigate to Entity Definitions" {
  persona  developer
  channels [ide]
  priority high
  tags     ["developer", "ide"]
  features [go_to_definition_and_references]
  flow     """
    1. Developer Ctrl+clicks on an entity ID in a .spec file
    2. LSP finds the entity's declaration
    3. Editor navigates to the declaration file and line
    4. Developer Ctrl+clicks on references to navigate further
  """
}

journey explore_entity_references "Explore Entity References" {
  persona  developer
  channels [ide]
  priority medium
  tags     ["developer", "ide"]
  features [go_to_definition_and_references]
  flow     """
    1. Developer right-clicks an entity ID
    2. Developer selects "Find All References"
    3. LSP returns all reference sites across workspace
    4. Developer clicks on results to navigate
  """
}

journey get_inline_help "Get Inline Help" {
  persona  developer
  channels [ide]
  priority medium
  tags     ["developer", "ide"]
  features [hover_and_autocomplete, extension_driven_lsp]
  flow     """
    1. Developer hovers over an entity ID
    2. Popup shows entity title, contract/guarantee, reference count
    3. Developer types in reference list [INV-
    4. Autocomplete suggests matching invariants with titles
    5. Developer selects from suggestions
  """
}

journey rename_entities_safely "Rename Entities Safely" {
  persona  developer
  channels [ide]
  priority medium
  tags     ["developer", "ide"]
  features [rename_refactoring]
  flow     """
    1. Developer triggers rename on an entity ID (F2)
    2. Developer types new ID
    3. LSP updates declaration and all references atomically
    4. All .spec files reflect the new name
  """
}

journey see_live_errors_while_typing "See Live Errors While Typing" {
  persona  developer
  channels [ide]
  priority high
  tags     ["developer", "ide"]
  features [live_diagnostics, extension_driven_lsp]
  flow     """
    1. Developer edits a .spec file
    2. LSP incrementally recompiles within 100ms
    3. Red squiggles appear on broken references
    4. Yellow squiggles appear on unreferenced entities
    5. Semantic tokens color entity IDs by type and highlight keywords
    6. Diagnostics update in real time as user types
  """
}

journey suggest_test_declarations_from_ide "Suggest Test Declarations from IDE" {
  persona  developer
  channels [ide]
  priority medium
  tags     ["developer", "ide"]
  features [code_actions]
  flow     """
    1. Developer opens a .spec file with untested behaviors
    2. LSP shows a lightbulb code action on behaviors lacking test coverage
    3. Developer clicks the code action
    4. LSP suggests adding verify declarations or linking test files
    5. Success: code action applies the suggested edit
  """
}

journey browse_file_structure "Browse File Structure" {
  persona  developer
  channels [ide]
  priority medium
  tags     ["developer", "ide"]
  features [outline_and_symbol_search, extension_driven_lsp]
  flow     """
    1. Developer opens outline panel in IDE
    2. LSP shows tree of entities in the current file
    3. Developer clicks on entity to jump to its location
    4. Developer uses workspace symbol search to find entities across files
  """
}

journey get_syntax_highlighting_without_lsp "Get Syntax Highlighting Without LSP" {
  persona  developer
  channels [ide]
  priority medium
  tags     ["developer", "ide"]
  features [editor_query_files]
  flow     """
    1. Developer installs tree-sitter-specforge grammar in their editor
    2. Editor loads highlights.scm, folds.scm, and indents.scm
    3. Developer opens a .spec file
    4. Keywords, strings, entity IDs, and types are syntax-highlighted
    5. Brace-delimited blocks are foldable
    6. Indentation adjusts automatically on new lines after braces
    7. No LSP server required
  """
}

// ── Product Manager + CLI ────────────────────────────────────

journey review_milestone_progress "Review Milestone Progress" {
  persona  product_manager
  priority medium
  tags     ["product_manager", "cli"]
  features [json_and_dot_render, traceability_serialization]
  flow     """
    1. Product manager runs specforge export --format=graph
    2. System computes milestone completion ratios for all milestones
    3. Product manager reviews which features are implemented vs unknown
    4. Product manager identifies blocked milestones via dependency chain
    5. Product manager exports data to a dashboard or spreadsheet
  """
}

journey review_deliverable_scope "Review Deliverable Scope" {
  persona  product_manager
  priority medium
  tags     ["product_manager", "cli"]
  features [traceability_serialization, json_and_dot_render]
  flow     """
    1. Product manager runs specforge trace (no arguments)
    2. System prints full traceability for all deliverables
    3. Product manager reviews which journeys and modules compose each deliverable
    4. Product manager identifies gaps in journey-to-module feature coverage
    5. Product manager communicates delivery status to stakeholders
  """
}

journey review_term_glossary "Review Term Glossary" {
  persona  product_manager
  priority medium
  tags     ["product_manager", "cli"]
  features [json_and_dot_render]
  flow     """
    1. Product manager runs specforge product terms to list all defined terms
    2. System returns paginated term list with definitions and alias counts
    3. Product manager identifies terms with zero see_also links (I010)
    4. Product manager reads the default human table, or --format json for tooling
    5. Product manager uses term graph query to explore related terms
    6. Product manager verifies shared vocabulary consistency across the team
    7. Success: glossary is reviewed and terminology drift is caught early
  """
}

journey review_persona_channel_landscape "Review Persona and Channel Landscape" {
  persona  product_manager
  priority medium
  tags     ["product_manager", "cli"]
  features [json_and_dot_render]
  flow     """
    1. Product manager runs specforge product:personas to list all personas
    2. Product manager runs specforge product:channels to list all channels
    3. Product manager identifies deprecated personas/channels without reasons (I069, I070)
    4. Product manager uses persona-journeys and channel-journeys queries to review coverage
    5. Product manager identifies unreferenced personas (I046) and channels (I047) not used by any journey
    6. Success: persona and channel landscape is reviewed for completeness and lifecycle hygiene
  """
}

// ── Architect + CLI ──────────────────────────────────────────

journey export_graph_as_json "Export Graph as JSON" {
  persona  architect
  priority medium
  tags     ["architect", "cli"]
  features [json_and_dot_render]
  flow     """
    1. Architect runs specforge render json ./output/
    2. System traverses the full graph
    3. System serializes all entities, edges, and metadata to JSON
    4. Success: JSON file written to output directory
    5. Architect feeds JSON to external dashboards, analyzers, or custom tooling
  """
}

journey review_full_traceability "Review Full Traceability" {
  persona  architect
  priority medium
  tags     ["architect", "cli"]
  features [traceability_serialization]
  flow     """
    1. Architect runs specforge trace (no arguments)
    2. System prints full traceability across all registered edge types
    3. Gaps in the chain are highlighted
    4. Architect reviews chain completeness
    5. Architect addresses gaps by adding missing links
  """
}

journey visualize_spec_graph "Visualize Spec Graph" {
  persona  architect
  priority medium
  tags     ["architect", "cli"]
  features [json_and_dot_render]
  flow     """
    1. Architect runs specforge graph | dot -Tsvg > spec.svg
    2. System emits DOT format graph
    3. Graphviz renders SVG visualization
    4. Architect reviews architecture and dependencies
  """
}

// ── Contributor + CLI ────────────────────────────────────────

journey author_a_domain_extension "Author a Domain Extension" {
  persona  contributor
  priority low
  tags     ["contributor", "cli"]
  features [extension_management, wasm_extension_authoring]
  flow     """
    1. Contributor runs specforge extension init to scaffold a Wasm domain extension
    2. Contributor defines entity_kinds, edge_types, and validation_rules in manifest
    3. Contributor implements validation exports using the PDK
    4. Contributor runs specforge extension build to compile to .wasm
    5. Contributor runs specforge extension test to verify against fixtures
    6. Contributor installs locally via specforge add ./path/to/extension.wasm
    7. Compiler loads .wasm and registers the domain vocabulary
    8. Contributor writes .spec files using the new entity kinds
    9. Success: extension entities participate in resolution, validation, and graph export
    10. Failure: Wasm traps or manifest errors are reported as diagnostics
  """
}

journey author_a_custom_provider "Author a Custom Provider" {
  persona  contributor
  priority low
  tags     ["contributor", "cli"]
  features [provider_based_ref_validation]
  flow     """
    1. Contributor creates a provider extension implementing the ref validation interface
    2. Provider registers supported schemes and kinds
    3. Contributor configures a test instance in specforge.json providers block
    4. Developer writes ref entities using the provider's scheme
    5. Compiler delegates ref validation to the provider
    6. Success: valid refs pass, invalid identifiers produce diagnostics
    7. Failure: unknown schemes emit I005 if provider not installed
  """
}

journey scaffold_wasm_extension "Scaffold a Wasm Extension" {
  persona  contributor
  priority low
  tags     ["contributor", "cli"]
  features [wasm_extension_authoring]
  flow     """
    1. Contributor runs specforge extension init
    2. System prompts for extension name and contribution types (entities, validators, renderers, providers, parsers, collectors)
    3. System scaffolds project with manifest v2, src/ skeleton, and build config
    4. Manifest declares entity_kinds, edge_types, validation_rules
    5. README includes PDK documentation and examples
    6. Success: project is ready for development
  """
}

journey test_wasm_extension_locally "Test Wasm Extension Locally" {
  persona  contributor
  priority low
  tags     ["contributor", "cli"]
  features [wasm_extension_authoring]
  flow     """
    1. Contributor runs specforge extension test
    2. System builds .wasm binary
    3. System loads binary in production sandbox
    4. System runs against test fixtures
    5. Success: test results printed
    6. Failure: sandbox violations or Wasm traps reported
  """
}

journey j_publish_wasm_extension "Publish Wasm Extension" {
  persona  contributor
  priority low
  tags     ["contributor", "cli"]
  features [wasm_extension_authoring]
  flow     """
    1. Contributor runs specforge extension publish
    2. System validates manifest v2 and .wasm binary
    3. System bundles binary and manifest
    4. System publishes to configured registry (npm, OCI, or GitHub Releases)
    5. Success: extension URL printed
    6. Failure: validation errors reported
  """
}

// ── Developer + CLI (Agent Integration) ──────────────────────

journey j_validate_agent_plan "Validate Agent Implementation Plan" {
  persona  developer
  priority medium
  tags     ["developer", "agent_integration"]
  features [traceability_serialization]
  flow     """
    1. AI agent produces a plan.json mapping entities to planned actions
    2. Developer runs specforge trace --plan plan.json
    3. System validates every testable entity has a planned implementation
    4. System checks no unreferenced plans reference nonexistent entities
    5. System verifies dependency order is valid
    6. Success: plan is consistent with the spec graph
    7. Failure: gaps and inconsistencies reported with suggestions
  """
}

journey j_collect_rust_test_results "Collect Rust Test Results" {
  persona  developer
  priority medium
  tags     ["developer", "agent_integration"]
  features [ct_cargo_test_collection]
  flow     """
    1. Developer runs specforge collect in a Cargo project
    2. System shows the cargo test command and asks for approval once
    3. System runs it; the specforge-test attribute writes one report per
       test binary
    4. @specforge/cargo-test maps the reports to entity IDs
    5. System writes specforge-report.json
    6. Developer runs specforge analyze coverage to see results
  """
}

journey annotate_tests_with_proc_macro "Annotate Tests with Proc Macro" {
  persona  developer
  priority medium
  tags     ["developer", "agent_integration"]
  features [rust_proc_macro_annotation]
  flow     """
    1. Developer adds specforge-test dependency to Cargo.toml
    2. Developer writes #[specforge_test(behavior = "entity_id")] in place
       of #[test]
    3. Developer runs specforge collect (or cargo test, then
       specforge collect --no-run)
    4. Drop guard records pass/fail to target/specforge/
    5. System records which entities the tests prove
  """
}

journey export_scoped_context_for_agent "Export Scoped Context for Agent" {
  persona  developer
  priority high
  tags     ["developer", "agent_integration"]
  features [agent_export]
  flow     """
    1. Developer runs specforge export --format=context --scope=payments
    2. System compiles only the payments subgraph
    3. System produces token-optimized output for agent context window
    4. Any AI agent (coding, PM, compliance, docs) receives focused context
    5. Token usage reduced by 90%+ compared to full graph export
  """
}

// ── CI + CI Surface ──────────────────────────────────────────

journey run_spec_validation_in_ci "Run Spec Validation in CI" {
  persona  ci
  channels [ci_surface]
  priority high
  tags     ["ci", "automation"]
  features [ci_integration]
  flow     """
    1. CI pipeline runs specforge check --strict
    2. System validates all .spec files
    3. Exit code 0: pipeline continues
    4. Exit code 1: pipeline fails with error details
    5. Warnings are treated as errors in strict mode
  """
}

journey gate_on_coverage_in_ci "Gate on Coverage in CI" {
  persona  ci
  channels [ci_surface]
  priority high
  tags     ["ci", "automation"]
  features [te_coverage_analysis]
  flow     """
    1. CI pipeline runs specforge collect --yes
    2. CI pipeline runs specforge analyze coverage --min 90
    3. Coverage above threshold: exit 0
    4. Coverage below threshold: exit 1 with summary
  """
}

journey validate_graph_in_ci "Validate Graph Integrity in CI" {
  persona  ci
  channels [ci_surface]
  priority medium
  tags     ["ci", "automation"]
  features [ci_integration]
  flow     """
    1. CI pipeline runs specforge check --strict
    2. System validates all .spec files and graph integrity
    3. CI pipeline runs specforge export --format=graph --check
    4. System verifies graph protocol output is consistent
    5. No issues: exit 0
    6. Issues detected: exit 1 with diagnostics
  """
}

// ── Agent + MCP ──────────────────────────────────────────

journey consume_graph_via_mcp "Consume Graph via MCP" {
  persona  agent
  channels [mcp]
  priority high
  tags     ["agent", "mcp"]
  features [mcp_resource_exposure, mcp_core_tools]
  flow     """
    1. AI agent connects to SpecForge MCP server
    2. Agent reads specforge://graph resource for full graph
    3. Agent reads specforge://schema resource to understand graph structure
    4. Agent reads specforge://context for token-optimized format
    5. Agent reads specforge://brief for minimal IDs + edges
    6. Agent reads specforge://diagnostics for current validation state
    7. Agent calls specforge.query tool for focused subgraph extraction
    8. Agent calls specforge.export tool for format-specific output
    9. Agent calls specforge.validate tool to validate after spec edits
    10. Agent calls specforge.search to discover entities by kind, field, or text
    11. Agent calls specforge.stats for project health overview
    12. Success: agent has structured, typed access to the full spec graph
  """
}

journey receive_delta_notifications_via_mcp "Receive Delta Notifications via MCP" {
  persona  agent
  channels [mcp]
  priority medium
  tags     ["agent", "mcp"]
  features [mcp_delta_notifications]
  flow     """
    1. AI agent subscribes to specforge://graph via MCP subscribe
    2. Agent subscribes to diagnostics via specforge/diagnosticsChanged
    3. Developer edits a .spec file
    4. System incrementally recompiles and computes GraphDelta
    5. System sends specforge/graphChanged to subscribed agents
    6. System sends specforge/diagnosticsChanged with DiagnosticsDelta
    7. Agent receives deltas with added/removed/modified nodes and diagnostics
    8. Agent updates its internal context incrementally
    9. Success: agent stays synchronized without polling
  """
}

journey navigate_spec_graph_via_mcp "Navigate Spec Graph via MCP" {
  persona  agent
  channels [mcp]
  priority high
  tags     ["agent", "mcp"]
  features [mcp_navigation_tools]
  flow     """
    1. AI agent connects to SpecForge MCP server
    2. Agent calls specforge.inspect to get entity details (kind, title, contract, references)
    3. Agent calls specforge.find_definition to locate an entity's source file and line
    4. Agent calls specforge.find_references to discover all usage sites
    5. Agent calls specforge.outline to browse entities in a specific file
    6. Agent calls specforge.suggest_fixes to get actionable fix suggestions for diagnostics
    7. Success: agent has LSP-equivalent navigation without an LSP client
  """
}

journey mutate_spec_project_via_mcp "Mutate Spec Project via MCP" {
  persona  agent
  channels [mcp]
  priority medium
  tags     ["agent", "mcp"]
  features [mcp_mutation_tools]
  flow     """
    1. AI agent connects to SpecForge MCP server
    2. Agent calls specforge.init to create a new spec project
    3. Agent calls specforge.add_extension to install domain extensions
    4. Agent calls specforge.format to enforce canonical formatting
    5. Agent calls specforge.rename to safely rename entities across all files
    6. Agent calls specforge.migrate to upgrade spec files to current format version
    7. Agent calls specforge.remove_extension to uninstall unused extensions
    8. Success: agent can fully manage spec project lifecycle through MCP
  """
}

journey manage_spec_project_via_mcp "Manage Spec Project via MCP" {
  persona  agent
  channels [mcp]
  priority medium
  tags     ["agent", "mcp"]
  features [mcp_project_management_tools]
  flow     """
    1. AI agent connects to SpecForge MCP server
    2. Agent calls specforge.extensions to list installed extensions
    3. Agent calls specforge.providers to list configured ref providers
    4. Agent calls specforge.doctor to diagnose extension conflicts and cache issues
    5. Agent calls specforge.collect to gather test results from framework extensions
    6. Agent calls specforge.render to produce JSON or DOT output files
    7. Success: agent has full project management visibility through MCP
  """
}

journey use_guided_prompts_via_mcp "Use Guided Prompts via MCP" {
  persona  agent
  channels [mcp]
  priority medium
  tags     ["agent", "mcp"]
  features [mcp_prompts]
  flow     """
    1. AI agent connects to SpecForge MCP server
    2. Agent invokes specforge://prompts/implement with an entity_id
    3. MCP returns implementation guidance with contract, related entities, and test expectations
    4. Agent invokes specforge://prompts/review with an entity_id for coverage analysis
    5. Agent invokes specforge://prompts/trace with a plan JSON for gap analysis
    6. Agent invokes specforge://prompts/explore with optional kind filter for graph discovery
    7. Success: agent receives pre-composed, context-rich workflows reducing multi-step tool composition
  """
}

// ── Developer + CLI (Migration) ──────────────────────────────

journey migrate_spec_files "Migrate Spec Files" {
  persona  developer
  priority low
  tags     ["developer", "migration"]
  features [spec_file_migration]
  flow     """
    1. Developer upgrades specforge compiler to a new version
    2. Developer runs specforge migrate --dry-run to preview changes
    3. System shows unified diff of all proposed transformations
    4. Developer reviews diff and runs specforge migrate
    5. System backs up files and transforms to current format version
    6. System validates post-migration integrity automatically
    7. Success: all .spec files updated, graph structurally equivalent
  """
}
