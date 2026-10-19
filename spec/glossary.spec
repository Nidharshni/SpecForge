// SpecForge Glossary — Ubiquitous language for the project
//
// This glossary contains terms for the core compiler (pipeline stages,
// entity model primitives, extension model, graph protocol, Wasm runtime,
// editor support) AND extension-specific domain terms. The glossary keyword
// is a singleton — only one per project — so extension terms are grouped
// under clearly labeled section headers below.

// ── Compiler Pipeline ──────────────────────────────────────

term t_spec_file "spec file" {
  definition """
    A source file with the .spec extension containing entity declarations
    in the SpecForge DSL. Parsed by the compiler into an AST.
  """
  aliases    ["dot-spec file", ".spec file"]
  see_also   [
    t_parser,
    t_extension,
    t_spec_root,
    t_zero_entity_core,
    t_sandbox_policy,
    t_wasm_trap,
    t_fuel_metering,
    t_structured_conditions,
    t_coverage_tracking_item,
    t_event_graph_linting,
    t_specification_depth_level,
    t_rpn,
  ]
}

term t_compiler_pipeline "compiler pipeline" {
  definition """
    The sequence of stages that transforms .spec source files into a validated
    in-memory graph. Compilation is two-phase with strict ordering:

      Phase 1: Parser (structural parsing of all .spec files)
               → Extension Loader (load manifests, populate registries)

      Phase 2: Resolver (link references, build graph edges)
               → Validator (structural + declarative checks)
               → Emitter (output artifacts)

    Phase 2 MUST NOT begin until all registries (KindRegistry, FieldRegistry,
    edge type set) are fully populated from Phase 1. Within each phase, stages
    are sequential — resolution completes before validation begins.
  """
  aliases    ["pipeline", "compilation pipeline"]
  see_also   [t_resolver, t_validator, t_emitter, t_spec_file]
}

term t_parser "parser" {
  definition """
    The first compiler stage. Uses a Tree-sitter grammar to transform
    .spec source text into a per-file Abstract Syntax Tree (AST).
  """
  context    "SpecForge uses Tree-sitter, not a hand-written recursive descent parser."
}

term t_resolver "resolver" {
  definition """
    The compiler stage that processes use imports and links entity ID
    references to their declarations. Records resolved references as
    pending edges for the graph builder to materialize. Processes files
    in topological order (dependencies first).
  """
  aliases    ["reference resolver", "import resolver"]
  see_also   [t_use_import, t_in_memory_graph, t_soft_reference]
}

term t_in_memory_graph "in-memory graph" {
  definition """
    A directed graph of nodes (entities) and edges (relationships) built from
    parsed .spec files. Serves as the compiler's database — no external database
    required. Mutable and incrementally updatable for watch mode.
  """
  aliases    ["graph", "spec graph", "entity graph"]
}

term t_validator "validator" {
  definition """
    The compiler stage that enforces graph invariants: no dangling references,
    no duplicate IDs, no import cycles, unreferenced detection. Emits diagnostics
    at error, warning, and info levels.
  """
}

term t_emitter "emitter" {
  definition """
    An output stage that traverses the in-memory graph to produce
    artifacts: Graph Protocol JSON, markdown documentation, DOT visualization,
    traceability reports, or token-optimized agent context. Code generation
    is NOT a SpecForge concern — AI agents generate code by consuming the
    Graph Protocol.
  """
  aliases    ["renderer"]
  see_also   [t_renderer, t_agent_context]
}

term t_diagnostic "diagnostic" {
  definition """
    A compiler message with a severity (error, warning, info), a
    diagnostic code, source location, and human-readable message.
    Styled like rustc output. Each code has exactly one meaning, one
    owner (core or a single extension) and one level, stated once in the
    diagnostic catalog (`specforge explain`, rendered to
    docs/diagnostics.md), which also gives every core code a typed
    constant. Codes an extension reports are checked where they enter
    the host. Third-party extensions use E900-E998, W900-W998,
    I900-I998 (I999 is core), at the level the prefix states.
  """
  aliases    ["compiler diagnostic", "validation message"]
}

