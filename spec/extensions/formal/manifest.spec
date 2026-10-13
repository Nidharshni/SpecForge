// @specforge/formal extension manifest declaration
//
// @specforge/formal contributes 5 entity kinds (property, axiom, protocol,
// refinement, process) and 10 edge types. It enhances @specforge/software
// entity kinds via entity_enhancements and contributes 5 compiler passes,
// 3 feature flags, and 4 verify kinds. Conditions are block fields
// (requires/ensures/maintains) of named conditions, not standalone
// entities and not edges. Formal analysis warnings require warning_level=strict.

use "extensions/formal/features"
use "extensions/formal/types"
use "types/zero-entity-core"

behavior fa_declare_manifest "Declare @specforge/formal Manifest" {
  category command
  types    [
    ExtensionDeclaration,
    CompilerPassDeclaration,
    FeatureFlagDeclaration,
    FormalProperty,
    FormalAxiom,
    FormalProtocol,
    FormalRefinement,
    FormalProcess,
  ]
  contract """
    The @specforge/formal extension MUST declare itself with name
    "@specforge/formal". Its declaration MUST declare
    5 entity kinds and 10 edge types.

    Entity kind declarations (all testable=false, supports_verify=false):
    - property: temporal/behavioral assertion (safety/liveness/fairness)
      Shape: { description string, property_type PropertyType, references EntityId[] @optional }
    - axiom: assumed-true foundation (no proof required, no coverage tracking item)
      Shape: { description string, justification string @optional, references EntityId[] @optional }
    - protocol: shared synchronization contract across events
      Shape: { description string, ordering string[] @optional, timeout string @optional, delivery DeliverySemantics @optional, references EntityId[] @optional }
    - refinement: abstract->concrete behavior mapping as first-class graph node
      Shape: { description string @optional, abstract_entity EntityId, concrete_entity EntityId, chains_to EntityId @optional, invariant_deltas string[] @optional }
      (conditions/status, specified by fa_parse_refinement_entity, are not declared yet)
    - process: CSP-style communicating process with alphabet and composition
      Shape: { description string, alphabet EntityId[] @optional, states ProcessState[] @optional, composition CompositionOperator @optional, references EntityId[] @optional }

    Conditions are block fields (requires/ensures/maintains) on behaviors,
    not standalone entities and not edges:
    - Inline:    requires { name "description" }
    Each entry is a named condition (ConditionEntry); the passes read the
    conditions by name.

    Edge types (10 total; graph edges carry the declaring field's name as
    their label, e.g. a refinement's abstract_entity edge is labelled
    "abstract_entity"):
    - BehaviorSatisfiesProperty:     behavior -> property (temporal property satisfaction)
    - BehaviorRefinesBehavior:       behavior -> behavior (field-form layering: `refines`)
    - EventFollowsProtocol:          event -> protocol (sync contract reference)
    - EventParticipatesInProcess:    event -> process (event membership in process alphabet)
    - PropertyDependsOnInvariant:    property -> invariant (property-invariant dependency)
    - AxiomAssumesInvariant:         axiom -> invariant (the invariant rests on this axiom)
    - RefinementRefinesAbstract:     refinement -> behavior (the abstract side)
    - RefinementRefinesConcrete:     refinement -> behavior (the concrete side)
    - RefinementChainsToRefinement:  refinement -> refinement (multi-level refinement)
    - ProcessComposesProcess:        process -> process (parallel/sequential/choice composition)

    Entity enhancements add formal fields to @specforge/software entities:
    - behavior: requires, ensures, maintains, abstract, refines, assumes, satisfies, refinement
    - invariant: maintains, expression (a claim, below)
    - event: sync, follows_protocol, process
    - port.methods: requires, ensures

    The requires/ensures/maintains fields are block fields: each entry a
    named condition (ConditionEntry). They are not standalone graph
    entities and write no edge. The sync fields are block fields too
    (barrier, timeout, delivery), and expression is a string field, written
    as text or as an `expr { }` group.

    Proof roles (ADR 0009): a property's expression declares the claim
    role (it must follow from the declared bounds); an axiom's expression
    declares the bound role (an axiom is assumed, not proved); and the
    invariant enhancement adds an optional expression declaring the claim
    role, so an invariant can state a machine-checkable claim.

    Compiler passes: condition_check, layering_verify, event_graph_analyze,
    coverage_tracking (with proper dependency ordering), which run under
    specforge analyze, and analysis_available, the one check-phase pass,
    which runs with every compile (I015). What each analyze pass reports:
    - condition_check: W096, W039, I011, W040
    - layering_verify: E031, E041, W030, W031, W110
    - event_graph_analyze: E034, E042, W029, W032, W033, W034, I009
    - coverage_tracking: W035, I008, I014

    Validation rules (declarative, run in every check): W123 unreferenced
    property, W124 empty property description, W126 unreferenced axiom, W127 empty axiom description, W128
    unreferenced protocol, W129 empty protocol description, W131 unreferenced
    refinement, W132 empty refinement description, W133 refinement
    without invariant_deltas (a custom rule), W134 unreferenced process, W135
    empty process description, W136 empty process alphabet.

    Verify kinds contributed: contract, refinement, deadlock_free, liveness.

    Feature flags: conditions (default true, no deps), layering (default
    true, requires conditions), concurrency (default true, no deps).

    Warning level requirement: all formal warnings (W029-W035, W039, W040, W096, W110, W123-W129, W131-W136)
    require warning_level=strict. This prevents overwhelming new users.

    Safety-critical scope: @specforge/formal is intended for projects
    that benefit from structural analysis — safety-critical systems,
    distributed architectures, and formally-inclined teams. It is NOT
    required for basic SpecForge usage.
  """
  requires {
    supported_protocol   "the handshake's protocol major version is the host's"
    valid_extension_name "name == '@specforge/formal'"
    wasm_module_exists   "the declaration is read from the compiled Wasm component"
  }
  ensures {
    five_entity_kinds        "entityKinds contains property, axiom, protocol, refinement, process (all testable=false, supports_verify=false)"
    ten_edge_types           "edgeTypes contains BehaviorSatisfiesProperty, BehaviorRefinesBehavior, EventFollowsProtocol, EventParticipatesInProcess, PropertyDependsOnInvariant, AxiomAssumesInvariant, RefinementRefinesAbstract, RefinementRefinesConcrete, RefinementChainsToRefinement, ProcessComposesProcess"
    assumes_edge             "AxiomAssumesInvariant: source=axiom, target=invariant"
    satisfies_edge           "BehaviorSatisfiesProperty: source=behavior, target=property"
    refines_behavior_edge    "BehaviorRefinesBehavior: source=behavior, target=behavior"
    follows_protocol_edge    "EventFollowsProtocol: source=event, target=protocol"
    property_depends_on_edge "PropertyDependsOnInvariant: source=property, target=invariant"
    refines_abstract_edge    "RefinementRefinesAbstract: source=refinement, target=behavior"
    refines_concrete_edge    "RefinementRefinesConcrete: source=refinement, target=behavior"
    refinement_chain_edge    "RefinementChainsToRefinement: source=refinement, target=refinement"
    participates_in_edge     "EventParticipatesInProcess: source=event, target=process"
    process_composition_edge "ProcessComposesProcess: source=process, target=process"
    five_passes              "passes contains condition_check, layering_verify, event_graph_analyze, coverage_tracking, and the check-phase analysis_available"
    pass_ordering            "layering_verify depends_on condition_check; event_graph_analyze depends_on layering_verify; coverage_tracking depends_on event_graph_analyze"
    validation_rules         "validation_rules contains W123, W124, W126, W127, W128, W129, W131, W132, W133, W134, W135, W136"
    three_feature_flags      "feature_flags contains conditions, layering, concurrency"
    flag_dependencies        "layering requires conditions"
    inline_condition_fields  "requires/ensures/maintains are block fields of named conditions (ConditionEntry)"
    enhancements_declared    "entity_enhancements add requires/ensures/maintains/satisfies/sync/abstract/refines to behavior, follows_protocol/participates_in/sync to event, and expression to invariant"
    proof_roles_declared     "property.expression is a claim, axiom.expression a bound, invariant.expression a claim"
    verify_kinds_declared    "verify_kinds contains contract, refinement, deadlock_free, liveness"
    peer_dep_software        "peer_dependencies contains @specforge/software ^1.0 (required)"
    warning_level_strict     "all formal warnings require warning_level=strict"
    no_sandbox_policy        "sandbox_policy is null — formal declares no sandbox policy and runs under the host's ceiling"
  }
  features [fa_progressive_warnings]
  verify unit "manifest name is @specforge/formal"
  verify unit "manifest declares 5 entity kinds (property, axiom, protocol, refinement, process)"
  verify unit "manifest declares 10 edge types"
  verify unit "all entity kinds have testable=false"
  verify unit "AxiomAssumesInvariant edge: axiom -> invariant"
  verify unit "BehaviorSatisfiesProperty edge: behavior -> property"
  verify unit "BehaviorRefinesBehavior edge: behavior -> behavior"
  verify unit "EventFollowsProtocol edge: event -> protocol"
  verify unit "PropertyDependsOnInvariant edge: property -> invariant"
  verify unit "RefinementRefinesAbstract and RefinementRefinesConcrete edges: refinement -> behavior"
  verify unit "RefinementChainsToRefinement edge: refinement -> refinement"
  verify unit "EventParticipatesInProcess edge: event -> process"
  verify unit "ProcessComposesProcess edge: process -> process"
  verify unit "manifest declares 5 passes in dependency order"
  verify unit "manifest declares the 13 formal validation rules"
  verify unit "manifest declares 3 feature flags"
  verify unit "layering flag requires conditions flag"
  verify unit "requires/ensures/maintains are block fields of named conditions"
  verify unit "entity_enhancements add formal fields to software entities"
  verify unit "entity_enhancements add refinement field to behavior"
  verify unit "entity_enhancements add process field to event"
  verify unit "property and invariant expressions are claims and axiom expressions bounds"
  verify unit "verify_kinds contains contract, refinement, deadlock_free, liveness"
  verify unit "peer_dependencies requires @specforge/software"
  verify unit "formal warnings require warning_level=strict"
}
