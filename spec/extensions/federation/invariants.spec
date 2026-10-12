// Federation-specific invariants

invariant federated_graph_determinism "Federated Graph Determinism" {
  guarantee """
    The order in which remote project graphs are loaded and merged MUST
    NOT affect the resulting federated graph. Given the same set of remote
    graphs with the same content, the merged FederatedGraph MUST be
    identical regardless of load order or merge sequence.
  """
  risk      medium
  verify property "different merge orders produce identical federated graph"
  verify unit "parallel and sequential loading produce the same result"
}

invariant project_qualified_id_uniqueness "Project-Qualified ID Uniqueness" {
  guarantee """
    Within a federated graph, every project-qualified entity ID
    (project::entity_id) MUST be unique. Two entities from different
    projects with the same raw entity_id MUST be disambiguated by their
    project prefix. A collision where two projects declare the same
    project prefix MUST be detected and reported as an error.
  """
  risk      high
  verify property "project-qualified IDs are unique across federated graph"
  verify unit "duplicate project prefix detected and reported as error"
}

invariant cross_project_reference_safety "Cross-Project Reference Safety" {
  guarantee """
    Cross-project reference graphs MUST NOT contain cycles across project
    boundaries. A local project graph MUST be independently valid without
    any remote project dependencies — remote projects provide additional
    context but are never required for local validation. Missing remote
    projects MUST degrade gracefully with an info diagnostic, never
    an error.
  """
  risk      medium
  verify property "no cycles exist across project boundaries"
  verify unit "local graph validates independently without remote projects"
  verify unit "missing remote project produces an info diagnostic, not an error"
}

invariant federated_graph_traversal_integrity "Federated Graph Traversal Integrity" {
  guarantee """
    Federated graph traversal operations (cross-project trace, federated
    subgraph extraction, federated export) MUST produce complete and
    deterministic results. Every reachable node across project boundaries
    along a traversal path MUST be included. Traversal order MUST be
    deterministic for identical federated graph inputs.
  """
  risk      high
  verify property "federated traversal visits every reachable cross-project node exactly once"
  verify unit "identical federated graph inputs produce identical traversal results"
}

invariant federated_schema_completeness "Federated Schema Completeness" {
  guarantee """
    The schema section of any federated Graph Protocol export MUST include
    every entity kind and edge type from all merged project graphs. No
    registered kind or edge type from any participating project MUST be
    omitted. The schema MUST accurately reflect the union of all project
    schemas.
  """
  risk      medium
  verify property "federated schema contains every kind and edge type from all merged projects"
  verify unit "newly added remote project kinds appear in federated schema"
}
