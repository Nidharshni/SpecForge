// Core Architecture Decision Records
//
// Decisions about the compiler engine, extension model, and infrastructure.
// Extension-specific decisions live in their respective extension directories
// under spec/extensions/.

use "invariants/core"
use "invariants/extensions"
use "invariants/surface"
use "invariants/wasm"

decision tree_sitter_for_parsing "Tree-sitter for Parsing" {
  status       accepted
  date         2026-03-01
  context      """
    SpecForge needs a parser that supports error recovery (partial ASTs on
    syntax errors), incremental re-parsing for watch mode, and source span
    tracking for diagnostics and LSP features. Options considered: hand-written
    recursive descent, nom/pest/chumsky combinators, or Tree-sitter.
  """
  decision     """
    Use Tree-sitter for the .spec grammar. It provides error recovery,
    incremental parsing, and a mature ecosystem with editor integrations.
    No hand-written recursive descent fallback.
  """
  consequences [
    "Error recovery out of the box — partial ASTs on syntax errors",
    "Incremental reparsing for watch mode and LSP",
    "Editor highlighting via existing Tree-sitter integrations",
    "Grammar development requires learning Tree-sitter DSL",
    "Build dependency on Tree-sitter CLI for grammar generation",
  ]
  invariants   [multi_error_collection, incremental_correctness]
}

decision adr_multi_error_collection "Multi-Error Collection" {
  status       accepted
  date         2026-03-01
  context      """
    Compilers that halt on the first error force users to fix issues one by
    one. This is slow for projects with hundreds of .spec files. The compiler
    needs to collect all diagnostics in a single pass.
  """
  decision     """
    The compiler collects all errors, warnings, and info diagnostics during
    a compilation pass and emits them together. No fail-fast behavior.
    Diagnostics are sorted by (file_path, line, column, code) — source
    order within each file, not grouped by severity. This matches the
    rustc convention where developers see diagnostics in the order they
    appear in the source.
  """
  consequences [
    "Users see all issues at once — faster fix cycles",
    "Diagnostic collection requires careful error handling to avoid cascading",
    "Memory proportional to diagnostic count, not file count",
    "Deterministic output order regardless of processing order",
  ]
  invariants   [multi_error_collection, diagnostic_determinism]
}

decision string_interning_with_lasso "String Interning with lasso" {
  status       accepted
  date         2026-03-01
  context      """
    Entity IDs and file paths are compared frequently during resolution and
    validation. Character-by-character comparison is O(n) per comparison.
    Interning reduces this to O(1) pointer comparison.
  """
  decision     """
    Use the lasso crate for string interning. Intern all entity IDs, file
    paths, and frequently compared strings. Use interned keys for all
    comparison and hashing operations.
  """
  consequences [
    "O(1) string comparison for entity IDs",
    "Reduced memory for duplicate strings across files",
    "Additional complexity for interning lifecycle management",
    "All string comparison code must use interned keys, not raw strings",
  ]
  invariants   [string_interning_consistency]
}

decision mutable_in_memory_graph "Mutable In-Memory Graph" {
  status       accepted
  date         2026-03-01
  context      """
    Watch mode and LSP require incrementally updating the graph when files
    change. An immutable graph would require full rebuilds on every change.
    A mutable graph allows adding/removing nodes and edges in place.
  """
  decision     """
    The in-memory graph is mutable. It supports adding/removing nodes and
    edges. Incremental compilation mutates the graph in place, removing
    stale entries and inserting new ones from re-parsed files.
  """
  consequences [
    "Watch mode and LSP can update incrementally — <100ms latency",
    "Graph mutation code must maintain consistency (no dangling edges)",
    "No persistent immutable snapshots — debugging requires graph dumps",
    "Concurrent access requires synchronization in LSP mode",
  ]
  invariants   [incremental_correctness]
}

decision snapshot_testing_with_insta "Snapshot Testing with insta" {
  status       accepted
  date         2026-03-01
  context      """
    Compiler output (diagnostics, rendered markdown, JSON graph output) must be
    tested against expected output. Manual assertion writing is tedious and
    brittle. Snapshot testing captures output and detects unexpected changes.
  """
  decision     """
    Use the insta crate for snapshot testing. Diagnostic output and rendered
    markdown are tested via inline and file-based
    snapshots. cargo insta review for interactive approval.
  """
  consequences [
    "Fast test authoring — capture output, approve once",
    "Automatic regression detection on output changes",
    "Snapshot files add to repository size",
    "Format changes require mass snapshot updates",
  ]
}

decision three_layer_traceability_model "Three-Layer Traceability Model" {
  status       accepted
  date         2026-03-02
  context      """
    Spec coverage based solely on counting verify declarations does not
    prove that tests exist, run, or pass. Teams need end-to-end proof
    that every testable entity has been implemented and tested.
  """
  decision     """
    Three-layer traceability: intent (verify declarations and extension-declared test file fields in .spec
    files), linkage (each test names the entity it proves, in its runner's
    idiom), and proof (the pass/fail results `specforge collect` records in
    specforge-report.json). `specforge analyze coverage` scores all three
    layers. (Linkage was first a `tests` field in .spec files; ADR 0002
    moved it into the tests themselves.)
  """
  consequences [
    "Agents can close the full loop: read spec → generate code → annotate tests → collect → analyze",
    "Coverage measured within the three-layer model: declared, recorded, passing",
    "Requires test runner extensions to produce specforge-report.json",
    "Additional complexity in the coverage pipeline",
  ]
  invariants   [traceability_chain_integrity]
}