// ── Entity Model ───────────────────────────────────────────

term t_entity "entity" {
  definition """
    A declared block in a .spec file that represents a specification concept.
    Each entity has a unique ID, typed fields, and participates in the
    traceability chain via compiler-checked cross-references. Entity types
    are NOT built into the core compiler — they are defined by installable
    extensions. The core is a pure typed-graph engine.
  """
  see_also   [t_spec_coverage, t_verify_statement, t_entity_mapping, t_compiler_pipeline]
}

term t_entity_id "entity ID" {
  definition """
    A globally unique free-form identifier for an entity. Any valid identifier
    (letters, digits, underscores, 2-60 chars, starts with a letter or
    underscore), and not a reserved word (E013). No
    enforced case convention — projects choose their own naming style.
  """
  aliases    ["ID", "entity identifier"]
  see_also   [t_string_interning]
}

term t_use_import "use import" {
  definition """
    A file-level directive that brings symbols from another .spec file
    into scope for reference resolution. Syntax: use path/to/file.
    The .spec extension is implicit.
  """
  aliases    ["import", "use directive"]
  context    "Not the same as a programming language import — brings spec symbols into scope, not code."
  see_also   [t_reference_list]
}

term t_reference_list "reference list" {
  definition """
    A bracket-delimited list of entity IDs that creates compiler-checked
    cross-references. Syntax: [spec_root_singleton, multi_error_collection]. Every ID must resolve
    to a declared entity or a diagnostic is emitted.
  """
}

term t_soft_reference "soft reference" {
  definition """
    A cross-module reference that degrades gracefully when the target
    extension is not installed. Emits I004 (info) instead of E003 (error)
    when the referenced entity's extension is missing.
  """
  context    "Used for cross-extension references when the target extension is not installed."
  see_also   [t_diagnostic]
}

// ── Extension Model ────────────────────────────────────────

term t_extension "extension" {
  definition """
    A Wasm extension (.wasm binary) that provides domain vocabulary to the
    compiler: entity kinds, edge types, validation rules, and testability
    flags. Loaded via the wasmtime runtime. The core compiler has zero built-in
    entity types — ALL domain knowledge comes from extensions. Official extensions:
    @specforge/software, @specforge/product, @specforge/governance. Domain
    extensions: @specforge/atomic-design, @specforge/compliance, @specforge/api-design.
  """
  aliases    ["plugin", "domain extension"]
  context    "Terraform-exact model: core has zero domain knowledge, all vocabulary from extensions. 'Extension' is the canonical term for entity IDs and code references; 'plugin' is accepted as a human-facing alias only."
  see_also   [t_provider, t_peer_dependency]
}

term t_provider "provider" {
  definition """
    A Wasm extension that registers ref schemes for external reference validation.
    Uses the specforge.http_get host function for network validation. Validates
    identifier patterns, resolves URLs, and supports multiple aliased instances.
    Example: @specforge/gh for GitHub references.
  """
  context    "A contribution type within an extension — extends ref validation, not the entity model."
}

term t_renderer "renderer" {
  definition """
    A contribution type within a Wasm extension that reads the compiled graph
    via the specforge.query_graph host function and produces non-code output
    files (reports, dashboards, traceability matrices) via specforge.emit_file.
    Code generation is NOT a SpecForge concern — AI agents generate code by
    consuming the Graph Protocol. Renderers produce supplementary artifacts
    only.
  """
  context    "A contribution type for non-code outputs only. Code generation is the responsibility of AI agents, not SpecForge. Renamed from 'exporter' to avoid confusion with code generation."
  see_also   [t_drift_detection]
}

// ── Compilation Concepts ───────────────────────────────────

