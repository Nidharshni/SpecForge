// Output types for rendering and emitting

type OutputFile "Output File" {
  path     string       @readonly
  content  string       @readonly
  checksum string       @readonly
  format   OutputFormat @readonly @optional
  verify unit "OutputFile schema is valid"
}

type OutputFormat "Output Format" = "json" | "dot" | "context" | "brief" | "graph" | "markdown"

type DiagnosticFormat "Diagnostic Format" = "text" | "json"

type AgentExportConfig "Agent Export Config" {
  format      OutputFormat @readonly
  scope       string       @readonly @optional
  depth       integer      @readonly @optional
  kind_filter string[]     @readonly @optional
  max_tokens  integer      @readonly @optional
  verify unit "AgentExportConfig schema is valid"
}

// ── Token Economics ───────────────────────────────────────

type TokenBudgetStrategy "Token Budget Strategy" = "prioritize"

type TokenBudgetResult "The `token_budget` block of an export that did not fit its budget whole." {
  strategy           TokenBudgetStrategy
  budget_tokens      integer
  estimated_tokens   integer
  /// The IDs of the entities dropped to fit, least central first.
  truncated_entities string[]
  verify unit "TokenBudgetResult schema is valid"
}

// Entity embedding types moved to spec/extensions/embeddings/types.spec

type DiagnosticSummary "Diagnostic Count Summary" {
  error_count integer
  warn_count  integer
  info_count  integer
  verify unit "Diagnostic Count Summary conforms to schema"
}

// ── Graph Annotations ──────────────────────────────────────
// Graph Protocol nodes MAY carry annotations prefixed with _ (underscore).
// Annotations are metadata computed by the compiler or extensions, not
// declared in .spec files. They are included in export output for agent
// consumption.

type GraphAnnotation "Graph Annotation" {
  key        string @readonly // Always starts with "_" (e.g., "_similarity")
  value_type string @readonly // JSON type: "number", "string", "boolean", "object"
  source     string @readonly // "core" or extension name that produced it
  verify unit "GraphAnnotation schema is valid"
}

// ── Schema Cache ───────────────────────────────────────────

type SchemaCacheEntry "Schema Cache Entry" {
  schema       GraphProtocolSchema @readonly
  content_hash string              @readonly // SHA-256 of the schema's JSON
  verify unit "SchemaCacheEntry schema is valid"
}

type ExportResult "Export Result" {
  format               OutputFormat
  entity_count         integer
  edge_count           integer
  // semver format: "MAJOR.MINOR.PATCH"
  schema_version       string   @optional
  token_budget_applied boolean  @optional
  truncated_entities   string[] @optional
  verify unit "ExportResult schema is valid"
}