decision wasm_component_extension_runtime "Wasm Component Model Extension Runtime" {
  status       accepted
  date         2026-03-03
  context      """
    The contribution-based extension model requires a unified runtime for
    all extension contributions — entity definitions, validators, renderers,
    and providers. The runtime must work uniformly across all SpecForge
    surfaces — CLI, LSP, and MCP. Subprocess JSON-RPC was considered but
    degrades for long-running surfaces: LSP would need to manage child
    process lifecycles per edit, and MCP would face the same overhead.
    Embedded scripting (Lua, Rhai, Starlark) was evaluated but tree-sitter
    grammars compile to C at build time, making parser-level extension
    impossible regardless of scripting engine.
  """
  decision     """
    Use WebAssembly components (wasmtime Component Model, wasip2 guests)
    as the unified extension runtime for all contribution types. The
    guest contract is the specforge:bridge WIT world: one `call` export
    dispatching the JSON wire protocol by host-function name.
    Extensions expose host functions (specforge.query_graph,
    specforge.emit_diagnostic) — components never touch the OS directly
    beyond their WASI grants.
    Renderers use specforge.emit_file(path, content) instead of raw
    filesystem access, with the host validating paths and enforcing
    sandboxing. Providers use specforge.http_get for external
    service validation. Universal .wasm components are distributed via npm,
    GitHub Releases, or OCI registries. Wasmtime's on-disk compile cache
    makes CLI cold starts cheap; LSP and MCP keep one engine with
    instantiated components alive per session.
  """
  consequences [
    "Hardware-enforced sandboxing — extensions cannot access filesystem or network without host consent",
    "Multi-language extension authoring — Rust, Go, TypeScript, Python via Wasm compilation",
    "Universal .wasm binary distribution — same artifact runs on all platforms",
    "Single runtime covers all contribution types and all three surfaces uniformly",
    "+5MB binary size increase from the wasmtime runtime",
    "Extension authors need Wasm toolchain (cargo-component, tinygo, javy, etc.)",
    "10-50ms load per extension from compile cache; one engine per LSP/MCP session",
  ]
  invariants   [reference_resolution_completeness]
}

decision extension_peer_dependencies "Extension Peer Dependencies" {
  status       accepted
  date         2026-03-03
  context      """
    Extensions naturally form dependency chains: @specforge/product entities
    reference core entities (capability bundles feature), governance entities
    reference core entities (decision protects invariant), and third-party
    extensions may depend on official extension entities. Today, cross-module
    references use implicit soft resolution — if the target module is not
    installed, the resolver emits I004 (info) instead of E001 (error). This
    is insufficient for Wasm extensions that explicitly declare entity types
    targeting another extension's entities. A third-party @myorg/epic-tracker
    defining an epic entity with a stories field referencing feature must
    fail hard if @specforge/product is not installed, not silently degrade.
  """
  decision     """
    Wasm extension manifests declare peer_dependencies with semver ranges. The
    registry build judges every peer dependency by one rule and loads each
    extension after the peers it declares, otherwise in specforge.json order
    (ADR 0041). All extensions share the same in-process graph via
    specforge.query_graph host function — no serialization between extensions.
    When a manifest declares a peer dependency that is not installed, the
    host emits a hard error. The existing I004 soft resolution remains for
    implicit cross-module references (fields without a manifest declaration).
  """
  consequences [
    "Explicit dependency chains — extension authors declare what they need upfront",
    "Hard error on missing peer dependency vs soft I004 for implicit references",
    "Topological loading order prevents extensions from querying entities not yet registered",
    "Zero serialization cost — all Wasm extensions share the graph via host function calls",
    "Manifest schema must include peer_dependencies with semver range validation",
    "Circular extension dependencies are detected and rejected at load time",
  ]
  invariants   [reference_resolution_completeness]
}

decision config_driven_kind_registry "Config-Driven Kind Registry" {
  status       accepted
  date         2026-03-03
  context      """
    Wasm extensions declare entity kinds in their manifests, and can add
    graph node instances at runtime via specforge.add_graph_node.
    Currently there is no conflict prevention: two extensions can declare
    the same kind name, and an extension can shadow a structural keyword
    like spec. The existing
    FieldEnhancementRegistry solved the analogous problem for field-level
    conflicts but no equivalent exists for entity kind names.
  """
  decision     """
    Entity kind names are unique across the loaded extensions: the
    registry build rejects any extension's kind already registered by a
    previously loaded extension, emitting E026, and the first registration
    wins. Entity ids that collide with the grammar's structural keywords
    or an extension's kind keyword are E013. Conflict resolution is the
    extension author's responsibility — rename the kind or declare a peer
    dependency. The compiler never arbitrates domain-level conflicts;
    if two installed extensions declare the same kind, it is a hard error
    at startup, not a runtime policy decision.
  """
  consequences [
    "Duplicate kind names are a hard error at extension load time",
    "Zero config surface — no entity_kind_policy or entity_kinds in specforge.json",
    "No @extension/kind qualified syntax needed — conflicts are impossible at parse time",
    "Extension authors resolve collisions via renames or peer dependencies",
  ]
  invariants   [entity_kind_uniqueness]
}

