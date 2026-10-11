// Wasm component extension runtime features

use "behaviors/surface-contributions"
use "behaviors/wasm-authoring"
use "behaviors/wasm-extensions"
use "behaviors/wasm-host-functions"
use "behaviors/wasm-lifecycle"
use "behaviors/wasm-sandbox"

feature wasm_extension_runtime "Wasm Extension Runtime" {
  status   done
  problem  """
    Extensions need a unified runtime that works across all platforms
    without requiring specific language runtimes (Node.js, Python, JVM)
    on the host machine. The runtime must handle module lifecycle,
    dependency ordering, and graceful error recovery.
  """
  solution """
    Wasm components (wasmtime Component Model) as the sole extension runtime. Extensions compile to .wasm
    binaries. The compiler loads modules, validates peer dependencies,
    initializes in topological order, calls validators, and handles traps
    gracefully — failed extensions do not affect others. Each extension
    runs with no capability and within the limits its handshake declares
    (enforce_wasm_sandbox, configure_sandbox_policy).
  """
}

feature wasm_host_function_api "Wasm Host Function API" {
  status   accepted
  problem  """
    Extensions need controlled access to compiler internals (graph queries,
    diagnostic emission, entity registration) and external resources (file
    I/O, HTTP) without escaping the sandbox. Each host function needs
    specific permission scoping.
  """
  solution """
    Seven host functions (specforge.query_graph, specforge.emit_diagnostic,
    specforge.add_graph_node, specforge.add_graph_edge, specforge.read_file,
    specforge.emit_file, specforge.http_get) with the per-call-site
    permissions they check (planned; the sandbox itself grants none,
    ADR 0037). The
    emit_file host function is restricted to non-code outputs only (reports,
    dashboards, traceability matrices, graph visualizations) — extensions
    MUST NOT use it to generate source code, configuration files, or
    executable artifacts. SpecForge provides context, agents produce code.
    Sandbox policy is computed by merging defaults, manifest, and project
    overrides. Host functions are synchronous leaf operations and
    intentionally produce no events — traceability comes from the calling
    behavior's events.
    Status: planned (milestone wasm_host_functions). The component
    runtime's bridge world imports none of these functions today; the
    Extism runtime registered three (emit_diagnostic, query_graph,
    read_file) until the component cutover removed it. The sandbox that
    exists (execution deadline, fuel, a WASI context that grants nothing)
    belongs to wasm_extension_runtime.
  """
}

feature wasm_performance_optimization "Wasm Performance Optimization" {
  status   done
  problem  """
    Cold-loading .wasm binaries on every compilation is too slow for CLI
    and unacceptable for interactive LSP/MCP contexts. Extensions need
    fast startup without sacrificing sandbox isolation.
  """
  solution """
    Wasmtime's on-disk compilation cache (selected via
    SPECFORGE_WASMTIME_CACHE) stores compiled machine code keyed by
    component bytes and engine config, so CLI cold starts deserialize
    instead of recompiling. One runtime engine per LSP/MCP session keeps
    instantiated components alive across calls. Unusable caches degrade
    to uncached compilation with a warning.
  """
}

feature wasm_extension_authoring "Wasm Extension Authoring" {
  status   done
  problem  """
    Extension authors need a streamlined workflow to create, test, and
    publish Wasm extensions. Without tooling, authors must manually
    configure build targets, sandbox policies, and registry publishing.
  """
  solution """
    specforge extension CLI subcommands: init scaffolds a project with
    PDK skeleton, build compiles to .wasm targeting wasm32-wasi,
    validate loads the binary in a production sandbox against fixtures,
    publish uploads to npm/OCI/GitHub Releases. Publishing adheres to
    the registry_api_openness invariant — the registry API specification
    is published as an open standard.
  """
}

feature entity_enhancement "Entity Enhancement" {
  status   done
  // Bridge: depends on validate_extension_manifest (contribution_based_extensions feature)
  // for manifest schema validation before enhancement registration proceeds.
  problem  """
    Extensions can add new entity types but cannot enhance existing entities
    with additional fields or edges. This blocks cross-cutting extensions
    that need to annotate entity kinds defined by other extensions with
    additional metadata.
  """
  solution """
    Extensions declare entity enhancements in their sidecar manifest.json.
    Manifest validation (validate_extension_manifest) runs before
    enhancement loading — owned by contribution_based_extensions.
    The compiler loads enhancement declarations at startup, builds a
    FieldRegistry combining extension-defined and enhanced fields, and threads
    it through the resolve/graph-build/validate pipeline. Enhanced
    reference fields create graph edges. An enhancement field never
    overwrites a field the kind already declares. The specforge doctor
    command provides visibility into all enhancements.
  """
}

feature entity_kind_conflict_prevention "Entity Kind Conflict Prevention" {
  status   done
  problem  """
    Wasm extensions register new entity kinds when they load, and two
    extensions can declare the same kind name.
  """
  solution """
    The registry build detects a kind an earlier-loaded extension already
    registered: the first extension in load order owns it and the
    duplicate is E026. Entity ids that collide with the grammar's
    structural keywords or an extension's kind keyword are E013.
  """
}

