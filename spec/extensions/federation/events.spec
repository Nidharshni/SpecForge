// Federation events — signals emitted during cross-project operations

use "extensions/federation/types"
use "types/config"

event federation_config_loaded "Federation Config Loaded" {
  channel "federation.federation_config_loaded"
  payload FederationConfigLoadedPayload
  verify integration "emits federation_config_loaded with correct dependencyCount and projectNames"
}

type FederationConfigLoadedPayload {
  dependencyCount integer
  projectNames    string[]
  timestamp       timestamp
  verify unit "FederationConfigLoadedPayload schema is valid"
}

event remote_graph_loaded "Remote Graph Loaded" {
  channel "federation.remote_graph_loaded"
  payload RemoteGraphLoadedPayload
  // consumers are extension points — federation consumers will be registered
  // by extensions that react to remote graph loading (e.g., caching, analytics)
  verify integration "emits remote_graph_loaded with correct projectName and graphNodeCount"
}

type RemoteGraphLoadedPayload {
  projectName    string
  graphNodeCount integer
  schemaVersion  string
  timestamp      timestamp
  verify unit "RemoteGraphLoadedPayload schema is valid"
}

event cross_project_refs_resolved "Cross-Project Refs Resolved" {
  channel "federation.cross_project_refs_resolved"
  payload CrossProjectRefsResolvedPayload
  // consumers are extension points — federation consumers will be registered
  // by extensions that react to reference resolution (e.g., reporting, dashboards)
  verify integration "emits cross_project_refs_resolved with correct resolvedCount and unresolvedCount"
}

type CrossProjectRefsResolvedPayload {
  resolvedCount   integer
  unresolvedCount integer
  timestamp       timestamp
  verify unit "CrossProjectRefsResolvedPayload schema is valid"
}

event cross_project_edges_validated "Cross-Project Edges Validated" {
  channel "federation.cross_project_edges_validated"
  payload CrossProjectEdgesValidatedPayload
  // consumers are extension points — federation consumers will be registered
  // by extensions that react to edge validation (e.g., compatibility reporting)
  verify integration "emits cross_project_edges_validated with correct compatibleCount and incompatibleCount"
}

type CrossProjectEdgesValidatedPayload {
  compatibleCount   integer
  incompatibleCount integer
  normalizedCount   integer
  timestamp         timestamp
  verify unit "CrossProjectEdgesValidatedPayload schema is valid"
}

event federated_graph_exported "Federated Graph Exported" {
  channel "federation.federated_graph_exported"
  payload FederatedGraphExportedPayload
  // consumers are extension points — federation consumers will be registered
  // by extensions that react to federated exports (e.g., CI hooks, notifications)
  verify integration "emits federated_graph_exported with correct totalNodes, totalEdges, and projectCount"
}

type FederatedGraphExportedPayload {
  totalNodes   integer
  totalEdges   integer
  projectCount integer
  timestamp    timestamp
  verify unit "FederatedGraphExportedPayload schema is valid"
}
