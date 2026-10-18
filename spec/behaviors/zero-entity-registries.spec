// Zero-entity core — the registry build's outcomes, grammar, bootstrap, unknown kinds, visualization

use "events/compilation"
use "invariants/core"
use "invariants/zero-entity-core"
use "ports/outbound"
use "types/config"
use "types/core"
use "types/diagnostics"
use "types/errors"
use "types/wasm"
use "types/zero-entity-core"

// -- Dynamic Entity Registration ---------------------------------------------

behavior report_define_blocks "Report Unsupported Define Blocks" {
  features   [zero_entity_bootstrap]
  invariants [zero_domain_knowledge_core]
  category   validation
  types      [SpecFile, Diagnostic]
  ensures {
    define_warned     "Each define block produces one W143 warning naming it"
    no_entity_created "A define block adds no entity to the graph, so its body is not read as references and its name is not checked as an ID"
  }
  contract   """
    Custom entity kinds come from extensions, not from .spec files: a
    project's kinds are a function of specforge.json and its loaded
    extensions only (ADR 0005). The grammar still parses
    define <name> { fields }, and define stays a reserved word, but the
    compiler MUST report each define block with one W143 warning that
    suggests writing an extension, and MUST NOT add it to the graph. An
    incremental rebuild MUST report a define block as a fresh compile
    does.
  """
  verify unit "a define block produces one W143 and adds no node to the graph"
  verify integration "an incremental rebuild reports a define block as a fresh compile does"
}

// -- Grammar Consolidation ---------------------------------------------------

behavior collapse_grammar_to_generic_entity_block "Collapse Grammar to Generic Entity Block" {
  features   [zero_entity_bootstrap, spec_file_parsing]
  invariants [zero_domain_knowledge_core]
  category   command
  types      [SpecFile, Entity]
  requires {
    grammar_source_available "Tree-sitter grammar source is available for compilation"
  }
  ensures {
    single_generic_rule              "Grammar contains exactly one generic entity_block rule for all extension keywords"
    structural_rules_preserved       "spec_block, ref_block, use_import, and define_block remain as separate grammar rules"
    no_keyword_validation_in_grammar "All keyword validation deferred to semantic phase"
  }
  contract   """
    The tree-sitter grammar MUST have exactly ONE generic entity_block rule
    that parses any keyword id [title] { fields } structure. All per-keyword
    block rules that existed in the pre-zero-entity grammar MUST be collapsed
    into this single generic rule. spec_block, ref_block, use_import, and
    define_block MUST remain as separate grammar rules because they have unique
    structural syntax (ref has scheme:identifier ID format, spec has singleton
    semantics). All keyword validation MUST happen in the semantic phase, not
    the grammar.
  """
  verify unit "grammar has single generic entity_block rule"
  verify unit "no per-keyword block rules remain in grammar"
  verify unit "spec_block remains as separate grammar rule"
  verify unit "ref_block remains as separate grammar rule"
  verify unit "use_import remains as separate grammar rule"
  verify unit "define_block remains as separate grammar rule"
  verify contract "Collapse Grammar to Generic Entity Block: grammar collapse holds — grammar_source_available, single_generic_rule, structural_rules_preserved, no_keyword_validation_in_grammar"
}

// -- Zero-Entity Bootstrap ---------------------------------------------------

behavior two_phase_parse_structural "Two-Phase Parse: Structural" {
  features   [zero_entity_bootstrap]
  invariants [
    registry_population_before_validation,
    zero_domain_knowledge_core,
    compilation_pipeline_ordering,
  ]
  category   command
  types      [SpecFile, Entity]
  produces   [structural_parse_complete]
  requires {
    spec_files_available "All .spec files discovered and readable from the configured spec_root"
  }
  ensures {
    structural_parse_produced      "SpecFile ASTs produced with generic entity blocks for every keyword name { } block"
    structural_parse_event_emitted "structural_parse_complete event emitted after all files are parsed"
    no_keyword_validation          "No keyword validation performed — all keywords accepted structurally"
  }
  contract   """
    In Phase 1 of the two-phase compilation, the parser MUST perform
    purely structural parsing of all .spec files. Every keyword name { }
    block MUST be parsed into a generic entity node regardless of whether
    the keyword is registered. No keyword validation MUST occur in Phase 1.
    The output MUST be a list of SpecFile ASTs with generic entity blocks.
  """
  verify unit "unknown keyword parsed into generic entity node"
  verify unit "no keyword validation in Phase 1"
  verify unit "all .spec files parsed before Phase 2"
  verify unit "parse errors collected without aborting"
  verify contract "Two-Phase Parse: Structural: structural parsing holds — spec_files_available, structural_parse_produced, structural_parse_event_emitted, no_keyword_validation"
}