term t_incremental_compilation "incremental compilation" {
  definition """
    The watch mode strategy: a file change re-parses only the changed
    files, patches the graph with their entities, re-links references
    across the whole graph, resolves every file's imports again and
    re-validates. Target: <100ms file-change-to-diagnostics.
  """
  aliases    ["incremental recompilation", "watch mode"]
}

term t_string_interning "string interning" {
  definition """
    A memory optimization where frequently compared strings (entity IDs,
    file paths, identifiers) are stored once and compared by pointer
    equality instead of character-by-character. SpecForge uses the
    lasso crate for this.
  """
  context    "An implementation detail that affects performance, not user-visible behavior."
  see_also   [t_incremental_compilation]
}

term t_spec_root "spec root" {
  definition """
    The project configuration in specforge.json that declares project
    identity (name, version), installed extensions, provider
    configurations, personas, and surfaces. Exactly one per project.
  """
  aliases    ["project root", "spec block"]
}

term t_domain_vocabulary "domain vocabulary" {
  definition """
    The set of entity kinds, edge types, and validation rules that an
    extension contributes to the compiler. Each extension defines its own
    domain-specific vocabulary — the core compiler has no vocabulary of
    its own. Extensions for different domains contribute entirely different
    entity kinds.
  """
  aliases    ["vocabulary", "domain model"]
  context    "The zero-entity core architecture means vocabulary is 100% extension-defined."
}

term t_zero_entity_core "zero-entity core" {
  definition """
    The architectural principle that the SpecForge compiler core has zero
    built-in entity types. The core is a pure typed-graph engine that parses
    any keyword-name-body block, resolves references, builds graphs, and
    validates constraints — but has no opinion about what entity types exist.
    Domain semantics are entirely defined by installable extensions.
  """
  aliases    ["zero-core", "entity-free core"]
  context    "Terraform-exact analogy: Terraform core has zero infrastructure knowledge, SpecForge core has zero domain knowledge."
  see_also   [t_domain_vocabulary]
}

// ── Graph Protocol & Agent Context ────────────────────────────

term t_graph_protocol "graph protocol" {
  definition """
    The stable JSON schema that specforge export produces — the contract
    between the compiler and any consuming AI agent. The Graph Protocol is
    SpecForge's primary product: a structured, validated, cross-referenced
    representation of human intent that any agent can consume for any task.
  """
  aliases    ["Graph Protocol", "agent context protocol"]
  context    "The graph schema is the standard. The compiler and DSL are implementation details."
  see_also   [t_multi_resolution_query]
}

term t_agent_context "agent context" {
  definition """
    A token-optimized export format produced by specforge export --format=context.
    Designed for direct injection into AI agent context windows. Contains entity
    IDs, contracts, relationships, and traceability chains in a compact JSON
    representation. Any AI agent (coding, PM, compliance, docs, security) can
    consume it.
  """
  aliases    ["context export", "agent-context output"]
  see_also   [t_port]
}

term t_multi_resolution_query "multi-resolution query" {
  definition """
    A graph query that returns entities at a specific zoom level — from
    project-wide summaries down to individual entity details. Zoom levels
    depend on installed extensions and the entity hierarchy they define.
    Allows agents to request exactly the context slice they need via
    specforge query --scope and --hop flags. Inspired by Large Concept
    Models research on operating at the right abstraction level.
  """
  aliases    ["scoped query", "resolution query"]
  see_also   [t_query_extension]
}

// ── Editor & Syntax Support ────────────────────────────────

term t_body_parser "body parser" {
  definition """
    An entity kind's has_body_parser flag: its extension says the kind's
    body carries syntax of its own. The core grammar reads every body and
    every parse error in it is reported, whatever the flag says.
    Extensions cannot plug in body parsing code.
  """
  see_also   [t_entity]
}

