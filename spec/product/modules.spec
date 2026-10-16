// Modules — code packages that implement features

use "extensions/cargo-test/features"
use "extensions/testing/features"
use "extensions/vitest/features"
use "features/extensions"
use "features/formatting"
use "features/incremental"
use "features/lsp"
use "features/mcp"
use "features/output"
use "features/parsing"
use "features/project-init"
use "features/validation"
use "features/wasm"
use "ports/inbound"
use "ports/outbound"

module specforge_coverage "specforge-coverage" {
  family      core
  description "The coverage rule of @specforge/testing: which obligations a project's recorded tests prove, per entity and in summary; linked by the extension's Wasm pass and the host surfaces"
  depends_on  [specforge_protocol_types]
  tags        ["leaf"]
}

module specforge_diagnostics "specforge-diagnostics" {
  family      core
  description "The catalog of SpecForge diagnostic codes: titles, owners, levels and explanations"
  tags        ["leaf"]
}

module specforge_package "specforge-package" {
  family      core
  description "The host's reading of what an extension package is called, which version is asked for and what its declared peers require (ADR 0036, 0041, 0050)"
  depends_on  [specforge_protocol_types]
  tags        ["leaf"]
}

module specforge_protocol_types "specforge-protocol-types" {
  family      core
  description "The guest wire of the SpecForge extension protocol: the handshake, the describe payloads, the operation payloads and the command-arg rule, which the host and every guest compile (ADR 0012, 0013, 0050)"
  tags        ["leaf"]
}

module specforge_repo_root "specforge-repo-root" {
  family      core
  description "The SpecForge checkout's root, for the tests and build tools that read files outside their crate"
  tags        ["leaf"]
}

module specforge_common "specforge-common" {
  family      core
  description "What every host crate shares: the diagnostic and its source span, the interned symbol, the project root and its specforge.json, .spec discovery, the JSON shape (ADR 0048) and how diagnostics print"
  features    [spec_file_parsing, error_recovery_during_parsing]
  depends_on  [
    specforge_coverage,
    specforge_diagnostics,
    specforge_package,
    specforge_protocol_types,
    specforge_shape_macros,
  ]
  tags        ["shared", "foundation"]
}

module specforge_parser "specforge-parser" {
  family      core
  description "The .spec language read from text: the one lexer, the recovered syntax, the AST, the expression grammar and the format version (ADR 0038, ADR 0063)"
  features    [spec_file_parsing, error_recovery_during_parsing]
  depends_on  [specforge_common, specforge_repo_root, tree_sitter_specforge]
  tags        ["parser", "pipeline"]
}

module specforge_shape_macros "specforge-shape-macros" {
  family      core
  description "Derive macro for specforge-common: a type's JSON shape is derived beside its serialization (ADR 0048)"
  tags        ["kernel"]
}

module tree_sitter_specforge "tree-sitter-specforge" {
  family      core
  description "The tree-sitter grammar of the .spec language: grammar.js, the generated parser and its scanner, the node-kind and field vocabulary, and the editor queries"
  features    [spec_file_parsing, error_recovery_during_parsing, editor_query_files]
  tags        ["grammar", "parser"]
}

module specforge_emitter "specforge-emitter" {
  family      core
  description "Export formats: a built graph and the registries that describe it as text (Graph Protocol JSON, the agent formats, DOT, the model and outline diagrams); no compile, no Wasm runtime, no file I/O (ADR 0007)"
  features    [
    json_and_dot_render,
    traceability_serialization,
    agent_export,
    self_describing_graph_protocol,
  ]
  depends_on  [
    specforge_common,
    specforge_coverage,
    specforge_diagnostics,
    specforge_graph,
    specforge_protocol_types,
    specforge_registry,
  ]
  tags        ["emitter", "pipeline"]
}