decision entity_enhancement_model "Entity Enhancement Model" {
  status       accepted
  date         2026-03-03
  context      """
    Extensions need to add fields and edges to existing entity types (e.g., a
    hexagonal architecture extension adding a layer field to behavior). Without
    entity enhancement, extensions are limited to creating new entity types —
    they cannot annotate or extend entities from other extensions. This blocks
    a significant class of architectural extensions.
  """
  decision     """
    Extension manifests declare entity enhancements with target entity kind,
    field name, field type, and optional edge mappings. The compiler builds
    a FieldRegistry combining extension-defined and enhanced fields, threads
    it through the resolve/graph-build/validate pipeline. An enhancement
    never overwrites a field the kind already has: the first registration
    in load order wins. specforge doctor provides visibility into all
    enhancements.
  """
  consequences [
    "Extensions can annotate existing entities without forking the grammar",
    "FieldRegistry becomes the single source of truth for field definitions",
    "No conflict policy to configure: the load order decides, deterministically",
    "Enhancements are applied when the registries are built, not lazily during validation",
  ]
  invariants   [enhancement_field_uniqueness]
}

decision wasm_compile_cache_strategy "Wasm Compile Cache Strategy" {
  status       accepted
  date         2026-03-03
  context      """
    Components can be compiled on every load (JIT) or compiled once and
    cached as native machine code (AOT). JIT has zero setup but pays full
    compilation on every CLI invocation; AOT trades one compile for fast
    subsequent loads. For a batch compiler like SpecForge where cold start
    matters (CLI, CI), cached AOT is preferred. A previous hand-rolled
    side cache (content-hash `.aot` byte copies) was never consumed by any
    runtime and was deleted in the Component Model cutover.
  """
  decision     """
    Use wasmtime's native on-disk compilation cache, configured when the
    engine is constructed (CacheConfig on Config::cache — after Engine
    creation the setting is inert, so the constructor is the only wiring
    point). The cache directory is selected by SPECFORGE_WASMTIME_CACHE
    (default $HOME/.cache/specforge/wasmtime; 'off' disables) with a
    500MB files-total-size soft limit. Entries are keyed by component
    bytes and engine config; integrity and cleanup are engine-owned.
    Host-side concerns stay separate: installed-binary integrity is the
    specforge.lock hash pin (E070) at load time.
  """
  consequences [
    "CLI cold start <50ms per extension on cache hit",
    "First-time compilation pays full Cranelift compilation (100-500ms)",
    "Cache directory grows proportionally to distinct compiled components",
    "Engine or platform changes naturally miss the cache and recompile",
    "No host-side invalidation logic to maintain or test",
  ]
  invariants   [wasm_compile_cache_integrity, extension_load_order_determinism]
}

decision contribution_based_extension_model "Contribution-Based Extension Model" {
  status       accepted
  date         2026-03-03
  context      """
    Extensions need a structured manifest format with per-entity metadata
    such as testable flags, field type definitions, and structured
    validation rule patterns. A 10-expert panel (RES-23) unanimously
    recommended a contribution-based model with structured manifest
    objects for richer extension metadata.
  """
  decision     """
    Structured manifest with typed objects: entity_kinds EntityKindDescriptor[],
    validation_rules ValidationRulePattern[], edge_types EdgeTypeDescriptor[].
    Each extension declares a contributes key listing what it provides:
    entities, validators, renderers, providers, parsers. The compiler routes to
    namespaced Wasm exports based on contributions. Per-call-site host
    function permissions enforce least-privilege for each export.
  """
  consequences [
    "Single extension = full integration — no more splitting @specforge/jira into three extensions",
    "Contribution-level indexing enables fine-grained enable/disable via config",
    "Lower barrier to entry — one extension to author, test, and publish",
    "Per-call-site permissions maintain defense-in-depth despite single-extension model",
  ]
  invariants   [reference_resolution_completeness]
}

decision adr_extension_source_resolution "Extension Source Resolution" {
  status       accepted
  date         2026-03-03
  context      """
    Extensions need to be installable from multiple sources for different
    development workflows. Local development requires loading from
    filesystem paths without publishing. Teams need to share extensions
    via git before a registry is available. Production deployments
    require pinned registry versions.
  """
  decision     """
    Three extension source types: registry (@scope/name@version for npm),
    local (./path for filesystem), and git (git:url#ref for repositories).
    The specifier format supports both string shorthand ("@specforge/product@^1.0")
    and object form ({ source: "local", path: "./my-extension" }) in
    specforge.json. Resolution produces a manifest + .wasm binary pair.
  """
  consequences [
    "Local development without publishing — ./path resolves to filesystem",
    "Git sharing before registry availability — git:url#ref for early adoption",
    "Reproducible production builds via pinned registry versions",
    "Three source types to implement and test",
    "Object form provides escape hatch for complex configurations",
  ]
  invariants   [reference_resolution_completeness]
}

decision extension_version_pinning "Extension Version Pinning" {
  status       accepted
  date         2026-03-03
  context      """
    Without version pinning, specforge add at different times produces
    different extension versions. This makes builds non-reproducible and
    leads to inconsistent behavior across team members and CI. The problem
    is amplified by the Wasm binary format — unlike source code, binary
    differences are opaque and hard to debug.
  """
  decision     """
    specforge.lock pins exact resolved versions with SHA256 wasm_hash for
    each installed extension. The lock file format is deterministic — same
    inputs always produce byte-identical output. Integrity verification
    compares .wasm binary hashes against lock entries. specforge update
    explicitly bumps versions and refreshes the lock file.
  """
  consequences [
    "Reproducible builds — same lock file produces identical compilation behavior",
    "Tamper detection via SHA256 integrity verification of .wasm binaries",
    "Explicit update workflow prevents accidental version drift",
    "Lock file must be committed to version control",
    "First install on a clean checkout requires network access to resolve",
  ]
  invariants   [reference_resolution_completeness]
}

