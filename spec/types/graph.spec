// Graph types — in-memory graph data shapes
//
// Node is the graph-level representation of an Entity. It has the same
// fields as Entity (from types/core) because the graph stores the full
// entity data at each node. Entity is the parser output; Node is the
// graph node. They are structurally identical but semantically distinct:
// Entity belongs to a SpecFile, Node belongs to a Graph.

use "types/core"
use "types/diagnostics"

type Graph {
  schema_version string @readonly
  nodes          Node[]
  edges          Edge[]
  file_index     FileIndex
  verify unit "Graph schema is valid"
}

type Node {
  id          EntityId   @readonly @unique
  kind        EntityKind
  title       string
  fields      FieldMap
  source_span SourceSpan @readonly
  verify unit "Node schema is valid"
}

type Edge {
  source    EntityId @readonly
  target    EntityId @readonly
  edge_type EdgeType
  verify unit "Edge schema is valid"
}

// EdgeType is an open string — not a closed enum.
// Extensions declare edge types via edgeTypes and field-to-edge mappings
// in their manifests. The core only knows edge labels are strings.
type EdgeType {
  label string @readonly
  verify unit "EdgeType schema is valid"
}

type FileIndex {
  files FileEntry[]
  verify unit "FileIndex schema is valid"
}

type FileEntry {
  path     string @readonly
  entities EntityId[]
  imports  string[]
  verify unit "FileEntry schema is valid"
}

type Subgraph {
  node_ids EntityId[]
  edges    Edge[]
  verify unit "Subgraph schema is valid"
}

type TraceChain {
  root  EntityId
  links TraceLink[]
  verify unit "TraceChain schema is valid"
}

type TraceLinkStatus = "resolved" | "missing"

type TraceLink {
  from      EntityId
  to        EntityId
  edge_type EdgeType
  depth     integer
  status    TraceLinkStatus
  verify unit "TraceLink schema is valid"
}

// ── Graph Protocol Schema Types ──────────────────────────────

type GraphProtocolSchema {
  schema_version SchemaVersion @readonly
  extensions     SchemaExtensionInfo[]
  entity_kinds   SchemaEntityKind[]
  edge_types     SchemaEdgeType[]
  verify unit "GraphProtocolSchema schema is valid"
}

type SchemaExtensionInfo {
  name    string @readonly
  version string @readonly
  verify unit "SchemaExtensionInfo schema is valid"
}

type SchemaEntityKind {
  name              string  @readonly
  extension         string  @readonly
  testable          boolean
  singleton         boolean
  fields            SchemaField[]
  has_body_parser   boolean @optional
  grammar_extension string  @optional
  verify unit "SchemaEntityKind schema is valid"
}

type SchemaField {
  name          string    @readonly
  field_type    FieldType @readonly
  required      boolean
  target_kind   string    @optional
  enum_values   string[]  @optional
  default_value string    @optional
  verify unit "SchemaField schema is valid"
}

type SchemaEdgeType {
  label        string   @readonly
  source_kinds string[] @optional
  target_kinds string[] @optional
  extension    string   @readonly
  verify unit "SchemaEdgeType schema is valid"
}

// ── Graph Protocol Versioning ──────────────────────────────

type SchemaVersion {
  major integer @readonly
  minor integer @readonly
  patch integer @readonly
  label string  @optional @readonly
  verify unit "SchemaVersion schema is valid"
}

type SchemaMigration {
  from_version SchemaVersion           @readonly
  to_version   SchemaVersion           @readonly
  breaking     boolean
  changes      SchemaMigrationChange[] @optional
  verify unit "SchemaMigration schema is valid"
}

type SchemaMigrationChange {
  change_type string @readonly // "kind_added" | "kind_removed" | "edge_added" | "edge_removed" | "field_added" | "field_removed"
  entity_kind string @optional
  edge_type   string @optional
  field_name  string @optional
  breaking    boolean
  verify unit "SchemaMigrationChange schema is valid"
}

type SchemaCompatibility {
  requested     SchemaVersion @readonly
  supported_min SchemaVersion @readonly
  supported_max SchemaVersion @readonly
  compatible    boolean
  // Sourced from CompilerConfig.supported_schema_min / .supported_schema_max (see ADR graph_protocol_version_management)
  source        string        @readonly
  verify unit "SchemaCompatibility schema is valid"
}

// ── Graph Delta Types ────────────────────────────────────────

// GraphDelta is what an update changed in the graph: watch prints it
// (watch --format json) and MCP sends it whole as specforge/graphChanged's
// params. It is NOT part of the stable Graph Protocol schema (not included
// in embed_schema_in_export output); its structure may evolve between
// compiler versions without a Graph Protocol major version bump.
// Node arrays are sorted by id, edge arrays by (source, target, label).
type GraphDelta {
  added_nodes    NodeChange[]
  removed_nodes  NodeChange[]
  modified_nodes ModifiedNodeChange[]
  added_edges    EdgeChange[]
  removed_edges  EdgeChange[]
  /// The files of the added, removed and modified nodes, sorted.
  affected_files string[]
  verify unit "GraphDelta schema is valid"
}

type NodeChange {
  id   string  @readonly
  kind string  @readonly
  file string  @readonly
  line integer @readonly
  verify unit "NodeChange schema is valid"
}

type ModifiedNodeChange {
  id              string       @readonly
  /// The fields whose values differ, by name, sorted: a field named kind or
  /// methods is a field here.
  changed_fields  string[]
  /// What else differs, sorted: its kind, title, methods or outgoing edges.
  changed_aspects NodeAspect[]
  file            string       @readonly
  line            integer      @readonly
  verify unit "ModifiedNodeChange schema is valid"
}

type NodeAspect = "kind" | "title" | "methods" | "edges"

/// One edge in a GraphDelta, as the graph export writes an edge.
type EdgeChange {
  source string @readonly
  target string @readonly
  label  string @readonly
  verify unit "EdgeChange schema is valid"
}

// The diagnostics one update added and removed, each as every surface
// presents a diagnostic (present_diagnostics_as_json); MCP computes it and
// sends it as specforge/diagnosticsChanged's params.
type DiagnosticsDelta {
  added   Diagnostic[]
  removed Diagnostic[]
  verify unit "DiagnosticsDelta schema is valid"
}

// ── Agent Plan Types ──────────────────────────────────────

type AgentPlan {
  plan_id string @readonly @unique
  entries AgentPlanEntry[]
  verify unit "AgentPlan schema is valid"
}

type AgentPlanEntry {
  entity_id    string   @readonly
  action       string
  dependencies string[] @optional
  verify unit "AgentPlanEntry schema is valid"
}