behavior two_phase_validate_semantic "Two-Phase Validate: Semantic" {
  features   [zero_entity_validation, zero_entity_bootstrap]
  invariants [
    registry_population_before_validation,
    zero_domain_knowledge_core,
    compilation_pipeline_ordering,
  ]
  category   validation
  types      [KindRegistryEntry, FieldRegistryEntry]
  consumes   [registries_populated]
  requires {
    registries_populated "registries_populated event MUST have fired, confirming KindRegistry, FieldRegistry, and edge type set are fully populated"
  }
  ensures {
    all_blocks_checked         "Every parsed entity block checked against the KindRegistry"
    unknown_keywords_diagnosed "Unknown keywords have E024 diagnostics emitted"
    fields_validated           "Known keywords have their fields validated against the FieldRegistry"
  }
  contract   """
    This behavior orchestrates the two-phase pipeline by invoking
    resolution and validation behaviors in sequence. It does not
    duplicate their logic.
    Phase 2 of the two-phase compilation includes resolution and semantic
    validation. First, the resolver MUST link references and build graph
    edges using the populated registries. Then, the validator MUST check
    all entity keywords against the populated KindRegistry. Unknown
    keywords MUST produce E024 diagnostics. Known keywords MUST have
    their fields validated against the FieldRegistry via
    detect_unknown_entity_fields. Phase 2 MUST NOT begin until all
    extension manifests are loaded and all registries are fully populated.
  """
  verify unit "known keyword passes semantic validation"
  verify unit "unknown keyword produces E024"
  verify unit "field validation uses FieldRegistry"
  verify unit "Phase 2 starts only after registries populated"
  verify contract "Two-Phase Validate: Semantic: semantic validation holds — unknown_keywords_diagnosed"
}

// The candidate vocabulary names, for each kind keyword and enhancement field
// the host could enable, the extension that declares it. The session hands the
// builtins' vocabulary to the registry build; it is generated with the
// builtins (ADR 0122) and needs no network.
// suggest_missing_extensions is invoked inline by the diagnostic pipeline when
// an E024 (unknown entity kind) is emitted — it is not event-driven. It enriches
// the diagnostic help text with extension suggestions from the candidate vocabulary.
behavior suggest_missing_extensions "Suggest Missing Extensions" {
  features   [zero_entity_validation, zero_entity_bootstrap]
  invariants [zero_domain_knowledge_core]
  category   command
  types      [KindRegistryEntry, UnknownKindError, CandidateVocabulary]
  requires {
    e024_diagnostic_emitted "An E024 (unknown entity kind) diagnostic has been emitted and requires help text enrichment"
  }
  ensures {
    suggestion_provided "Help text includes extension suggestion from the candidate vocabulary or fallback to specforge search"
    no_network          "The vocabulary is data the host embeds, never fetched"
  }
  contract   """
    When an E024 (unknown entity kind) diagnostic is emitted, the help
    text MUST suggest which extension provides the unknown keyword, if known.
    The suggestion MUST use the candidate vocabulary the registry build is
    handed (not compiler source or data). The vocabulary maps keywords to
    their providing extensions. If no mapping exists for the unknown keyword,
    the help text MUST suggest running specforge search. A build handed no
    vocabulary suggests specforge search. If the vocabulary's text is
    malformed, the behavior MUST silently fall back to suggesting specforge
    search for all unknown keywords. The vocabulary changes only
    suggestions, never which diagnostics are reported.
  """
  verify unit "E024 for keyword in index suggests the providing extension"
  verify unit "E024 for keyword not in index suggests specforge search"
  verify unit "the candidate vocabulary changes only suggestions, never which diagnostics a check reports"
  verify unit "the builtin vocabulary is what the builtins' pinned declarations declare"
  verify contract "Suggest Missing Extensions: missing extension suggestions holds — e024_diagnostic_emitted, suggestion_provided, no_network"
}

// detect_unknown_entity_kinds and suggest_missing_extensions live in the
// registries file because they query the KindRegistry directly to determine
// whether a keyword is registered. They are validation behaviors that depend
// on registry state rather than on the validation rule engine.
behavior detect_unknown_entity_kinds "Detect Unknown Entity Kinds" {
  features   [zero_entity_validation, zero_entity_bootstrap]
  invariants [zero_domain_knowledge_core, registry_population_before_validation]
  category   validation
  types      [KindRegistryEntry, UnknownKindError]
  consumes   [registries_populated]
  requires {
    registries_populated_fired "registries_populated event has fired, confirming KindRegistry is fully populated"
    structural_parse_ready     "All .spec files have been structurally parsed into generic entity blocks"
  }
  ensures {
    unknown_kinds_diagnosed   "E024 diagnostic emitted for every keyword not present in the KindRegistry"
    registered_kinds_accepted "Registered keywords pass without E024 diagnostics"
  }
  contract   """
    After Phase 1 parsing and registry population, the compiler MUST scan
    all parsed entity blocks and check each keyword against the KindRegistry.
    Every keyword not present in the registry MUST produce an E024 diagnostic
    with the entity's source span. The diagnostic MUST include the unrecognized
    keyword and the file location.
  """
  verify unit "unregistered keyword produces E024"
  verify unit "E024 includes keyword name and source span"
  verify unit "registered keyword does not produce E024"
  verify unit "define-block keywords not checked against KindRegistry"
  verify contract "Detect Unknown Entity Kinds: unknown entity kind detection holds — registries_populated_fired, structural_parse_ready, unknown_kinds_diagnosed, registered_kinds_accepted"
}