decision specforge_is_specification_language "SpecForge is a Specification Language" {
  status       accepted
  date         2026-03-04
  context      """
    The primary consumer of SpecForge specs is AI agents that read
    specifications and generate code themselves. SpecForge provides
    structured context; agents produce output. These are fundamentally
    different jobs — conflating them creates friction: drift between
    generated and actual code, ownership conflicts over generated files,
    and a generator that must keep up with every target language's idioms.
  """
  decision     """
    SpecForge is a specification language that provides structured context
    for AI agents and humans to do their jobs better. SpecForge is
    domain-agnostic — it serves coding agents, compliance agents, PM agents,
    and any domain that benefits from validated entity graphs. The `verify`
    keyword declares test intent inside testable entity blocks. The renderer
    contribution type in Wasm extensions produces non-code outputs only
    (reports, dashboards, traceability matrices). The `collect` command and
    all spec validation/coverage features provide test traceability.
  """
  consequences [
    "Specs are pure declarations — no file paths coupling specs to implementation",
    "AI agents and humans consume structured context to produce their own outputs",
    "No drift between generated code and specs — agents regenerate as needed",
    "Renderer contribution type produces non-code outputs only",
    "collect command and coverage pipeline provide test traceability",
  ]
  invariants   [traceability_chain_integrity]
}

decision zero_entity_core_architecture "Zero-Entity Core Architecture" {
  status       accepted
  date         2026-03-04
  context      """
    SpecForge's 14 domain entity kinds (6 software + 5 product + 3 governance)
    plus 2 structural keywords (spec, ref) — 16 entity-bearing keywords total — limit the tool to
    software engineering. The vision of being the structured context standard
    for ALL AI agents requires supporting any domain: UI design (atomic design),
    compliance (regulation, control, evidence), data pipelines, API design,
    infrastructure, and domains not yet imagined. Hardcoding entity types into
    the compiler core prevents this expansion.
  """
  decision     """
    The SpecForge compiler core has ZERO built-in entity types. All domain
    vocabulary comes from installable extensions. The core is a pure typed-graph
    engine: it parses any keyword-name-body block via the generic_entity_block
    grammar rule, resolves references, builds graphs, and validates constraints.
    Extensions declare entity_kinds, edge_types, validation_rules, and testability
    flags in their manifest. @specforge/software provides 6 entity kinds
    (behavior, invariant, feature, event, type, port),
    @specforge/product provides 5, @specforge/governance provides 3 — together
    reproducing all 14 domain entity kinds (6+5+3). ref remains a core
    structural keyword with a dedicated grammar rule. New domain
    extensions: @specforge/atomic-design, @specforge/compliance, @specforge/api-design,
    etc. This is the Terraform-exact model: core has zero domain knowledge.
  """
  consequences [
    "SpecForge becomes domain-agnostic — any industry can use it with appropriate extensions",
    "Projects declare domain extensions in specforge.json (@specforge/software + @specforge/product + @specforge/governance)",
    "Extension manifest declares entity_kinds, edge_types, validation_rules, and testability",
    "Parser already supports generic_entity_block — no grammar changes needed",
    "KindRegistry, FieldRegistry, WasmEntityKind, Custom(String) already exist — ~60% ready",
    "Validation rules become declarative patterns interpreted by core, not hardcoded passes",
    "Community can create domain extensions without modifying the compiler",
    "Estimated 5-7 weeks implementation in 7 phases",
  ]
  invariants   [reference_resolution_completeness]
}

decision per_call_site_sandbox "Per-Call-Site Sandbox" {
  status       accepted
  date         2026-03-03
  context      """
    The contribution-based model means one extension can contribute entities,
    rendering, and provider validation. A per-extension sandbox policy is
    too coarse — an extension's validator should not have emit_file access
    just because the same extension also contributes a renderer. The
    principle of least privilege requires finer-grained permissions.
  """
  decision     """
    Host function permissions are enforced per export call site, not per
    extension. Each contribution type has a fixed permission set: validators
    get query_graph + emit_diagnostic, renderers additionally get emit_file,
    providers additionally get http_get, parsers get read_file +
    emit_diagnostic + add_graph_node + add_graph_edge (but NOT query_graph,
    since they run during graph construction). The runtime checks the current
    export context before dispatching host function calls.
  """
  consequences [
    "Least-privilege per operation — validator cannot write files even if same extension has a renderer",
    "Defense in depth — compromised validator export cannot escalate to filesystem access",
    "Runtime must track which export is currently executing per call",
    "Permission model is static and predictable — no runtime configuration needed",
  ]
  invariants   [wasm_sandbox_integrity]
}