term t_generic_entity_block "generic entity block" {
  definition """
    The single grammar rule that parses ALL entity blocks as
    keyword name [title] { fields } structures. There are no
    "built-in" keywords at the grammar level — every entity block
    uses this rule. Produces a clean AST node with a kind field
    containing the keyword, enabling syntax highlighting, code
    folding, and document symbols for all entity types. Only spec,
    ref, use, and define have separate dedicated grammar rules due
    to unique structural syntax (ref uses scheme:identifier format).
  """
  aliases    ["generic_entity_block", "entity_block"]
  context    "Part of the zero-entity core architecture. The grammar is keyword-agnostic."
  see_also   [t_graph_protocol]
}

term t_query_extension "query extension" {
  definition """
    A .scm query pattern shipped by an extension in its manifest to extend
    editor syntax highlighting, code folding, or indentation for
    extension-specific entity types. Composed with base query files by
    string concatenation in extension load order.
  """
  aliases    ["query extension pattern", ".scm extension"]
  context    "Part of Tier 2 of the 3-tier highlighting architecture. See RES-22."
  see_also   [t_semantic_token]
}

term t_semantic_token "semantic token" {
  definition """
    An LSP 3.16+ protocol mechanism for runtime syntax classification
    beyond tree-sitter static queries. The LSP server assigns token
    types (keyword, property, type, etc.) to source ranges. Used to
    classify entity keywords, enhanced fields, and cross-extension
    references that static query files cannot capture.
  """
  context    "Part of Tier 3 of the 3-tier highlighting architecture. See RES-22."
  see_also   [t_glossary_entity_enhancement, t_entity_id]
}

// ── Wasm Component Runtime ───────────────────────────────────

term t_wasm "Wasm" {
  definition """
    WebAssembly — a portable binary instruction format used as the
    universal extension runtime for SpecForge. Extensions compile to
    wasip2 components that run in a sandboxed environment via the
    wasmtime Component Model runtime.
  """
  aliases    ["WebAssembly", ".wasm"]
  see_also   [t_compile_cache]
}

term t_host_function "host function" {
  definition """
    A function provided by the SpecForge compiler that Wasm extensions can
    call during execution. Host functions are the only way extensions
    interact with the compiler and host system. Standard host functions:
    specforge.query_graph (read the compiled graph), specforge.emit_diagnostic
    (emit a compiler diagnostic), specforge.emit_file (write an output file),
    specforge.http_get (fetch a URL for provider validation).
    Additionally, specforge.add_graph_node and specforge.add_graph_edge
    allow extensions to add graph node and edge instances at runtime.
    Entity kinds and edge types are declared in extension manifests.
  """
  aliases    ["host fn"]
  see_also   [t_linear_memory]
}

term t_linear_memory "linear memory" {
  definition """
    The contiguous block of memory available to a Wasm module. Each
    extension instance has its own isolated linear memory, capped at 64MB
    by default. The host cannot be accessed outside this boundary —
    attempts to do so result in a trap.
  """
}

term t_compile_cache "compile cache" {
  definition """
    Wasmtime's on-disk cache of compiled component machine code, selected
    via SPECFORGE_WASMTIME_CACHE (default $HOME/.cache/specforge/wasmtime;
    'off' disables). Entries are keyed by component bytes and engine
    configuration; a cache hit deserializes precompiled code instead of
    recompiling, reducing extension load to <50ms. Platform-specific —
    engine config is part of the key.
  """
  aliases    ["AOT compilation", "ahead-of-time compilation"]
}

term t_peer_dependency "peer dependency" {
  definition """
    An extension's declared requirement on another extension: its name, a
    SemVer range (read as Cargo reads one) and whether it is optional. The
    registry build loads an extension after its peers and judges each peer
    by one rule (ADR 0041): a required peer must be loaded, a loaded one at
    a version its range accepts. Unsatisfied is E027; a range that is not
    SemVer is E073.
  """
  see_also   [t_edk]
}

