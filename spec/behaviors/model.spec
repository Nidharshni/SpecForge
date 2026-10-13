// Logical Data Model behaviors — schema-level visualization

use "events/compilation"
use "invariants/core"
use "invariants/zero-entity-core"
use "ports/inbound"
use "ports/outbound"
use "types/errors"
use "types/model"
use "types/output"

behavior build_model_intermediate "Build Model Intermediate Representation" {
  features   [logical_data_model]
  invariants [diagnostic_determinism, zero_domain_knowledge_core]
  category   query
  types      [
    ModelIntermediate,
    ModelEntity,
    ModelField,
    ModelRelationship,
    ModelExtension,
    Cardinality,
    GraphProtocolSchema,
  ]
  ports      [CompilerApi]
  consumes   [validation_complete]
  requires {
    schema_available "GraphProtocolSchema has been generated from the KindRegistry, EdgeRegistry, and FieldRegistry"
  }
  ensures {
    all_kinds_mapped            "Every entity kind in the schema produces exactly one ModelEntity in the IR"
    synthetic_id_added          "Every ModelEntity has an id field with is_primary_key=true as its first field"
    cardinality_inferred        "Every ModelRelationship has a non-null cardinality derived from the source field type"
    extension_metadata_computed "Every ModelExtension's entity_count is the kinds it declares that the model keeps, and its edge_count the edge types it declares that name a drawn relationship"
  }
  contract   """
    When the model command is invoked, the system MUST build a
    ModelIntermediate from the GraphProtocolSchema. Every SchemaEntityKind
    MUST produce one ModelEntity. A synthetic id field (string, required,
    primary_key=true) MUST be prepended to each entity's field list.
    Every SchemaEdgeType MUST produce one ModelRelationship with cardinality
    inferred from the source entity's field type: reference -> ManyToOne,
    reference_list -> ManyToMany, unknown -> ManyToMany (safe default).
    The IR MUST carry extension metadata for grouping renderers: name,
    version, the entity kinds it declares that the model keeps, and the
    edge types it declares that name at least one relationship the model
    draws. A standalone edge type (no source or target kinds) draws
    nothing and is not counted.
  """
  verify unit "every schema entity kind maps to a ModelEntity"
  verify unit "a field's type is named as its extension declares it; DBML writes its own column type"
  verify unit "synthetic id field is first field with primary_key=true"
  verify unit "reference field produces ManyToOne cardinality"
  verify unit "reference_list field produces ManyToMany cardinality"
  verify unit "unknown field type defaults to ManyToMany cardinality"
  verify unit "extension metadata has correct entity and edge counts"
  verify unit "an extension's edge count is the edge types the model draws"
  verify unit "empty schema produces empty ModelIntermediate"
  verify contract "Build Model Intermediate Representation: model IR construction holds — schema_available, all_kinds_mapped, synthetic_id_added, cardinality_inferred, extension_metadata_computed"
}

behavior render_model_markdown "Render Model as Markdown" {
  features   [model_multi_format]
  invariants [diagnostic_determinism, zero_domain_knowledge_core]
  category   query
  types      [ModelIntermediate, ModelOptions, ModelFormat]
  ports      [CompilerApi]
  requires {
    model_ir_built "ModelIntermediate has been constructed from the schema"
  }
  ensures {
    preamble_present      "Output starts with a framing preamble explaining what the model is"
    extension_summary     "Output includes an extension summary table"
    field_tables_present  "Each entity kind section has a field table (when fields != none)"
    relationships_listed  "Each entity kind section lists its relationships with cardinality"
    grouping_respected    "When group_by=extension, entities are grouped under extension headers"
    field_level_respected "Only fields matching the requested field level are included"
  }
  contract   """
    When specforge model --format=markdown is invoked, the system MUST
    render the ModelIntermediate as Markdown. The output MUST start with
    a framing preamble explaining what entity kinds, fields, and relationships
    mean. The output MUST include an extension summary table, entity kind
    sections with field tables (respecting the --fields level), and
    relationship lists with cardinality notation. When --group-by=extension,
    entity kinds MUST be grouped under extension section headers. Text the
    model carries (names, contributions, descriptions) MUST NOT split a
    table row: a pipe is escaped and a line break is a space.
  """
  verify unit "output starts with framing preamble"
  verify unit "extension summary table is present"
  verify unit "field table includes type, required, description, enum values"
  verify unit "relationships show cardinality notation"
  verify unit "group_by=extension groups entities under extension headers"
  verify unit "group_by=none lists entities flat"
  verify unit "fields=none omits field tables"
  verify unit "fields=keys shows only id, required, and reference fields"
  verify unit "fields=all shows every field"
  verify unit "empty model produces valid Markdown with zero-entity message"
  verify unit "declared text with a pipe or a line break stays in its table cell"
  verify contract "Render Model as Markdown: Markdown rendering holds — model_ir_built, preamble_present, extension_summary, field_tables_present, relationships_listed, grouping_respected, field_level_respected"
}