decision auth_failure_vs_offline_fallback "Auth Failure vs Offline Fallback" {
  status       accepted
  date         2026-03-07
  context      """
    Registry authentication failures (401/403) and network-level failures
    (DNS resolution, TCP timeout, TLS handshake) are distinct error classes.
    The offline_first_extension_resolution invariant guarantees cached
    extensions work without network access, but does not specify behavior
    when the network IS reachable and the server actively rejects credentials.
    Using cached binaries after an auth rejection could allow use of
    extensions the user is no longer authorized to access (revoked license,
    removed from team, expired trial).
  """
  decision     """
    Authentication failures (HTTP 401/403) MUST NOT fall back to cached
    binaries. Only network-level failures (DNS, TCP, TLS errors) trigger
    the offline cache fallback path. When a registry returns 401 or 403,
    the system MUST emit an E-level diagnostic identifying the registry
    and extension, and MUST NOT load the extension from cache. This treats
    auth rejection as an explicit server decision that the client must
    respect, even when a cached copy exists locally.
  """
  consequences [
    "Security boundary: revoked credentials immediately prevent extension use",
    "Inconvenience when registry has transient auth issues — no silent fallback",
    "Clear separation: network failures are infrastructure, auth failures are access control",
    "Error messages must distinguish auth failure from network failure for actionable guidance",
  ]
  invariants   [offline_first_extension_resolution, credential_secrecy]
}

// ── Intentional Trade-Off ADRs (Audit Wave 1) ─────────────────────────

decision verify_gherkin_as_core_grammar "Verify/Gherkin as Core Grammar" {
  status       superseded
  date         2026-03-08
  context      """
    Principle 2 (zero domain knowledge in core) prohibits domain-specific
    constructs in the compiler. However, verify and gherkin blocks are
    grammar-level constructs that enable the spec→test→result traceability
    feedback loop (Principle 5). Without them in the grammar, the compiler
    cannot parse traceability declarations, and extensions cannot build
    coverage tracking on top of structured test intent.
  """
  decision     """
    verify and gherkin REMAIN as core grammar constructs. They are NOT
    domain-specific — they are traceability primitives. The grammar parses
    them structurally (verify <kind> <string>, gherkin { ... }). The set
    of valid verify kinds is extension-defined (registry_build_kinds),
    not hardcoded. Gherkin support is gated by the supportsGherkin flag in
    extension manifests. The core grammar provides the syntax; extensions
    provide the semantics. This is a deliberate P5 exception to P2.
  """
  consequences [
    "Traceability feedback loop (P5) works without requiring extensions to reimplement parsing",
    "Core grammar has exactly two traceability constructs — this is the ceiling, not a precedent",
    "Extensions control which verify kinds are valid and which entities support gherkin",
    "New traceability constructs MUST NOT be added to the core grammar without an ADR",
  ]
  invariants   [zero_domain_knowledge_core]
}

decision gherkin_as_extension_field "Gherkin as Extension-Declared Field" {
  status       accepted
  date         2026-03-08
  context      """
    ADR verify_gherkin_as_core_grammar treated gherkin as a core grammar
    construct alongside verify. However, "gherkin" is the name of a specific
    BDD testing methodology (Cucumber's language). Principle 2 test: "If a
    line of code in the compiler references a specific domain concept, it is
    in the wrong place." Principle 7 test: "Could this be an extension
    instead? If yes, it becomes an extension." Gherkin is a file-reference
    list that extensions can declare as a typed field with file_reference=true.
    verify remains in core because it is genuinely structural — a generic
    test-intent declaration whose kind is extension-defined.
  """
  decision     """
    gherkin is removed from the core grammar. It becomes a regular
    extension-declared field: @specforge/software declares a gherkin field
    with type string_list and file_reference=true on entity kinds that
    support BDD traceability. The supportsGherkin manifest flag is removed.
    The GherkinList AST type is removed — gherkin fields are parsed as
    standard StringList values. File existence validation is handled by
    the existing validate_file_reference_paths behavior which already
    operates on any field with file_reference=true. verify REMAINS as a
    core grammar construct — it is a domain-agnostic traceability primitive.
  """
  consequences [
    "Core grammar loses one domain-specific keyword — closer to zero domain knowledge",
    "Extensions have full control over BDD-style file references",
    "Any extension can declare file-reference fields — not just gherkin",
    "supportsGherkin flag removed from EntityKindDescriptor and KindRegistryEntry",
    "GherkinList removed from core AST types — one fewer FieldValue variant",
    "verify remains as the sole core traceability construct",
    "W018 (missing gherkin) becomes a pure extension validation pattern",
    "E016 (missing file) unchanged — validate_file_reference_paths is generic",
  ]
  invariants   [zero_domain_knowledge_core]
}

decision bundled_keyword_extension_index "Bundled Keyword-Extension Index" {
  status       accepted
  context      """
    Principle 2 (zero domain knowledge in core) prohibits hardcoding domain
    vocabulary. Principle 8 (seconds to value) requires helpful error messages
    without network access. When a user writes an unknown entity keyword, the
    compiler should suggest which extension provides it. This requires a
    keyword→extension mapping, which is inherently domain-aware data.
  """
  decision     """
    A static KeywordExtensionIndex JSON file is generated from the extension
    registry at release time and shipped as a bundled data file. It maps known
    entity keywords to their providing extension names. This data file does NOT
    affect compilation semantics — the compiler does not use it to validate,
    resolve, or register entity kinds. It is ONLY used in the help text of E024
    diagnostics (suggest_missing_extensions behavior). The index is a convenience
    layer for P8 compliance, not a domain knowledge backdoor.
  """
  consequences [
    "E024 diagnostics include actionable 'install @specforge/X' suggestions without network access",
    "Index must be regenerated on each release — stale data produces outdated suggestions, not wrong behavior",
    "Index does not affect compilation: removing it changes only help text quality",
    "Index is the ONLY bundled domain-aware data file — this is the ceiling, not a precedent",
  ]
  invariants   [zero_domain_knowledge_core]
}