module specforge_formatter "specforge-formatter" {
  family      core
  description "The .spec formatter: a document, or the blocks a range touches, in canonical layout under its .specforgefmt.toml, a form it cannot read kept as written (ADR 0021, ADR 0063)"
  features    [code_formatting, lsp_formatting]
  depends_on  [specforge_common, specforge_parser, tree_sitter_specforge]
  tags        ["formatter", "tooling"]
}

module specforge_graph "specforge-graph" {
  family      core
  description "The typed entity graph: the graph build over parsed .spec files, its nodes and edges, the graph delta between two builds, field forms and verify obligations (ADR 0032)"
  features    [incremental_compilation]
  depends_on  [specforge_common, specforge_parser, specforge_protocol_types]
  tags        ["graph", "pipeline"]
}

module specforge_migrate "specforge-migrate" {
  family      core
  description "Format-version migration of a project's .spec files: the rewrite, its backups and rollback record, and the W054 report of what it changed in the graph"
  depends_on  [specforge_common, specforge_formatter, specforge_graph, specforge_parser]
  tags        ["pipeline"]
}

module specforge_registry "specforge-registry" {
  family      core
  description "The Registry build: extension declarations in load order turned into kind, field and edge registries, the rule set and pass order, the entity snapshot of a built graph and the checks over it (ADR 0019, ADR 0031)"
  depends_on  [
    specforge_common,
    specforge_coverage,
    specforge_diagnostics,
    specforge_graph,
    specforge_package,
    specforge_parser,
    specforge_protocol_types,
  ]
  tags        ["pipeline"]
}

module specforge_resolver "specforge-resolver" {
  family      core
  description "Import resolution over a compile's sources: an import names a source by its key, its path relative to the spec root; nothing here reads a file (ADR 0055)"
  features    [reference_resolution, graph_construction]
  depends_on  [specforge_common, specforge_parser]
  tags        ["resolver", "pipeline"]
}

module specforge_component "specforge-component" {
  family      core
  description "The wasmtime component-model runtime: the production adapter of the WasmRuntime port, which enforces the sandbox's limits, and the builtin blobs the host embeds (ADR 0037, ADR 0070)"
  depends_on  [specforge_common, specforge_protocol_types, specforge_repo_root, specforge_wasm]
  tags        ["runtime"]
}

module specforge_extension_sdk "specforge-extension-sdk" {
  family      core
  description "Author SpecForge extensions in Rust: an extension's declaration and the handlers of its commands, passes and other operations, compiled to a wasm32-wasip2 component"
  depends_on  [specforge_extension_sdk_macros, specforge_protocol_types]
  tags        ["runtime"]
}

module specforge_extension_sdk_macros "specforge-extension-sdk-macros" {
  family      core
  description "Attribute macro for the SpecForge extension SDK: builds an extension's declaration from its Contributions impl"
  depends_on  [specforge_protocol_types]
  tags        ["runtime"]
}

module specforge_installed "specforge-installed" {
  family      core
  description "A project's installed extensions: layout, specforge.lock, the hash pin, the extension load and the all-or-nothing change"
  depends_on  [
    specforge_common,
    specforge_package,
    specforge_protocol_types,
    specforge_registry,
    specforge_wasm,
  ]
  tags        ["runtime"]
}

module specforge_wasm "specforge-wasm" {
  family      core
  description "The host side of the extension protocol: the WasmRuntime port, every typed extension call and its E028 failure, the sandbox limits and the in-process test runtime (ADR 0013, ADR 0037)"
  features    [wasm_extension_runtime, wasm_extension_authoring]
  depends_on  [specforge_common, specforge_extension_sdk, specforge_protocol_types]
  tags        ["wasm", "runtime"]
}

module specforge_project "specforge-project" {
  family      core
  description "The compiled project and the project session: one environment load, one compile, kept current for watch, the LSP and MCP (ADR 0004, ADR 0047)"
  depends_on  [
    specforge_common,
    specforge_component,
    specforge_coverage,
    specforge_diagnostics,
    specforge_graph,
    specforge_installed,
    specforge_parser,
    specforge_protocol_types,
    specforge_registry,
    specforge_resolver,
    specforge_wasm,
  ]
  tags        ["session"]
}

