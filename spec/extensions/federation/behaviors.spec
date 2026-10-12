// Federation behaviors — cross-project references and graph merging

use "extensions/federation/events"
use "extensions/federation/invariants"
use "extensions/federation/types"
use "ports/inbound"
use "ports/outbound"
use "types/config"
use "types/diagnostics"
use "types/graph"
use "types/output"

behavior load_federation_config "Load Federation Config" {
  features   [cross_project_references]
  category   command
  invariants [cross_project_reference_safety]
  types      [ProjectDependency, FederatedGraph, Diagnostic, FederationConfig]
  ports      [FileSystem]
  produces   [federation_config_loaded]
  contract   """
    At startup, the compiler MUST parse the federation section from
    specforge.json into a list of ProjectDependency entries. Each entry
    MUST declare a project name and a local filesystem path to the
    remote project's published graph. Duplicate project names MUST
    produce a warning naming the duplicate. Entries with missing required
    fields (name or path) MUST produce an error naming the field. The
    federation extension's diagnostics are catalogued (ADR 0026) when it is
    built; this proposal names none. When no federation section
    exists, the system MUST proceed with an empty dependency list — the
    absence of federation config is not an error.
  """
  verify unit "federation section parsed from specforge.json"
  verify unit "ProjectDependency entries validated"
  verify unit "duplicate project name produces a warning"
  verify unit "missing required field produces an error"
  verify unit "absent federation section yields empty dependency list"
}

behavior resolve_cross_project_references "Resolve Cross-Project References" {
  features   [cross_project_references]
  category   command
  invariants [
    cross_project_reference_safety,
    federated_graph_traversal_integrity,
    federated_graph_determinism,
    project_qualified_id_uniqueness,
  ]
  types      [FederatedEntityId, ProjectDependency, Graph, Edge]
  produces   [cross_project_refs_resolved]
  contract   """
    When a reference contains a project::entity_id qualifier, the system MUST
    locate the remote project's published graph, resolve the entity within it,
    and create a cross-project edge in the local graph. The project prefix MUST
    match a declared dependency in the FederationConfig. Unknown project prefixes
    MUST produce a warning. Missing remote graphs MUST produce an info
    diagnostic and skip the reference without blocking local compilation.
    Cross-project reference cycles MUST be detected and rejected with
    an error naming the cycle. A cycle exists when project A references
    project B and project B (transitively) references project A.
  """
  verify unit "project::entity_id resolves to remote entity"
  verify unit "unknown project prefix produces a warning"
  verify unit "missing remote graph produces an info diagnostic"
  verify unit "cross-project edge created in local graph"
  verify unit "cross-project reference cycle detected and rejected"
}

behavior load_remote_project_graph "Load Remote Project Graph" {
  features   [cross_project_references]
  category   command
  invariants [cross_project_reference_safety]
  types      [ProjectDependency, Graph, FederatedGraph]
  ports      [FileSystem]
  produces   [remote_graph_loaded]
  contract   """
    For each declared dependency in FederationConfig, the system MUST load
    the remote project's published Graph Protocol JSON from the configured
    local filesystem path. The loaded graph MUST be cached for the duration
    of the compilation. Remote graphs MUST be loaded in parallel when
    possible. Missing paths MUST produce an info diagnostic without
    failing the local compilation. Network-based federation is not
    supported in the compiler core — registry access is a CLI-level concern.
  """
  verify unit "remote graph loaded from configured local filesystem path"
  verify unit "remote graph cached for compilation duration"
  verify unit "missing path produces an info diagnostic"
}

behavior validate_cross_project_edge_consistency "Validate Cross-Project Edge Consistency" {
  features   [cross_project_references]
  category   query
  invariants [
    cross_project_reference_safety,
    federated_graph_traversal_integrity,
    project_qualified_id_uniqueness,
  ]
  types      [FederatedGraph, Edge, SchemaCompatibility]
  produces   [cross_project_edges_validated]
  contract   """
    After cross-project edges are created, the system MUST validate edge type
    compatibility between the local and remote schemas. If the remote project
    uses a different schema version, the system MUST check compatibility via
    SchemaCompatibility. Incompatible edge types MUST produce a warning.
    Compatible edge types from different schema versions MUST be normalized
    to the local schema. When a dependency uses a newer major schema version
    than the local project, the system MUST produce a warning. Edges
    from forward-incompatible projects MUST be excluded from the merged
    graph with a diagnostic explaining the version mismatch.
  """
  verify unit "compatible edge types validated successfully"
  verify unit "incompatible edge types produce a warning"
  verify unit "edge types normalized to local schema version"
  verify unit "newer major schema version produces a warning"
  verify unit "edges from forward-incompatible projects excluded with diagnostic"
}

behavior export_federated_graph "Export Federated Graph" {
  features   [federated_graph_export]
  category   command
  invariants [
    federated_graph_traversal_integrity,
    federated_schema_completeness,
    federated_graph_determinism,
    project_qualified_id_uniqueness,
  ]
  types      [FederatedGraph, Graph, OutputFile, FederatedExportConfig, FederationMetadata]
  ports      [CompilerApi]
  produces   [federated_graph_exported]
  contract   """
    When specforge export --federated is invoked, the system MUST merge the
    local graph with all loaded remote project graphs into a FederatedGraph.
    The merged graph MUST include cross-project edges. Entity IDs in the
    merged graph MUST be qualified with project prefixes to prevent collisions.
    The output MUST conform to the Graph Protocol schema with a federation
    metadata section as defined in the Graph Protocol schema.
    Federation metadata (merged_at, source_projects, schema_compatible)
    MUST conform to the open Graph Protocol JSON Schema specification —
    it is not a compiler-specific addition.
    The federated export MUST support --format=context|graph|brief with
    the same semantics as non-federated export. Context format includes
    full contract text; graph format is the raw JSON graph; brief format
    is a token-optimized summary. The federated export MUST respect
    --max-tokens, truncating by project priority (as declared in
    FederationConfig dependency order) then by entity priority within
    each project.
    When include_projects or exclude_projects filters are specified in
    FederatedExportConfig, the export MUST apply them before graph merging.
    A depth of 0 MUST export only the local project. Unknown project names
    in filters MUST emit a warning.
  """
  verify unit "federated export supports context, graph, and brief formats"
  verify unit "federated export respects max-tokens with project then entity priority"
  verify unit "federated export merges local and remote graphs"
  verify unit "cross-project edges included in merged graph"
  verify unit "entity IDs qualified with project prefixes"
  verify unit "output conforms to Graph Protocol schema"
  verify unit "include_projects filter limits merged projects"
  verify unit "exclude_projects filter removes specified projects"
  verify unit "depth of 0 exports only local project"
  verify unit "unknown project name in filter emits a warning"
}
