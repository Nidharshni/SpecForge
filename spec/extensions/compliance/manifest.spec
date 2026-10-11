// @specforge/compliance extension manifest declaration

use "types/zero-entity-core"

behavior ce_declare_manifest "Declare @specforge/compliance Manifest" {
  features [ce_core_entity_kinds]
  category command
  types    [ExtensionDeclaration, EntityKindDescriptor, EdgeTypeDescriptor]
  contract """
    The @specforge/compliance extension MUST declare itself with name
    "@specforge/compliance". Its declaration MUST declare
    exactly 4 entity kinds (regulation, control, evidence, audit), 4 edge
    types (Governs, ImplementedBy, ProvidedBy, Audits), and all associated
    validation rules. The compiled Wasm component MUST serve it.
  """
  requires {
    supported_protocol   "the handshake's protocol major version is the host's"
    valid_extension_name "name == '@specforge/compliance'"
    wasm_module_exists   "the declaration is read from the compiled Wasm component"
  }
  ensures {
    four_entity_kinds   "entityKinds.length == 4"
    four_edge_types     "edgeTypes.length == 4"
    all_kinds_named     "entityKinds contains regulation, control, evidence, audit"
    all_edges_named     "edgeTypes contains Governs, ImplementedBy, ProvidedBy, Audits"
    declared_categories "the declaration declares entities, edges and validation_rules"
  }
  verify unit "manifest name is @specforge/compliance"
  verify unit "manifest declares exactly 4 entity kinds"
  verify unit "manifest declares exactly 4 edge types"
  verify unit "the handshake's protocol major is the host's"
  verify unit "the declaration declares entities, edges and validation_rules"
}

invariant ce_manifest_four_entity_kinds "Four Entity Kinds" {
  guarantee """
    The @specforge/compliance manifest MUST declare exactly 4 entity kinds:
    regulation, control, evidence, audit. These represent the minimal
    complete set for regulatory compliance specification.
  """
  risk      high
  verify property "manifest entityKinds array has exactly 4 entries"
}

invariant ce_manifest_four_edge_types "Four Edge Types" {
  guarantee """
    The @specforge/compliance manifest MUST declare exactly 4 edge types:
    Governs (regulation -> control), ImplementedBy (control -> evidence),
    ProvidedBy (evidence -> audit), Audits (audit -> regulation). These
    edges model the compliance traceability chain.
  """
  risk      medium
  verify property "manifest edgeTypes array has exactly 4 entries"
}
