// Zero-entity core — declarative validation engine and field validation

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

// -- Declarative Validation --------------------------------------------------

// No consumes — synchronous helper the registry build runs (registry_build_rules)
behavior parse_validation_rule_pattern "Parse Validation Rule Pattern" {
  features   [declarative_validation_rules]
  invariants [zero_domain_knowledge_core, declarative_validation_determinism]
  category   command
  types      [ValidationRulePattern, ValidationPatternKind, FieldConstraint]
  requires {
    manifest_rules_available "Extension manifest's validationRules array is accessible as structured data"
  }
  ensures {
    patterns_parsed     "Each validationRules entry parsed into a well-formed ValidationRulePattern"
    unrecognized_warned "Unrecognized pattern kinds produce warning diagnostics with extension name"
    ignored_warned      "Properties a check does not read produce W147; the rule is registered without them"
  }
  contract   """
    When the registry build reads an extension declaration's validation
    rules, it MUST turn each into a typed rule whose check carries exactly
    what that check reads. The check MUST be one of the extension
    vocabulary's kinds: no_incoming_edges, no_outgoing_edges, no_edges,
    missing_field_when_flag_set, missing_required_field,
    conditional_field_required, field_value_constraint, cycle_detection,
    file_exists, verify_kind_allowlist, no_verify_statements, custom.
    A rule that cannot work as declared — an unrecognized check, a field,
    constraint, edge type or wasm_function its check requires and lacks, an
    empty values list, a regex that does not compile — or a rule that reads
    verify statements on a declared kind that accepts none — MUST produce
    W112 with the extension name and MUST NOT be registered. A property its
    check does not read (an edge_type on a field check, a constraint on an
    edge check, a wasm_function on a declarative check, a constraint kind
    or a pattern or values its check does not read) MUST produce W147, and
    the rule MUST be registered without it.
  """
  verify unit "parses no_incoming_edges pattern from manifest"
  verify unit "parses missing_field_when_flag_set pattern from manifest"
  verify unit "unrecognized pattern kind produces warning"
  verify unit "all required fields validated on each rule"
  verify unit "parses field_value_constraint pattern from manifest"
  verify unit "a cycle_detection rule without an edge_type produces W112 and is not registered"
  verify unit "a verify_kind_allowlist rule without values produces W112 and is not registered"
  verify unit "a rule that reads verify statements on a kind that accepts none produces W112 and is not registered"
  verify unit "a property its check does not read produces W147 and the rule is registered without it"
  verify unit "a conditional_field_required constraint of another kind produces W147 and is read as when_field_equals"
  verify contract "Parse Validation Rule Pattern: validation rule parsing holds — manifest_rules_available, patterns_parsed, unrecognized_warned, ignored_warned"
}

behavior execute_validation_pattern "Execute Validation Pattern" {
  features   [declarative_validation_rules]
  invariants [zero_domain_knowledge_core, declarative_validation_determinism]
  category   command
  types      [ValidationRulePattern, ValidationPatternKind, Diagnostic]
  ports      [WasmRuntime]
  consumes   [graph_built]
  produces   [declarative_validation_executed]
  requires {
    patterns_parsed "ValidationRulePattern is parsed and well-formed"
    graph_available "Compiled graph is available for querying"
  }
  ensures {
    all_entities_matched "Pattern matched against all applicable entities in the graph"
    violations_diagnosed "Diagnostics emitted for every pattern violation"
  }
  maintains {
    deterministic_order "Patterns executed in code-sorted order producing identical diagnostics across runs"
  }
  contract   """
    The declarative validation engine MUST execute each registered pattern
    against the compiled graph. no_incoming_edges MUST check that every
    entity of the target kind has at least one incoming edge. no_outgoing_edges
    MUST check outgoing edges. An edge rule with an edge type counts only
    the edges of that type: an edge is of the type its field writes from
    the kind of the entity it starts at (registry_build_edges). The rule is
    inert when no loaded field can write such an edge at its target kind.
    missing_field_when_flag_set MUST check that
    entities whose kind has the specified flag set to true have the specified field. field_value_constraint MUST
    check that a named field on entities of the target kind satisfies a
    value predicate (non-empty, matches regex, or is one of an allowed set).
    cycle_detection MUST report each entity of the target kind (every
    entity when no target kind is set) that sits on a cycle of the edges
    of its edge type, whichever fields write them.
    file_exists MUST check that file-reference fields point to existing
    files, a relative path resolved against the spec root (never the
    working directory). A list field's items are each a path. A rule without a target kind applies to entities
    of every kind. custom MUST dispatch to the Wasm function registered by
    register_custom_validation_patterns. Each pattern violation MUST
    produce a diagnostic with the configured code and severity.
  """
  verify unit "no_incoming_edges detects unreferenced entities"
  verify unit "no_outgoing_edges detects entities with zero outgoing edges"
  verify unit "an edge rule counts only edges of its edge type and is dropped when no extension declares the kind at its far end"
  verify unit "an edge rule over an edge type several kinds write without declaring it counts the edges of every writing kind"
  verify unit "an edge rule does not count an edge of another edge type, whatever kinds it links"
  verify unit "missing_field_when_flag_set detects missing specified field on flagged entity"
  verify unit "field_value_constraint rejects invalid field value"
  verify unit "cycle_detection finds cycles in edge type"
  verify unit "a cycle_detection rule without a target_kind reports every entity on a cycle of its edge type"
  verify unit "cycle_detection follows every field that writes its edge type"
  verify unit "a cycle_detection rule follows only the edges of its edge type, never another kind's field of the same name"
  verify unit "an edge rule no loaded field can satisfy at its target kind is inert"
  verify unit "the builtin extensions' rules register with no W112, W147 or W021"
  verify unit "file_exists reports missing file-reference field targets"
  verify unit "file_exists resolves a relative path against the spec root, never the working directory"
  verify unit "file_exists checks each item of a list field as its own path"
  verify unit "custom pattern dispatches to registered Wasm function"
  verify unit "pattern violation produces diagnostic with configured code and severity"
  verify contract "Execute Validation Pattern: declarative validation holds — all_entities_matched, violations_diagnosed, deterministic_order"
}

