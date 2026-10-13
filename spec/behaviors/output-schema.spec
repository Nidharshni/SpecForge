// Output behaviors — schema generation, versioning, MCP graph serving, embeddings

use "events/compilation"
use "invariants/core"
use "invariants/extensions"
use "invariants/validation"
use "invariants/zero-entity-core"
use "ports/inbound"
use "ports/outbound"
use "types/graph"
use "types/output"
use "types/zero-entity-core"

// ── Self-Describing Graph Protocol Schema ─────────────────────────────

behavior generate_schema_from_registries "Generate Schema From Registries" {
  features   [self_describing_graph_protocol]
  invariants [
    graph_schema_completeness,
    diagnostic_determinism,
    registry_population_before_validation,
    zero_domain_knowledge_core,
  ]
  category   query
  types      [
    GraphProtocolSchema,
    SchemaEntityKind,
    SchemaEdgeType,
    SchemaField,
    KindRegistryEntry,
    FieldRegistryEntry,
    RegistryBuild,
    SchemaExtensionInfo,
  ]
  ports      [CompilerApi]
  consumes   [registries_populated]
  produces   [schema_generated]
  requires {
    registries_populated_fired "registries_populated event has fired, confirming KindRegistry and FieldRegistry are fully populated from all extension manifests"
  }
  ensures {
    all_kinds_in_schema      "Every registered entity kind produces a SchemaEntityKind entry in the schema"
    all_edges_in_schema      "Every registered edge type produces a SchemaEdgeType entry in the schema"
    schema_cached            "Schema is generated once per compilation cycle and cached for reuse"
    schema_generated_emitted "schema_generated event is emitted after schema construction completes"
  }
  contract   """
    After all extension registries are populated, the system MUST serialize
    the KindRegistry and FieldRegistry into a GraphProtocolSchema object.
    Each registered entity kind MUST produce a SchemaEntityKind entry with
    its fields, testable flag, and singleton flag. Each registered edge type
    MUST produce a SchemaEdgeType entry.
    Each SchemaEdgeType names the ends the registry build resolved for
    its edge type (registry_build_edges): the declared kind on a side it
    declares, else the kinds of the fields that write it (sources) and
    the target kinds they declare (targets), sorted; a side nothing
    resolves is absent. The schema is generated from the registry build
    alone, its extensions included.
    The schema MUST be generated once
    per compilation and cached for reuse by embed_schema_in_export and
    serve_schema_resource. In watch mode or LSP mode, the schema MUST be
    regenerated on each compilation cycle — "once per compilation" means
    once per compile cycle, not once for the lifetime of the process. The
    cache MUST be invalidated at the start of each new compilation cycle.
    Each SchemaField is its registered field's declaration: its type is
    the field type the extension declares, named as the extension
    protocol names it (string, integer, bool, enum, string_list,
    reference, reference_list, block), an enum field carries its declared
    values, and a declared default value is carried.
  """
  verify unit "schema includes all registered entity kinds"
  verify unit "schema includes all registered edge types"
  verify unit "schema fields match FieldRegistry entries"
  verify unit "a field's declared default value reaches the schema"
  verify unit "a field's type is named in the schema as its extension declares it"
  verify unit "schema generated once per compilation and cached"
  verify unit "zero extensions produces valid empty schema"
  verify contract "Generate Schema From Registries: schema generation from registries holds — registries_populated_fired, all_kinds_in_schema, all_edges_in_schema, schema_cached, schema_generated_emitted"
  verify unit "SchemaVersion Display"
  verify unit "SchemaVersion FromStr"
  verify unit "SchemaVersion Ord"
  verify unit "populated schema serde round-trip"
  verify unit "an edge type that declares no source or target kind names those of the fields that write it"
  verify unit "an edge type several kinds write without declaring it names every writing kind"
}