term t_sandbox_policy "sandbox policy" {
  definition """
    The limits a Wasm extension's handshake declares for its sandbox: the
    wall-clock budget of one call (max_execution_ms) and the instance's
    linear memory (max_memory_mb), each held to the host's ceiling and
    enforced by the wasmtime runtime. It grants nothing: a component has
    no file, network, environment or stdin access whatever it declares
    (ADR 0037).
  """
}

term t_edk "EDK" {
  definition """
    Extension Development Kit — the set of libraries, templates, and
    documentation for authoring SpecForge Wasm extensions. Available for
    Rust, Go, JavaScript/TypeScript, and other languages with Wasm
    compilation targets. Accessed via specforge extension init.
  """
  aliases    ["Extension Development Kit"]
  see_also   [t_host_function]
}

term t_glossary_entity_enhancement "entity enhancement" {
  definition """
    An extension's ability to add fields and edges to existing entity types
    via declarations in its manifest.json. Enhanced fields participate
    in parsing, resolution, and validation like extension-defined fields.
    Conflicts between extensions are resolved via configurable enhancement
    policies.
  """
  aliases    ["field enhancement", "enhancement"]
  see_also   [t_body_parser]
}

term t_wasm_trap "Wasm trap" {
  definition """
    An unrecoverable WebAssembly error such as out-of-bounds memory
    access, stack overflow, or unreachable instruction. The wasmtime
    runtime catches all traps and converts them to Result errors.
    Trapped extensions transition to the failed lifecycle state.
  """
  aliases    ["trap", "Wasm fault"]
}

term t_fuel_metering "fuel metering" {
  definition """
    Wasmtime's instruction counting mechanism for enforcing
    execution time limits on Wasm extensions. Each Wasm instruction
    consumes fuel; when the fuel budget is exhausted, the extension
    traps. Prevents runaway extensions from blocking compilation.
  """
  aliases    ["fuel"]
}

term t_content_addressed_cache "content-addressed cache" {
  definition """
    A cache strategy where artifacts are keyed by a hash of the bytes
    that produced them, so identical inputs always hit and changed inputs
    always miss. The grammar artifact cache keys on content hash + ABI
    version; the component compile cache keys on bytes + engine config
    inside wasmtime.
  """
  see_also   [t_wasm, t_compile_cache]
}

// ── @specforge/formal Terms ──────────────────────────────────
//
// @specforge/formal's vocabulary was deliberately renamed away from academic
// formal-methods terms (see decision formal_terminology_rename in
// spec/extensions/formal/decisions.spec): the checks are heuristic
// structural analysis on string-based conditions, not mathematical proof,
// and the old names ("Design by Contract", "B-Method Refinement", "CSP
// Event Flow Analysis", "Verification Obligations") oversold that. The
// terms below use the current, renamed vocabulary — the academic terms are
// kept only as `aliases` so old searches still resolve.

term t_structured_conditions "structured conditions" {
  definition """
    Inline requires/ensures/maintains blocks on behaviors and invariants,
    checked for internal consistency (not mathematically proven) by the
    condition_consistency pass of @specforge/formal.
  """
  aliases    ["Design by Contract", "DbC", "pre/postconditions"]
  see_also   [t_coverage_tracking_item]
}

term t_refinement_chain "refinement chain" {
  definition """
    Sequence of behavior refinements from abstract specification to
    concrete implementation, declared either by a `refines` field on the
    concrete behavior (the abstract marked `abstract true`) or by a
    first-class `refinement` entity naming abstract_entity and
    concrete_entity. Checked by the layering_verify compiler pass:
    cycles produce E041, dropped ensures conditions E031.
  """
  aliases    ["refinement path", "specification layering", "B-Method Refinement"]
}

