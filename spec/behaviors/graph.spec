// Graph building behaviors — constructing the in-memory graph

use "events/compilation"
use "invariants/core"
use "invariants/zero-entity-core"
use "types/core"
use "types/graph"
use "types/zero-entity-core"

behavior build_in_memory_graph "Build In-Memory Graph" {
  features   [graph_construction]
  invariants [string_interning_consistency, entity_id_uniqueness]
  category   command
  types      [Graph, Node, Edge, SpecFile, EdgeType, FileIndex, JsonValue, JsonObject]
  consumes   [resolution_complete]
  produces   [graph_built]
  requires {
    resolution_complete "resolution_complete event has fired, confirming all use imports are resolved and entity references are linked"
  }
  ensures {
    one_node_per_entity    "Graph contains exactly one node per declared entity"
    one_edge_per_reference "Graph contains one edge per resolved reference"
    no_dangling_edges      "No dangling edges exist (every edge connects two existing nodes)"
    edges_written          "every edge the linker makes records where each of its references is written"
  }
  contract   """
    After resolution, the compiler MUST construct an in-memory directed
    graph where each entity becomes a node and each resolved reference
    becomes a typed edge. The graph builder materializes the pending
    edges recorded by link_entity_references. The graph MUST contain
    exactly one node per declared entity and one edge per resolved
    reference.
  """
  verify unit "graph contains one node per entity"
  verify unit "graph contains one edge per resolved reference"
  verify unit "edge types match relationship semantics"
  verify unit "every edge connects two existing nodes"
  verify unit "an edge records the span of each reference it was linked from"
  verify unit "an incremental build records what a cold build records"
  verify contract "Build In-Memory Graph: in-memory graph construction holds — resolution_complete, one_node_per_entity, one_edge_per_reference, no_dangling_edges"
  verify unit "W060 carries actionable suggestion"
  verify unit "W061 carries actionable suggestion"
  verify unit "build_graph emits W061 for reference cycles"
  verify unit "W061 names the cycle's entities in its data"
  verify unit "build_graph no W061 for acyclic refs"
  verify unit "detects cycles in directed graph"
  verify unit "detects self-referencing cycle"
  verify unit "no false positives for acyclic graph"
  verify unit "same ID with different kinds does not produce E002"
}

behavior apply_file_defaults "Apply File Defaults" {
  features   [graph_construction]
  invariants [zero_domain_knowledge_core, entity_id_uniqueness]
  category   command
  types      [Graph, Node, SpecFile]
  produces   [] // part of the graph build: its nodes surface through graph_built
  contract   """
    A `defaults <kind> { field value … }` block MUST give each of its fields to
    every block of that kind in the same file that does not write the field; a
    field the block writes MUST replace the default whole. The graph MUST hold the
    resolved value, written where the default is. A default for a field the kind
    does not declare, for a kind no loaded extension declares, or for a structural
    kind MUST be E077 and MUST NOT be applied. A title, a verify or method member, a
    field given twice, a second block for the kind, or a block after the file's
    first entity block MUST be E078. A block field equal to its default, and a
    default no block takes, MUST be W161. A defaults block is never a node.
  """
  verify unit "a default reaches every block of its kind in its file"
  verify unit "a block's own field replaces the default whole"
  verify unit "a default reaches no other file and no other kind"
  verify unit "a defaults block is never a node"
  verify unit "an inherited reference is written at the default"
  verify unit "a default for a field its kind does not declare is E077 and is not applied"
  verify unit "a malformed defaults block is E078"
  verify unit "a default that changes nothing is W161"
  verify unit "the defaults answer which entry applies and whether a field restates it"
  verify unit "changing a default changes every block that inherits it"
  verify unit "find-references lists an inherited reference once per block"
  verify unit "rename edits an inherited reference once"
  verify unit "navigation answers an inherited reference's token once"
  verify unit "query, trace and the graph export show the inherited list"
  verify unit "a defaults block completes, hovers and navigates as the kind it names"
  verify unit "the editor shows an inherited reference's location once"
  verify unit "the written tokens of a file include a default no block takes"
}