behavior embed_schema_in_export "Embed Schema in Export" {
  features   [self_describing_graph_protocol]
  invariants [
    graph_traversal_integrity,
    graph_schema_completeness,
    diagnostic_determinism,
    zero_domain_knowledge_core,
    schema_version_backward_compatibility,
  ]
  category   query
  types      [Graph, GraphProtocolSchema, OutputFile]
  ports      [CompilerApi]
  // Barrier-join: schema pipeline must complete (schema_version_computed) AND
  // validation must complete (validation_complete) before export embedding.
  consumes   [schema_version_computed, validation_complete]
  requires {
    schema_version_computed_fired "schema_version_computed event has fired, confirming schema version is determined"
    validation_complete_fired     "validation_complete event has fired, confirming the graph is ready for export"
  }
  ensures {
    schema_embedded              "Full-graph exports embed GraphProtocolSchema as a top-level schema key in the JSON output"
    format_version_set           "format_version is set to 2.0 when schema or schema_ref is present, 1.0 when suppressed"
    schema_ref_names_full_schema "Scoped exports carry a schema_ref (url + content_hash) naming the full-project published schema instead of embedding it"
  }
  contract   """
    When exporting the full graph as JSON (specforge export or specforge
    render json), the system MUST embed the GraphProtocolSchema as a
    top-level "schema" key in the output, placed before the "nodes" and
    "edges" keys. The format_version field MUST be set to "2.0" when the
    schema is present or referenced. The --no-schema flag MUST suppress
    schema embedding for backward compatibility, in which case
    format_version remains "1.0". A scoped extraction MUST NOT embed the
    full schema — it MUST instead carry a "schema_ref" object holding the
    published schema's url and content_hash, so agents retain access to the
    broader context (the full-project schema is retrievable by that
    reference) without taping the whole schema into every scoped letter.
    Every JSON export, the schema attached or not, MUST carry the
    schema_version of the project's versioned schema; no export carries a
    version the project's schema is not at.
  """
  verify unit "schema embedded as top-level key in full JSON export"
  verify unit "format_version set to 2.0 with schema"
  verify unit "--no-schema suppresses schema and keeps format_version 1.0"
  verify unit "scoped exports carry schema_ref (url and content_hash) instead of embedded schema"
  verify unit "every JSON export carries the project's schema version, whether the schema is attached or not"
  verify contract "Embed Schema in Export: schema embedding in export holds — schema_version_computed_fired, validation_complete_fired, schema_embedded, format_version_set, schema_ref_names_full_schema"
}

behavior persist_schema_cache "Persist Schema Cache" {
  features   [graph_protocol_versioning]
  invariants [
    graph_schema_completeness,
    diagnostic_determinism,
    schema_version_backward_compatibility,
    zero_domain_knowledge_core,
  ]
  category   command
  types      [GraphProtocolSchema, SchemaCacheEntry]
  ports      [FileSystem]
  consumes   [schema_generated]
  produces   [schema_cache_persisted]
  requires {
    schema_generated_fired "schema_generated event has fired, confirming the GraphProtocolSchema is constructed and ready for persistence"
  }
  ensures {
    cache_written_atomically       "Schema cache file is written atomically via temp file then rename"
    schema_cache_persisted_emitted "schema_cache_persisted event is emitted after successful cache write"
  }
  contract   """
    After the GraphProtocolSchema is generated, the system MUST persist it
    to `.specforge/schema-cache.json` for use by detect_breaking_schema_changes
    in subsequent compilations. The cache file MUST be overwritten atomically
    (write to temp, then rename). `specforge check` writes no files, so the
    cache is not updated by a check.
  """
  verify unit "schema-cache.json written after schema generation"
  verify unit "cache file overwritten atomically via temp+rename"
  verify integration "persisted cache feeds breaking change detection in the next compilation"
  verify contract "Persist Schema Cache: schema cache persistence holds — schema_generated_fired, cache_written_atomically, schema_cache_persisted_emitted"
  verify unit "content hash changes"
}