behavior graceful_degradation_without_extensions "Graceful Degradation Without Extensions" {
  features   [zero_entity_bootstrap]
  invariants [zero_domain_knowledge_core]
  category   command
  types      [SpecFile, KindRegistryEntry]
  consumes   [registries_populated]
  requires {
    registries_populated_fired "registries_populated event has fired (with empty registries when no extensions are installed)"
  }
  ensures {
    i002_emitted                "I002 info diagnostic emitted indicating no extensions are configured"
    structural_mode_operational "Compiler operates in structural-only mode with generic entity nodes"
    valid_export_produced       "specforge export produces valid JSON from structural-only graph"
  }
  contract   """
    When no extensions are installed, the compiler MUST still function. It
    MUST emit an I002 info diagnostic indicating that no extensions are
    configured. It MUST parse all .spec files structurally. The graph MUST
    be built with generic entity nodes but no kind-specific validation.
    The LSP MUST provide basic features (syntax highlighting, folding)
    without extension-driven completions.

    Reference edges are structural — they are implicit edges created by the
    parser when one entity's reference list mentions another entity's ID.
    These edges do not require registered edge types in the EdgeRegistry.
    The EdgeRegistry governs extension-defined semantic edge types only.

    In structural-only mode, the kind field of exported entity nodes MUST
    contain the raw keyword string as parsed from the .spec file.
  """
  verify unit "no extensions installed emits I002 info"
  verify unit "structural parsing works without extensions"
  verify unit "graph built with generic nodes"
  verify unit "LSP provides basic features without extensions"
  verify unit "specforge export produces valid JSON from structural-only graph"
  verify unit "generic entity nodes appear as nodes in exported graph"
  verify unit "references between generic entities produce edges"
  verify integration "specforge check with zero extensions exits cleanly with I002"
  verify contract "Graceful Degradation Without Extensions: graceful degradation holds — registries_populated_fired, i002_emitted, structural_mode_operational, valid_export_produced"
}

behavior handle_all_extensions_failed_to_load "Handle All Extensions Failed to Load" {
  features   [zero_entity_bootstrap]
  invariants [zero_domain_knowledge_core, multi_error_collection]
  category   command
  types      [ExtensionError, Diagnostic]
  consumes   [extension_manifests_loaded]
  requires {
    extension_manifests_loaded_fired "extension_manifests_loaded event has fired, confirming load attempts completed (even if all failed)"
  }
  ensures {
    per_extension_errors_emitted "E-level diagnostic emitted for each failed extension"
    structural_mode_fallback     "System transitions to structural-only mode after all failures"
    no_crash_guaranteed          "System does not crash or enter an undefined state"
  }
  contract   """
    If all declared extensions in specforge.json fail to load (network errors,
    missing Wasm binaries, invalid manifests), the compiler MUST emit an E-level
    diagnostic for each failed extension, then transition to structural-only mode
    (identical to graceful_degradation_without_extensions). The system MUST NOT
    crash or enter an undefined state. An I002 info diagnostic MUST indicate
    the system is operating in structural-only mode due to load failures.
  """
  verify unit "all extensions failing produces per-extension E-level diagnostics"
  verify unit "system transitions to structural-only mode after all failures"
  verify integration "specforge check with all extensions unavailable exits cleanly"
  verify contract "Handle All Extensions Failed to Load: all-extensions-failed handling holds — extension_manifests_loaded_fired, per_extension_errors_emitted, structural_mode_fallback, no_crash_guaranteed"
}

// -- Extension-Driven Visualization ------------------------------------------
// These visualization behaviors are synchronous delegates called by
// serialize_dot_visualization (behaviors/output.spec) — they do not consume
// events or own ports directly.

behavior render_extension_defined_dot_shapes "Render Extension-Defined DOT Shapes" {
  features   [extension_driven_visualization]
  invariants [zero_domain_knowledge_core]
  category   query
  types      [KindRegistryEntry]
  requires {
    kind_registry_available "KindRegistry is populated and queryable for dot_shape, dot_color, dot_fillcolor metadata"
    graph_available         "Compiled graph with entity nodes is available for rendering"
  }
  ensures {
    shapes_applied "Every entity node rendered with its extension-defined DOT shape or default 'box'"
    colors_applied "dot_color and dot_fillcolor attributes set on nodes when specified in manifest"
  }
  contract   """
    When rendering DOT visualization, the graph renderer MUST query the
    KindRegistry for each entity's dot_shape field. Entity nodes MUST use
    the extension-defined DOT shape (box, ellipse, diamond, hexagon, etc.).
    If no dot_shape is specified in the manifest, the default shape MUST
    be "box". The shape MUST appear in the DOT node attribute list.
    When dot_color is specified in the manifest, the renderer MUST set
    the DOT node color attribute. When dot_fillcolor is specified, the
    renderer MUST set the DOT node fillcolor attribute AND add
    style=filled to the node attributes.
  """
  verify unit "entity uses extension-defined dot_shape"
  verify unit "default shape is box when dot_shape not specified"
  verify unit "dot_shape appears in DOT node attributes"
  verify unit "dot_color sets DOT node color attribute"
  verify unit "dot_fillcolor sets DOT node fillcolor and style=filled attributes"
  verify integration "multi-extension graph renders each kind with its declared shape"
  verify contract "Render Extension-Defined DOT Shapes: DOT shape rendering holds — kind_registry_available, graph_available, shapes_applied, colors_applied, be"
}