term t_event_graph_linting "event graph linting" {
  definition """
    Structural analysis of event synchronization: `sync` blocks (barrier
    behaviors and timeouts) plus shared `protocol` entities (ordering,
    timeout, delivery semantics) checked by the event_graph_analyze pass
    of @specforge/formal. Flags unmitigated cycles instead of claiming
    formal deadlock detection.
  """
  aliases    ["sync block", "CSP synchronization", "CSP Event Flow Analysis", "deadlock detection"]
  see_also   [t_coverage_tracking_item]
}

term t_coverage_tracking_item "coverage tracking item" {
  definition """
    Machine-readable record that a formal analysis pass expects evidence
    for. Each item tracks an entity ID, kind (condition_coverage,
    invariant_coverage, layering_coverage, process_coverage), description,
    and discharge status (pending, auto_proved, test_verified). Renamed
    from "verification obligation" because these are heuristic structural
    checks, not machine-checked proofs.
  """
  aliases    ["verification obligation", "proof obligation"]
  see_also   [t_refinement_chain]
}

term t_specification_depth_level "specification depth level" {
  definition """
    Five-level adoption path for formal methods in specifications:
    prose, entity_graph, conditions, invariants, proofs. Each level adds
    machine-checkable rigor without requiring the next. Per RES-25.
  """
  aliases    ["progressive formality", "formality levels"]
}

term t_formal_property "formal property" {
  definition """
    A temporal/behavioral assertion about a system over time — kind
    safety, liveness, or fairness. Distinct from a structured condition:
    conditions describe point-in-time state, properties describe behavior
    across a sequence of states.
  """
  see_also   [t_structured_conditions]
}

term t_formal_process "formal process" {
  definition """
    A CSP-style communicating process entity: an alphabet of events, a set
    of states (initial/accepting), and a composition operator (parallel,
    sequential, choice, interleaving) describing how it combines with
    other processes. The event-graph-linting counterpart to a state
    machine.
  """
  see_also   [t_event_graph_linting]
}

// ── @specforge/governance Terms ──────────────────────────────

term t_rpn "RPN" {
  definition """
    Risk Priority Number. Calculated as severity x occurrence x detection
    in an FMEA failure_mode block. Higher RPN means higher risk priority.
    The compiler validates the arithmetic (E005).
  """
  aliases    ["risk priority number"]
}

// ── @specforge/rust Terms ────────────────────────────────────

term t_specforge_test "specforge-test" {
  definition """
    The runtime Rust crate that provides the #[specforge_test(...)]
    attribute (re-exported from specforge-test-macros) and the
    Drop-based TestGuard for recording test results. The attribute
    registers the test itself; @specforge/cargo-test collects the results.
  """
  aliases    ["specforge test crate"]
  see_also   [t_specforge_test_macros, t_test_guard]
}

term t_specforge_test_macros "specforge-test-macros" {
  definition """
    The proc macro Rust crate that implements the #[specforge_test]
    attribute. Its expansion calls into specforge-test for the runtime
    guard.
  """
  aliases    ["specforge test macros crate"]
}

term t_test_guard "test guard" {
  definition """
    A Drop-based Rust struct (TestGuard) that records whether a test
    passed or failed. On drop, checks std::thread::panicking() — false
    means pass, true means fail (inverted under #[should_panic]). Results
    are written to target/specforge/ via an atexit handler.
  """
  aliases    ["TestGuard", "drop guard"]
}

term t_naming_convention "naming convention" {
  definition """
    The convention for Rust test function names that enables convention-based
    entity mapping: {entity_id}__{description_slug}. The double underscore
    separates the entity ID from the test description. A test whose name
    carries no entity is linked to the innermost enclosing module named
    after one. `specforge collect` applies it to plain `#[test]` functions.
  """
  see_also   [t_double_underscore_separator]
}

term t_double_underscore_separator "double underscore separator" {
  definition """
    The __ delimiter used in Rust test function names to separate the
    entity ID from the description slug. Unambiguous because entity IDs
    (snake_case) never contain double underscores. Example:
    validate_input__rejects_empty_name.
  """
}