behavior serve_schema_resource "Serve Schema Resource" {
  features   [self_describing_graph_protocol]
  invariants [
    graph_traversal_integrity,
    graph_schema_completeness,
    diagnostic_determinism,
    zero_domain_knowledge_core,
  ]
  category   command
  types      [GraphProtocolSchema, SchemaEntityKind]
  ports      [CompilerApi]
  consumes   [validation_complete]
  requires {
    validation_complete_fired "validation_complete event has fired, confirming the schema reflects the current compilation state"
  }
  ensures {
    full_schema_output     "specforge schema outputs the complete GraphProtocolSchema as JSON"
    kind_filter_supported  "Optional --kind filter restricts the schema to that entity kind and the edge types that can start or end at it"
    mcp_resource_available "In MCP server mode, schema is available as specforge://schema resource"
  }
  contract   """
    When specforge schema is invoked, the system MUST output the
    GraphProtocolSchema as JSON to stdout. An optional --kind filter MUST
    restrict the schema to that entity kind and the edge types that can
    start or end at it: the document the specforge.schema tool returns for
    the same kind. --no-edges MUST leave the edge types out and
    --validation-rules MUST add the validation rules the loaded extensions
    declare, as include_edges and include_validation_rules do on the tool.
    In MCP server mode,
    the schema MUST be available as the specforge://schema resource for
    agent introspection. The schema MUST always reflect the current
    compilation state.
  """
  verify unit "specforge schema outputs full schema as JSON"
  verify unit "--kind filter restricts to single entity kind"
  verify unit "MCP resource specforge://schema returns schema"
  verify unit "schema reflects current compilation state"
  verify unit "--no-edges and --validation-rules select what include_edges and include_validation_rules select"
  verify contract "Serve Schema Resource: schema resource serving holds — validation_complete_fired, full_schema_output, kind_filter_supported, mcp_resource_available"
  verify unit "missing kind error"
}

// ── MCP Graph Resources (Principle 3: agents are first-class consumers) ──

// This behavior provides the data layer for MCP resource exposure. The MCP
// behaviors (expose_graph_as_mcp_resource, expose_context_as_mcp_resource,
// expose_brief_as_mcp_resource) delegate to this behavior for graph serialization.
// Forward-ref: MCP transport-level resource registration is in
// behaviors/mcp-server.spec (expose_graph_as_mcp_resource, etc.).
// Cross-feature: contributes to agent_export feature (features/output.spec).
behavior serve_graph_resource "Serve Graph Resource via MCP" {
  features   [agent_export, mcp_resource_exposure]
  invariants [
    graph_traversal_integrity,
    graph_schema_completeness,
    diagnostic_determinism,
    zero_domain_knowledge_core,
  ]
  category   command
  types      [Graph, GraphProtocolSchema, AgentExportConfig, DiagnosticSummary]
  ports      [CompilerApi, McpProtocol]
  consumes   [validation_complete]
  produces   [graph_resource_served]
  requires {
    validation_complete_fired "validation_complete event has fired, confirming graph data is available for MCP resource serving"
    mcp_server_available      "MCP server mode is active and ready to serve resources"
  }
  ensures {
    three_formats_served          "specforge://graph, specforge://context, and specforge://brief resources are all served"
    scope_parameter_supported     "All three resources support a scope query parameter for subgraph extraction"
    schema_embedded_in_resources  "Resources follow the export schema policy: the full graph embeds the GraphProtocolSchema, a scoped graph references it"
    error_resource_on_failure     "Compilation failure returns an error resource with DiagnosticSummary instead of empty resource"
    graph_resource_served_emitted "graph_resource_served event is emitted after resources are served"
  }
  contract   """
    In MCP server mode, the system MUST expose graph data as MCP resources
    for agent introspection without requiring CLI invocation. The
    specforge://graph resource MUST return the full Graph Protocol JSON
    (equivalent to specforge export --format=graph). The
    specforge://context resource MUST return the token-optimized context
    format (equivalent to specforge export --format=context). The
    specforge://brief resource MUST return the minimal brief format
    (equivalent to specforge export --format=brief). All three resources
    MUST support a scope query parameter (root is accepted as its alias) to
    restrict output to a subgraph rooted at a specific entity (e.g.,
    specforge://context?scope=auth_login), with depth, kinds and max_tokens as
    specforge export takes them. Query keys and values are percent-decoded; an
    unknown key, a repeated key, scope with root, a scope on a templated URI,
    a value that does not parse, or depth without a scope is refused as
    invalid input naming the key, never ignored; a count is read as a tool's
    count argument is (a non-negative integer, with its wording).
    Resources MUST follow the export schema policy through the function
    specforge export uses: specforge://graph embeds the GraphProtocolSchema
    for the full graph, carries a schema_ref when scoped, and leaves the
    schema out under max_tokens, as context and brief do; every graph
    document carries a schema_version field. Resources MUST reflect the current compilation
    state and update when the graph changes. When compilation fails, the
    server MUST return an error resource containing a diagnostic summary
    instead of an empty resource. The error resource MUST include the
    DiagnosticSummary and a top-level "error" key set to true. When zero
    extensions are installed, resources MUST return the structural-only
    graph containing raw keyword strings as entity kinds.
  """
  verify unit "specforge://graph returns full Graph Protocol JSON"
  verify unit "specforge://context returns token-optimized format"
  verify unit "specforge://brief returns minimal format"
  verify unit "scope query parameter restricts to subgraph"
  verify unit "root is accepted as an alias of the scope query parameter"
  verify unit "an unknown query key, a repeated key or a malformed value is invalid_input naming the key"
  verify unit "a count in a resource query is read as a tool's count argument is"
  verify integration "specforge://graph under max_tokens stays within the budget, as the budgeted export does"
  verify unit "resources reflect current compilation state"
  verify unit "compilation failure returns error resource with diagnostic summary"
  verify contract "Serve Graph Resource via MCP: MCP graph resource serving holds — validation_complete_fired, mcp_server_available, three_formats_served, scope_parameter_supported, schema_embedded_in_resources, error_resource_on_failure, graph_resource_served_emitted"
}

