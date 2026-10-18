// @specforge/formal validation rules — formal-specific declarative validation patterns
//
// W058 (feature coverage mismatch, downgraded from E033): trimmed, no
// sound algorithm (ADR 0040).
// W059-W060: removed (were condition entity validation, condition entity kind removed)
// W123-W124: property entity validation
// W126-W127: axiom entity validation
// W128-W129: protocol entity validation (W130, ordering conflict, trimmed: ADR 0040)
// W131-W133: refinement entity validation
// W134-W136: process entity validation
//
// All of them run in every check: they are declarative rules, not passes.

use "extensions/formal/invariants"
use "extensions/formal/types"
use "types/zero-entity-core"

defaults behavior {
  types    [FormalRefinement]
  features [fa_refinement_layering]
}

// W059-W060 removed: condition entity kind no longer exists.
// Conditions are inline fields, not standalone entities.

// ── Property Validation (W123-W124) ──────────────────────────

behavior fa_validate_unreferenced_property "W123: Unreferenced Property" {
  category   query
  invariants [fa_property_entity_reachability]
  types      [FormalProperty]
  contract   """
    Detect property entities with no incoming Satisfies edges. An unreferenced
    property is a temporal assertion that no behavior claims to satisfy —
    it should either be referenced via the satisfies field or removed.
    Requires warning_level=strict to fire.
  """
  requires {
    graph_built          "entity graph is fully constructed with all edges"
    strict_warning_level "warning_level is set to strict"
  }
  ensures {
    unreferenced_detected "property with no incoming Satisfies edges produces W123 warning"
    referenced_passes     "property with at least one incoming Satisfies edge produces no diagnostic"
    correct_template      "message template is: property '{id}' is not satisfied by any behavior"
  }
  features   [fa_temporal_properties]
  verify unit "property with no incoming Satisfies edges produces W123"
  verify unit "property with Satisfies edge passes"
  verify unit "W123 only fires at warning_level=strict"
}

behavior fa_validate_empty_property_description "W124: Empty Property Description" {
  category query
  types    [FormalProperty]
  contract """
    Detect property entities with an empty or whitespace-only description.
    Properties are temporal assertions — a blank description makes the
    property opaque to agents and reviewers.
    A property that writes no description is not reported.
  """
  ensures {
    empty_warned     "property with empty description produces W124 warning"
    non_empty_passes "property with non-empty description passes"
    correct_template "message template is: property '{id}' has empty description"
  }
  features [fa_temporal_properties]
  verify unit "property with empty description produces W124"
  verify unit "property with non-empty description passes"
}

behavior fa_validate_property_type "Property Type Vocabulary" {
  category query
  types    [FormalProperty, PropertyType]
  contract """
    A property's property_type (safety, liveness or fairness) classifies
    the temporal assertion; it is required (E006 when missing) and declared
    an enum of PropertyType, so another value is the core's E061.
  """
  ensures {
    declared_enum "property_type is a required enum of safety, liveness, fairness"
  }
  features [fa_temporal_properties]
  verify unit "property_type is a declared enum of safety, liveness, fairness"
}

// ── Axiom Validation (W126-W127) ─────────────────────────────

behavior fa_validate_unreferenced_axiom "W126: Unreferenced Axiom" {
  category   query
  invariants [fa_axiom_entity_reachability]
  types      [FormalAxiom]
  contract   """
    Detect axiom entities with no incoming AssumedBy edges. An unreferenced
    axiom is an assumption that no condition depends on — it should
    either be referenced via the assumes field or removed.
    Requires warning_level=strict to fire.
  """
  requires {
    graph_built          "entity graph is fully constructed with all edges"
    strict_warning_level "warning_level is set to strict"
  }
  ensures {
    unreferenced_detected "axiom with no incoming AssumedBy edges produces W126 warning"
    referenced_passes     "axiom with at least one incoming AssumedBy edge produces no diagnostic"
    correct_template      "message template is: axiom '{id}' is not assumed by any condition"
  }
  features   [fa_axiom_foundations]
  verify unit "axiom with no incoming AssumedBy edges produces W126"
  verify unit "axiom with AssumedBy edge passes"
  verify unit "W126 only fires at warning_level=strict"
}