behavior render_extension_defined_edge_styles "Render Extension-Defined Edge Styles" {
  features   [extension_driven_visualization]
  invariants [zero_domain_knowledge_core]
  category   query
  types      [EdgeTypeDescriptor, EdgeRegistryEntry]
  requires {
    edge_registry_available "Edge type registry is populated and queryable for style metadata"
    graph_available         "Compiled graph with typed edges is available for rendering"
  }
  ensures {
    styles_applied      "Every edge rendered with its extension-defined style or default 'solid'"
    edge_colors_applied "edge_color and edge_arrowhead attributes set on edges when specified in manifest"
  }
  contract   """
    When rendering graph visualization, the compiler MUST query the edge
    type registry for style metadata on each edge. Edge styles MUST be
    one of: solid, dashed, or dotted. If no edge_style is defined for an
    edge type in the extension manifest, the compiler MUST default to a
    solid line. The style MUST appear in the DOT edge attribute list as
    the style property. When edge_color is specified, the renderer MUST
    set the DOT edge color attribute (default: "black"). When
    edge_arrowhead is specified, the renderer MUST set the DOT edge
    arrowhead attribute (default: "normal").
  """
  verify unit "edge uses extension-defined edge_style"
  verify unit "default style is solid when edge_style not specified"
  verify unit "edge_style appears in DOT edge attributes"
  verify unit "edge_color sets DOT edge color attribute"
  verify unit "edge_arrowhead sets DOT edge arrowhead attribute"
  verify integration "multi-extension graph renders each edge type with its declared style"
  verify contract "Render Extension-Defined Edge Styles: DOT edge style rendering holds — edge_registry_available, graph_available, styles_applied, edge_colors_applied"
}

// -- Registry build outcomes -------------------------------------------------
// The registry build (build_registries_from_declarations) turns the loaded
// declarations, in load order, into the kind, field and edge registries and
// the rules. Each behavior below is one outcome of that build that a test can
// observe: it gives the build declarations and reads the RegistryBuild, never a
// step.

behavior registry_build_kinds "Registry Build Registers Kinds" {
  features   [dynamic_entity_registration, entity_kind_conflict_prevention, extension_manifest]
  invariants [
    zero_domain_knowledge_core,
    registry_population_before_validation,
    testable_entity_classification,
  ]
  category   command
  types      [ExtensionDeclaration, EntityKindDescriptor, KindRegistryEntry, RegistryBuild]
  requires {
    declarations_in_load_order "The registry build reads the loaded declarations in load order: it puts them there first, dependencies first (registry_build_load_order)"
  }
  ensures {
    kinds_registered "Every declared kind is in the build's KindRegistry under its keyword, embedding its descriptor and naming the extension that declared it"
    first_wins       "A keyword two extensions declare belongs to the first in load order; the later declaration is E026"
    flags_consistent "A testable kind that cannot declare obligations is W017; testability and verify kinds come only from declarations"
  }
  contract   """
    The registry build MUST register every kind each declaration declares,
    under its keyword (else its name), embedding the declared descriptor
    whole (testable, singleton, supports_verify, verify kinds, semantic
    token, LSP icon, DOT shape, description) and naming the declaring
    extension. No kind is testable, and no verify kind is allowed, unless a
    declaration says so: the host knows no kind and no verify kind. A
    kind's lifecycle_field MUST name one of its own fields or one of its
    extension's shared fields; any other one is refused with a W021
    warning and not recorded. When a later declaration declares a keyword
    again, the first in load order keeps it and the later one is E026
    naming both extensions. A kind declared testable that does not support
    verify statements is W017 (advisory: the kind registers); one that
    supports verify without being testable (a formal property) is not
    reported. Load order is the build's own (registry_build_load_order):
    an extension after the peers it declares, otherwise in entry order.
  """
  verify unit "every declared kind is registered under its keyword, naming the extension that declared it"
  verify unit "a kind's declared flags and presentation reach its registry entry"
  verify unit "a kind is testable only when its declaration says so"
  verify unit "a kind's verify kinds are exactly the ones it declares"
  verify unit "a kind's lifecycle_field must name a field it declares"
  verify unit "a keyword a later extension declares again is E026 and the first in load order keeps it"
  verify unit "a testable kind without verify support is W017"
  verify unit "a kind that accepts verify statements but is not testable produces no diagnostic"
  verify contract "Registry Build Registers Kinds: kind registration holds — declarations_in_load_order, kinds_registered, first_wins, flags_consistent"
}