behavior emit_diagnostic_from_pattern "Emit Diagnostic From Pattern" {
  features   [declarative_validation_rules]
  invariants [zero_domain_knowledge_core, declarative_validation_determinism]
  category   command
  types      [ValidationRulePattern, Diagnostic]
  requires {
    violation_detected "A declarative validation pattern has detected a violation for an entity"
    pattern_configured "The pattern's messageTemplate, code, and severity fields are available"
  }
  ensures {
    diagnostic_emitted    "Diagnostic emitted with interpolated message, configured code, and correct severity"
    template_interpolated "All interpolation variables ({id}, {kind}, {field}, {value}) resolved in messageTemplate"
  }
  contract   """
    When a declarative validation pattern detects a violation, the engine
    MUST emit a diagnostic using the pattern's messageTemplate with
    interpolation variables: {id} for the entity ID, {kind} for the
    entity kind, {field} for the field name, {value} for the field value.
    The diagnostic code MUST be the pattern's code field. The severity
    MUST match the pattern's severity field (error, warning, info).
  """
  verify unit "message template interpolates {id} and {kind}"
  verify unit "message template interpolates {field} and {value}"
  verify unit "diagnostic code matches pattern code"
  verify unit "diagnostic severity matches pattern severity"
  verify contract "Emit Diagnostic From Pattern: pattern diagnostic emission holds — violation_detected, pattern_configured, diagnostic_emitted, template_interpolated"
}