decision finite_validation_pattern_kinds "Finite Validation Pattern Kinds" {
  status       accepted
  context      """
    Declarative validation rules use a fixed set of pattern kinds
    (no_incoming_edges, missing_field_when_flag_set, cycle_detection,
    file_exists, etc.). These are graph-structural primitives, not domain
    concepts. However, a finite set could be seen as limiting extensions
    that need novel validation logic.
  """
  decision     """
    The declarative validation pattern kinds are graph-structural primitives
    that operate on the generic entity graph. They are finite because they
    correspond to fundamental graph operations (node degree, field presence,
    cycle detection, file existence). For validation logic that cannot be
    expressed as a declarative pattern, extensions MAY export Wasm validator
    functions using the emit_diagnostic host function. This is the explicit
    escape hatch. The pattern kind set MAY grow when new graph-structural
    primitives are identified, but MUST NOT grow to accommodate domain-specific
    checks — those belong in Wasm validators.
  """
  consequences [
    "Declarative patterns cover ~80% of real-world validation rules without Wasm overhead",
    "Wasm validators provide unlimited expressiveness for complex/domain-specific checks",
    "Adding a new pattern kind requires a compiler change — this is intentional gatekeeping",
    "Pattern kinds are documented in the extension manifest schema for discoverability",
  ]
  invariants   [zero_domain_knowledge_core, declarative_validation_determinism]
}

decision multi_event_consumption_pattern "Multi-Event Consumption Pattern" {
  status       accepted
  context      """
    Several behaviors consume multiple events (e.g.,
    emit_incremental_diagnostics consumes incremental_rebuild_complete,
    graph_delta_computed, and incremental_validators_dispatched). This
    barrier-join pattern requires all consumed events to fire before the
    behavior executes. An alternative design would split such behaviors
    into single-event consumers, but this would fragment related logic.
  """
  decision     """
    Behaviors that act as barrier-joins (waiting for multiple prerequisite
    events) are preferred over splitting into single-event consumers when
    the behavior's contract is semantically indivisible. The consumes list
    on a behavior declares a join barrier: ALL listed events MUST fire
    before the behavior executes. This is documented in each behavior's
    contract with an explicit barrier comment. Splitting would force
    intermediate state management and partial execution, which is more
    error-prone than a clean barrier-join.
  """
  consequences [
    "Multi-event consumers have clear barrier semantics documented in their contracts",
    "Runtime must implement join-barrier logic for multi-event consumers",
    "Testing multi-event consumers requires firing all prerequisite events",
    "Event ordering within a barrier is irrelevant — only completion matters",
  ]
  invariants   [incremental_correctness]
}

decision extension_file_parsers "Extension File Parsers" {
  status       accepted
  date         2026-03-07
  context      """
    Extensions declare file-reference fields (e.g., gherkin with
    file_reference=true in @specforge/software). The core compiler validates
    that referenced files exist (E016) but cannot parse their content — that
    would require domain knowledge (gherkin syntax, protobuf schema, OpenAPI
    structure), violating Principle 2. Without content parsing, file-reference
    fields are opaque paths: the graph knows a file exists but nothing about
    what is inside it. This forces agents to read raw files separately,
    losing the "graph is the product" guarantee for file-referenced content.
    The alternative — agents reading raw files — means the graph is
    incomplete: it models entities and relationships but not the structured
    content those entities reference.
  """
  decision     """
    A new contribution type "parser" allows extensions to read and parse
    domain-specific files referenced by file-reference fields, injecting
    structured data back into the graph. Parser contributions declare which
    file patterns they handle (e.g., *.feature, *.proto, *.yaml). When the
    compiler encounters a file-reference field matching a registered pattern,
    it dispatches to the owning extension's parser export.

    Parser host function permissions (per-call-site, extending ADR
    per_call_site_sandbox): read_file (scoped to spec root, read-only,
    restricted to patterns declared in manifest), emit_diagnostic,
    add_graph_node, add_graph_edge. Parsers do NOT get query_graph,
    emit_file, or http_get.

    query_graph is excluded because: (1) parsers extract and inject
    content — consistency checking is the validator's job (validators
    CAN query_graph); (2) other parsers may not have run yet, so
    query results are incomplete and ordering-dependent; (3) edges
    to not-yet-existing targets are valid — validation catches
    dangling edges after all parsers complete; (4) granting
    query_graph would blur the parser/validator boundary, encouraging
    parsers to embed domain-specific validation logic.

    The read_file host function enforces: path must be under spec root,
    path must match the parser's declared file patterns, read-only access
    only, maximum file size enforced by SandboxPolicy (default 1MB).

    Compilation pipeline placement: parsers run AFTER initial .spec parsing
    and reference resolution, but BEFORE validation. This lets parsed
    content participate in the full validation pass.
  """
  consequences [
    "File-referenced content becomes structured graph data — the graph is truly complete",
    "Domain-specific parsing stays in extensions — core remains domain-agnostic",
    "@specforge/software can parse .feature files into scenario/step nodes",
    "@specforge/api-design can parse .proto/.yaml into endpoint/schema nodes",
    "Any future extension can register file parsers for its domain file formats",
    "read_file is scoped and pattern-restricted — no arbitrary filesystem access",
    "Parsers cannot query the graph — they read files, emit nodes/edges, and emit diagnostics; validators handle cross-parser consistency",
    "Six contribution types total: entities, validators, renderers, providers, parsers, collectors",
  ]
  invariants   [wasm_sandbox_integrity, zero_domain_knowledge_core]
}

