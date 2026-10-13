// Output and serialization features
//
// P7 Justification — DOT stays in core:
// DOT is the Graph Protocol's native visual representation, analogous to
// how JSON is its data representation. The serialize_dot_visualization
// behavior contains zero domain knowledge: it walks generic graph nodes and
// edges, delegating all entity-kind-aware rendering (shapes, styles) to
// extension-provided callbacks via render_extension_defined_dot_shapes and
// render_extension_defined_edge_styles. Adding a new domain (e.g.,
// @specforge/compliance) requires ZERO compiler changes for DOT output —
// the extension's manifest declares dot_shape and edge_style, and the core
// DOT serializer reads them from the KindRegistry. This is identical to
// how the formatting engine (features/formatting.spec) is core despite
// formatting extension-defined entity blocks. DOT is a diagnostic/debugging
// format for the graph engine, not a domain-specific output.

use "behaviors/output"
use "behaviors/output-schema"

// markdown_documentation_generation moved to spec/extensions/markdown-renderer/
// Vision: "SpecForge does not produce documentation." Markdown rendering is
// a renderer contribution from the @specforge/markdown-renderer extension.

feature json_and_dot_render "JSON and DOT Render" {
  status   done
  // render_extension_defined_dot_shapes is owned by extension_driven_visualization
  // in zero-entity-core.spec; serialize_dot_visualization delegates to it.
  problem  """
    External tools (dashboards, analyzers, visualizers) need machine-readable
    access to the spec graph. The graph must be exportable in standard formats.
  """
  solution """
    specforge render json exports the full graph as JSON conforming to the
    Graph Protocol schema — the versioned, open interchange format that any
    tool or agent framework can consume. specforge render dot exports a DOT
    format graph compatible with Graphviz for visualization.
  """
}

// Note: specforge export (--format=context|graph|brief) is defined in product/capabilities.spec as an agent-facing command distinct from specforge render

feature traceability_serialization "Traceability Serialization" {
  status   done
  problem  """
    Architects and auditors need to trace entities upstream and downstream
    through the graph. Manual traceability matrices are error-prone and
    always stale. AI agents produce implementation plans that must be
    validated against the spec graph to ensure completeness and consistency.
  """
  solution """
    specforge trace computes and serializes traceability chains by traversing the
    graph across all registered edge types. Single-entity trace shows
    upstream and downstream links. Full trace shows every chain with gap
    detection. specforge trace --plan validates agent implementation plans
    against the graph, ensuring every referenced entity exists, every testable
    entity is covered, and dependency ordering is consistent.

    This closes the P5 feedback loop: declare intent (verify declarations and extension-declared test file fields) →
    trace gaps (specforge trace) → implement → collect proof
    (specforge collect) → trace again to confirm coverage.
  """
}

feature agent_export "Agent-Optimized Export" {
  status   done
  problem  """
    AI agents need structured, token-efficient context from the spec graph.
    Full JSON export wastes context window. Agents need scoped, multi-resolution
    access to exactly the subgraph relevant to their task. Agents operating
    in MCP server mode need resource-based access without CLI invocation.
  """
  solution """
    specforge export provides agent-optimized formats: context (token-optimized
    with contracts and relationships), brief (IDs and contracts only), and
    graph (full JSON). specforge query provides multi-resolution access via
    --scope, --depth, and --kind parameters for precise context slicing.
    In MCP server mode, specforge://graph, specforge://context, and
    specforge://brief resources provide the same formats as MCP resources
    for direct agent consumption.
  """
}

feature ci_integration "CI Integration" {
  status   done
  problem  """
    CI pipelines need a single command that validates all .spec files,
    exits with an appropriate code, and produces deterministic output
    suitable for automated checks.
  """
  solution """
    specforge check parses, resolves, and validates without writing files;
    only --cache writes, recording the build's statuses in
    specforge-cache.json for history rules such as status transitions.
    Exit code 0 for clean, 1 for errors. --strict treats warnings as errors.
    --format=json produces machine-readable diagnostic output for CI tools
    and AI agents. Output is deterministic for reproducible CI runs.

    Bridge: diagnostic_reporting (features/validation.spec) defines the
    diagnostic infrastructure that ci_integration consumes. Diagnostic codes,
    severity levels, and structured output format are shared between both
    features via the DiagnosticBag type.
  """
}

feature self_describing_graph_protocol "Self-Describing Graph Protocol" {
  status   done
  problem  """
    The Graph Protocol JSON export contains nodes and edges but no schema
    describing what entity kinds exist, what fields they have, or what edge
    types connect them. Agents consuming the graph must rely on external
    documentation or heuristics to understand the graph structure, reducing
    first-attempt accuracy.
  """
  solution """
    Self-describing schema embedded in Graph Protocol exports. After all
    extension registries are populated, the compiler serializes the
    KindRegistry and FieldRegistry into a GraphProtocolSchema object.
    Each registered entity kind produces a SchemaEntityKind entry; each
    registered edge type produces a SchemaEdgeType entry. The schema is
    generated once per compilation and embedded as a top-level "schema"
    key in JSON exports (format_version 2.0). specforge schema CLI command
    and specforge://schema MCP resource provide standalone schema access
    for agent introspection. The schema dynamically reflects whichever
    extensions are installed — no static maintenance required.
  """
}

feature graph_protocol_versioning "Graph Protocol Versioning" {
  status   done
  problem  """
    The Graph Protocol schema evolves as extensions add entity kinds and edge
    types, but there is no versioning mechanism. Agents consuming the graph
    cannot negotiate compatible versions, and breaking changes are undetectable.
  """
  solution """
    Semantic versioning for the Graph Protocol schema, derived from the
    extensions: each export versions the schema against the one the last
    export recorded in .specforge/schema-cache.json, comparing every
    attribute. Additions bump the minor version, documentation and
    provenance the patch version, and anything a reader of the recorded
    schema relies on that is removed or reshaped (removed kinds, edge types
    or fields, a new required field, a changed field type, required flag,
    edge type or target kind, a lost enum value or edge end) the major
    version, warned as W053. One version names one document. A consumer
    may name the version it reads (--schema-version, the export tool's
    schema_version): it is admitted when a reader of that version reads the
    current schema, and refused otherwise. specforge schema --publish
    exports the schema as a standalone JSON Schema (draft 2020-12) for
    third-party validation. See also: spec_file_migration feature for
    migration tooling.
  """
}

// plan_validation removed — validate_agent_plan is already covered by
// the traceability_serialization feature above.

// entity_embedding_search moved to spec/extensions/embeddings/features.spec