behavior register_custom_validation_patterns "Register Custom Validation Patterns" {
  features   [declarative_validation_rules]
  invariants [zero_domain_knowledge_core, declarative_validation_determinism]
  category   command
  types      [ValidationRulePattern, CustomCall, ExtensionDeclaration]
  refs       [provide_host_function_query_graph]
  ports      [WasmRuntime]
  consumes   [extension_manifests_loaded]
  requires {
    extension_manifests_loaded_fired "extension_manifests_loaded event has fired, confirming all manifests are parsed and accessible"
    wasm_runtime_available           "WasmRuntime port is available for resolving and dispatching Wasm exports"
  }
  ensures {
    custom_patterns_registered "Custom validation patterns registered alongside declarative patterns"
    wasm_functions_resolved    "wasm_function fields resolved to Wasm exports or warnings emitted for unresolvable names"
  }
  contract   """
    When an extension declares validation rules with check kind "custom",
    the compiler MUST resolve the wasm_function field to a Wasm export in
    the extension's module. The custom pattern MUST be registered alongside
    declarative patterns. During validation, custom patterns MUST be
    dispatched to the Wasm runtime via the extension's exported function.
    The Wasm function receives the protocol's ValidatorContext (the entity,
    the resolution of its references, the declared types and the host's
    primitives) and answers the protocol's ValidatorVerdict (`pass`, or
    `fail` with the offending field and value), read strictly: an answer
    that is not a verdict is a failed call (call_extension_exports), never
    a pass or a fail.
    The function body MAY call the `specforge.query_graph` host function
    (see provide_host_function_query_graph) to access the compiled graph
    for cross-entity semantic checks and custom graph traversals.
    On failure, the engine MUST emit a diagnostic using the pattern's
    configured code, severity, and message template. Unresolvable
    wasm_function names MUST produce a warning (W112) at registration
    time: when the extensions load, each custom rule's wasm_function is
    called once on an entity of the rule's target kind that declares
    nothing, and a call that does not answer with a verdict is reported.
    The rule stays registered. During validation, the entities whose call
    fails are not checked; they MUST be reported once per rule per check
    as W148, naming how many failed and the first one with its error, and
    carrying every failed entity with its error as the diagnostic's data. A custom rule that names no
    wasm_function MUST produce W112 and MUST NOT be registered.
  """
  verify unit "custom pattern registered with wasm_function reference"
  verify unit "unresolvable wasm_function produces warning"
  verify unit "custom rule without a wasm_function produces warning and is not registered"
  verify unit "custom pattern dispatched to Wasm runtime during validation"
  verify unit "custom pattern failure emits configured diagnostic"
  verify unit "a custom validator's verdict is read as the protocol's ValidatorVerdict, and a failure is reported once as W112"
  verify unit "a custom rule whose function fails on entities produces one W148 per check naming how many were not checked"
  verify contract "Register Custom Validation Patterns: custom validation pattern registration holds — extension_manifests_loaded_fired, wasm_runtime_available, custom_patterns_registered, wasm_functions_resolved"
}

behavior snapshot_entities_once "Snapshot the Entities Once per Compile" {
  features   [declarative_validation_rules]
  invariants [
    zero_domain_knowledge_core,
    declarative_validation_determinism,
    testable_entity_classification,
  ]
  category   command
  types      [PassEntity, ValidatorContext, Diagnostic]
  consumes   [graph_built]
  requires {
    graph_and_registries "the graph is built and the registry build it was built with is available"
  }
  ensures {
    one_text_per_field "every field an entity writes has one text, the same for every reader"
    one_standing       "every entity has one standing: its kind testable or not, owing obligations or not, counting toward coverage or not"
    built_once         "the checks, the check passes and the coverage of one compile read one snapshot"
  }
  contract   """
    After the graph is built, the compiler MUST take one snapshot of its
    entities, read with the registry build. Every check after the build
    (the registry checks, the extensions' declarative and custom rules,
    the check-phase passes) and the coverage of that compile MUST read
    it. A session MUST take a new one with every update.

    Edges: every edge the snapshot holds is typed by the edge type its
    field writes from the kind it starts at, and each entity's edges are
    counted in total and by that type. An edge whose field writes no edge
    type has none and counts in the total only.

    Field text: every field an entity writes has exactly one text, which
    declarative rules match, custom validators receive as the field's
    value (always a string) and compiler passes receive in `fields`. A
    string, identifier or date is its text as written; an integer or a
    boolean its literal; a list of strings or references its items
    joined by ", "; a variant list or a type union its members joined by
    " | "; a mixed list its items' texts joined by ", "; an expression
    group its expressions joined by ", "; verify statements their texts
    joined by "; "; a block its keys joined by ", ". An empty list or
    block is written and its text is empty. No written field is left
    out and no value is null. A name written twice has the last one's
    value. Beside its text, every written field keeps its value shape
    (a quoted string, a bare word, a date, an integer, a boolean, a list
    of strings, of references, mixed or of variants, a type union, a
    block, verify statements, an expression) and the span of its value,
    for the host's own checks; neither crosses the protocol.

    Standing: an entity's kind is testable when its extension says so.
    It owes obligations of its own when a no_verify_statements rule
    applies to its kind (a rule without a target kind applies to every
    kind) and neither a union body, nor a set field whose registry entry
    exempts obligations, nor its kind accepting no verify statements
    exempts it. It counts toward coverage when its
    kind is testable and it owes obligations or declares some: the
    coverage rule's one counting rule, read with the standing's facts.
    The rules, the pass input's `owes_none` (it owes none, and may still
    count by declaring some), stats and the verify-stub fix all read this
    one standing.
  """
  verify unit "every field an entity writes has one text, the same for declarative rules, custom validators and compiler passes"
  verify unit "a variant list or type union is its members joined by ' | ', a mixed list or expression group its items joined by ', '"
  verify unit "an empty list or block is written, with empty text, never left out or null"
  verify unit "every edge in the snapshot is typed by the edge type its field writes from the kind it starts at, and counted by that type"
  verify unit "an entity owes obligations when a no_verify_statements rule applies to its kind and neither a union body nor an exempting flag exempts it"
  verify unit "a rule without a target kind applies to every kind, for the rule, the standing and the verify stub alike"
  verify unit "a kind that accepts no verify statements owes no obligations, whatever rule applies to it"
  verify unit "the checks, the check passes and the coverage of one compile read one snapshot"
  verify unit "a session's snapshot follows every update"
  verify unit "every written field keeps its value shape and value span beside its text"
  verify unit "the coverage rule reads each entity as the pass input carries it"
  verify unit "the standing counts an entity toward coverage exactly as the coverage rule counts the pass entity it sends"
}