module specforge_watch "specforge-watch" {
  family      core
  description "What keeps a project session current from the file system: the file watcher, and the debounce rule and the follow loop watch and the LSP share (ADR 0035, ADR 0099)"
  features    [incremental_compilation]
  depends_on  [specforge_project]
  tags        ["watch", "incremental"]
}

module specforge_ops "specforge-ops" {
  family      core
  description "User-level operations shared by the CLI, the MCP server and the LSP: a typed request in, a typed outcome or a refusal out (ADR 0015, ADR 0029)"
  depends_on  [
    specforge_common,
    specforge_diagnostics,
    specforge_emitter,
    specforge_extension_sdk,
    specforge_formatter,
    specforge_graph,
    specforge_installed,
    specforge_migrate,
    specforge_package,
    specforge_parser,
    specforge_project,
    specforge_protocol_types,
    specforge_registry,
    specforge_wasm,
  ]
  tags        ["app"]
}

module specforge_ops_registry "specforge-ops-registry" {
  family      integration
  description "Which registries a project uses, and the adapter behind specforge-ops' Registry port: the fetch policy over the registry client and the publish signed as the user (ADR 0010)"
  depends_on  [
    specforge_common,
    specforge_ops,
    specforge_package,
    specforge_protocol_types,
    specforge_registry_client,
    specforge_registry_wire,
  ]
  tags        ["adapters"]
}

module specforge_registry_client "specforge-registry-client" {
  family      integration
  description "The package-registry client: search, resolve and publish over HTTP, credentials in the OS keyring, publisher key pinning and package signing"
  depends_on  [
    specforge_common,
    specforge_package,
    specforge_protocol_types,
    specforge_registry_wire,
  ]
  tags        ["adapters"]
}

module specforge_registry_wire "specforge-registry-wire" {
  family      integration
  description "The package registry's HTTP contract: paths, JSON bodies, the publish form, the package signature, error codes (ADR 0044, ADR 0061)"
  depends_on  [specforge_package, specforge_protocol_types]
  tags        ["adapters"]
}

module specforge_cli "specforge-cli" {
  family      platform
  description "The specforge binary: every command's arguments, the terminal it writes to and asks from, and its exit code, over specforge-ops (ADR 0029)"
  features    [project_initialization, ci_integration]
  depends_on  [
    specforge_common,
    specforge_component,
    specforge_diagnostics,
    specforge_formatter,
    specforge_mcp,
    specforge_ops,
    specforge_ops_registry,
    specforge_package,
    specforge_project,
    specforge_protocol_types,
    specforge_registry_client,
    specforge_watch,
  ]
  tags        ["cli", "platform"]
}

module specforge_lsp "specforge-lsp" {
  family      platform
  description "The language server: open documents held in the project session, published diagnostics, navigation, completion, hover and formatting (ADR 0023, ADR 0043, ADR 0046)"
  features    [
    go_to_definition_and_references,
    hover_and_autocomplete,
    rename_refactoring,
    live_diagnostics,
    code_actions,
    outline_and_symbol_search,
  ]
  depends_on  [
    specforge_common,
    specforge_diagnostics,
    specforge_graph,
    specforge_ops,
    specforge_parser,
    specforge_project,
    specforge_registry,
    specforge_resolver,
    specforge_watch,
  ]
  tags        ["lsp", "platform"]
}

module specforge_mcp "specforge-mcp" {
  family      platform
  description "The MCP server: tools, prompts, resources and events, each one typed definition over specforge-ops, served over the stdio transport (ADR 0024, ADR 0062)"
  features    [
    mcp_resource_exposure,
    mcp_core_tools,
    mcp_navigation_tools,
    mcp_mutation_tools,
    mcp_project_management_tools,
    mcp_delta_notifications,
    mcp_prompts,
  ]
  depends_on  [
    specforge_common,
    specforge_component,
    specforge_graph,
    specforge_mcp_macros,
    specforge_ops,
    specforge_ops_registry,
    specforge_project,
    specforge_protocol_types,
    specforge_registry,
    specforge_wasm,
  ]
  tags        ["mcp", "platform"]
}