behavior fa_validate_empty_axiom_description "W127: Empty Axiom Description" {
  category query
  types    [FormalAxiom]
  contract """
    Detect axiom entities with an empty or whitespace-only description.
    Axioms are assumed-true foundations — a blank description makes
    the assumption invisible and unjustifiable.
    A axiom that writes no description is not reported.
  """
  ensures {
    empty_warned     "axiom with empty description produces W127 warning"
    non_empty_passes "axiom with non-empty description passes"
    correct_template "message template is: axiom '{id}' has empty description"
  }
  features [fa_axiom_foundations]
  verify unit "axiom with empty description produces W127"
  verify unit "axiom with non-empty description passes"
}

// ── Protocol Validation (W128-W129) ──────────────────────────

behavior fa_validate_unreferenced_protocol "W128: Unreferenced Protocol" {
  category   query
  invariants [fa_protocol_entity_reachability]
  types      [FormalProtocol]
  contract   """
    Detect protocol entities with no incoming FollowsProtocol edges.
    An unreferenced protocol is a sync contract that no event follows — it
    should either be referenced via the follows_protocol field or removed.
    Requires warning_level=strict to fire.
  """
  requires {
    graph_built          "entity graph is fully constructed with all edges"
    strict_warning_level "warning_level is set to strict"
  }
  ensures {
    unreferenced_detected "protocol with no incoming FollowsProtocol edges produces W128 warning"
    referenced_passes     "protocol with at least one incoming FollowsProtocol edge produces no diagnostic"
    correct_template      "message template is: protocol '{id}' is not followed by any event"
  }
  features   [fa_protocol_contracts]
  verify unit "protocol with no incoming FollowsProtocol edges produces W128"
  verify unit "protocol with FollowsProtocol edge passes"
  verify unit "W128 only fires at warning_level=strict"
}

behavior fa_validate_empty_protocol_description "W129: Empty Protocol Description" {
  category query
  types    [FormalProtocol]
  contract """
    Detect protocol entities with an empty or whitespace-only description.
    Protocols are synchronization contracts — a blank description makes
    the contract opaque to agents and event graph analysis.
    A protocol that writes no description is not reported.
  """
  ensures {
    empty_warned     "protocol with empty description produces W129 warning"
    non_empty_passes "protocol with non-empty description passes"
    correct_template "message template is: protocol '{id}' has empty description"
  }
  features [fa_protocol_contracts]
  verify unit "protocol with empty description produces W129"
  verify unit "protocol with non-empty description passes"
}

// ── Refinement Validation (W131-W133) ───────────────────────

behavior fa_validate_unreferenced_refinement "W131: Unreferenced Refinement" {
  category   query
  invariants [fa_refinement_entity_reachability]
  contract   """
    Detect refinement entities with no RefinementRefinesAbstract,
    RefinementRefinesConcrete, or RefinementChainsToRefinement edges. An unreferenced refinement is a graph node
    that captures an abstract-to-concrete mapping but is disconnected
    from all behaviors and other refinements — it should either be
    connected or removed. Requires warning_level=strict to fire.
  """
  requires {
    graph_built          "entity graph is fully constructed with all edges"
    strict_warning_level "warning_level is set to strict"
  }
  ensures {
    unreferenced_detected "refinement with no refinement edges produces W131 warning"
    referenced_passes     "refinement with at least one refinement edge produces no diagnostic"
    correct_template      "message template is: refinement '{id}' is not connected to any behavior or refinement chain"
  }
  verify unit "refinement with no edges produces W131"
  verify unit "refinement with abstract_entity and concrete_entity edges passes"
  verify unit "refinement with RefinementChainsToRefinement edge passes"
  verify unit "W131 only fires at warning_level=strict"
}

behavior fa_validate_empty_refinement_description "W132: Empty Refinement Description" {
  category query
  contract """
    Detect refinement entities with an empty or whitespace-only description.
    Refinements capture abstract-to-concrete mappings — a blank description
    makes the mapping opaque to agents and reviewers.
    A refinement that writes no description is not reported.
  """
  ensures {
    empty_warned     "refinement with empty description produces W132 warning"
    non_empty_passes "refinement with non-empty description passes"
    correct_template "message template is: refinement '{id}' has empty description"
  }
  verify unit "refinement with empty description produces W132"
  verify unit "refinement with non-empty description passes"
}