// ── Traceability & Coverage (core concepts) ─────────────────

term t_traceability_chain "traceability chain" {
  definition """
    The directed path through the entity graph following registered edge
    types from extensions. The chain shape depends entirely on installed
    extensions — different extension combinations produce different chains.
    Every link is compiler-checked.
  """
  aliases    ["trace chain", "traceability path"]
}

term t_spec_coverage "spec coverage" {
  definition """
    The percentage of testable entities that have passing tests. Distinct
    from code coverage — spec coverage measures how many testable
    entities (kinds @specforge/testing makes testable) have recorded
    passing test results. `specforge analyze coverage --min` gates on it.
  """
  aliases    ["specification coverage"]
}

term t_verify_statement "verify statement" {
  definition """
    A declaration inside a testable entity block specifying how to test
    that entity. Syntax: verify <kind> "description". Verify kinds are
    extension-defined — not hardcoded: `verify` is reserved syntax whose
    meaning comes from the registry. @specforge/testing makes kinds
    testable and declares each kind's verify kinds (for example unit,
    contract, integration, property and performance on behavior) — these
    are NOT core defaults, they exist only when the extension is enabled.
    Testable entities without verify statements trigger W004.
  """
}

term t_specforge_report_json "specforge-report.json" {
  definition """
    The project's recorded test results, written by `specforge collect`
    and read, at the root the project was compiled from and never an
    ancestor's, by `specforge analyze`, `specforge stats` and the MCP
    coverage views. Contains per-entity test results (name, pass/fail, the
    verify obligation, duration, and the runner that recorded it) for any
    testable entity kind.
  """
  aliases    ["coverage report", "test report"]
}

term t_collect_command "collect command" {
  definition """
    The specforge collect subcommand. It detects the project's test
    runners from the enabled runner extensions, runs each one's declared
    command after the user approves it (or parses an existing report
    with --no-run), and records the results in specforge-report.json.
  """
  aliases    ["specforge collect"]
  see_also   [t_specforge_report_json]
}

term t_entity_mapping "entity mapping" {
  definition """
    The process of resolving which spec entity a test function corresponds
    to, in the runner's own idiom: the #[specforge_test] attribute in Rust,
    vitest test metadata in TypeScript. Explicit linkage takes precedence
    over naming conventions; spec files carry no test paths.
  """
  aliases    ["test-to-entity mapping", "entity resolution"]
  see_also   [t_specforge_test, t_naming_convention]
}

// ── @specforge/software Terms ────────────────────────────────
// These terms are defined by the @specforge/software extension.
// They exist in this glossary because the glossary keyword is a singleton.

term t_port "port" {
  definition """
    An entity kind defined by @specforge/software representing an interface
    boundary between the domain and the outside world. Inbound ports define
    what the system offers; outbound ports define what the system requires.
    Ports are declarative specifications — AI agents consume port
    declarations from the Graph Protocol.
  """
  context    "Extension-defined entity kind from @specforge/software. In this domain, port means a hexagonal architecture port, not a network port."
  see_also   [t_traceability_chain]
}

// ── Extension Concerns (renderer-specific) ─────────────────

term t_drift_detection "drift detection" {
  definition """
    The process of verifying that output files (from third-party renderers)
    match the current state of .spec files. Renderer extensions implement
    drift detection via checksum headers. This is an extension concern, not
    a core compiler feature.
  """
  see_also   [t_drift_checksum]
}

term t_drift_checksum "drift checksum" {
  definition """
    A SHA256 hash embedded in a @specforge-checksum header comment at
    the top of renderer output files. Used by third-party renderer
    extensions to detect when their output is stale relative to the
    current spec state. This is a renderer extension concern, not a
    core compiler feature.
  """
  aliases    ["checksum header", "@specforge-checksum"]
}