behavior registry_build_fields "Registry Build Registers Fields" {
  features   [dynamic_entity_registration]
  invariants [zero_domain_knowledge_core, registry_population_before_validation]
  category   command
  types      [ExtensionDeclaration, FieldDescriptor, FieldRegistryEntry, FieldType, RegistryBuild]
  requires {
    declarations_in_load_order "The registry build reads the loaded declarations in load order: it puts them there first, dependencies first (registry_build_load_order)"
  }
  ensures {
    fields_registered "Every kind's declared fields, and its extension's shared fields, are in the build's FieldRegistry for that kind, embedding their descriptor"
    types_parsed      "Each registered field has one of the protocol's field types; a field of any other type is W019 and not registered"
    roles_checked     "A field's proof_role is bound or claim; any other value is W021 and the field has no role"
    enums_checked     "An enum field declares its values, and a declared default is one of them; otherwise W021, and the field is still registered"
  }
  contract   """
    The registry build MUST register, for every kind a declaration
    declares, the kind's own fields and the declaring extension's shared
    fields, each embedding its declared descriptor whole (normative flag,
    default value, enum values, description, required, edge, target kind)
    and naming the declaring extension. A field type is one of string,
    integer, bool, enum, identifier, string_list, reference, reference_list and block,
    each also accepted with a `_type` suffix (and bool as boolean); any other type MUST produce a
    W019 warning and the field is not registered. A field's normative flag
    MUST be kept so exports tell what an entity promises from prose without
    core knowing any field's name. A field's proof_role (bound or claim)
    MUST reach its entry so the prove pass reads declared roles and no
    field name; any other role value MUST be refused with W021 and the
    field registered without a role. An enum field MUST declare its values,
    and a default_value it declares MUST be one of them; otherwise W021, and
    the field is registered as declared. `verify` is not a field type and the
    entity title is not a field: both are grammar.
    A field's registry entry is built from its descriptor alone and only
    through that build's one constructor: its type, enum values and proof
    role are read from the descriptor, and the descriptor it embeds names
    the type canonically.
  """
  verify unit "every kind's declared fields and its extension's shared fields are registered for that kind"
  verify unit "each field type and its _type alias register as that type"
  verify unit "a field of a type the protocol does not define is W019 and not registered"
  verify unit "a field's normative flag reaches its registry entry"
  verify unit "a field's proof_role reaches the field registry"
  verify unit "a proof_role other than bound or claim is refused"
  verify unit "a field's declared descriptor reaches its registry entry whole"
  verify unit "a registry entry is built only from a descriptor whose type the host reads"
  verify unit "a registry entry names its type canonically, whatever spelling was declared"
  verify unit "an enum field without values, or whose default_value is not one of its values, is W021 and still registered"
  verify unit "a field declared with a vocabulary registers as an enum of its values, its absent value as its default"
  verify contract "Registry Build Registers Fields: field registration holds — declarations_in_load_order, fields_registered, types_parsed, roles_checked, enums_checked"
}

behavior registry_build_edges "Registry Build Registers Edge Types" {
  features   [dynamic_entity_registration]
  invariants [zero_domain_knowledge_core, registry_population_before_validation]
  category   command
  types      [
    ExtensionDeclaration,
    EdgeTypeDescriptor,
    FieldDescriptor,
    EdgeRegistryEntry,
    RegistryBuild,
  ]
  requires {
    declarations_in_load_order "The registry build reads the loaded declarations in load order: it puts them there first, dependencies first (registry_build_load_order)"
  }
  ensures {
    edge_types_registered "Every declared edge type, and every edge label a field maps to without declaring it, is in the build's EdgeRegistry"
    every_writer_named    "Every edge type names every field that writes it, in load order: a kind's own, shared and enhancement fields; its ends are its declared ones, else its writers' kinds and target kinds"
    one_type_per_field    "Each field writes at most one edge type, which the build answers for that field on its kind"
    first_wins            "An edge label two extensions declare belongs to the first in load order; the later declaration is W018 and its constraints are discarded"
  }
  contract   """
    The registry build MUST register every edge type a declaration
    declares, embedding its descriptor (label, description, source and
    target kind, style) and naming the declaring extension. Once every
    kind, field and enhancement is registered, it MUST resolve each edge
    type once: every registered field that names it as its edge (a kind's
    own fields, its extension's shared fields, every enhancement's fields)
    is one of its writers, ordered by extension in load order, then kind
    and field. A field that writes a declared edge type from a kind other
    than its declared source_kind, or to a target kind other than its
    declared target_kind, MUST produce W021 and writes no edge of that
    type. An edge label no extension declares is an edge type of its
    writers, owned by the first writer's extension. Its ends are the
    declared kind on a side it declares, else its writers' kinds (source)
    and declared target kinds (target), sorted, each once. The build MUST
    answer which edge type a field of a kind writes; the rule set, the
    entity snapshot, the schema and trace read these answers and derive
    none. An edge label declared again by a later extension MUST produce a
    W018 warning naming both extensions; the first in load order keeps the
    label and its constraints, which graph validation uses.
  """
  verify unit "edge type registered with label and description"
  verify unit "source/target kind constraints recorded"
  verify unit "a field's edge label no edge type declares is registered as an edge from the field's kind to its target kind"
  verify unit "an edge label several kinds write without declaring it names every writer, in load order"
  verify unit "an edge type names every field that writes it: a kind's own, shared and enhancement fields alike"
  verify unit "a field that writes a declared edge type from a kind outside its declared source kind, or to a kind outside its declared target kind, is W021 and writes no edge of that type"
  verify unit "an edge label a later extension declares again is W018 and the first in load order keeps it"
  verify contract "Registry Build Registers Edge Types: edge type registration holds — declarations_in_load_order, edge_types_registered, every_writer_named, one_type_per_field, first_wins"
}