behavior render_model_mermaid "Render Model as Mermaid erDiagram" {
  features   [model_multi_format]
  invariants [diagnostic_determinism, zero_domain_knowledge_core]
  category   query
  types      [ModelIntermediate, ModelOptions, ModelFormat, Cardinality]
  ports      [CompilerApi]
  requires {
    model_ir_built "ModelIntermediate has been constructed from the schema"
  }
  ensures {
    valid_mermaid_produced       "Output is valid Mermaid erDiagram syntax"
    cardinality_notation_correct "Relationships use correct Mermaid cardinality notation (||, o{, etc.)"
    field_types_shown            "Entity blocks include field types when fields != none"
    grouping_via_comments        "Extension grouping uses %% comment headers"
  }
  contract   """
    When specforge model --format=mermaid is invoked, the system MUST
    render the ModelIntermediate as a Mermaid erDiagram. Cardinality MUST
    use standard Mermaid notation: ||--|| for 1:1, ||--o{ for 1:N,
    }o--|| for N:1, }o--o{ for N:M. Entity blocks MUST include field
    definitions when --fields is not none. Extension grouping MUST use
    %% @extension-name comment separators. Declared text MUST stay inside
    the Mermaid string it is written in (Mermaid's entity codes for a
    quote, `#`, `<` and `&`; a line break as a space), and an entity
    or attribute name that is not a Mermaid name MUST be written as one.
  """
  verify unit "output is valid Mermaid erDiagram syntax"
  verify unit "1:1 cardinality uses ||--|| notation"
  verify unit "1:N cardinality uses ||--o{ notation"
  verify unit "N:1 cardinality uses }o--|| notation"
  verify unit "N:M cardinality uses }o--o{ notation"
  verify unit "entity blocks include field definitions at keys and all levels"
  verify unit "fields=none produces entities without blocks"
  verify unit "extension grouping uses comment headers"
  verify unit "empty model produces valid erDiagram with no entities"
  verify unit "declared text with a quote, markup or a line break stays inside its Mermaid string"
  verify unit "an entity or attribute name that is not a Mermaid name is written as one"
  verify contract "Render Model as Mermaid erDiagram: Mermaid rendering holds — model_ir_built, valid_mermaid_produced, cardinality_notation_correct, field_types_shown, grouping_via_comments"
}

behavior render_model_dot "Render Model as DOT" {
  features   [model_multi_format]
  invariants [diagnostic_determinism, zero_domain_knowledge_core]
  category   query
  types      [ModelIntermediate, ModelOptions, ModelFormat]
  ports      [CompilerApi]
  requires {
    model_ir_built "ModelIntermediate has been constructed from the schema"
  }
  ensures {
    valid_dot_produced      "Output is valid Graphviz DOT syntax"
    html_labels_used        "Entity nodes use HTML-like <table> labels"
    extension_color_coding  "Header rows are colored by extension"
    required_fields_bolded  "Required fields are bolded in the label"
    reference_markers_shown "Reference fields show -> target markers"
    cluster_grouping        "Extension grouping uses subgraph cluster_* with dashed borders"
    edges_labeled           "Edges include relationship name and cardinality"
  }
  contract   """
    When specforge model --format=dot is invoked, the system MUST render
    the ModelIntermediate as a DOT digraph. Entity nodes MUST use HTML-like
    <table> labels with a header row colored by extension and field rows
    below. Required fields MUST be bolded. Reference fields MUST show
    a -> target_entity marker. Extension grouping MUST use subgraph
    cluster_* with dashed borders and extension-colored borders. Edges
    MUST be labeled with the relationship name and cardinality.
  """
  verify unit "output is valid Graphviz DOT syntax"
  verify unit "entity nodes use HTML-like table labels"
  verify unit "header row colored by extension"
  verify unit "required fields are bolded"
  verify unit "reference fields show -> target marker"
  verify unit "extension grouping uses subgraph clusters"
  verify unit "edges labeled with name and cardinality"
  verify unit "fields=none produces header-only nodes"
  verify unit "empty model produces valid DOT with no nodes"
  verify contract "Render Model as DOT: DOT rendering holds — model_ir_built, valid_dot_produced, html_labels_used, extension_color_coding, required_fields_bolded, reference_markers_shown, cluster_grouping, edges_labeled"
}