behavior fa_validate_refinement_without_delta "W133: Refinement Without Condition Delta" {
  category query
  types    [FormalRefinement, ConditionDelta]
  contract """
    Detect refinement entities that declare no invariant_deltas: the
    field is absent or an empty list. The invariant_deltas field records
    what changes between abstract and concrete (the invariants the
    refinement adds or relaxes) — without it, the refinement is purely
    structural with no formal content. A declarative rule: it runs in
    every check.
  """
  ensures {
    missing_warned   "refinement with no invariant_deltas produces W133 warning"
    present_passes   "refinement with a non-empty invariant_deltas produces no diagnostic"
    correct_template "message template is: refinement '{id}' declares no invariant_deltas"
  }
  verify unit "refinement with no invariant_deltas produces W133"
  verify unit "refinement with invariant_deltas passes"
}

// ── Refinement Structural Validation (E041) ─────────────────

behavior fa_validate_refinement_self_reference "E041b: Refinement Self-Reference" {
  category query
  contract """
    Detect refinement entities where abstract_entity equals concrete_entity.
    A refinement that maps a behavior to itself is structurally invalid —
    it creates a trivial cycle. Produces E041 error.
  """
  requires {
    graph_built "entity graph is fully constructed with all edges"
  }
  ensures {
    self_ref_detected "refinement with abstract_entity == concrete_entity produces E041 error"
    distinct_passes   "refinement with distinct abstract_entity and concrete_entity produces no diagnostic"
    correct_template  "message template is: refinement '{id}' maps behavior '{behavior_id}' to itself"
  }
  verify unit "refinement with abstract_entity == concrete_entity produces E041"
  verify unit "refinement with distinct IDs passes"
}

// ── Process Validation (W134-W136) ──────────────────────────

behavior fa_validate_unreferenced_process "W134: Unreferenced Process" {
  category   query
  invariants [fa_process_entity_reachability]
  types      [FormalProcess]
  contract   """
    Detect process entities with no incoming ParticipatesIn edges. An
    unreferenced process is a communicating process that no event participates
    in — it should either have events assigned to its alphabet or be
    removed. Requires warning_level=strict to fire.
  """
  requires {
    graph_built          "entity graph is fully constructed with all edges"
    strict_warning_level "warning_level is set to strict"
  }
  ensures {
    unreferenced_detected "process with no incoming ParticipatesIn edges produces W134 warning"
    referenced_passes     "process with at least one incoming ParticipatesIn edge produces no diagnostic"
    correct_template      "message template is: process '{id}' has no events participating in it"
  }
  features   [fa_process_modeling]
  verify unit "process with no incoming ParticipatesIn edges produces W134"
  verify unit "process with ParticipatesIn edge passes"
  verify unit "W134 only fires at warning_level=strict"
}

behavior fa_validate_empty_process_description "W135: Empty Process Description" {
  category query
  types    [FormalProcess]
  contract """
    Detect process entities with an empty or whitespace-only description.
    Processes model communicating sequential processes — a blank description
    makes the process opaque to agents and event graph analysis.
    A process that writes no description is not reported.
  """
  ensures {
    empty_warned     "process with empty description produces W135 warning"
    non_empty_passes "process with non-empty description passes"
    correct_template "message template is: process '{id}' has empty description"
  }
  features [fa_process_modeling]
  verify unit "process with empty description produces W135"
  verify unit "process with non-empty description passes"
}

behavior fa_validate_process_without_alphabet "W136: Process Without Alphabet" {
  category query
  types    [FormalProcess]
  contract """
    Detect process entities that write an empty alphabet. The alphabet
    defines which events the process can engage in — without it, the
    process is disconnected from the event graph. The field is required,
    so an absent alphabet is E006 (missing required field), not W136. A
    declarative rule: it runs in every check.
  """
  ensures {
    missing_warned   "process with an empty alphabet produces W136 warning"
    present_passes   "process with non-empty alphabet produces no diagnostic"
    correct_template "message template is: process '{id}' has no alphabet (no events declared)"
  }
  features [fa_process_modeling]
  verify unit "process with empty alphabet produces W136"
  verify unit "process with non-empty alphabet passes"
}