behavior registry_build_graph_inputs "Registry Build Derives the Graph Inputs" {
  features   [dynamic_entity_registration]
  invariants [zero_domain_knowledge_core, registry_population_before_validation]
  category   command
  types      [ExtensionDeclaration, FieldRegistryEntry, RegistryBuild, GraphInputs]
  requires {
    registries_populated "The build's kind and field registries hold every loaded declaration (registry_build_kinds, registry_build_fields)"
  }
  ensures {
    every_input_derived      "Every input the graph build reads of the extensions comes from the build: single reference fields, the owners of absent target kinds, value coercions and derived references"
    registries_agree         "Each input says what the build's declarations and field registry say, and nothing else"
    deterministic            "The same declarations give the same inputs, whatever order the registries hold them in"
    graph_built_in_the_build "Every graph read with the build is built in it"
  }
  contract   """
    The registry build MUST derive every input the graph build reads of the
    loaded extensions, and no caller derives one: every registered field of the
    reference type as a single reference field, none while no kind is
    registered; the extension that owns each reference field's target kind no loaded
    extension declares, where the candidate vocabulary names one; how each registered field's value is coerced to
    its declared type (string_list and reference_list to a list, integer,
    bool, string and enum to that type, a single reference and a block read
    as written); and every reference field whose edges the host derives
    (derived_from naming a known source, with a target kind), sorted by kind
    and field. The registry build MUST build every graph that is read with it,
    from its own inputs and the provider schemes it is handed (ADR 0123); no
    caller builds such a graph with inputs of its own.
  """
  verify unit "the registry build derives every graph input from the declarations"
  verify unit "a field's value coercion follows its registered type"
  verify unit "a derived reference field is a graph input only with a target kind and a known source"
  verify unit "a reference field's form says whether its target kind is loaded"
  verify unit "the graph inputs name the extension that owns each absent target kind"
  verify unit "a graph built in the registry build reads its own graph inputs and the provider schemes it is handed"
  verify contract "Registry Build Derives the Graph Inputs: graph inputs hold — registries_populated, every_input_derived, registries_agree, deterministic, graph_built_in_the_build"
}