// ── Graph Protocol Versioning ─────────────────────────────────────

behavior negotiate_schema_version "Negotiate Schema Version" {
  features   [graph_protocol_versioning]
  invariants [
    graph_schema_completeness,
    diagnostic_determinism,
    schema_version_backward_compatibility,
    zero_domain_knowledge_core,
  ]
  category   command
  types      [SchemaVersion, SchemaCompatibility, GraphProtocolSchema, ExportResult]
  ports      [CompilerApi]
  consumes   [validation_complete, schema_breaking_change_detected]
  produces   [schema_version_negotiated]
  requires {
    validation_complete_fired             "validation_complete event has fired, confirming the graph is ready for version-negotiated export"
    schema_breaking_change_detected_fired "schema_breaking_change_detected event has fired, confirming breaking change classification is complete"
  }
  ensures {
    compatible_version_resolved       "Requested version within supported range is resolved to nearest compatible version"
    incompatible_version_rejected     "Incompatible version request produces E027 diagnostic with supported range"
    default_to_latest                 "When no version is requested, the latest supported version is used"
    schema_version_negotiated_emitted "schema_version_negotiated event is emitted after negotiation completes"
  }
  contract   """
    When an agent requests a specific Graph Protocol schema version, the
    system MUST check the requested version against the supported compatibility
    range (supported_min to supported_max). If compatible, the system MUST
    resolve the nearest compatible version and use it for the export. If
    incompatible, the system MUST emit an E027 diagnostic with the supported
    range and reject the request. The default behavior when no version is
    requested MUST use the latest supported version. Consumers MAY request
    a specific schema version via the --schema-version CLI flag or the
    schema_version query parameter in MCP resource URIs. A requested
    version is negotiated whatever the export's format and whether the
    schema is attached.
  """
  verify unit "compatible version within range is resolved"
  verify unit "incompatible version produces E027 with supported range"
  verify unit "no version requested defaults to latest"
  verify unit "--schema-version CLI flag selects requested version"
  verify unit "schema_version MCP query parameter selects requested version"
  verify unit "a requested version is negotiated whatever the format, the schema attached or not"
  verify contract "Negotiate Schema Version: schema version negotiation holds — validation_complete_fired, schema_breaking_change_detected_fired, compatible_version_resolved, incompatible_version_rejected, default_to_latest, schema_version_negotiated_emitted"
}