decision progressive_dbc_adoption "Progressive Design-by-Contract Adoption" {
  status       accepted
  date         2026-03-07
  context      """
    Design-by-Contract (requires/ensures/maintains) blocks on behaviors
    formalize pre-conditions, post-conditions, and loop invariants. Applying
    DbC to all ~270 behaviors at once would be a large, low-signal change.
    The audit identified 8 critical-path behaviors where formal contracts
    would catch the most bugs: registry population, semantic validation,
    declarative validation execution, graph delta computation, incremental
    dispatch, and migration operations.
  """
  decision     """
    Apply DbC progressively: start with 8 critical-path behaviors identified
    by the spec audit, then expand in subsequent audit cycles. Priority
    criteria: (1) behaviors that gate pipeline phase transitions, (2) behaviors
    with complex pre-conditions that are easy to violate, (3) behaviors
    referenced by multiple invariants. Each audit cycle identifies the next
    batch of behaviors for DbC application based on these criteria.
  """
  consequences [
    "Critical-path behaviors get formal contracts first — highest bug-prevention ROI",
    "Avoids bulk DbC dump that would be hard to review and maintain",
    "Progressive adoption allows learning from early DbC applications",
    "Audit cycles provide natural checkpoints for expanding DbC coverage",
  ]
}

decision migration_as_core_infrastructure "Migration as Core Infrastructure" {
  status       accepted
  date         "2026-03-07"
  invariants   [migration_idempotency, migration_semantic_preservation]
  context      """
    The spec audit flagged migration features (features/migration.spec) as
    potential P2 (zero domain knowledge in core) violations because they live
    in the core compiler rather than an extension. This parallels the question
    previously resolved for formatting (features/formatting.spec lines 1-12).
  """
  decision     """
    Migration is core infrastructure, not domain knowledge. It operates on
    .spec file syntax structure (indentation, field ordering, block layout)
    before extensions are loaded. Migration transforms are purely syntactic —
    they do not inspect entity kinds, field semantics, or domain vocabulary.
    Extension-specific migration is delegated to extension authors via Wasm
    migration hooks (ExtensionDeclaration.migration_hook).

    This parallels the formatting precedent: core owns generic CST-level
    operations; extensions contribute domain-specific behavior via hooks.
    The P7 test — "does this require a compiler change when a new domain
    appears?" — is satisfied: adding @specforge/compliance requires zero
    changes to the migration engine.

    Migration types (FormatVersion, MigrationResult, MigrationBackup,
    MigrationSummary) are extracted to types/migration.spec to keep
    types/core.spec focused on fundamental compiler data shapes.

    See also: zero_entity_core_architecture ADR, formatting P7 justification
    in features/formatting.spec.
  """
  consequences [
    "Migration stays in core — no extension extraction needed",
    "Extension migration hooks provide the domain-specific escape hatch",
    "Migration types live in types/migration.spec for clean separation",
    "P7 is satisfied: new domains require zero migration engine changes",
  ]
}

decision diagnostic_code_i016_allocation "Diagnostic Code I016 Allocation" {
  status       accepted
  date         2026-03-07
  context      "Audit found I008 collision: core output-schema used I008 but glossary allocates I008 to @specforge/software formal-proofs. Core info range (I001-I007) had no room."
  decision     "Reallocate core schema-cache-not-found to I016 (first reserved slot). Follows non-contiguous pattern established by Wasm I013 and Migration W053."
  consequences [
    "Core info codes are now I001-I007, I013 (Wasm), I016 (schema cache)",
    "Reserved info range shrinks by one: I017-I099",
  ]
}

decision graph_protocol_version_management "Graph Protocol Version Management" {
  status       accepted
  date         2026-03-07
  context      """
    SchemaVersion is compared and negotiated (negotiate_schema_version) but
    never explicitly incremented. SchemaMigration describes changes but
    migration_steps is never populated by any behavior. SchemaCompatibility
    references supported_min/supported_max but their origin is undefined.
    Without a clear version management policy, schema versions are manually
    managed, error-prone, and inconsistent.
  """
  decision     """
    Schema version is derived automatically from registry state, against
    the schema the last export recorded. Major = something a reader of the
    recorded schema relies on was removed or reshaped; minor = something
    was added; patch = documentation or provenance changed (ADR 0067).
    First export without a cache = version 1.0.0. A requested version is
    admitted when its readers read the current schema (same major, not
    newer); there is no configured supported range.
  """
  consequences [
    "Schema version is deterministic — the same registry state against the same record always produces the same version",
    "No manual version bumping required",
    "Every attribute of the schema is compared, so one version names one document",
    "First export always starts at 1.0.0",
  ]
  invariants   [schema_version_backward_compatibility]
}

