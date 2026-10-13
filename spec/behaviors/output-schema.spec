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
  types      [GraphProtocolSchema, SchemaCacheEntry, VersionedSchema]
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
    After a successful `specforge export` (every format), the system MUST
    record the versioned schema it exported against, with its content hash,
    in `.specforge/schema-cache.json`, for detect_breaking_schema_changes
    and compute_schema_version on the next read. The cache file MUST be
    overwritten atomically (write to temp, then rename). Only `specforge
    export` records: `specforge check`, `watch`, `schema` and every MCP
    tool and resource only read the cache. A failed export leaves it as it
    was.
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
  types      [SchemaVersion, SchemaCompatibility, GraphProtocolSchema, ExportResult, VersionedSchema]
  ports      [CompilerApi]
  consumes   [validation_complete, schema_breaking_change_detected]
  produces   [schema_version_negotiated]
  requires {
    validation_complete_fired             "validation_complete event has fired, confirming the graph is ready for version-negotiated export"
    schema_breaking_change_detected_fired "schema_breaking_change_detected event has fired, confirming breaking change classification is complete"
  }
  ensures {
    compatible_version_resolved       "A requested version whose readers read the current schema (the same major version, not newer) is admitted, and the export carries the current schema at its own version"
    incompatible_version_rejected     "A requested version of another major, or newer than the current schema, is refused as incompatible_schema_version naming the range of versions whose readers read the current schema"
    default_to_latest                 "When no version is requested, the export carries the current schema version"
    schema_version_negotiated_emitted "schema_version_negotiated event is emitted after negotiation completes"
  }
  contract   """
    A consumer MAY name the schema version it reads, with --schema-version
    on specforge export or schema_version on the specforge.export tool. The
    system MUST admit it when a reader of that version reads the current
    schema's documents: the same major version and not newer than the
    current version. An admitted request changes nothing in the export: it
    carries the current schema at its own version, never relabelled, so a
    schema version names one document. A request of another major version
    or a newer one MUST be refused as incompatible_schema_version, naming
    the current version and the readable range (MAJOR.0.0 to the current
    version); a version that does not parse is invalid_schema_version. The
    request is checked whatever the format, including formats that carry no
    schema.
  """
  verify unit "a requested version of the same major, not newer, is admitted and the export carries the current version"
  verify unit "a requested version of another major, or a newer one, is refused as incompatible_schema_version naming the readable range"
  verify unit "no version requested defaults to latest"
  verify unit "--schema-version and the export tool's schema_version are checked whatever the format"
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
  types      [SchemaVersion, VersionedSchema, GraphProtocolSchema, SchemaCacheEntry, SchemaChange]
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
    breaking_changes_classified             "Every change that removes something a reader of the recorded schema relies on is classified as breaking (major): a removed kind, edge type or field, a field added as required, a field's type, required flag, edge type or target kind changing, an enum value lost, an edge type's source or target kind lost"
    nonbreaking_changes_classified          "Additions are minor (a kind, an edge type, an optional field, an enum value, an edge type's source or target kind) and documentation and provenance are patch (descriptions, defaults, colours, the testable flag, source extensions, extension versions, load order)"
    migration_record_emitted                "The versioned schema lists each change since the recorded schema, in schema order"
    schema_breaking_change_detected_emitted "schema_breaking_change_detected event is emitted after classification completes"
  }
  contract   """
    The system MUST compare the schema the loaded extensions produce with
    the one recorded in `.specforge/schema-cache.json`, attribute by
    attribute: every attribute of the schema, its extensions, kinds, fields
    and edge types, so that any difference is a change. Kinds, fields and
    edge types are matched by name; the order the record holds them in is
    no change. Within a major version a reader of an earlier version reads
    every later document, so a set the schema publishes (kinds, edge types,
    a kind's fields, an enum field's values, an edge type's source and
    target kinds) may grow in a minor version and shrinks only in a major
    one, and a single-valued attribute that shapes a document (a field's
    type, required flag, edge type and target kind) changes only in a major
    version. A field added as required is breaking. Descriptions, default
    values, DOT colours, the testable flag, the extension that contributed
    a kind, field or edge type, extension versions and their load order are
    patch changes. When a field's type changes, its enum values are not
    compared as well. Each breaking change is reported as one W053 warning
    naming the change and the version the schema goes from and to; the
    export is still written. With no cache there is no previous schema and
    nothing is reported. A cache that exists but cannot be read (not JSON,
    not a schema, a field type this host does not read) MUST be reported as
    I016: the version starts again at 1.0.0. A field type is compared as the
    type, not its spelling: a cache an older host wrote with an accepted
    older name (`boolean`) reads as the same type, so it is no change.
  """
  verify unit "removed entity kind detected as breaking"
  verify unit "added optional field detected as non-breaking"
  verify unit "new required field detected as breaking"
  verify unit "the versioned schema lists each change since the recorded schema"
  verify unit "no previous schema treats all changes as non-breaking"
  verify unit "a missing schema cache emits no diagnostic"
  verify unit "new entity kind detected as non-breaking"
  verify unit "new edge type detected as non-breaking"
  verify unit "removed edge type detected as breaking"
  verify unit "previous schema is read from .specforge/schema-cache.json"
  verify unit "missing schema cache yields no previous schema"
  verify unit "a field's required flag changing either way is breaking"
  verify unit "an edge type losing a source or target kind is breaking; gaining one is an addition"
  verify unit "an unreadable schema cache restarts the version at 1.0.0 with I016"
  verify unit "every attribute of the schema is compared: a change to any one moves the version"
  verify unit "an enum field losing a value is breaking; gaining one is an addition"
  verify unit "a field's edge type or target kind changing is breaking"
  verify unit "the W053 message names the change and the versions"
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
  types      [SchemaVersion, VersionedSchema, GraphProtocolSchema, SchemaCacheEntry]
  ports      [CompilerApi]
  // compute_schema_version depends on breaking change classification from
  // detect_breaking_schema_changes, not the raw schema_generated event.
  consumes   [schema_breaking_change_detected]
  produces   [schema_version_computed]
  requires {
    schema_breaking_change_detected_fired "schema_breaking_change_detected event has fired, confirming breaking change classification is available for version computation"
  }
  ensures {
    version_auto_computed           "The version is the recorded one bumped by the largest change since: major for a breaking change, minor for an addition, patch for documentation and provenance"
    first_compilation_baseline      "First compilation without a cache produces version 1.0.0"
    version_attached                "Computed version is attached to the GraphProtocolSchema before export"
    schema_version_computed_emitted "schema_version_computed event is emitted after version computation completes"
  }
  contract   """
    The system MUST version the generated schema against the schema the
    last export recorded in .specforge/schema-cache.json: the recorded
    version bumped by the largest change since, as detect_breaking_schema_changes
    classifies each one (major resets minor and patch, minor resets patch).
    With no change the version is the recorded one. Without a cache, or
    with one that cannot be read, the version MUST be 1.0.0. The computed
    version MUST be attached to the GraphProtocolSchema before export, and
    every reader of the project's schema (specforge schema, specforge
    schema --publish, specforge.schema, specforge://schema, every export)
    MUST report the same version for the same record. One schema version
    names one document: two schemas versioned against one record with the
    same version have the same content hash.
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
  verify unit "an extension's version changing is a patch"
  verify unit "one schema version names one document: equal versions have equal content hashes"
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
    third_party_usable         "Published schema is usable by any JSON Schema validator to validate Graph Protocol exports: an export of a project with no errors validates against the published schema of its format"
    render_complete_emitted    "render_complete event is emitted after schema publication"
  }
  contract   """
    When specforge schema --publish is invoked, the system MUST export a
    standalone JSON Schema document (draft 2020-12 or later) suitable for
    third-party consumers, with a $schema meta-reference identifying the
    draft.

    The published schema MUST be the JSON Schema of the export document
    itself, derived from the types that write it, narrowed by the
    project: each kind an export can hold (every declared kind, and the
    structural ref and spec) is one node branch whose fields are its
    declared fields' wire forms, any other field a field value, and the
    embedded schema's edge type labels are the registered ones. It MUST
    use only keywords the in-repo checker reads (type, enum, properties,
    required, additionalProperties, items, oneOf, anyOf, minimum, $ref,
    $defs) and the annotations $schema, $id, title, description and
    examples, so it is usable by any JSON Schema validator and checked
    exactly by SpecForge's own. An export of a project whose check reports
    no error MUST validate against the published schema of its format.
    Each format's document has its own $id.
    A node of every format MAY carry coverage_status (covered, partial
    or uncovered), the status a query with coverage gives each entity;
    the published schema of every format admits it.
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
  verify unit "the published schema of every format admits a node's coverage_status"
  verify contract "Publish Schema Specification: schema specification publication holds — schema_version_computed_fired, validation_complete_fired, valid_json_schema_produced, all_kinds_described, third_party_usable, render_complete_emitted"
  verify unit "description includes version"
}

// Entity embeddings moved to spec/extensions/embeddings/behaviors.spec