behavior registry_build_rules "Registry Build Collects Rules" {
  features   [declarative_validation_rules]
  invariants [
    zero_domain_knowledge_core,
    declarative_validation_determinism,
    registry_population_before_validation,
  ]
  category   command
  types      [ExtensionDeclaration, ValidationRulePattern, RegistryBuild]
  requires {
    declarations_in_load_order "The registry build reads the loaded declarations in load order: it puts them there first, dependencies first (registry_build_load_order)"
  }
  ensures {
    rules_collected        "Every declared rule that parses is in the build's rules with the extension that declared it, ordered by code"
    duplicates_warned      "A rule code two extensions declare is W023, and both rules are kept"
    required_enforced      "Every field registered as required has a host-generated E006 rule"
    files_enforced         "Every field registered as a file reference has a host-generated E016 rule"
    unloaded_targets_inert "A rule whose target kind or edge type no loaded extension declares is not registered, so it reports nothing"
    rule_codes_checked     "A rule whose code its extension may not use is reported (W150) and still registered"
  }
  contract   """
    The registry build MUST collect every extension's validation rules
    into one rule set, each rule with the extension that declared it
    (custom rules are dispatched to it), ordered by code; rules sharing a
    code keep load order. A rule that does not parse is W112
    (parse_validation_rule_pattern). A code two extensions declare MUST
    produce a W023 warning naming the code and both extensions; both rules
    are kept. A rule's target kind and edge type are resolved against the
    loaded registries: a rule whose target kind or edge type no loaded
    extension declares is not registered, so it reports nothing when rules
    run (and the entities written with that keyword are E024), since it
    belongs to an optional peer that is not installed (an edge rule whose
    edge type's far-end kind no extension declares is dropped too). A
    structural kind (spec, ref) is always there. An edge type is
    resolved through the edge registry only, never read as a field name. A
    rule naming a target kind or an edge type that neither its extension,
    its loaded peers nor its target_extension declare is the extension
    author's mistake, reported as W021 when the extensions load, as is a field or edge type that references an
    undeclared kind (registry_build_declaration_consistency). After the
    extensions' rules,
    the rule set MUST contain a host-generated E006 rule, owned by no
    extension, for every field registered as required, then an E016
    file_exists rule for every field registered as a file reference.

    Each rule's code is checked against the diagnostic catalog when it
    is registered: an extension's rule uses its own catalogued code at
    the catalogued level, or a code in E900-E998, W900-W998 or I900-I998
    whose prefix states the rule's severity. Any other is reported as
    W150 naming the extension, the code and why, once per code and
    severity; the rule is still registered and runs with the code it
    declares.
  """
  verify unit "every declared rule is in the build's rules with the extension that declared it"
  verify unit "the extensions' rules are ordered by code"
  verify unit "a rule code two extensions declare is W023 and both rules are kept"
  verify unit "the build drops a rule whose target kind or edge type no loaded extension declares"
  verify unit "a rule whose edge type no loaded extension declares reports nothing"
  verify unit "a rule targeting a kind no loaded extension declares reports nothing"
  verify unit "extensions produce E006 rules for required fields"
  verify unit "E006 covers all required fields from builtin extensions"
  verify unit "extensions produce E016 rules for file-reference fields"
  verify unit "a rule whose code the extension may not use is reported (W150) and still registered"
  verify unit "every builtin rule uses a code its extension owns, at the catalogued level"
  verify contract "Registry Build Collects Rules: rule collection holds — declarations_in_load_order, rules_collected, duplicates_warned, required_enforced, files_enforced, unloaded_targets_inert, rule_codes_checked"
}

behavior registry_build_declaration_consistency "Registry Build Checks Declaration Consistency" {
  features   [extension_manifest, dynamic_entity_registration]
  invariants [zero_domain_knowledge_core, registry_population_before_validation]
  category   validation
  types      [
    ExtensionDeclaration,
    EntityKindDescriptor,
    EdgeTypeDescriptor,
    FieldDescriptor,
    RegistryBuild,
  ]
  requires {
    declarations_in_load_order "The registry build reads the loaded declarations in load order: it puts them there first, dependencies first (registry_build_load_order)"
    peers_loaded               "A declaration's peers are among the loaded declarations, or their kinds are unknown"
  }
  ensures {
    references_resolved        "Every field target_kind names a kind the declaration or a loaded peer declares, and every edge label its kinds' own, shared and enhancement fields name an edge type the declaration declares"
    authoring_errors_diagnosed "Every other reference, and a derived_from the host can't apply, is a W021 warning naming it"
    compile_not_failed         "W021 never fails the compile: the declaration's kinds still register"
  }
  contract   """
    The registry build MUST check each declaration against the loaded
    declarations, by structure alone (no domain-specific logic), before it
    registers anything. A field's target_kind MUST name a kind the
    declaration or one of its loaded peers declares; a kind only a
    non-peer extension declares is W021 too, naming that extension (the
    kind exists but the dependency is undeclared). While a named peer is
    not loaded its kinds are unknown, so any target is let through. A
    edge label of every field the declaration contributes (its kinds' own
    fields, its shared fields, its enhancements' fields) MUST name an edge
    type the declaration declares. A
    validation rule's target_kind and edge_type MUST name a kind or edge
    type the declaration, one of its loaded peers or the rule's
    target_extension declares: with no target_extension, anything goes
    while a named peer is not loaded and a kind only a non-peer declares is
    W021 suggesting target_extension; a target_extension that is not
    loaded makes that rule alone inert (no W021); one that is loaded
    without the kind or edge type is W021. A
    field's derived_from MUST name type_expressions or method_signatures
    and sit on a reference or reference_list field with a target_kind.
    Every violation is a W021 warning among the build's declaration
    diagnostics, after E030 and before E027. These are authoring errors
    in the extension, not in the user's spec: they never fail the compile,
    and the declaration's kinds still register.
  """
  verify unit "a target_kind the extension or a loaded peer declares passes"
  verify unit "an edge label the extension declares an edge type for passes"
  verify unit "a target_kind no loaded extension declares is a W021 warning"
  verify unit "a target_kind only a non-peer extension declares is a W021 warning naming that extension"
  verify unit "an edge label the extension declares no edge type for is a W021 warning"
  verify unit "a derived_from the host can't apply produces a W021 warning"
  verify unit "a rule's edge type that neither its extension nor its peers declare produces W021"
  verify unit "a rule's target kind that neither its extension nor its peers declare produces W021"
  verify unit "a rule's target_extension, loaded, must declare its target kind and edge type; not loaded, the rule is inert and costs no W021"
  verify unit "cross-validation uses no domain-specific logic"
  verify integration "a declaration's W021 does not fail the compile, and its kinds still register"
  verify contract "Registry Build Checks Declaration Consistency: declaration consistency holds — declarations_in_load_order, peers_loaded, references_resolved, authoring_errors_diagnosed, compile_not_failed"
}

