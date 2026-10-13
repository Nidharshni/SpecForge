// @specforge/governance extension behaviors — entity kind and edge registration

use "extensions/governance/invariants"
use "extensions/governance/types"
use "types/zero-entity-core"

behavior ge_register_entity_kinds "Register Governance Entity Kinds" {
  category   command
  invariants [ge_manifest_three_entity_kinds]
  types      [EntityKindDescriptor, GovernanceDecision, GovernanceConstraint, GovernanceFailureMode]
  contract   """
    The @specforge/governance extension MUST register 3 entity kinds with
    full metadata in the KindRegistry. All three are declarative records:
    testable=false, supportsVerify=false.
  """
  requires {
    manifest_loaded    "ExtensionDeclaration is parsed and schema-validated"
    no_duplicate_kinds "KindRegistry has no entries with names matching this extension's kinds"
  }
  ensures {
    decision_registered     "KindRegistry contains decision: testable=false, supportsVerify=false, semanticToken=class, lspIcon=Event, dotShape=note"
    constraint_registered   "KindRegistry contains constraint: testable=false, supportsVerify=false, semanticToken=property, lspIcon=Key, dotShape=octagon"
    failure_mode_registered "KindRegistry contains failure_mode: testable=false, supportsVerify=false, semanticToken=event, lspIcon=Warning, dotShape=doubleoctagon"
    three_kinds_total       "KindRegistry has exactly 3 domain entries after registration"
  }
  features   [ge_core_entity_kinds]
  verify unit "decision registered with testable=false and dotShape=note"
  verify unit "constraint registered with testable=false and dotShape=octagon"
  verify unit "failure_mode registered with testable=false and dotShape=doubleoctagon"
}

behavior ge_register_edge_types "Register Governance Edge Types" {
  invariants [ge_manifest_four_edge_types]
  category   command
  types      [EdgeTypeDescriptor]
  contract   """
    The @specforge/governance extension MUST register 4 edge types that
    model relationships between governance entities and invariants/behaviors.
  """
  requires {
    kinds_registered "all 3 entity kinds are in KindRegistry"
  }
  ensures {
    decision_invariant     "EdgeTypeSet contains DecisionInvariant (decision->invariant)"
    constrains_behavior    "EdgeTypeSet contains ConstrainsBehavior (constraint->behavior, cross-extension)"
    protects_invariant     "EdgeTypeSet contains ProtectsInvariant (constraint->invariant)"
    failure_mode_invariant "EdgeTypeSet contains FailureModeInvariant (failure_mode->invariant)"
    four_edges_total       "EdgeTypeSet has exactly 4 entries"
  }
  features   [ge_core_entity_kinds]
  verify unit "all 4 edge types registered in edge set"
  verify unit "DecisionInvariant edge has sourceKind=decision and targetKind=invariant"
  verify unit "ConstrainsBehavior edge has sourceKind=constraint and targetKind=behavior"
  verify unit "ProtectsInvariant edge has sourceKind=constraint and targetKind=invariant"
}

behavior ge_register_field_definitions "Register Governance Field Definitions" {
  category command
  types    [
    FieldDescriptor,
    EntityKindDescriptor,
    ConstraintCategory,
    ConstraintPriority,
    DecisionStatus,
    PostMitigation,
  ]
  contract """
    The @specforge/governance extension MUST register field definitions for
    each entity kind with name, type, edge mapping, and target kind. A
    constraint's metric MUST declare the bound proof role: its comparisons
    are facts the prove pass assumes (ADR 0009).
  """
  requires {
    kinds_and_edges_registered "all 3 kinds and 4 edge types are registered"
  }
  ensures {
    decision_fields     "decision has: status(enum DecisionStatus, required), date(string), context(string), decision(string), consequences(string[]), invariants(reference[]->invariant, DecisionInvariant)"
    constraint_fields   "constraint has: category(string), priority(enum ConstraintPriority), metric(string, proof_role bound), constrains(reference[]->behavior, ConstrainsBehavior), protects(reference[]->invariant, ProtectsInvariant)"
    failure_mode_fields "failure_mode has: invariant(reference->invariant, FailureModeInvariant), severity(enum FailureSeverity, required), occurrence(enum FailureOccurrence), detection(enum FailureDetection), post_severity(enum FailureSeverity), post_occurrence(enum FailureOccurrence), post_detection(enum FailureDetection), rpn(integer), cause(string), effect(string), mitigation(string), post_mitigation(block)"
  }
  features [ge_core_entity_kinds]
  verify unit "decision invariants field registered with DecisionInvariant edge"
  verify unit "constraint constrains field registered with ConstrainsBehavior edge"
  verify unit "constraint protects field registered with ProtectsInvariant edge"
  verify unit "failure_mode post_mitigation field registered as block type"
  verify unit "constraint metric field declares the bound proof role"
}

behavior ge_register_validation_rules "Register Governance Validation Rules" {
  category command
  types    [ValidationRulePattern, ValidationPatternKind]
  contract """
    The @specforge/governance extension MUST register declarative validation
    rules in its manifest: E005 (RPN arithmetic), W047 (unmitigated high-risk
    invariants), W048 (constraints with no protected invariants).
  """
  requires {
    field_definitions_registered "all field definitions are in FieldRegistry"
  }
  ensures {
    rules_registered "E005, W047, W048 rules are registered"
    rules_sorted     "rules are sorted by diagnostic code for deterministic execution"
  }
  features [ge_core_entity_kinds, ge_validation_suite]
  verify unit "validation rules registered from manifest"
  verify unit "rules include E005, W047, W048"
  verify unit "rules sorted by diagnostic code"
}