behavior render_model_json "Render Model as ERD JSON" {
  features   [model_multi_format]
  invariants [diagnostic_determinism, zero_domain_knowledge_core]
  category   query
  types      [ModelIntermediate, ModelOptions, ModelFormat]
  ports      [CompilerApi]
  requires {
    model_ir_built "ModelIntermediate has been constructed from the schema"
  }
  ensures {
    valid_json_produced            "Output is valid JSON"
    erd_schema_conformed           "Output follows the ERD-oriented schema (entities, relationships, cardinality)"
    distinct_from_specforge_schema "Output does NOT include compiler metadata (testable, singleton, incremental)"
    model_version_present          "Output includes a model_version field"
  }
  contract   """
    When specforge model --format=json is invoked, the system MUST
    serialize the ModelIntermediate as ERD-oriented JSON. The output MUST
    include model_version, extensions, entities (with fields filtered by
    --fields level), and relationships (with cardinality). The JSON schema
    is distinct from specforge schema output — it MUST NOT include compiler
    metadata such as testable, singleton, or incremental flags.
  """
  verify unit "output is valid JSON"
  verify unit "output includes model_version"
  verify unit "output carries the schema_version specforge schema prints for the project"
  verify unit "entities have fields with type, required, primary_key, references"
  verify unit "relationships have cardinality"
  verify unit "output does not include testable, singleton, or incremental"
  verify unit "fields level filters the field array"
  verify unit "empty model produces valid JSON with empty arrays"
  verify contract "Render Model as ERD JSON: ERD JSON rendering holds — model_ir_built, valid_json_produced, erd_schema_conformed, distinct_from_specforge_schema, model_version_present"
}

behavior render_model_dbml "Render Model as DBML" {
  features   [model_multi_format]
  invariants [diagnostic_determinism, zero_domain_knowledge_core]
  category   query
  types      [ModelIntermediate, ModelOptions, ModelFormat]
  ports      [CompilerApi]
  requires {
    model_ir_built "ModelIntermediate has been constructed from the schema"
  }
  ensures {
    valid_dbml_produced "Output is valid DBML syntax compatible with dbdiagram.io"
    table_per_entity    "Each entity kind produces one Table declaration"
    synthetic_pk        "Every table has an id string [pk] column"
    enum_definitions    "Enum fields produce standalone Enum declarations"
    table_groups        "Extension grouping produces TableGroup declarations"
    named_refs          "Each reference is one named Ref declaration, between columns the output writes"
    required_not_null   "Required fields have [not null] annotation"
  }
  contract   """
    When specforge model --format=dbml is invoked, the system MUST render
    the ModelIntermediate as DBML. Each ModelEntity MUST produce a Table
    with a synthetic id string [pk] column. Enum fields MUST produce
    standalone Enum declarations named {entity}_{field}. Required fields
    MUST have [not null] annotation. A name that is not a DBML identifier
    MUST be double-quoted, and a note MUST escape its quotes and line
    breaks. Extension grouping MUST use TableGroup declarations. Each
    reference MUST be one named Ref declaration, written only when both
    of its columns are written.
  """
  verify unit "output is valid DBML syntax"
  verify unit "each entity kind produces a Table"
  verify unit "every table has id string [pk]"
  verify unit "enum fields produce Enum declarations"
  verify unit "required fields have [not null]"
  verify unit "extension grouping uses TableGroup"
  verify unit "edge types produce named Ref declarations"
  verify unit "field descriptions use [note: '...']"
  verify unit "empty model produces valid DBML with no tables"
  verify unit "a name or note with a quote, an apostrophe or a line break stays one DBML name or string"
  verify unit "each reference is one named Ref between columns the output writes"
  verify contract "Render Model as DBML: DBML rendering holds — model_ir_built, valid_dbml_produced, table_per_entity, synthetic_pk, enum_definitions, table_groups, named_refs, required_not_null"
}