behavior registry_build_peer_dependencies "Registry Build Checks Peer Dependencies" {
  features   [wasm_extension_runtime]
  invariants [
    zero_domain_knowledge_core,
    registry_population_before_validation,
    peer_dependency_satisfaction,
  ]
  category   validation
  types      [ExtensionDeclaration, PeerDependency, RegistryBuild]
  produces   [extension_loading_failed]
  requires {
    declarations_in_load_order "The registry build reads the loaded declarations in load order: it puts them there first, dependencies first (registry_build_load_order)"
  }
  ensures {
    peers_checked      "Every declared peer is judged by the one peer rule (ADR 0041) against the loaded declarations' versions"
    unsatisfied_failed "A required peer missing, or any peer loaded at a version its range does not accept (a version that is not SemVer included), is E027, which fails the check"
    unreadable_failed  "A peer range that is not a SemVer requirement is E073, whether the peer is loaded, missing or optional, which fails the check"
  }
  contract   """
    The registry build MUST judge every declaration's peer dependencies by
    the one peer rule (specforge_package::peers, ADR 0041) against
    the loaded declarations. The range is read first, as Cargo reads a
    SemVer requirement (caret, tilde, comparisons, wildcards, and a bare
    version as a caret): a range that is not one is E073, whether the peer
    is loaded, missing or optional, since no version can satisfy it. A
    required peer MUST be loaded; an optional one may be absent. A loaded
    peer MUST be at a version its range accepts: one it does not accept,
    or a version that is not SemVer (which no range accepts), is E027
    naming the extension, the peer, its range and the installed version.
    Both are errors that fail the check. The extension's kinds are still
    registered, so its entities are checked rather than each reported as
    an unknown kind (E024).
  """
  verify unit "satisfied peer dependency passes validation"
  verify unit "missing peer dependency produces hard error"
  verify unit "incompatible version produces hard error with required range"
  verify unit "missing optional peer dependency passes validation"
  verify unit "installed optional peer outside its range produces hard error"
  verify unit "a peer range matches as semver: caret, tilde or exact"
  verify unit "a peer range that is not SemVer is E073, whether its peer is loaded, missing or optional"
  verify unit "a peer loaded at a version that is not SemVer is E027"
  verify unit "an extension with an unsatisfied peer still registers its kinds"
  verify integration "specforge check reports a missing required peer dependency"
  verify contract "Registry Build Checks Peer Dependencies: peer dependency checking holds — declarations_in_load_order, peers_checked, unsatisfied_failed, unreadable_failed"
}

behavior registry_build_load_order "Registry Build Orders the Extensions" {
  features   [wasm_extension_runtime]
  invariants [extension_load_order_determinism]
  category   command
  types      [ExtensionDeclaration, PeerDependency, RegistryBuild]
  requires {
    declarations_in_entry_order "The loaded declarations are given in the order specforge.json's extensions entries name them"
  }
  ensures {
    dependencies_first "Each declaration comes after the peers it declares: every required peer, and every optional one unless its edge would close a cycle"
    entry_order_kept   "Otherwise the declarations keep the order they were given in"
    deterministic      "The same declarations in the same order give the same load order, and a load order given again comes back unchanged"
    cycles_failed      "A cycle among required peers is one E027 naming its extensions, which still load, together and in the order they were given"
  }
  contract   """
    The registry build MUST put the loaded declarations in load order
    before it reads them, and every first-wins rule (E026 kinds, W018 edge
    labels, enhancement fields, W023 rule codes, surfaces and short names)
    and every in-order list (passes, rules, the extensions a schema names,
    migration hooks) MUST follow it. Load order is entry order, except that
    an extension comes after the peers it declares: every required one,
    and an optional one unless that would close a cycle, the optional
    edges taken in dependent-then-peer name order after the required ones.
    A cycle among required peers MUST be reported once as E027 naming its
    extensions; they still load, together, in entry order, after their
    other peers. The order MUST be deterministic and a load order given
    again MUST come back unchanged (ADR 0041).
  """
  verify unit "a dependent listed before its peer loads after it"
  verify unit "extensions with no peer between them keep the order they were given in"
  verify unit "extensions naming each other as optional peers load without a cycle"
  verify unit "a load order given again comes back unchanged"
  verify unit "E026, W018, passes and surfaces follow the load order"
  verify unit "a cycle among required peers is one E027 naming its extensions"
  verify integration "specforge check reports a cycle among required peers"
  verify contract "Registry Build Orders the Extensions: load order holds — declarations_in_entry_order, dependencies_first, entry_order_kept, deterministic, cycles_failed"
}