module specforge_mcp_macros "specforge-mcp-macros" {
  family      platform
  description "Derive macro for specforge-mcp: a tool's or prompt's arguments are one typed definition (ADR 0033)"
  tags        ["surfaces"]
}

module specforge_registry_server "specforge-registry-server" {
  family      platform
  description "The package registry server (specforge-registry): the registry contract's routes over SQLite and blob storage, publish authentication and signature checks (ADR 0044, ADR 0061)"
  depends_on  [specforge_package, specforge_protocol_types, specforge_registry_wire]
  tags        ["surfaces"]
}

module specforge_package_software "specforge-package-software" {
  family      extension
  description "Wasm package for @specforge/software: behavior, invariant, event, type, port entities"
  features    [extension_management, se_core_entity_kinds, se_validation_suite, se_gherkin_bridge]
  depends_on  [specforge_wasm]
  tags        ["extension", "domain"]
}

module specforge_package_formal "specforge-package-formal" {
  family      extension
  description "Wasm package for @specforge/formal: property, axiom, protocol, refinement, process entities"
  features    [extension_management]
  depends_on  [specforge_wasm]
  tags        ["extension", "domain"]
}

module specforge_package_product "specforge-package-product" {
  family      extension
  description "Wasm package for @specforge/product: journey, deliverable, milestone, module, term, feature, persona, channel entities"
  features    [extension_management]
  depends_on  [specforge_wasm]
  tags        ["extension", "domain"]
}

module specforge_package_governance "specforge-package-governance" {
  family      extension
  description "Wasm package for @specforge/governance: decision, constraint, failure_mode entities"
  features    [extension_management]
  depends_on  [specforge_wasm]
  tags        ["extension", "domain"]
}

module specforge_provider_gh "specforge-provider-gh" {
  family      extension
  description "GitHub ref provider: validates gh.issue, gh.pr, gh.discussion schemes via GitHub API"
  features    [provider_based_ref_validation]
  depends_on  [specforge_wasm]
  tags        ["extension", "provider"]
}

module specforge_package_testing "specforge-package-testing" {
  family      extension
  description "Wasm package for @specforge/testing: verify obligations, W004/W009 and the coverage analysis pass"
  features    [te_test_vocabulary, te_coverage_analysis]
  depends_on  [specforge_wasm]
  tags        ["extension", "traceability"]
}

module specforge_package_cargo_test "specforge-package-cargo-test" {
  family      extension
  description "Wasm package for @specforge/cargo-test: runs cargo test for specforge collect and maps specforge-test reports to entities"
  features    [ct_cargo_test_collection, test_result_collection]
  depends_on  [specforge_package_testing, specforge_wasm]
  tags        ["extension", "traceability"]
}

module specforge_package_vitest "specforge-package-vitest" {
  family      extension
  description "Wasm package for @specforge/vitest: runs vitest for specforge collect and maps tests linked through meta.specforge"
  features    [vt_vitest_collection, test_result_collection]
  depends_on  [specforge_package_testing, specforge_wasm]
  tags        ["extension", "traceability"]
}

module specforge_test_lib "specforge-test" {
  family      extension
  description "Test traceability for SpecForge — link Rust tests to spec entities"
  features    [rust_proc_macro_annotation]
  depends_on  [specforge_protocol_types, specforge_test_macros_lib]
  tags        ["extension", "traceability"]
}

module specforge_test_macros_lib "specforge-test-macros" {
  family      extension
  description "Proc macros for specforge-test"
  features    [rust_proc_macro_annotation]
  tags        ["extension", "traceability"]
}
