// @specforge/software extension manifest declaration

use "extensions/software/types"
use "types/zero-entity-core"

behavior se_declare_manifest "Declare @specforge/software Manifest" {
  features [se_core_entity_kinds]
  category command
  types    [ExtensionDeclaration, EntityKindDescriptor, EdgeTypeDescriptor]
  contract """
    The @specforge/software extension MUST declare itself with name
    "@specforge/software". Its declaration MUST declare
    exactly 6 entity kinds (behavior, invariant, feature, event, type,
    port), 9 edge types (References, Implements, Produces, Consumes,
    UsesType, UsesPort, Enforces, Imports, LinksTo), and all associated
    validation rules. The compiled Wasm component MUST serve it.
  """
  requires {
    supported_protocol   "the handshake's protocol major version is the host's"
    valid_extension_name "name == '@specforge/software'"
    wasm_module_exists   "the declaration is read from the compiled Wasm component"
  }
  ensures {
    six_entity_kinds      "entityKinds.length == 6"
    nine_edge_types       "edgeTypes.length == 9"
    all_kinds_named       "entityKinds contains behavior, invariant, feature, event, type, port"
    all_edges_named       "edgeTypes contains References, Implements, Produces, Consumes, UsesType, UsesPort, Enforces, Imports, LinksTo"
    declared_categories   "the declaration declares entities, edges, enhancements and validation_rules"
    optional_product_peer "peer_dependencies holds one optional peer, @specforge/product, whose kinds the feature/module/milestone links target"
  }
  verify unit "manifest name is @specforge/software"
  verify unit "manifest declares exactly 6 entity kinds"
  verify unit "manifest declares exactly 9 edge types"
  verify unit "the handshake's protocol major is the host's"
  verify unit "the declaration declares entities, edges, enhancements and validation_rules"
  verify unit "the only peer is @specforge/product, and it is optional"
}

invariant se_manifest_six_entity_kinds "Six Entity Kinds" {
  guarantee """
    The @specforge/software manifest MUST declare exactly 6 entity kinds:
    behavior, invariant, feature, event, type, port. No more, no fewer.
    This count was validated by a 10-expert panel (RES-27) and represents
    the minimal complete set for software engineering specification.
  """
  risk      high
  verify property "manifest entityKinds array has exactly 6 entries"
}

invariant se_manifest_nine_edge_types "Nine Edge Types" {
  guarantee """
    The @specforge/software manifest MUST declare exactly 9 edge types:
    References, Implements, Produces, Consumes, UsesType, UsesPort,
    Enforces, Imports, LinksTo. These edges model all relationships
    between the 6 entity kinds in the software engineering domain.
  """
  risk      medium
  verify property "manifest edgeTypes array has exactly 9 entries"
}
