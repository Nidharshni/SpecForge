// Zero-entity core architecture features

use "behaviors/extensions"
use "behaviors/lsp"
use "behaviors/validation"
use "behaviors/zero-entity-lsp"
use "behaviors/zero-entity-registries"
use "behaviors/zero-entity-validation"

feature declarative_validation_rules "Declarative Validation Rules" {
  status   done
  problem  """
    Validation passes are hardcoded functions in the compiler.
    Adding a new validation rule requires modifying Rust source code,
    recompiling the compiler, and releasing a new version. Extension authors
    cannot define domain-specific validation rules for their entity types
    without forking the compiler.
  """
  solution """
    Extensions declare validation rules as declarative patterns in their
    manifest. Each pattern specifies a check kind (no_incoming_edges,
    missing_field_when_flag_set, cycle_detection, file_exists, etc.), a
    target entity kind, a diagnostic code, severity, and message template
    with interpolation variables ({id}, {kind}, {field}). The core engine
    interprets these patterns against the compiled graph, eliminating the
    need for extension-specific Rust code. For complex validations that
    cannot be expressed declaratively, extensions MAY export Wasm validator
    functions that call the `specforge.query_graph` host function for graph
    access and `specforge.emit_diagnostic` for reporting. Declarative
    patterns are the primary mechanism; Wasm validators are the escape hatch.

    Extension-defined entity kinds are NOT automatically unreferenced-checked.
    Unreferenced detection (no_incoming_edges) is an opt-in validation pattern
    that each extension explicitly declares in its validationRules array.
    An entity kind without an explicit no_incoming_edges pattern in its
    extension manifest will NOT produce unreferenced warnings, even if it has
    zero incoming edges. This is intentional — some entity kinds are
    naturally root nodes with no expected incoming edges.
  """
}

feature extension_manifest "Extension Manifest" {
  status   done
  problem  """
    Extensions need a structured manifest format that carries rich metadata
    for each entity kind: LSP integration (semantic tokens, icons),
    visualization (DOT shapes), testability configuration, and typed field
    definitions. Verify kinds need to be extension-defined rather than
    hardcoded.
  """
  solution """
    Structured entityKinds entries in the extension manifest carry full
    metadata: testable flag, singleton flag, supportsVerify flag,
    semantic_token for LSP classification, lsp_icon
    for outline SymbolKind, dot_shape for visualization, typed field
    definitions with edge mappings, custom allowedVerifyKinds, and
    extension-level verifyKinds. The manifest schema is validated at
    load time. Boolean flag consistency (e.g., testable vs supportsVerify)
    is checked as a generic manifest lint pass — the core does not
    interpret what these flags mean semantically, only that they are
    not contradictory.
  """
}

feature dynamic_entity_registration "Dynamic Entity Registration" {
  status   done
  problem  """
    Entity types are defined as a closed enum with hardcoded
    variants. Adding a new entity type requires modifying the enum, updating
    match arms across the codebase, and recompiling. The Custom(String)
    variant exists but has limited support throughout the pipeline.
  """
  solution """
    Registries (KindRegistry, FieldRegistry, edge type set) start empty
    and are populated exclusively from extension manifests in topological
    order. The parser recognizes any keyword structurally; the registries
    determine which keywords are semantically valid. Cross-validation
    ensures internal consistency of all registered data before semantic
    validation begins. Unknown field names on registered entity kinds
    produce W020 warnings during semantic validation.
  """
}

feature extension_driven_lsp "Extension-Driven LSP" {
  status   done
  problem  """
    LSP features for extension-defined entity kinds get generic treatment
    with no keyword completion, no semantic classification, default icons,
    and minimal hover information. The LSP needs extension-aware logic to
    provide rich editor integration for any entity kind from any extension.
  """
  solution """
    Five extension-layer bridge behaviors query the KindRegistry and
    FieldRegistry for extension-defined metadata: keyword completion with
    snippet templates, semantic token classification using the manifest's
    semantic_token field, hover showing source extension and testability,
    outline icons using the manifest's lsp_icon field, and field name
    completion using the FieldRegistry. User-facing LSP features
    (autocomplete, code actions, rename, etc.) delegate to these bridge
    behaviors for domain-aware logic.
  """
}