behavior detect_breaking_schema_changes "Detect Breaking Schema Changes" {
  features   [graph_protocol_versioning]
  invariants [
    graph_schema_completeness,
    diagnostic_determinism,
    schema_version_backward_compatibility,
    zero_domain_knowledge_core,
  ]
  category   validation
  types      [
    SchemaVersion,
    SchemaMigration,
    GraphProtocolSchema,
    SchemaCacheEntry,
    SchemaMigrationChange,
  ]
  ports      [CompilerApi, FileSystem]
  // Reads the previous schema from .specforge/schema-cache.json written by the
  // PRIOR compilation. Does NOT depend on persist_schema_cache in the current
  // compilation — it reads from disk, not from the current event stream.
  consumes   [schema_generated]
  produces   [schema_breaking_change_detected]
  requires {
    schema_generated_fired "schema_generated event has fired, confirming the current schema is available for comparison"
    filesystem_available   "FileSystem port is available for reading .specforge/schema-cache.json from prior compilation"
  }
  ensures {
    breaking_changes_classified             "Removed entity kinds, changed edge semantics, and new required fields are classified as breaking"
    nonbreaking_changes_classified          "Added optional fields and new entity kinds are classified as non-breaking"
    migration_record_emitted                "SchemaMigration record is emitted describing the changes"
    schema_breaking_change_detected_emitted "schema_breaking_change_detected event is emitted after classification completes"
  }
  contract   """
    When the Graph Protocol schema version changes between compilations, the
    system MUST detect breaking changes by comparing the previous and current
    schemas. Removed entity kinds, changed edge type semantics, and new
    required fields MUST be classified as breaking. Added optional fields and
    new entity kinds MUST be classified as non-breaking. The system MUST emit
    a SchemaMigration record describing the changes. Breaking changes MUST
    only occur on major version increments. The previous schema version MUST be
    read from `.specforge/schema-cache.json` (maintained by persist_schema_cache),
    or from the most recent Graph Protocol export file in the output directory
    as a fallback. If no previous schema exists, all changes are treated as
    non-breaking (initial export). The `.specforge/schema-cache.json` file is
    authoritative. The output directory export is a fallback source used only
    when the cache file does not exist. When no previous schema is found and
    the project has been exported before (output directory contains prior
    exports), the system SHOULD emit an I016 info diagnostic indicating the
    schema cache was not found and breaking change detection was skipped.
    A field type is compared as the type, not its spelling: a cache an
    older host wrote with an accepted older name (`boolean`) reads as the
    same type, so it is no change.
  """
  verify unit "removed entity kind detected as breaking"
  verify unit "added optional field detected as non-breaking"
  verify unit "new required field detected as breaking"
  verify unit "SchemaMigration record emitted on version change"
  verify unit "no previous schema treats all changes as non-breaking"
  verify unit "missing cache with prior exports emits I016 info diagnostic"
  verify unit "missing cache without prior exports emits no diagnostic"
  verify unit "new entity kind detected as non-breaking"
  verify unit "new edge type detected as non-breaking"
  verify unit "removed edge type detected as breaking"
  verify unit "previous schema is read from .specforge/schema-cache.json"
  verify unit "missing schema cache yields no previous schema"
  verify contract "Detect Breaking Schema Changes: breaking schema change detection holds — schema_generated_fired, filesystem_available, breaking_changes_classified, nonbreaking_changes_classified, migration_record_emitted, schema_breaking_change_detected_emitted"
  verify unit "identical schemas no changes"
  verify unit "removed field is breaking"
  verify unit "a cache an older host wrote reads its field types unchanged"
}

