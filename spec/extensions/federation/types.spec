// Federation types — cross-project reference and graph merging

use "types/graph"

type ProjectDependency "Project Dependency" {
  name     string @readonly
  // Relative to the directory containing specforge.json
  path     string @optional
  // registry is resolved at CLI level (specforge add/install),
  // not by the compiler core. The compiler only uses local paths.
  registry string @optional
  // Semver constraint when from registry, git ref when from git, ignored for local path
  version  string @optional
  verify unit "ProjectDependency schema is valid"
}

type FederatedEntityId "Federated Entity Id" {
  project   string @readonly
  entity_id string @readonly
  verify unit "FederatedEntityId schema is valid"
}

// Project identifiers: lowercase alphanumeric with hyphens, 2-64 chars
// The qualified form is "project::entity_id"

type FederationConfig "Federation Config" {
  dependencies ProjectDependency[]
  verify unit "FederationConfig schema is valid"
}

type FederationMetadata "Federation Metadata" {
  merged_at         timestamp
  source_projects   string[]
  schema_compatible boolean
  verify unit "FederationMetadata schema is valid"
}

type FederatedGraph "Federated Graph" {
  local               Graph   @readonly
  remote_graphs       Graph[] @optional
  remote_projects     ProjectDependency[]
  cross_edges         Edge[]
  schema_version      string
  federation_metadata FederationMetadata
  verify unit "FederatedGraph schema is valid"
}

type FederatedExportConfig "Federated Export Config" {
  include_projects string[]                      @optional
  exclude_projects string[]                      @optional
  /// Default: unlimited (all transitive dependencies)
  depth            integer                       @optional
  format           "context" | "graph" | "brief" @optional
  max_tokens       integer                       @optional
  verify unit "FederatedExportConfig schema is valid"
}