feature extension_driven_visualization "Extension-Driven Visualization" {
  status   done
  problem  """
    Graph visualization uses hardcoded shapes and styles for entity nodes.
    Extension-defined entity kinds get generic default rendering with no
    domain-aware visual differentiation. The DOT serializer needs extension-
    aware logic to provide meaningful visual output for any entity kind
    from any extension.
  """
  solution """
    The DOT graph serializer queries the KindRegistry for each entity's
    visual attributes from the extension manifest. Entity nodes use the
    extension-defined DOT shape, color, and fill color from KindRegistryEntry
    (shape: box, ellipse, diamond, hexagon, etc.; color: inherited when
    unspecified; fillcolor: none when unspecified), with a sensible default
    shape of "box" when no shape is specified. Edge
    rendering delegates to render_extension_defined_edge_styles which
    reads edge style metadata (color, style, arrowhead) from the edge
    type registry. Both node shapes and edge styles are fully extension-
    driven, ensuring visual output reflects the domain vocabulary without
    hardcoding any visual properties in the core serializer.
  """
}

feature zero_entity_bootstrap "Zero-Entity Bootstrap" {
  status   done
  problem  """
    The compiler assumes entity keywords are known at parse time because
    they are hardcoded. With zero built-in entities, the parser cannot
    validate keywords during parsing because extensions have not been loaded
    yet. A chicken-and-egg problem exists: the parser needs to know valid
    keywords, but keywords come from extensions loaded after parsing.
  """
  solution """
    A two-phase compilation model. Phase 1 performs purely structural
    parsing — every keyword name { } block becomes a generic entity node
    with no keyword validation. Phase 2 loads extensions, populates
    registries, then validates all keywords against the KindRegistry.
    Pipeline event sequence: all_files_parsed → extension_manifests_loaded →
    registries_populated → validation_complete.
    Unknown keywords produce E024 with help text suggesting which extension
    to install. Every entity kind comes from an extension: a define block
    in a .spec file is reported (W143) and ignored (ADR 0005).
    suggest_missing_extensions consumes the candidate
    vocabulary the registry build is handed (the builtins', generated with them, ADR 0122). With zero extensions, the compiler gracefully degrades to
    structural-only mode with an I002 info diagnostic. Export commands
    (specforge export) MUST still produce valid Graph Protocol JSON from
    the structural-only graph, with generic entity nodes and reference
    edges preserved. The structural-only export guarantee is enforced by
    graceful_degradation_without_extensions.
  """
}

// extension_driven_schema_export merged into self_describing_graph_protocol
// in features/output.spec — same 3 behaviors, consolidated to avoid duplication.

feature extension_driven_code_actions "Extension-Driven Code Actions" {
  status   done
  // Owned: code_actions_for_missing_verify, code_action_create_entity_stub,
  //   code_action_replace_unresolved
  // Bridge: listed in features/lsp.spec code_actions
  problem  """
    LSP code actions (quick fixes, refactorings) depend on extension
    metadata — testability flags, field registries, entity kind definitions.
    Without extension awareness, code actions cannot suggest adding missing
    verify statements or creating entity stubs for unresolved references.
  """
  solution """
    Code action providers query the KindRegistry for testability flags and
    the FieldRegistry for expected fields. Missing test code actions are
    offered only for entity kinds with testable=true. Entity stub creation
    uses the registered field definitions to generate complete block
    templates. Both actions adapt to whichever extensions are installed.
  """
}

feature extension_driven_coverage "Extension-Driven Coverage" {
  status   done
  // Bridge: compute_project_statistics is defined in behaviors/output.spec and
  // also listed in ci_integration (features/output.spec). This feature owns the
  // extension-aware coverage aspect; ci_integration owns the CLI/exit-code aspect.
  problem  """
    Coverage percentage must only count entity kinds that are testable,
    but testability is an extension-level declaration, not a hardcoded
    property. Without flowing extension metadata into coverage calculations,
    coverage numbers would be incorrect or include untestable entities.
  """
  solution """
    This feature is intentionally minimal — it documents the single point
    where coverage computation depends on extension-provided testability
    flags. The behavior itself (compute_project_statistics) is defined in
    behaviors/output.spec. It queries the KindRegistry for the testable
    flag on each entity kind. Only entities whose kind has testable=true
    in the extension manifest contribute to the coverage denominator.
    An entity is declared when it has at least one verify statement.
    The declared percentage is declared testable entities /
    testable_entity_count (0% when testable_entity_count = 0); the proof
    percentage, from recorded test results, is the share proven. This
    ensures coverage percentages accurately reflect which entities are
    expected to have test evidence. Coverage is part of the traceability
    feedback loop (P5): agents read coverage to prioritize untested
    entities, closing the intent→proof cycle.

    The zero-denominator case (testable_entity_count = 0) MUST produce
    0% coverage, not a division error or undefined value. This occurs
    when no extensions with testable entity kinds are installed.
  """
}