behavior filter_model "Filter Model by Extension, Kind, or Depth" {
  features   [model_filtering]
  invariants [diagnostic_determinism, zero_domain_knowledge_core]
  category   query
  types      [ModelIntermediate, ModelOptions, ModelRoot]
  ports      [CompilerApi]
  requires {
    model_ir_built "ModelIntermediate has been constructed from the schema"
  }
  ensures {
    extension_filter_applied "When --extension is set, only entities from that extension are included"
    kind_filter_applied      "When --kinds is set, only the listed entity kinds are included; an empty list includes every kind"
    depth_filter_applied     "When --root is set, only kinds within --depth hops of root in the kind adjacency graph are included"
    edges_pruned             "Relationships where source or target is filtered out are excluded"
    filters_compose          "Multiple filters are applied as intersection"
    unknown_names_answered   "A root or an extension the project does not have is refused; a listed kind it does not know is reported as I020 and selects nothing"
  }
  contract   """
    After building the ModelIntermediate, the system MUST apply any
    requested filters before rendering. --extension filters to entities
    from a single loaded extension. --kinds filters to a comma-separated
    list of entity kind names; an empty list filters nothing. --root
    builds a kind-level adjacency graph (where kinds are connected if an
    edge type exists between them) and includes only kinds within --depth
    hops of --root, every connected kind when --depth is absent; --depth
    without --root MUST be refused. Relationships where either source or
    target has been filtered out MUST be excluded. All filters compose as
    intersection. A --root no loaded extension declares MUST be refused
    as unknown_kind, naming the closest declared kind. An --extension the
    project does not load MUST be refused as extension_not_found, naming
    the loaded extension it most likely means. A kind of --kinds the
    project does not know MUST be reported as I020, naming the closest
    kind, and selects nothing. The model export itself is total: a name
    its schema does not have selects nothing. After a selection, each
    extension's entity and edge counts are recomputed by
    build_model_intermediate's rule over what is kept.
  """
  verify unit "a selection recounts each extension's entities and drawn edge types"
  verify unit "extension filter includes only matching entities"
  verify unit "extension filter excludes cross-extension edges when both endpoints not included"
  verify unit "kind filter includes only listed kinds"
  verify unit "an empty kind list selects every kind"
  verify unit "root+depth=0 includes only the root kind"
  verify unit "root+depth=1 includes root and directly connected kinds"
  verify unit "a depth without a root is refused"
  verify unit "filtered relationships exclude edges with missing endpoints"
  verify unit "multiple filters compose as intersection"
  verify unit "a root no loaded extension declares is refused with unknown_kind naming the closest declared kind"
  verify unit "an extension the project does not load is refused, naming the loaded one meant"
  verify unit "a listed kind the project does not know is reported as I020 and selects nothing"
  verify unit "a listed kind the project does not know selects nothing in the export"
  verify contract "Filter Model by Extension, Kind, or Depth: model filtering holds — model_ir_built, extension_filter_applied, kind_filter_applied, depth_filter_applied, edges_pruned, filters_compose, unknown_names_answered"
}

behavior expose_model_mcp_tool "Expose Model as MCP Tool" {
  features   [model_agent_access]
  invariants [diagnostic_determinism, zero_domain_knowledge_core]
  category   query
  types      [ModelIntermediate, ModelOptions, ModelFormat, McpToolDescriptor]
  ports      [McpProtocol, CompilerApi]
  consumes   [validation_complete]
  requires {
    validation_complete_fired "validation_complete event has fired, confirming schema is ready"
  }
  ensures {
    tool_registered       "specforge.model is registered in the MCP tool list"
    all_formats_available "All five formats are selectable via the format parameter"
    all_filters_available "extension, kinds, root, depth, group_by, fields parameters are supported"
    result_is_string      "Tool result is the rendered string in the requested format"
  }
  contract   """
    The specforge.model MCP tool MUST accept the same parameters as the
    CLI command: format (default: markdown), group_by, fields, extension,
    kinds, root, depth. The tool MUST compile the project, build the
    ModelIntermediate from the schema, apply filters, and render in the
    requested format. The result MUST be the rendered string.
    A kind of kinds the project does not know rides in the result's
    diagnostics (I020); an unknown root or extension, or a depth
    without a root, is refused as invalid input on that argument
    (extension_not_found for the extension).
  """
  verify unit "specforge.model appears in MCP tool list"
  verify unit "default format is markdown"
  verify unit "all five formats produce valid output"
  verify unit "filter parameters are passed through to model options"
  verify integration "MCP tool produces same output as CLI command"
  verify contract "Expose Model as MCP Tool: MCP model tool holds — validation_complete_fired, tool_registered, all_formats_available, all_filters_available, result_is_string"
}