decision adr_surface_contribution_model "Surface Contribution Model" {
  status       accepted
  date         "2026-03-07"
  context      """
    Extensions can extend the compilation pipeline (entities, validators,
    renderers, providers, collectors, grammars, body_parsers) but cannot
    extend the tooling surfaces — CLI, MCP, LSP. This creates a capability
    asymmetry: @specforge/software declares analyze commands but has no
    registration mechanism; Rust test collection was hardcoded in the
    CLI; extensions cannot register MCP tools so agents
    only see core tools.

    RES-24 conducted a 10-expert analysis recommending Option A: Static
    Manifest + Lazy Wasm Loading (8/10 votes). Static manifest discovery
    preserves CLI cold start performance while Wasm lazy loading keeps
    memory usage low until a surface contribution is actually invoked.
  """
  decision     """
    Extensions declare surface contributions in their manifest's surfaces
    field with three contribution types: commands[] (CLI), mcp_tools[]
    (MCP tools), mcp_resources[] (MCP resources). The compiler discovers
    contributions from manifests at startup (manifest-driven, no code
    execution). Wasm modules are loaded lazily on first invocation.

    Naming conventions: CLI exports use cmd__{id}, MCP exports use
    mcp__{name}. CLI commands are auto-promoted to MCP tools with the
    specforge.{ext_short}.{cmd_id} naming convention, giving agents
    automatic access to all extension CLI commands.

    Every surface export runs with no capability; a per-contribution
    sandbox override was dropped (ADR 0037): with nothing granted it had
    nothing to restrict.

    Phase 1 covers CLI commands, MCP tools, and MCP resources. LSP
    providers are deferred to Phase 2.

    P7 compliance: Surface contribution dispatch (register, validate, dispatch
    behaviors) is core infrastructure, not domain logic. This parallels
    call_extension_exports, the generic mechanism by which the core calls
    extension-provided Wasm exports without inspecting content. The content of each surface contribution is extension-defined;
    the dispatch mechanism is structural plumbing. Extracting dispatch into
    a separate extension would create a bootstrap paradox: the dispatch
    extension would need to be loaded before any other extension could
    register surfaces. Per the Terraform analogy: Terraform's provider
    loading is core, not a provider itself.
  """
  consequences [
    "CLI commands become extension-contributed — removes hardcoded command registration",
    "MCP tools scale dynamically with installed extensions — agents discover new tools automatically",
    "CLI cold start unaffected — manifest-only discovery, no Wasm loading at startup",
    "Auto-promotion ensures every CLI command is also an MCP tool — agents and CLI stay in sync",
    "Surface exports get no capability, so no per-contribution permission is needed",
    "Phase 2 will extend the model to LSP providers (completion, hover, code_actions, diagnostics)",
  ]
  invariants   [surface_contribution_uniqueness, surface_sandbox_ceiling]
}

decision wasm_component_runtime "All Extensions Execute as wasip2 Components" {
  status       accepted
  date         2026-09-27
  context      """
    The original build assumption compiled extension guests to
    wasm32-unknown-unknown core modules and loaded them through the Extism
    runtime, with the four builtin extensions also shipped as native Rust
    behind a composite runtime. That story drifted: the native tier made
    "Wasm is the only extension runtime" false, and the host/guest boundary
    was stringly-typed (ad-hoc JSON over extism calls) with no IDL.
  """
  decision     """
    Every extension - builtin or third-party - is a WIT-typed wasip2
    component executed through a single wasmtime component engine
    (crates/specforge-component). The native tier is deleted; a gate test
    prevents its reintroduction. The guest boundary is the
    specforge:bridge world (call: name, export-name, input -> result).
    Every call gets a deterministic fuel budget and a wall-clock
    deadline, and every instance a memory limit (ADR 0037).
    The builtin blobs are vendored under extensions/<name>/wasm/ and
    embedded at compile time, each with an inputs.json fingerprint of the
    sources it was built from; `cargo run -p xtask --bin build-builtins
    -- --install` rebuilds and re-vendors them (wasm32-wasip2 target) in a
    staged workspace that holds each extension and its path crates at
    their repo-relative paths, with the cargo home remapped, so a blob's
    bytes depend only on its sources and the toolchain, never on the
    checkout path. CI's `--check` fails when a vendored blob drifts from
    its sources, and `--verify` rebuilds every blob and fails when one
    differs.
  """
  consequences """
    Positive: one execution path, typed entry point, warm single engine
    across extensions, sandbox posture unchanged. Negative: guests need
    the wasm32-wasip2 rustup target and wit-bindgen; the old
    wasm32-unknown-unknown blobs are gone. Neutral: HostApi host functions
    were removed with the extism runtime; a typed component host-import
    surface is future work.
  """
}

decision test_runner_extensions "Tests Belong to @specforge/testing and One Extension per Runner" {
  status       accepted
  date         2026-09-29
  context      """
    Test vocabulary was spread across core (the verify grammar, the
    coverage pass, the report format, the collect normalizer) and
    @specforge/software (testability flags, W004/W009). `collect` only
    parsed one hard-coded report shape; users had to run their test
    runner exactly as SpecForge expected. See docs/adr/0002.
  """
  decision     """
    A runner-agnostic @specforge/testing builtin owns the test vocabulary:
    testability of kinds and their verify kinds (contributed to other
    extensions' kinds through enhancements), W004/W009, coverage, and the
    normalized result format. One extension per test runner
    (@specforge/cargo-test, @specforge/vitest first) declares a collector:
    how to detect the runner, one command to run it, and a pure export that
    maps its report to entity results. The compiler still never executes
    anything; `specforge collect` runs a runner extension's declared
    command after per-project consent. Tests link to entities by
    annotation in the runner's own idiom; the spec-side `tests` field is
    retired.
  """
  consequences """
    Positive: core and kind-owning extensions carry no test vocabulary;
    several runners coexist without field conflicts; `collect` runs the
    project's real runner. Negative: projects using verify must enable
    @specforge/testing (init does it alongside @specforge/software);
    supersedes "SpecForge never executes tests" in README and vision.
  """
}