// -- Entity Checks -----------------------------------------------------------

behavior check_entities_in_one_order "Check the Entities in One Order" {
  features   [structural_validation, zero_entity_validation]
  invariants [
    diagnostic_determinism,
    validation_pipeline_ordering,
    registry_population_before_validation,
    zero_domain_knowledge_core,
  ]
  category   validation
  types      [RegistryBuild, Diagnostic]
  consumes   [registries_populated]
  requires {
    snapshot_taken "the entity snapshot of the built graph is taken with the registry build it was built with"
  }
  ensures {
    one_order      "the structural checks and the rule set run in one fixed order, the same on every surface"
    one_gate       "with no entity kind registered, the checks that read kinds, fields and identifiers do not run"
    gate_announced "extensions that load but declare no entity kind leave one W151 naming the entities nobody checks"
    one_file_set   "the files the checks read are one set: those file_reference fields name and those file_exists rules read"
  }
  contract   """
    After the entity snapshot is taken, the registry build MUST run every
    check over its records, in this order: unreferenced refs (W012),
    missing referenced files (E016), unknown kinds (E024), reserved
    entity IDs (E013), the identifier length contract (E014), unknown
    fields (W020), references to an entity of the wrong kind (E022),
    field values that cannot be their declared type (E061), then the
    rule set. Check, watch, the LSP and MCP MUST report them in this
    order, and no check reads a graph node.

    When no loaded extension declares an entity kind, the project is
    structural-only: E024, E013, E014, W020, E022 and E061 MUST NOT run,
    while W012, E016 and the rules still do. With no extension loaded,
    I002 says so. With extensions loaded that declare no kind, one
    W151 MUST name how many entities, and of which kinds, are left
    unchecked, instead of nothing.

    An entity ID equal to a structural keyword (spec, ref, use, define)
    or to a registered kind's keyword is E013. An ID shorter than 2 or
    longer than 60 characters is E014. A reference whose field declares a
    target kind, naming an existing entity of another kind, is E022.

    The files the checks read, those a file_reference field names and
    those a file_exists rule reads, resolved against the spec root, are
    the session's check inputs.
  """
  verify unit "the structural checks run in one order: W012, E016, E024, E013, E014, W020, E022, E061, then the rule set"
  verify unit "with no entity kind registered, E024, E013, E014, W020, E022 and E061 do not run, and W012, E016 and the rules still do"
  verify unit "extensions that load but declare no entity kind report one W151 naming the unchecked entities"
  verify unit "an entity ID equal to a structural keyword or a registered kind's keyword is E013"
  verify unit "an entity ID shorter than 2 or longer than 60 characters is E014"
  verify unit "a reference to an existing entity of another kind than its field's target kind is E022"
  verify unit "the files the checks read are those file_reference fields name and those file_exists rules read"
  verify integration "a compile reports the structural checks in the order the registry build runs them"
}

// -- Field Validation --------------------------------------------------------

