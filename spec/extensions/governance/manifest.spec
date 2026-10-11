// @specforge/governance extension manifest declaration

use "extensions/governance/types"
use "types/zero-entity-core"

behavior ge_declare_manifest "Declare @specforge/governance Manifest" {
  features [ge_core_entity_kinds]
  category command
  types    [ExtensionDeclaration, EntityKindDescriptor, EdgeTypeDescriptor]
  contract """
    The @specforge/governance extension MUST declare itself with name
    "@specforge/governance". Its declaration MUST declare
    exactly 3 entity kinds (decision, constraint, failure_mode), 4 edge types
    (DecisionInvariant, ConstrainsBehavior, ProtectsInvariant,
    FailureModeInvariant), and all associated validation rules.
  """
  requires {
    supported_protocol   "the handshake's protocol major version is the host's"
    valid_extension_name "name == '@specforge/governance'"
  }
  ensures {
    three_entity_kinds  "entityKinds.length == 3"
    four_edge_types     "edgeTypes.length == 4"
    all_kinds_named     "entityKinds contains decision, constraint, failure_mode"
    all_edges_named     "edgeTypes contains DecisionInvariant, ConstrainsBehavior, ProtectsInvariant, FailureModeInvariant"
    declared_categories "the declaration declares entities, edges and validation_rules"
    optional_peer_dep   "peer_dependencies contains @specforge/software ^1.0 (optional, for ConstrainsBehavior cross-extension edge targeting behavior kind)"
    no_sandbox_policy   "sandbox_policy is null — governance declares no sandbox policy and runs under the host's ceiling"
  }
  verify unit "manifest name is @specforge/governance"
  verify unit "manifest declares exactly 3 entity kinds"
  verify unit "manifest declares exactly 4 edge types"
  verify unit "the handshake's protocol major is the host's"
  verify unit "the declaration declares entities, edges and validation_rules"
  verify unit "peer_dependencies includes optional @specforge/software"
  verify unit "sandbox_policy is null"
}

invariant ge_manifest_three_entity_kinds "Three Entity Kinds" {
  guarantee """
    The @specforge/governance manifest MUST declare exactly 3 entity kinds:
    decision, constraint, failure_mode. All three are declarative records
    with testable=false and supportsVerify=false.
  """
  risk      high
  verify property "manifest entityKinds array has exactly 3 entries"
}

invariant ge_manifest_four_edge_types "Four Edge Types" {
  guarantee """
    The @specforge/governance manifest MUST declare exactly 4 edge types:
    DecisionInvariant (decision->invariant), ConstrainsBehavior
    (constraint->behavior, cross-extension), ProtectsInvariant
    (constraint->invariant), FailureModeInvariant (failure_mode->invariant).
  """
  risk      medium
  verify property "manifest edgeTypes array has exactly 4 entries"
}