behavior compute_schema_version "Compute Schema Version" {
  features   [graph_protocol_versioning]
  invariants [
    graph_schema_completeness,
    diagnostic_determinism,
    schema_version_backward_compatibility,
    zero_domain_knowledge_core,
  ]
  category   query
  types      [SchemaVersion, SchemaMigration, GraphProtocolSchema, SchemaCacheEntry]
  ports      [CompilerApi]
  // compute_schema_version depends on breaking change classification from
  // detect_breaking_schema_changes, not the raw schema_generated event.
  consumes   [schema_breaking_change_detected]
  produces   [schema_version_computed]
  requires {
    schema_breaking_change_detected_fired "schema_breaking_change_detected event has fired, confirming breaking change classification is available for version computation"
  }
  ensures {
    version_auto_computed           "Version bump is computed automatically: major for breaking, minor for new kinds/edges, patch for metadata"
    first_compilation_baseline      "First compilation without a cache produces version 1.0.0"
    version_attached                "Computed version is attached to the GraphProtocolSchema before export"
    schema_version_computed_emitted "schema_version_computed event is emitted after version computation completes"
  }
  contract   """
    After the schema is generated, the system MUST compare the current schema
    against the previous schema from .specforge/schema-cache.json. The version
    bump MUST be computed automatically: major for breaking changes (as
    classified by detect_breaking_schema_changes), minor for new entity kinds
    or edge types, patch for non-structural metadata changes. First compilation
    without a cache MUST produce version 1.0.0. The computed version MUST be
    attached to the GraphProtocolSchema before export.
  """
  verify unit "first compilation without cache produces version 1.0.0"
  verify unit "new entity kind triggers minor version bump"
  verify unit "removed entity kind triggers major version bump"
  verify unit "field metadata change triggers patch version bump"
  verify unit "new edge type triggers minor version bump"
  verify unit "removed edge type triggers major version bump"
  verify unit "new required field triggers major version bump"
  verify contract "Compute Schema Version: schema version computation holds — schema_breaking_change_detected_fired, version_auto_computed, first_compilation_baseline, version_attached, schema_version_computed_emitted"
  verify unit "added optional field bumps minor"
  verify unit "no changes returns previous"
  verify unit "removed field bumps major"
}

behavior publish_schema_specification "Publish Schema Specification" {
  features   [graph_protocol_versioning]
  invariants [
    graph_schema_completeness,
    diagnostic_determinism,
    registry_api_openness,
    zero_domain_knowledge_core,
  ]
  category   command
  types      [GraphProtocolSchema, OutputFile]
  ports      [CompilerApi, FileSystem]
  consumes   [schema_version_computed, validation_complete]
  produces   [render_complete]
  requires {
    schema_version_computed_fired "schema_version_computed event has fired, confirming the schema version is determined"
    validation_complete_fired     "validation_complete event has fired, confirming all entity kinds and edge types are registered"
  }
  ensures {
    valid_json_schema_produced "Output is a valid JSON Schema document (draft 2020-12 or later)"
    all_kinds_described        "Published schema describes all registered entity kinds and edge types"
    third_party_usable         "Published schema is usable by any JSON Schema validator to validate Graph Protocol exports"
    render_complete_emitted    "render_complete event is emitted after schema publication"
  }
  contract   """
    When specforge schema --publish is invoked, the system MUST export the
    current GraphProtocolSchema as a standalone JSON Schema document suitable
    for third-party consumers. The output MUST be a valid JSON Schema
    (draft 2020-12 or later) that describes the Graph Protocol format
    including all registered entity kinds, edge types, and field definitions.
    The published schema MUST be usable by any JSON Schema validator to
    validate Graph Protocol exports. The schema MUST include a $schema
    meta-reference identifying the JSON Schema draft version used.
    The published schema describes the export format --format names
    (graph, context or brief; json is graph). --format applies only with
    --publish, and --kind cannot be combined with --publish: either is
    refused as a usage error (exit 2).
  """
  verify unit "published schema is valid JSON Schema"
  verify unit "published schema describes all registered entity kinds"
  verify unit "published schema describes all edge types"
  verify unit "third-party validator can use published schema"
  verify unit "published schema validates known-good export"
  verify unit "the published schema lists the field types the host reads"
  verify unit "published schema requires the Graph Protocol top-level properties"
  verify unit "published context schema admits declared headline and normative fields"
  verify unit "--kind with --publish, and --format without it, are refused"
  verify unit "a context or brief export validates against the published schema of its format"
  verify unit "an export of a project with no errors validates against the published schema"
  verify integration "this repository's and its example projects' exports validate against their published schemas"
  verify unit "the published schema is the export document's schema narrowed by the project"
  verify unit "the published schema uses only keywords the in-repo checker reads"
  verify contract "Publish Schema Specification: schema specification publication holds — schema_version_computed_fired, validation_complete_fired, valid_json_schema_produced, all_kinds_described, third_party_usable, render_complete_emitted"
  verify unit "description includes version"
}

// Entity embeddings moved to spec/extensions/embeddings/behaviors.spec