behavior detect_unknown_entity_fields "Detect Unknown Entity Fields" {
  features   [zero_entity_validation, dynamic_entity_registration]
  invariants [zero_domain_knowledge_core, registry_population_before_validation]
  category   validation
  types      [FieldRegistryEntry, KindRegistryEntry, Diagnostic]
  consumes   [registries_populated]
  requires {
    registries_populated_fired "registries_populated event has fired, confirming FieldRegistry and KindRegistry are fully populated"
  }
  ensures {
    unknown_fields_diagnosed "W020 warning emitted for every unrecognized field name on registered entity kinds"
    cascading_avoided        "Field validation skipped for entities with unregistered kinds (already E024)"
  }
  contract   """
    During Phase 2 semantic validation, the compiler MUST scan all parsed
    entity blocks whose kind is registered in the KindRegistry and check
    each field name against the FieldRegistry for that kind. Every field
    name not present in the FieldRegistry for the entity's kind MUST
    produce a W020 warning diagnostic with the entity's source span, the
    unrecognized field name, and the entity kind name. `title` is
    structural and MUST NOT be checked against the FieldRegistry. Every
    other name, `expression` included, MUST be checked like any field: the
    prove pass reads only fields an extension declares a proof role for
    (ADR 0009), so an undeclared `expression` is W020. When a builtin
    extension's enhancement declares the field on that kind (the bundled
    field index, e.g. an invariant's `expression` from @specforge/formal),
    the W020 MUST suggest installing that extension, as E024 does for a
    kind.
    `verify` is reserved syntax whose meaning extensions supply
    (ADR 0002): it MUST be accepted only on kinds an extension made
    testable (supports_verify) and produce W020 elsewhere. When the entity's kind
    itself is unregistered (already reported as E024), field validation
    MUST be skipped for that entity to avoid cascading diagnostics.
  """
  verify unit "unregistered field name produces W020"
  verify unit "W020 includes field name, entity kind, and source span"
  verify unit "registered field name does not produce W020"
  verify unit "structural fields (title, verify) not checked against FieldRegistry"
  verify unit "verify on a kind no extension made testable produces W020"
  verify unit "expression is checked like any other field (W020 where undeclared)"
  verify unit "an undeclared field a builtin enhancement adds suggests its extension"
  verify unit "field validation skipped when entity kind is unregistered"
  verify contract "Detect Unknown Entity Fields: unknown field detection holds — registries_populated_fired, unknown_fields_diagnosed, cascading_avoided"
}

behavior check_field_value_types "Check Field Value Types" {
  features   [dynamic_entity_registration]
  invariants [zero_domain_knowledge_core, registry_population_before_validation]
  category   validation
  types      [FieldRegistryEntry, Diagnostic]
  consumes   [registries_populated]
  requires {
    registries_populated_fired "registries_populated event has fired, confirming the FieldRegistry holds each field's declared type"
  }
  ensures {
    single_values_listed "A string, bare word, integer, boolean or date on a string_list or reference_list field is stored as a one-item list, and a mixed list as a list of its items' text"
    mismatches_diagnosed "E061 error for every value whose shape the field's declared type never takes, or whose value it refuses; a value it admits is one the published schema admits"
    undeclared_untouched "Unknown fields keep their parsed value and produce no E061"
  }
  contract   """
    The grammar parses every field value generically; the value's type
    comes from the FieldRegistry, read through one field form per field
    (ADR 0064). During Phase 2 semantic validation the compiler MUST
    store each registered field's value as its declared type before
    references resolve and before anything is exported: a single string,
    integer, boolean or date on a string_list or reference_list field MUST
    become a one-item list (a bare word the one-item reference list
    `field [value]` parses to), and a list mixing strings, integers and
    booleans the list of its items' text. A quoted integer or boolean on an
    integer or bool field MUST become that integer or boolean, and an
    integer or boolean on a string or enum field MUST become its text.
    Coercion MUST NOT produce a diagnostic.

    A stored value its declared type does not take MUST produce an E061
    error at the value's source span naming the field, the declared type
    and the value given. A string field takes text, a bare word, a date, a
    type union or an expression group; an enum one of its declared values
    (suggesting the closest); an integer field an integer; a bool field
    true or false; a reference field one id; a string_list or
    reference_list field a list; a block field a block. Fields no
    extension registers keep their parsed value (W020 reports them). The
    check MUST run wherever the registry checks run: check, watch and the
    LSP. It reads each field's value shape and value span from the entity
    snapshot, never a graph node.
  """
  verify unit "a single string on a string_list field becomes a one-item list"
  verify unit "a single reference on a reference_list field becomes a one-item list"
  verify unit "a value that is not the declared integer, bool or enum type is an error"
  verify unit "a list on a field declared as a single value is an error"
  verify unit "an enum value suggests the closest declared value"
  verify unit "an export with a coerced string_list validates against the published schema"
  verify unit "a single integer, boolean or date, or a mixed list, on a list field becomes a list of strings"
  verify unit "a value of a shape its declared type never takes is an error"
  verify unit "a value the check admits is written as the published schema admits it"
  verify contract "Check Field Value Types: declared field types hold — registries_populated_fired, single_values_listed, mismatches_diagnosed, undeclared_untouched"
}