behavior link_derived_references "Link Derived References" {
  features   [graph_construction]
  invariants [zero_domain_knowledge_core]
  category   command
  types      [Graph, Edge, FieldDescriptor, FieldRegistryEntry, DerivedReferenceSource]
  produces   [] // part of the graph build: its edges surface through graph_built
  requires {
    fields_registered "the field registry holds every loaded extension's fields"
  }
  ensures {
    field_type_names_linked       "every name in the entity's type-syntax field values that resolves to an entity of the field's target kind gets an edge labelled with the field"
    method_signature_names_linked "every name in the entity's method parameter and return types that resolves to an entity of the field's target kind gets an edge labelled with the field"
    unresolved_names_unlinked     "a primitive, a generic wrapper or an undeclared name creates no edge"
    derived_names_written         "each derived edge records where every name it was derived from is written"
  }
  contract   """
    A field an extension registers MAY declare `derived_from`. The host
    then gives the field edges from type names the entity writes
    elsewhere, as if the entity had listed them in the field:

    - `type_expressions`: the names in the entity's field values written as
      type syntax: an identifier, an array type `T[]` or a type union
      `A | B`, with generics inside them. Quoted strings, numbers, lists,
      blocks and the entity's single-reference fields (which link on their
      own) don't count.
    - `method_signatures`: the names in the parameter and return types of
      the entity's methods.

    A quoted member of a union (`type Level = "Task" | Other`) is a literal, not a
    type, and names nothing. A type expression reduces to the names inside it:
    `Result<TsProject, EmitterError>` names Result, TsProject and
    EmitterError; `TsClassMember[]` names TsClassMember. Each name that
    resolves to an entity of the field's target kind, other than the
    entity itself, gets one edge labelled with the field's name. A
    primitive, a generic wrapper or an undeclared name resolves to nothing
    and creates no edge; reporting undeclared names stays with the
    extension that owns the kind (E004).

    The core names no kind here (zero_domain_knowledge_core): which kinds
    derive edges, from where and to which kind comes only from the fields
    extensions register. Derived edges join the graph after reference
    cycle detection, so a recursive type is not a reference cycle (W061).
    Each such name is where the edge is written: navigation and rename read
    it (ADR 0127).
  """
  verify unit "a name in a type-syntax field value links through the derived field"
  verify unit "a name in a method parameter or return type links through the derived field"
  verify unit "a primitive, a generic wrapper or an undeclared name creates no edge"
  verify unit "a name that resolves to an entity of another kind creates no edge"
  verify unit "quoted strings and single-reference fields derive no edge"
  verify unit "a quoted union member derives no edge"
  verify unit "an entity naming itself creates no edge"
  verify unit "a recursive type derives no reference cycle"
  verify unit "a kind with no derived field derives no edge"
  verify unit "a derived edge is written at each name it was derived from"
  verify unit "the written tokens of a file include every name a derived field links"
  verify integration "a field's derived_from reaches the graph from the extension's manifest"
}

behavior maintain_mutable_graph "Maintain Mutable Graph" {
  features   [graph_construction, incremental_compilation]
  invariants [incremental_correctness, graph_traversal_integrity]
  category   command
  types      [Graph, Subgraph]
  produces   [] // passive API behavior: graph mutations surface via rebuild events, not events of its own
  requires {
    graph_initialized "An in-memory graph instance exists and is accessible for mutation"
  }
  ensures {
    mutations_applied          "Node and edge additions and removals are reflected in the graph"
    no_dangling_edges_enforced "After any mutation, no edges point to removed nodes"
  }
  maintains {
    graph_consistency "Graph remains structurally consistent across all incremental mutations"
  }
  contract   """
    The in-memory graph MUST support incremental mutation: adding nodes,
    removing nodes, adding edges, and removing edges. After mutation,
    the graph MUST remain consistent — no dangling edges pointing to
    removed nodes. This mutability is required for watch mode.
  """
  verify unit "add and remove nodes from graph"
  verify unit "removing a node removes its edges"
  verify unit "graph consistency after batch mutations"
  verify unit "added edges are reflected in outgoing edge queries"
  verify contract "Maintain Mutable Graph: mutable graph maintenance holds — graph_initialized, mutations_applied, no_dangling_edges_enforced, graph_consistency"
}