feature wasm_extension_installation "Wasm Extension Installation" {
  status   done
  problem  """
    Extensions need a reliable install/uninstall/upgrade workflow that
    resolves from multiple sources (registry, local, git) and maintains
    project configuration integrity.
  """
  solution """
    Install resolves from multiple sources, verifies integrity, and
    places the binary atomically. Uninstall removes the extension and
    checks peer dependencies. Upgrade checks compatibility and handles
    breaking peer dependencies.
  """
}

feature wasm_lock_management "Wasm Lock Management" {
  status   done
  problem  """
    Reproducible builds require pinning exact extension versions with
    integrity verification. Without a lock file, different environments
    may resolve different extension versions.
  """
  solution """
    Lock file (specforge.lock) pins exact versions with SHA256 integrity
    hashes for reproducible builds. Lock file is read at startup to verify
    installed extensions match expected hashes. Refresh updates the lock
    file when extensions are added or upgraded.
  """
}

feature wasm_extension_maintenance "Wasm Extension Maintenance" {
  status   done
  problem  """
    Extension ecosystems need discovery, cache management, and bulk
    update capabilities. Without these, users must manage extensions
    individually and manually clear stale caches.
  """
  solution """
    Discovery queries registries for available extensions. The engine
    compile cache is content-keyed, so replaced binaries never serve
    stale artifacts and need no host-side invalidation. Bulk update
    checks all extensions for newer versions and upgrades them in
    dependency order.
  """
}

feature contribution_based_extensions "Contribution-Based Extensions" {
  status   done
  problem  """
    Extensions need a structured way to declare what they contribute
    (the ten describe categories: entities, edges, shared_fields,
    enhancements, validation_rules, surfaces, collectors, analyzers,
    passes, feature_flags) with per-entity metadata such
    as testable flags, field type definitions, and structured validation
    rule patterns.
  """
  solution """
    Structured manifest format with typed objects (entity_kinds
    EntityKindDescriptor[], validation_rules ValidationRulePattern[],
    edge_types EdgeTypeDescriptor[]). What an extension contributes is what its declaration declares in
    those categories (ExtensionDeclaration::declares, ADR 0050); its
    handshake says providers when it implements configured providers.
    The host calls an extension's exports through one typed
    module (call_extension_exports, ADR 0013): handshake and describe, a
    command, an MCP tool or resource, a compiler pass, a collector, a
    custom validator, a scanner and the migration hook, each with the
    protocol's input and answer types and every failure E028. Per-call-site
    permissions for host functions belong to the planned
    wasm_host_function_api feature: no export can call the host yet. This
    feature owns the compile-time categories (entities, edges,
    shared_fields, enhancements, validation_rules, passes, feature_flags). Test result collection (collectors) is owned by the
    test_result_collection feature. Surface contributions (CLI commands,
    MCP tools, MCP resources) are owned by the surface_contributions feature.
    Collector behaviors (register_collector_contributions,
    auto_detect_collector, approve_collector_command, run_collector_command,
    dispatch_collector, ingest_collector_report) are owned by the
    test_result_collection feature.
  """
}

feature test_result_collection "Test Result Collection" {
  status   done
  problem  """
    Coverage needs to know which entities the project's tests prove, but
    every test runner has its own command and report format. Wiring them
    into the compiler would put runner knowledge in core, and asking users
    to run each runner in exactly the way SpecForge expects makes the loop
    fragile (ADR 0002).
  """
  solution """
    Runner extensions (`@specforge/cargo-test`, …) declare a collector: the
    project files that select it, the command that runs the runner, where
    the report lands and a pure export that maps the report to entities.
    `specforge collect` detects the collectors that apply, asks the user to
    approve each command once per project, runs it on the extension's
    behalf, hands the report to the export and merges the answer into
    `specforge-report.json`, which `specforge analyze` reads. The compiler
    still never executes anything, and extensions stay pure wasm; `--no-run`
    parses an existing report without running anything.
  """
}

feature surface_contributions "Surface Contributions" {
  status   done
  problem  """
    Extensions can extend the compilation pipeline (entities, validation rules,
    collectors, passes) but cannot
    extend the tooling surfaces — CLI and MCP server. This creates a
    capability asymmetry: domain extensions need CLI commands (e.g., analyze, audit) but has no registration mechanism; extensions cannot register
    MCP tools so agents only see core tools.
  """
  solution """
    Extensions declare surface contributions in their manifest's surfaces
    field: commands[] for CLI, mcp_tools[] for MCP tools, mcp_resources[]
    for MCP resources. Core discovers contributions from manifests (static,
    no code execution), validates Wasm exports exist, and dispatches lazily
    on invocation. CLI commands are auto-promoted to MCP tools with the
    specforge.{ext}.{cmd} naming convention. Per-contribution sandbox
    overrides can only restrict below the type ceiling (MCP resources
    cannot fs_write). Phase 1 covers CLI commands, MCP tools, and MCP
    resources. LSP providers are deferred to Phase 2.
  """
}
