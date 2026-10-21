// Output behaviors — serialization and export of the spec graph

use "events/compilation"
use "features/zero-entity-core"
use "invariants/core"
use "invariants/validation"
use "invariants/zero-entity-core"
use "ports/inbound"
use "ports/outbound"
use "types/diagnostics"
use "types/errors"
use "types/graph"
use "types/output"
use "types/zero-entity-core"

defaults behavior {
  features [agent_export]
  types    [Graph, Diagnostic]
}

// render_markdown_documentation moved to spec/extensions/markdown-renderer/behaviors.spec

behavior serialize_json_graph "Serialize JSON Graph" {
  features   [json_and_dot_render]
  invariants [
    graph_traversal_integrity,
    diagnostic_determinism,
    graph_schema_completeness,
    zero_domain_knowledge_core,
  ]
  category   query
  types      [Graph, GraphProtocolSchema, OutputFile, EmitterError, OutputFormat]
  ports      [GraphSerializer, FileSystem]
  consumes   [validation_complete]
  produces   [render_complete]
  requires {
    validation_complete_fired "validation_complete event has fired, confirming all diagnostics are collected and the graph is ready for serialization"
  }
  ensures {
    all_nodes_serialized    "JSON output contains exactly one entry per graph node"
    all_edges_serialized    "JSON output contains exactly one entry per graph edge"
    schema_version_present  "Output includes a schema_version field identifying the Graph Protocol version"
    valid_json_produced     "Output is valid JSON parseable by standard tools"
    render_complete_emitted "render_complete event is emitted after successful serialization"
  }
  contract   """
    When specforge render json is invoked, the system MUST serialize
    the entire in-memory graph to JSON conforming to the Graph Protocol
    schema. The JSON MUST contain all nodes, edges, and metadata. The
    output MUST include a schema_version field identifying the Graph
    Protocol version. The output MUST be valid JSON parseable by
    standard tools. Every field value MUST be written in its one wire form
    (ADR 0064): text as a string, an expression group as its conjuncts in
    their display form, one per line, an integer or boolean as a JSON
    scalar, a list as an array, a block as an object of its entries; an entity's
    verify statements as {kind, description} objects under the node's own
    `verify`, beside its fields. No position and no parse tree
    is ever written.
  """
  verify unit "JSON output contains all nodes"
  verify unit "JSON output contains all edges"
  verify unit "output is valid JSON"
  verify unit "output includes schema_version field"
  verify unit "empty graph produces valid JSON with empty nodes and edges arrays"
  verify unit "schema is included even for empty graph"
  verify integration "structural-only graph (zero extensions) produces valid Graph Protocol JSON with raw keywords in kind field"
  verify unit "an expression group is written as its conjuncts, one per line, never as a parse tree"
  verify contract "Serialize JSON Graph: JSON graph serialization holds — validation_complete_fired, all_nodes_serialized, all_edges_serialized, schema_version_present, valid_json_produced, render_complete_emitted"
}

// P7 Justification: DOT serialization is domain-agnostic graph visualization.
// It walks generic nodes and edges, delegating all kind-aware rendering
// (shapes, styles) to extensions via render_extension_defined_dot_shapes
// and render_extension_defined_edge_styles. See features/output.spec for
// the full P7 rationale.
behavior serialize_dot_visualization "Serialize DOT Visualization" {
  features   [json_and_dot_render]
  invariants [graph_traversal_integrity, diagnostic_determinism, zero_domain_knowledge_core]
  category   query
  types      [Graph, OutputFile, EmitterError]
  ports      [GraphSerializer, FileSystem]
  consumes   [validation_complete]
  produces   [render_complete]
  requires {
    validation_complete_fired "validation_complete event has fired, confirming the graph is finalized and ready for visualization"
  }
  ensures {
    valid_dot_produced      "Output is valid Graphviz DOT syntax"
    nodes_labeled           "All nodes are labeled with entity IDs and titles"
    edges_labeled           "All edges are labeled with edge types"
    render_complete_emitted "render_complete event is emitted after successful DOT serialization"
  }
  contract   """
    When specforge render dot is invoked, the system MUST emit a DOT format
    graph compatible with Graphviz. Nodes MUST be labeled with entity
    IDs and titles. Edges MUST be labeled with edge types. Node shape
    rendering MUST delegate to render_extension_defined_dot_shapes
    (behaviors/zero-entity-core.spec) which reads the dot_shape field
    from the KindRegistry entry for each entity kind. If no dot_shape
    is specified, the default shape MUST be "box".
  """
  verify unit "DOT output is valid Graphviz syntax"
  verify unit "nodes are labeled with IDs"
  verify unit "edges are labeled with types"
  verify unit "node shapes use extension-defined dot_shape"
  verify contract "Serialize DOT Visualization: DOT visualization holds — validation_complete_fired, valid_dot_produced, nodes_labeled, edges_labeled, render_complete_emitted"
  verify unit "clusters group by declaring extension"
  verify unit "kind filter drops other kinds"
  verify unit "labels toggle emits bare IDs"
}

// The extension outline: specforge outline and MCP specforge.outline_extensions.
behavior render_extension_outline "Render the Extension Outline" {
  features   [extension_driven_visualization]
  invariants [diagnostic_determinism, zero_domain_knowledge_core]
  category   query
  types      [ExtensionDeclaration]
  ports      [CompilerApi]
  requires {
    declarations_loaded "the project's extension declarations are loaded"
  }
  ensures {
    one_card_per_extension  "each loaded extension is one card naming its version, the describe categories it declares, and providers when its handshake says so"
    declared_text_contained "text an extension declares stays inside the label, string or table cell it is written in, in every format"
    deps_selected           "the dependencies every format shows, json included, are the ones --deps selects"
  }
  contract   """
    When specforge outline (or MCP specforge.outline_extensions) is
    invoked, the system MUST render one card per loaded extension, with
    its dependencies and enhancements, as markdown, mermaid, dot or json.
    The --deps level (direct, effective: direct and used transitive,
    full: every transitive) selects the dependencies every format shows,
    json included.
    Text an extension declares (its name, version, kind keywords, field
    names) MUST be written through the escaping of the syntax it sits in:
    Mermaid's entity codes in a label, the record escapes in a DOT record
    field, an escaped pipe in a Markdown cell. An extension name used as
    an identifier MUST be reduced to a bare one.
  """
  verify unit "declared text with a quote, markup or a line break stays inside its Mermaid label"
  verify unit "declared text with a pipe or a line break stays in its Markdown table cell"
  verify unit "the builtins' outline is unchanged in every format"
  verify unit "each extension's card names the describe categories it declares, and providers when its handshake says so"
  verify unit "the dependencies every format shows, json included, are the ones --deps selects"
  verify unit "an enhancement of a kind no extension declares names no owner"
  verify contract "Render the Extension Outline: outline rendering holds — declarations_loaded, one_card_per_extension, declared_text_contained, deps_selected"
}

behavior compute_traceability_chain "Compute Traceability Chain" {
  features   [traceability_serialization]
  invariants [graph_traversal_integrity, diagnostic_determinism, zero_domain_knowledge_core]
  category   query
  types      [Graph, TraceChain, TraceLink, TraceLinkStatus]
  ports      [CompilerApi]
  consumes   [validation_complete]
  produces   [trace_chain_computed]
  requires {
    validation_complete_fired "validation_complete event has fired, confirming entity references are resolved and the graph is traversable"
  }
  ensures {
    full_chain_traversed         "Trace covers both upstream and downstream connections from the target entity"
    missing_links_flagged        "Expected edges from extension manifests that are not instantiated are flagged with missing status"
    trace_chain_computed_emitted "trace_chain_computed event is emitted after successful traversal"
  }
  contract   """
    When specforge trace is invoked with an entity ID, the system MUST
    traverse the graph both upstream and downstream from that entity
    following all registered edge types. The trace MUST show the full
    chain of connected entities regardless of their kind. Missing links
    in expected edges (as declared by extension manifests) MUST be flagged.
    A TraceLink with status "missing" indicates an edge type registered
    in an extension manifest that is NOT instantiated between the two
    entities in the current graph. This distinguishes from broken
    references (E003), which are caught during resolution.
    A relation is written on its owning field only (ADR 0150): its other
    side is the target's upstream, under the owning field's label.
    The JSON trace output MUST carry the Graph Protocol schema_version.
    Tracing an entity the graph does not have MUST be E003, naming the
    closest entity when one is near.
    Each direction is walked breadth-first: a link's depth is its distance
    from the traced entity, and its edge label is the label of the edge it
    was first reached over, nearer entities first and each entity's edges
    in the order they are written. Links are listed by depth, then id.
  """
  verify unit "trace from entity shows upstream and downstream connections"
  verify unit "trace shows full chain depth"
  verify unit "missing link in chain is flagged"
  verify unit "a relation written on its owning field shows on the other side as upstream, under the owning field's label"
  verify unit "trace output includes schema version"
  verify unit "tracing an entity the graph lacks is E003 naming the closest entity"
  verify unit "a reference field another extension adds to a kind is not an expected link"
  verify unit "a link carries the label of the edge it was first reached over, nearer entities first and each entity's edges in written order"
  verify contract "Compute Traceability Chain: traceability chain computation holds — validation_complete_fired, full_chain_traversed, missing_links_flagged, trace_chain_computed_emitted"
}

// CLI query command (specforge stats). Read-only: produces []. Output is terminal,
// not a pipeline signal.
behavior compute_project_statistics "Compute Project Statistics" {
  invariants [diagnostic_determinism, testable_entity_classification, zero_domain_knowledge_core]
  category   query
  types      [Graph, KindRegistryEntry, McpStatsResult, McpEntityCount, McpDiagnosticSummary]
  consumes   [validation_complete]
  features   [extension_driven_coverage]
  requires {
    validation_complete_fired "validation_complete event has fired, confirming graph state is finalized for statistics computation"
  }
  ensures {
    entity_counts_produced "Statistics include entity counts grouped by kind"
    coverage_computed      "Coverage percentage is computed over testable entities only"
    zero_testable_safe     "Coverage is reported as 0% when testable_entity_count is zero, not as a division error"
  }
  contract   """
    When specforge stats is invoked, the system MUST compute and display:
    entity counts by kind, the declared percentage, the proof percentage
    when tests are recorded, the unconnected entity count (the entities no edge
    links to another entity, read_views_over_the_project_view), and diagnostic
    summary. Statistics MUST be derived from the current
    graph state. Coverage percentage MUST be computed only over entity
    kinds with testable=true in the KindRegistry, not over all entities,
    and without the entities W004 exempts that declare no obligations
    (union types, abstract entities, governance kinds): the coverage
    pass's testable count.
    An entity is "declared" when it has at least one verify statement.
    The declared percentage is declared testable entities /
    testable_entity_count. When testable_entity_count is zero,
    it MUST be reported as 0%, not as a division error. When the project
    has recorded test results (specforge-report.json), stats also reports
    the proof percentage: the share of testable entities the coverage
    rule proves, the analyze coverage --min gate's figure. Without
    results it is absent; a report that exists but cannot be read is an
    error.
    specforge stats --format json prints the statistics document
    specforge.stats answers (McpStatsResult): the entity count and the
    counts by kind, the edge count, the unconnected, testable, declared and
    verified counts, the declared and proof percentages and the diagnostic
    summary by severity; an absent proof percentage is no key, never null.
  """
  verify unit "stats reports correct entity counts"
  verify unit "stats reports coverage percentage"
  verify unit "stats reports the unconnected entity count"
  verify unit "stats reports diagnostic summary"
  verify unit "coverage is 0% when testable_entity_count is zero"
  verify unit "stats leaves the entities W004 exempts out of the testable count"
  verify unit "the stats document has no proof_pct without recorded test results"
  verify integration "stats reports the declared and proof percentages"
  verify contract "Compute Project Statistics: project statistics computation holds — validation_complete_fired, entity_counts_produced, coverage_computed, zero_testable_safe"
}

// The read views are operations over the project view: one per view,
// shared by the CLI and MCP.
behavior read_views_over_the_project_view "Read Views over the Project View" {
  features   [mcp_core_tools, extension_driven_coverage, traceability_serialization]
  invariants [diagnostic_determinism, testable_entity_classification, zero_domain_knowledge_core]
  category   query
  types      [Graph, KindRegistryEntry, GraphProtocolSchema, TraceChain]
  ports      [CompilerApi, McpProtocol]
  requires {
    project_compiled "A compiled project, one-shot or a project session's, supplies the project view"
  }
  ensures {
    one_report_rule        "The recorded test report and the schema cache are the view root's, never an ancestor's"
    one_coverage_per_state "Coverage is computed once per compile and per recorded report content"
    surfaces_agree         "The CLI, MCP and the LSP hover report the same numbers, chains, schema version, diagrams and entity facts for one project"
  }
  contract   """
    Stats, trace (one entity or every entity), the coverage view, the
    model and outline diagrams, the versioned Graph Protocol schema,
    inspect (one entity's facts: its kind, standing, headline, references,
    coverage and the diagnostics about it), query, list and search
    (the entities a selection over the view returns), the exploration
    (explore_the_graph), the review (review_coverage_gaps) and the
    inference guide and plan (compute_inference_guide,
    provide_infer_plan_scope) MUST each be one operation over the project
    view, shared by the surfaces that show them (the CLI, MCP's tools and
    prompts, and the LSP hover and keyword completion); a surface maps its
    arguments and renders the outcome. A read selection selects what its
    entity reaches within its depth (every entity without one), of the kinds
    it keeps; the entity itself is selected only when its kind is kept, and
    an export or a query draws it always (ADR 0190). A kind the project knows is one a
    loaded extension declares or an entity is written with; names are
    exact. A kind filter that names another kind matches nothing and is
    reported as I020; an argument that needs a kind's declaration refuses
    an undeclared one (unknown_kind). Both name the closest kind, a kind
    equal but for case first. The
    view's root is the root the project was compiled from: its recorded
    test report is <root>/specforge-report.json and its schema cache
    <root>/.specforge/schema-cache.json, and no view looks in an ancestor
    directory. A report that exists but cannot be used is the same error on
    every view that reads it (E045), of the kind the operation decides: schema_mismatch
    when it is not a specforge-report.json, permission_denied when the system
    refuses to read it, invalid_input over MCP when it is a link that leads
    outside the call's project (MCP reads it only from inside; the CLI and the
    LSP follow the user's link), else internal_error. A schema
    cache that cannot be read is I016 and versioning starts again at 1.0.0; over
    MCP one that leads outside the call's project or has more than one name is
    not read and I016 names why, never what it holds (the CLI and the LSP follow
    the user's link). The view says what its surface reports for the
    project: what specforge check reports for the compile behind it, then
    what the surface adds (MCP: I017). Coverage is computed once per
    compiled project or session state and per content of the recorded
    report; a rewritten report is read again. A view reads the recorded
    report only for what the report decides (what an entity's tests prove);
    who is testable and what an entity declares are its standing in the
    entity snapshot, so the plan check and inspect's facts read no report.
    An entity is unconnected when no edge links it to another entity, in
    either direction: a reference that does not resolve is no edge (E003
    or I004 reports it), and an edge from an entity to itself links it to
    nothing else. Stats counts the unconnected entities, the exploration
    lists them and the review flags those that count toward coverage, by
    this one rule. The entities of each kind are counted once, for every
    kind an entity is written with. A coverage row is one JSON document
    on every surface that lists rows.
    An entity is unverified when it counts toward coverage and is not
    proven.
  """
  verify unit "the recorded test report is read at the view's root, never an ancestor's"
  verify unit "a view reports what its compile reported, then what its surface adds"
  verify unit "the schema cache is the view root's, never an ancestor's"
  verify unit "coverage is computed once per compile and report content, and again after the report changes"
  verify unit "an entity is unverified when it counts toward coverage and is not proven"
  verify unit "inspect reports an entity's standing as the coverage view counts it"
  verify unit "a report that cannot be read is the coverage's error, and the standing still holds"
  verify unit "an unusable report is the same failure, of the kind the operation decides, on every view"
  verify unit "the CLI and the LSP read the recorded report where its link points"
  verify unit "over MCP, a recorded report that leads outside the project is the same refusal on every view"
  verify unit "over MCP, a schema cache that leads outside the project or has more than one name is not read"
  verify unit "the CLI and the LSP read the schema cache where its link points"
  verify integration "specforge stats --format json and specforge.stats answer the same document"
  verify integration "specforge trace and specforge.trace return the same chain for an entity"
  verify integration "specforge schema and specforge.schema carry the same version"
  verify integration "specforge schema --kind and specforge.schema with a kind return the same document"
  verify integration "specforge outline and specforge.outline_extensions render the same text"
  verify integration "specforge.inspect and the LSP hover report the same facts for an entity"
  verify integration "specforge query and specforge.query return the same document for an entity"
  verify unit "an entity is unconnected when no edge links it to another entity: a reference that does not resolve or names the entity itself links nothing"
  verify unit "the entities of each kind are counted once, for every kind an entity is written with"
  verify unit "a coverage row is one JSON document on every surface"
  verify unit "a kind filter reports each kind the project does not know with I020, naming the closest"
  verify unit "a read selection selects what its entity reaches within its depth, of its kinds; the entity is selected only when its kind is kept, and an export draws it always"
  verify unit "an argument naming an undeclared kind is refused with unknown_kind naming the closest declared kind"
  verify unit "the reach of some entities is every entity their trace chains reach, the seeds included, and its unverified entities are those that count toward coverage and are not proven"
  verify contract "Read Views over the Project View: read views hold — project_compiled, one_report_rule, one_coverage_per_state, surfaces_agree"
}

// The coverage view (specforge_ops::read::coverage): one row per entity, by the
// coverage rule, over the project view. specforge.coverage renders it.
behavior compute_the_coverage_view "Compute the Coverage View" {
  features   [extension_driven_coverage, mcp_core_tools]
  invariants [diagnostic_determinism, testable_entity_classification]
  category   query
  types      [CoverageStatus]
  ports      [CompilerApi]
  requires {
    project_compiled "A compiled project, one-shot or a project session's, supplies the project view"
  }
  ensures {
    rows_are_what_counts "With no filter, the rows are the entities that count toward coverage"
    status_by_the_rule   "A row's status is the coverage rule's verdict over the recorded report"
  }
  contract   """
    The coverage view MUST give each entity a row: its obligation count, its
    proven count and its unproven verify texts, and a status. An obligation
    is proven when a passing recorded test names its verify text, the rule
    analyze coverage applies: an entity is covered only when every
    obligation is proven and no recorded test fails, so it is never covered
    while analyze reports A015 or A014; it is partial when some obligation is
    proven or a recorded test fails, else uncovered. A failing test proves
    nothing, even when it names an obligation. A verify property obligation
    counts as proven only through a passing test, as in analyze coverage
    without --prove. The obligations are the entity's verify statements; a
    field named verify is a field, never an obligation. Statuses are computed
    from the entity's verify declarations and the recorded report; no
    extension input decides them. With no filter the rows are the entities
    that count toward coverage: an entity of a kind the extension manifests
    declare testable, less the entities W004 exempts that declare no
    obligations (union types, abstract entities, governance kinds), the
    entities stats counts as testable. An entity_id names one entity
    whether it counts or not; each row says whether the entity is exempt;
    one the graph lacks is E003 naming the closest entity.
    kind and status narrow the rows, with an entity_id too; a kind the
    project does not know matches nothing and is an I020 notice.
  """
  verify unit "with no filters the rows are the entities that count toward coverage"
  verify unit "an exempt entity named by entity_id is returned with exempt true"
  verify unit "an entity_id the graph lacks is E003 naming the closest entity"
  verify unit "an unknown kind matches nothing and is an I020 notice; kind narrows an entity_id's row as status does"
  verify unit "an entity with an unproven obligation is partial, not covered"
  verify unit "a failing recorded test keeps an entity from being covered"
  verify unit "a field named verify does not hide an entity's verify statements"
}

// list and search (specforge_ops::read::query::{list, search}): the entities a
// selection over the project view returns. specforge.list, specforge.search,
// the entities resource and the LSP's workspace symbols render them.
behavior select_entities_over_the_view "Select Entities over the Project View" {
  features   [mcp_core_tools]
  invariants [diagnostic_determinism, zero_domain_knowledge_core]
  category   query
  types      [Graph]
  ports      [CompilerApi]
  requires {
    project_compiled "A compiled project, one-shot or a project session's, supplies the project view"
  }
  ensures {
    filters_anded "Every filter narrows the selection: text, kinds, a field's text and references"
    ranked_once   "Search ranks as the LSP's workspace symbols and completion rank"
  }
  contract   """
    Search MUST find the entities whose id or title matches the text, or
    whose string field text contains it, ranked exact, prefix, substring,
    field text, then Jaro-Winkler similarity of at least 0.8 over id and
    title. An empty text matches every entity. Every filter is ANDed: kinds,
    a field's text (by the one field-text rule: a list by its items, an
    expression by its written form, a block by its keys), and references
    (the entities that reference the named one; an entity the graph lacks is
    E003 naming the closest entity). At most limit hits are
    returned, twenty when none is given. List MUST return the entities of a
    kind, sorted by id, those whose fields hold every where value, from
    offset, at most limit; an empty kind is a kind the project does not
    know, not every kind.
  """
  verify unit "text search finds entities matching by name or contract"
  verify unit "empty query returns all entities up to limit"
  verify unit "the references filter combines with the other filters"
  verify unit "a references entity the graph lacks is E003 naming the closest entity"
  verify unit "the field filter reads a field's text: a list by its items, an expression by its written form, a block by its keys; never the export's JSON"
  verify unit "a search returns at most limit hits, twenty when no limit is given"
  verify unit "list pages the entities of a kind sorted by id, from offset, at most limit"
  verify unit "list keeps the entities whose fields hold every where value"
  verify unit "an empty kind is a kind the project does not know: it lists nothing and is an I020 notice"
}

// A spec file a request names (specforge_ops::view::ProjectView::source and source_place, ADR 0195).
behavior read_a_named_spec_file "Read a Spec File a Request Names" {
  features   [mcp_core_tools]
  invariants [diagnostic_determinism, zero_domain_knowledge_core]
  category   query
  types      [Graph]
  ports      [CompilerApi]
  contract   """
    A spec file a request names (a typed create's file, outline's file, the
    fixes' file_path) MUST be read into its source key, the path relative to
    the spec root that the compile and the editor name it by: every spelling
    of one file is that file (a leading or inner ., a doubled separator, its
    absolute path under the spec root). A spelling that leaves the spec root
    (a .. or an absolute path elsewhere) MUST be refused as not_a_source. A
    read of a file that is none of the compiled sources MUST be
    file_not_found; a create MUST be refused as not_a_source unless the file
    is one discovery would keep. A spelling from the project root of an
    existing source MUST be refused naming its spec-root spelling.
  """
  verify unit "every spelling of a spec file under the spec root reads as its source key"
  verify unit "a spelling that leaves the spec root is not_a_source"
  verify unit "a read of a file that is no compiled source is file_not_found"
  verify unit "a spec file named from the project root is refused naming its spec-root spelling"
}

// Inspect (specforge_ops::read::inspect): one entity's facts. specforge.inspect,
// the context prompt and the LSP hover render them.
behavior inspect_entity_facts "Inspect an Entity's Facts" {
  features   [mcp_core_tools]
  invariants [diagnostic_determinism, testable_entity_classification]
  category   query
  ports      [CompilerApi]
  requires {
    project_compiled "A compiled project, one-shot or a project session's, supplies the project view"
  }
  ensures {
    facts_of_one_entity "An entity's node, kind entry, headline, standing, obligations, references and diagnostics"
  }
  contract   """
    Inspect MUST answer one entity's facts: its node and kind entry, its
    headline statement, its standing and obligations, its references in both
    directions, and the diagnostics about it: those its data names it in,
    else those inside its block, never one whose message merely quotes it or
    whose id contains its id. Its coverage is the coverage view's verdict for
    its row. An entity the graph lacks is E003, entity_not_found, naming the
    closest entity.
  """
  verify unit "an entity's coverage is its coverage row's verdict, obligation by obligation"
  verify unit "the diagnostics about an entity are its own, not those of an entity whose id contains it"
  verify unit "an unknown entity is E003, entity_not_found"
}

// The exploration and the review: the explore and review prompts'
// payloads, and specforge explore / specforge review, read one view each.
behavior explore_the_graph "Explore the Graph" {
  features   [agent_export, mcp_prompts]
  invariants [diagnostic_determinism, zero_domain_knowledge_core]
  category   query
  requires {
    project_compiled "A compiled project, one-shot or a project session's, supplies the project view"
  }
  ensures {
    one_selection    "entity_id, depth and kind select the entities every list is about"
    connected_ranked "starting points and the most connected entities are connected entities, ranked by their edges to other entities"
    unconnected_kept "the selected unconnected entities are listed"
  }
  contract   """
    The exploration (specforge_ops::read::explore) MUST select the entities
    entity_id reaches within depth hops over edges both ways (every entity
    without entity_id; unbounded without depth), of kind when given, and
    answer about that selection only: the selected entities in id order;
    the path from entity_id to each selected entity it reaches, nearest
    first, with the labels of its edges; the starting points, the
    selected connected entities with the highest lead (edges to other
    entities minus edges from them), ties by id, at most five; the most
    connected, the selected connected entities with the most edges to and
    from other entities, ties by id, at most ten; and the selected
    unconnected entities (read_views_over_the_project_view). Degrees count
    every edge of the project. A kind the project does not know selects
    nothing and is an I020 notice naming the closest kind; an entity_id
    the graph lacks is E003 naming the closest entity; a depth without
    entity_id is refused as invalid input about entity_id. The exploration
    reaches exactly the entities the review reaches at the same depth.
  """
  verify unit "the exploration selects the entities entity_id reaches within depth, of kind when given, and every list is about that selection"
  verify unit "starting points are the selected connected entities that lead most, ties by id, at most five"
  verify unit "the most connected are the selected connected entities with the most edges to other entities, ties by id, at most ten"
  verify unit "unconnected lists the selected entities no edge links to another entity"
  verify unit "relationship paths run from entity_id to each selected entity it reaches, nearest first, with their edge labels"
  verify unit "an unknown kind selects nothing and is an I020 notice naming the closest kind"
  verify unit "a depth without entity_id is refused as invalid input about entity_id"
  verify unit "the exploration and the review reach the same entities at the same depth"
}

behavior review_coverage_gaps "Review Coverage Gaps" {
  features   [agent_export, mcp_prompts, extension_driven_coverage]
  invariants [diagnostic_determinism, testable_entity_classification, zero_domain_knowledge_core]
  category   query
  requires {
    project_compiled "A compiled project, one-shot or a project session's, supplies the project view"
  }
  ensures {
    neighbourhood_rows "the coverage view's rows of the entities that count toward coverage within depth hops of entity_id"
    gaps_found         "each row's missing obligations and unconnectedness are findings"
  }
  contract   """
    The review (specforge_ops::read::review) MUST list the coverage view's rows
    of the entities that count toward coverage within depth hops of
    entity_id (default one hop; a depth without entity_id is refused as
    invalid input about entity_id), or of the whole project without
    entity_id, in id order, and for each row a warning finding when it
    declares no obligation and an info finding when it is unconnected
    (read_views_over_the_project_view). An entity_id the graph lacks is
    E003 naming the closest entity; a recorded report that cannot be read
    is its E045 failure.
  """
  verify unit "the review lists the coverage view's rows of the entities within depth hops of entity_id, or of every entity that counts toward coverage"
  verify unit "an entity that declares no obligation is a warning finding"
  verify unit "an unconnected entity is an info finding"
  verify unit "the review's depth defaults to one hop"
  verify unit "a depth without entity_id is refused as invalid input about entity_id"
  verify unit "a project with nothing that counts toward coverage has no rows and no findings"
  verify unit "a recorded report that cannot be read is the review's E045 failure"
}

behavior provide_explore_cli "Provide CLI Explore Command" {
  invariants [diagnostic_determinism]
  category   cli
  contract   """
    specforge explore [ENTITY] [--kind KIND] [--depth N] [--path PATH]
    [--format human|json] MUST render the exploration (explore_the_graph)
    of the project compiled at PATH: --format json prints the document the
    explore prompt's payload is for the same arguments; human output lists
    the selection's size, the starting points, the most connected entities
    with their edge counts, the unconnected entities and, with ENTITY, the
    paths from it. Notices go to stderr under human output; under --format
    json the document carries them and stderr is empty. An unknown ENTITY is E003 naming
    the closest entity, exit 1. --depth without ENTITY is a usage error, exit 2.
  """
  verify integration "specforge explore --format json is the explore prompt's payload for the same arguments"
  verify integration "an unknown entity is E003 naming the closest entity, exit 1"
  verify integration "under --format json the notices are the document's alone: nothing on stderr"
}

behavior provide_review_cli "Provide CLI Review Command" {
  features   [agent_export, extension_driven_coverage]
  invariants [diagnostic_determinism]
  category   cli
  contract   """
    specforge review [ENTITY] [--depth N] [--path PATH] [--format
    human|json] MUST render the review (review_coverage_gaps) of the
    project compiled at PATH: --format json prints the document the review
    prompt's payload is for the same arguments; human output lists each
    row (entity, kind, status, proven obligations) and each finding. An
    unknown ENTITY is E003, exit 1; --depth without ENTITY is a usage error,
    exit 2; a recorded report that cannot be read is E045, exit 1.
  """
  verify integration "specforge review --format json is the review prompt's payload for the same arguments"
  verify integration "a recorded report that cannot be read is E045, exit 1"
}

// An enumerated argument is one option table (ADR 0027): the CLI's possible
// values and MCP's input schema are built from it, and both parse with it.
behavior name_enumerated_options_once "Name Enumerated Options Once" {
  features   [mcp_core_tools, agent_export]
  invariants [diagnostic_determinism, mcp_structured_error_responses]
  category   query
  types      [McpToolDescriptor]
  ports      [CompilerApi, McpProtocol]
  requires {
    option_tables_declared "Each enumerated argument an operation reads is declared once, with its names, aliases, help and default"
  }
  ensures {
    surfaces_list_the_table  "The CLI's possible values and default and the MCP input schema's enum and default are the table's"
    surfaces_accept_the_same "A name one surface accepts for an argument the other accepts too, as the same value"
    one_refusal              "An unknown name is refused naming the argument, the expected names and the closest one"
  }
  contract   """
    Every argument an operation takes from a closed set of names (an export
    format, the model's format, grouping and field level, the outline's
    format, detail and dependency depth, a render format, a coverage status,
    a reference direction) MUST be one option table in specforge-ops: its
    listed names in order, the aliases it also accepts, a one-line help per
    name, and its default. The CLI's possible values and default and the MCP
    tool's input-schema enum (listed names, then aliases) and default MUST be
    built from the table, and both surfaces MUST parse the argument with it.
    Every surface MUST answer an absent argument with the table's default;
    no surface declares another. An unknown name MUST be refused as
    "Unknown <argument>: <name>. Expected: <names>", with the closest name
    as a suggestion, as the one failure kind invalid_input (a table has no
    error code of its own); the CLI refuses it before compiling (exit 2), MCP
    as invalid_input on the argument. Whatever a refusal offers as the
    available choices (specforge.render's available_renderers) is the list
    the message names: the table's names, never an alias. A set the project decides (analysis
    passes, entity kinds) is not a table: the operation checks the name
    against the project, and both surfaces relay its refusal.
  """
  verify unit "a table parses its names and aliases and refuses any other naming the expected names"
  verify unit "each enumerated MCP argument advertises the table's names and default"
  verify unit "a refusal's available choices are the names its message lists, an alias never among them"
  verify integration "the CLI and MCP accept the same names for each enumerated argument"
  verify integration "the analysis passes the CLI accepts are the project's"
}

behavior explain_diagnostic_code "Explain a Diagnostic Code" {
  features   [diagnostic_reporting]
  invariants [diagnostic_determinism, zero_domain_knowledge_core]
  category   query
  types      [McpExplainResult]
  requires {
    code_given "The caller names a diagnostic code"
  }
  ensures {
    entry_printed   "The catalogued entry, or the retired code and its replacement, is printed with its docs link"
    one_document    "Under --format json the CLI prints the document specforge.explain answers"
    unknown_refused "A code the catalog neither has nor retired is refused with unknown_code"
  }
  contract   """
    specforge explain <code> and specforge.explain MUST be one operation
    (specforge_ops::read::explain) over the diagnostic catalog: the code is read
    in any case with surrounding whitespace ignored; a catalogued code
    answers its code, title, owner, level, explanation and docs link; a
    retired code answers retired true and the entry of the code that
    replaced it, if any. The CLI MUST print the entry as text, its last
    line the docs link, and under --format json the document
    specforge.explain answers (McpExplainResult), nothing else. A code the
    catalog neither has nor retired MUST be refused with unknown_code
    (invalid input) and the code forms as its suggestion, on both surfaces.
  """
  verify unit "specforge explain prints the catalogued entry with its docs link, and a retired code with the entry that replaced it"
  verify integration "specforge explain --format json prints the document specforge.explain answers"
  verify unit "specforge explain reads the code as specforge.explain does: any case, surrounding whitespace ignored"
  verify unit "an uncatalogued code is refused with unknown_code on both surfaces"
}

behavior print_diagnostics_structured "Print Diagnostics Structured" {
  features   [diagnostic_reporting]
  invariants [multi_error_collection, diagnostic_determinism, zero_domain_knowledge_core]
  category   command
  types      [Diagnostic, DiagnosticBag]
  ports      [CompilerApi]
  consumes   [validation_complete]
  requires {
    validation_complete_fired "validation_complete event has fired, confirming all diagnostics are collected in the DiagnosticBag"
  }
  ensures {
    structured_format_enforced "Every diagnostic is formatted with file path, line number, column, source snippet, and color-coded severity"
    color_coding_applied       "Errors are red, warnings yellow, info blue"
  }
  contract   """
    All diagnostics MUST be formatted in a structured style: file path, line
    number, column, source context snippet, suggestion, and color-coded
    severity. Errors MUST be red, warnings yellow, info blue. The format
    MUST be consistent across all output modes.
    Colour follows one rule wherever the CLI writes: NO_COLOR set turns
    it off, CLICOLOR_FORCE set (and not 0) turns it on, otherwise only a
    terminal is coloured; specforge explain bolds the code it explains by
    the same rule.
  """
  verify unit "error diagnostic is formatted with file:line:col"
  verify unit "diagnostic includes context snippet"
  verify unit "suggestion is displayed when available"
  verify contract "Print Diagnostics Structured: structured diagnostic printing holds — validation_complete_fired, structured_format_enforced, color_coding_applied"
  verify unit "spanless diagnostic uses code as fallback location"
  verify unit "spanless error diagnostic also uses code"
  verify unit "diagnostics are plain when output is not a terminal"
  verify unit "NO_COLOR disables diagnostic colour"
  verify unit "specforge explain bolds the code only where its output is coloured"
}

behavior exit_code_reflects_diagnostic_severity "Exit Code Reflects Diagnostic Severity" {
  features   [ci_integration]
  invariants [multi_error_collection, diagnostic_determinism, zero_domain_knowledge_core]
  category   command
  types      [DiagnosticBag]
  ports      [CompilerApi]
  consumes   [validation_complete]
  requires {
    validation_complete_fired "validation_complete event has fired, confirming all diagnostics are finalized"
  }
  ensures {
    exit_zero_on_clean   "Exit code is 0 when no error-level diagnostics exist"
    exit_one_on_errors   "Exit code is 1 when any error-level diagnostic exists"
    strict_mode_enforced "In --strict mode, warnings also cause exit code 1"
  }
  contract   """
    specforge check MUST exit with code 0 if no errors exist. It MUST
    exit with code 1 if any error-level diagnostic exists. With --strict,
    warnings MUST also cause exit code 1.
    A command-line value outside a flag's allowed set, such as an
    unknown --format, an unknown --lint profile or an unknown --severity,
    MUST be rejected while arguments are parsed, with exit code 2, before
    anything is compiled.
  """
  verify unit "exit 0 with no errors"
  verify unit "exit 1 with errors"
  verify unit "exit 1 with warnings in strict mode"
  verify unit "a typo'd --format fails with a clap error (exit 2), not a bespoke runtime error"
  verify unit "an unknown --lint profile fails with a clap error (exit 2), before anything is compiled"
  verify contract "Exit Code Reflects Diagnostic Severity: exit code severity mapping holds — validation_complete_fired, exit_zero_on_clean, exit_one_on_errors, strict_mode_enforced"
}

behavior report_command_outcome "Report a Command's Outcome" {
  features   [ci_integration]
  invariants [diagnostic_determinism, zero_domain_knowledge_core]
  category   command
  types      [DiagnosticBag]
  ports      [CompilerApi]
  requires {
    operation_ran "the command's operation returned its outcome or refused"
  }
  ensures {
    one_refusal_shape "a refusal is error[CODE]: message with its hint on stderr, or the error document under --format json; a fixed-format command's is always on stderr"
    one_exit_table    "the exit code is the run's verdict, the refusal, or that the command could not judge"
    surfaces_agree    "the CLI's exit code and MCP's ok are one verdict"
  }
  contract   """
    Every core command MUST end in one of three ways. Its run passed:
    exit 0. Its run's verdict failed (check found an error, format --check
    a file that would change, migrate failed or rolled back, analyze an
    error finding or a gate below its minimum, doctor an error-level
    finding or an unusable credential) or its operation refused:
    exit 1. The command could not judge the project, because the command
    line was refused or a measuring command (stats, analyze) cannot read
    what it measures against, or analyze's coverage gate has no figure:
    exit 2. A refusal MUST be printed on stderr
    as error[CODE]: message, then "  hint: " and the suggestion when there
    is one, then "  wrote: " and each file the failed operation left
    written; under --format json (the human|json flag) it MUST instead be the
    error document {error, code}, with suggestion when the refusal has one
    and files_written when files were left written, on stdout, with
    nothing on stderr. A fixed-format command (export, schema, model,
    query, outline) prints a document a reader parses or nothing: its
    refusal MUST be on stderr whatever its --format names, JSON documents
    included, with nothing on stdout. Under --format json a command that changes project
    files prints files_written beside its document: absent from a preview
    (--dry-run, format --check or --diff), and otherwise every file it
    wrote, [] when it wrote none, as specforge's MCP reply does (ADR 0022,
    ADR 0162). A project file a command could not write MUST be
    refused as file_unwritable, one it could not read as file_unreadable,
    and one that changed since the command read it as file_changed,
    whichever command it is; the message names the file from the project's
    or the spec root, after what the command was doing. Beside its document
    a command MUST write a note
    for a person only under human output, and a notice once: in the
    document when the document carries it, else on stderr as
    severity[CODE]: message before the document, under every format.
    A command run
    outside any project where it needs one MUST refuse with no_project.
    Every core command whose output has a human and a JSON form MUST take
    --format human|json (human by default; trace's is json); no core
    command takes --json.
    A reader that closes stdout before the command has written
    everything (specforge export | head) MUST end the output quietly:
    no panic and nothing more on stderr, and the exit code stays the
    command's.
    The verdict is the operation's: the CLI's exit code and the ok MCP
    returns for check, analyze, format, migrate and doctor MUST agree
    (doctor's credentials are the CLI's alone).
    Under --format json a command prints its operation's outcome document,
    the one the matching MCP tool answers, adding only what it alone knows
    (the files it wrote, doctor's credentials, infer-status's --gaps and
    --gaps-detail sections), never renaming or dropping a key.
  """
  verify unit "an operation's refusal is error[CODE]: message, its hint and the files it left written, on stderr"
  verify unit "under --format json a refusal is the error document on stdout and nothing on stderr"
  verify unit "a passed run exits 0, a failed verdict or a refusal 1, a refusal of a measuring command 2"
  verify unit "stats, trace, analyze, migrate and init refuse with the error document under --format json"
  verify unit "a fixed-format command (export, schema, model, query, outline) refuses on stderr whatever its --format names, with nothing on stdout"
  verify unit "a command run outside any project refuses with no_project"
  verify unit "every core command with a JSON form takes --format human|json, and none takes --json"
  verify unit "under --format json a note is never written and a notice is written once: in the document when it carries it, else on stderr"
  verify unit "a closed stdout ends the output quietly and keeps the command's exit"
  verify unit "a project file a command could not write, read or found changed is file_unwritable, file_unreadable or file_changed on every command"
  verify integration "the CLI's exit code and MCP's ok agree for check, analyze, format and migrate"
  verify integration "the CLI's --format json and MCP's reply are one outcome document for every operation both surfaces run"
  verify integration "a preview lists no files_written and a run lists every file it wrote, none included, on the command line as over MCP"
  verify contract "Report a Command's Outcome: command outcome holds — operation_ran, one_refusal_shape, one_exit_table, surfaces_agree"
}

behavior serialize_traceability_data "Serialize Traceability Data" {
  features   [traceability_serialization]
  invariants [graph_traversal_integrity, diagnostic_determinism, zero_domain_knowledge_core]
  category   query
  types      [Graph, TraceChain, TraceLink, OutputFile]
  ports      [GraphSerializer, FileSystem]
  consumes   [validation_complete]
  produces   [render_complete]
  requires {
    validation_complete_fired "validation_complete event has fired, confirming the graph is finalized and all edge types are registered"
  }
  ensures {
    full_trace_serialized      "Full traceability chain from root to leaf entities is serialized as structured JSON"
    gaps_included              "Missing links in the chain are included with a missing status"
    graph_protocol_conformance "Output conforms to the Graph Protocol schema"
    render_complete_emitted    "render_complete event is emitted after successful serialization"
  }
  contract   """
    When specforge trace is invoked without an entity ID, the system MUST
    compute and serialize the full traceability chain across all registered
    edge types from root entities to leaf entities as structured JSON graph
    traversal data. Gaps in the chain MUST be included in the output with
    a "missing" status. The output MUST conform to the Graph Protocol schema.
    The document's schema_version MUST be the project's versioned schema
    version.
  """
  verify unit "full trace covers all root entities across registered edge types"
  verify unit "gaps in chain are highlighted"
  verify unit "output conforms to Graph Protocol schema"
  verify unit "the trace document carries the project's schema version"
  verify contract "Serialize Traceability Data: traceability data serialization holds — validation_complete_fired, full_trace_serialized, gaps_included, graph_protocol_conformance, render_complete_emitted"
}

behavior validate_agent_plan "Validate Agent Implementation Plan" {
  features   [traceability_serialization]
  invariants [graph_traversal_integrity, diagnostic_determinism, zero_domain_knowledge_core]
  category   validation
  types      [Graph, AgentPlan, AgentPlanEntry, McpTracePlanResult, McpTraceGap]
  ports      [CompilerApi]
  consumes   [validation_complete]
  produces   [plan_validated]
  requires {
    validation_complete_fired "validation_complete event has fired, confirming the spec graph is finalized for plan comparison"
  }
  ensures {
    unresolvable_ids_diagnosed "Every plan entity ID that does not resolve to a declared entity produces an E003 diagnostic"
    missing_entries_warned     "Every testable entity missing from the plan produces a warning"
    ordering_validated         "Plan dependency order is validated against graph edge structure"
    structured_report_produced "Output is a structured JSON report listing validated entries, gaps, and ordering violations"
    plan_validated_emitted     "plan_validated event is emitted after validation completes"
  }
  contract   """
    specforge trace --plan <file> and the plan argument of specforge.trace
    and of the specforge://prompts/trace prompt MUST check an agent plan
    against the current spec graph. The plan MUST be an AgentPlan
    (types/graph.spec): a value that is not one MUST be refused, naming
    every place it departs from the type. Every entity ID referenced in the
    plan MUST resolve to a declared entity in the graph; an unresolvable ID
    MUST produce an E003 gap. Every testable entity that declares
    obligations MUST have an entry in the plan; a missing entry MUST be
    reported as a warning gap. The plan's entry order is its dependency
    order: an entry ordered before an entity it depends on in the graph
    MUST produce an ordering gap. The check reads the graph and the entity
    snapshot, never the recorded test report. The outcome MUST be one JSON
    document on every surface (McpTracePlanResult): ok (no unresolved entry
    and no ordering gap), the entries that name an entity, and the gaps;
    specforge trace --plan exits 1 when ok is false. An entity and a plan
    are two requests: every surface refuses both together as invalid input
    on the plan.
  """
  verify unit "plan with all valid entity IDs passes validation"
  verify unit "plan referencing nonexistent entity ID produces E003"
  verify unit "testable entity missing from plan produces warning"
  verify unit "plan dependency order contradicting graph produces diagnostic"
  verify unit "output is structured JSON"
  verify unit "the plan check reads no recorded test report"
  verify unit "a plan that is not an AgentPlan is refused, naming every place it departs from the type"
  verify unit "a plan passes when every entry names an entity and none comes before what it depends on; a missing entry is a warning"
  verify unit "an entity and a plan together are invalid input about the plan, on every surface"
  verify integration "specforge trace --plan and specforge.trace answer the same document for a plan"
  verify integration "specforge trace and specforge.trace both refuse an entity with a plan"
  verify integration "specforge trace --plan exits 1 when an entry names no entity or comes before what it depends on"
  verify contract "Validate Agent Implementation Plan: agent plan validation holds — validation_complete_fired, unresolvable_ids_diagnosed, missing_entries_warned, ordering_validated, structured_report_produced, plan_validated_emitted"
}

// render_index_files and selective_render_by_entity_type moved to
// spec/extensions/markdown-renderer/behaviors.spec

behavior deterministic_output "Deterministic Output" {
  features   [ci_integration]
  invariants [diagnostic_determinism, zero_domain_knowledge_core, graph_traversal_integrity]
  category   command
  types      [OutputFile]
  ports      [CompilerApi]
  consumes   [validation_complete]
  requires {
    validation_complete_fired "validation_complete event has fired, confirming input graph state is finalized"
  }
  ensures {
    byte_identical_output      "Given identical input spec files, all output paths produce byte-for-byte identical output"
    no_nondeterministic_values "Output contains no timestamps, random values, or iteration-order-dependent content"
  }
  contract   """
    Given identical input .spec files, all output paths — file emitters
    (specforge render) and stdout emitters (specforge export, specforge query)
    — MUST produce byte-for-byte identical output. Output MUST NOT depend on
    filesystem iteration order, hashmap ordering, or timestamps.
  """
  verify property "same input produces identical output across runs"
  verify unit "entity ordering is independent of hashmap iteration"
  verify unit "file emission order is independent of filesystem readdir order"
  verify unit "output contains no timestamps or non-deterministic values"
  verify unit "edge ordering is independent of hashmap iteration"
  verify contract "Deterministic Output: deterministic output holds — validation_complete_fired, byte_identical_output, no_nondeterministic_values"
}

behavior check_mode_for_ci "Check Mode for CI" {
  features   [ci_integration]
  invariants [multi_error_collection, diagnostic_determinism, zero_domain_knowledge_core]
  category   validation
  types      [DiagnosticBag]
  ports      [CompilerApi]
  consumes   [validation_complete]
  requires {
    validation_complete_fired "validation_complete event has fired, confirming all spec files have been parsed, resolved, and validated"
  }
  ensures {
    no_output_files_produced "Check mode produces zero output files on disk"
    diagnostics_to_stderr    "All diagnostics are printed to stderr"
    appropriate_exit_code    "Exit code reflects diagnostic severity per exit_code_reflects_diagnostic_severity"
  }
  contract   """
    specforge check MUST parse, resolve, and validate all .spec files
    without producing any output files. It MUST print diagnostics to
    stderr and exit with an appropriate exit code. This is the primary
    CI integration point.
  """
  verify unit "check mode produces no output files"
  verify unit "check mode prints diagnostics to stderr"
  verify integration "check mode works in CI environment"
  verify contract "Check Mode for CI: CI check mode holds — validation_complete_fired, no_output_files_produced, diagnostics_to_stderr, appropriate_exit_code"
}

behavior check_diagnostic_policy "Apply the Diagnostic Policy Once" {
  features   [ci_integration, diagnostic_reporting]
  invariants [diagnostic_determinism, zero_domain_knowledge_core]
  category   validation
  types      [Diagnostic]
  ports      [CompilerApi]
  requires {
    project_compiled "the project compiled and its diagnostics are known"
  }
  ensures {
    policy_once        "lint profiles add their diagnostics, then strict promotes warnings, the same way for specforge check and MCP validate"
    closed_profiles    "the lint profiles are inferred and pedantic; any other name is refused"
    verdict_unfiltered "the check passes when nothing reported is an error, whatever a severity filter shows"
  }
  contract   """
    What a project reports is decided once, for specforge check and the
    MCP specforge.validate tool alike: the compiled project's
    diagnostics, then those of each requested lint profile (inferred:
    I200 and I202 from specforge-infer.json; pedantic adds nothing, as
    info diagnostics are always reported), then strict promotion of
    warnings to errors. A lint profile SpecForge does not define MUST be
    refused: by the CLI while arguments are parsed (exit 2), by MCP as
    invalid input. The check passes when no reported diagnostic is an
    error; a severity filter selects what is shown and MUST NOT change
    whether the check passes or whether the build cache is written.
  """
  verify unit "an unknown lint profile is refused by name, and pedantic adds nothing"
  verify unit "each named lint profile adds its diagnostics once, before strict promotes warnings"
  verify unit "the verdict and the cache decision are taken over every reported diagnostic, never the filtered ones"
  verify unit "strict promotes warnings before the verdict, so a strict check with warnings is not clean"
  verify unit "--lint pedantic is accepted and changes nothing"
}

behavior filter_reported_diagnostics "Filter Reported Diagnostics by Severity" {
  features   [ci_integration, diagnostic_reporting]
  invariants [diagnostic_determinism]
  category   query
  types      [Diagnostic]
  ports      [CompilerApi]
  requires {
    policy_applied "the diagnostic policy has been applied"
  }
  ensures {
    shows_one_severity "only diagnostics of the named severity (error, warning or info, any case) are shown, after strict promotion"
    same_both_surfaces "specforge check --severity and MCP validate severity_filter show the same diagnostics"
    exit_unaffected    "the exit code and the build cache decision are those of the unfiltered check"
  }
  contract   """
    specforge check --severity <error|warning|info> and the
    severity_filter argument of specforge.validate show only the
    reported diagnostics of that severity, after strict promotion, in
    check's order. The name matches ignoring case; any other value MUST
    be refused (CLI: exit 2 while arguments are parsed; MCP: invalid
    input naming severity_filter). The filter MUST NOT change the exit
    code, the build cache decision, or MCP's verdict.
  """
  verify unit "check --severity prints only that severity and never changes the exit code"
  verify integration "check --severity and MCP validate severity_filter report the same diagnostics"
}

behavior export_diagnostics_as_json "Export Diagnostics as JSON" {
  features   [ci_integration, diagnostic_reporting]
  invariants [multi_error_collection, diagnostic_determinism, zero_domain_knowledge_core]
  category   query
  types      [DiagnosticBag, Diagnostic, DiagnosticFormat]
  ports      [CompilerApi]
  consumes   [validation_complete]
  requires {
    validation_complete_fired "validation_complete event has fired, confirming all diagnostics are collected"
  }
  ensures {
    json_array_produced        "All diagnostics are serialized as a JSON array to stdout"
    diagnostic_fields_complete "Each diagnostic includes code, severity, message, file path, line, column"
    exit_code_unaffected       "The --format=json flag does not alter exit code behavior"
  }
  contract   """
    When specforge check --format=json is invoked, the system MUST output
    all diagnostics as a JSON array to stdout. Each diagnostic MUST include
    code, severity, message, file path, line, column, and optional suggestion.
    The JSON output MUST conform to a stable schema suitable for consumption
    by CI tools and AI agents. This format is an alternative to the default
    structured text output (file:line:col format). The --format=json flag MUST NOT affect exit
    code behavior — exit codes remain governed by
    exit_code_reflects_diagnostic_severity.
  """
  verify unit "diagnostics serialized as JSON array to stdout"
  verify unit "each diagnostic includes code, severity, message, file, line, column"
  verify unit "JSON output is valid and parseable"
  verify unit "exit code unaffected by format flag"
  verify unit "suggestion field included when available"
  verify contract "Export Diagnostics as JSON: JSON diagnostic export holds — validation_complete_fired, json_array_produced, diagnostic_fields_complete, exit_code_unaffected"
  verify unit "max diagnostics limit truncates output"
  verify unit "no truncation under limit"
}

// One JSON shape for diagnostics on every surface (ADR 0004 D1-c): a
// superset of the CLI's nested span and MCP's flat location, so no reader
// of either breaks.
behavior present_diagnostics_as_json "Present Diagnostics as JSON" {
  features   [ci_integration, diagnostic_reporting]
  invariants [diagnostic_determinism, zero_domain_knowledge_core]
  category   query
  types      [Diagnostic]
  ports      [CompilerApi]
  requires {
    diagnostics_collected "the diagnostics to present have been collected"
  }
  ensures {
    one_shape_everywhere "specforge check --format json, MCP validate, the specforge://diagnostics resource, tool and read _meta, the diagnosticsChanged notification and McpError present diagnostics in one shape"
    location_both_ways   "each entry carries its span nested under span and the span's start flat as file, line and column"
    data_only_when_typed "an entry carries data, the diagnostic's typed payload, only when the diagnostic has one"
  }
  contract   """
    Wherever SpecForge prints diagnostics as JSON (specforge check
    --format json, specforge collect and specforge watch --format json,
    the MCP specforge.validate tool, the specforge://diagnostics resource, the
    diagnostics in a tool result's or a resource read's _meta, the specforge/diagnosticsChanged
    notification and an McpError's diagnostic), it MUST present them in one
    shape: an array of
    entries with code, severity, message and suggestion (null when there
    is none), the span nested under span (file, start_line, start_col,
    end_line, end_col; null without a location), and the span's start
    flat as file, line and column (null without a location). The shape
    is a superset of the nested and the flat shapes these surfaces used
    before, so no reader of either breaks. A diagnostic that carries a
    typed payload (DiagnosticData, e.g. the target of an unresolved
    reference) MUST present it under data, tagged by kind; one without
    MUST NOT gain the key, so its entry is unchanged.
  """
  verify unit "diagnostics are presented as one JSON array"
  verify unit "each diagnostic carries code, severity, message, file, line and column"
  verify unit "suggestion is included when available"
  verify unit "the presented JSON is valid and parseable"
  verify unit "the span is nested beside the flat location, with its end positions"
  verify unit "a typed payload is presented under data, and its absence adds no key"
  verify integration "check and MCP validate present the same diagnostics identically"
  verify contract "Present Diagnostics as JSON: JSON diagnostic presentation holds — diagnostics_collected, one_shape_everywhere, location_both_ways, data_only_when_typed"
}

// ── Agent-Optimized Export (Principle 3: agents are first-class consumers) ──
// `specforge export` writes to stdout (graph, context, brief, dot, markdown); `specforge.render` writes the same formats into the project.

behavior export_agent_context_format "Export Agent Context Format" {
  invariants [
    graph_traversal_integrity,
    diagnostic_determinism,
    graph_schema_completeness,
    zero_domain_knowledge_core,
  ]
  category   query
  types      [Graph, GraphProtocolSchema, OutputFile, AgentExportConfig, GraphAnnotation]
  ports      [CompilerApi]
  consumes   [validation_complete]
  produces   [export_complete]
  requires {
    validation_complete_fired "validation_complete event has fired, confirming the graph is ready for agent export"
  }
  ensures {
    token_optimized_output  "Output omits verbose prose fields to minimize token consumption"
    schema_version_present  "Output includes a schema_version field identifying the Graph Protocol version"
    scope_enforced          "When --scope is specified, only the reachable subgraph is returned"
    invalid_scope_diagnosed "Non-existent scope entity produces E003 and exit code 1"
    export_complete_emitted "export_complete event is emitted after successful export"
  }
  contract   """
    When specforge export --format=context is invoked, the system MUST produce
    a token-optimized representation of the graph containing entity IDs,
    contracts, relationships, and coverage status. Each entity also carries
    the fields its extension declares normative (an invariant's guarantee,
    a decision's decision text), so every kind keeps the text that states
    what it promises; core reads the flag and knows no field by name. The
    fields an extension declares headline (a behavior's contract, a status)
    sit at the entity's top level instead; a field no extension declares
    headline is never lifted, whatever its name. The output MUST omit
    verbose fields (full descriptions, prose) to minimize token consumption.
    The format MUST be valid JSON conforming to the Graph Protocol schema.
    The output MUST include a schema_version field identifying the Graph
    Protocol version. An optional --scope parameter MUST allow scoping to
    a subgraph rooted at a specific entity. If --scope references a
    non-existent entity ID, the system MUST emit an E003 diagnostic
    and exit with code 1. When coverage metadata is available from
    compute_project_statistics, the context export MUST include
    coverage_pct and testable_entity_count in the graph metadata.
  """
  verify unit "context format includes entity IDs and contracts"
  verify unit "context format omits verbose prose fields"
  verify unit "context format keeps each entity's normative fields"
  verify integration "export --format context keeps an invariant's guarantee"
  verify unit "scoped export returns only reachable subgraph"
  verify unit "non-existent scope entity produces E003 and exit code 1"
  verify unit "output conforms to Graph Protocol schema"
  verify unit "output includes schema_version field"
  verify contract "Export Agent Context Format: agent context export holds — validation_complete_fired, token_optimized_output, schema_version_present, scope_enforced, invalid_scope_diagnosed, export_complete_emitted"
  verify unit "the context export leaves the schema out unless --with-schema is given"
}

behavior export_agent_brief_format "Export Agent Brief Format" {
  invariants [
    graph_traversal_integrity,
    diagnostic_determinism,
    graph_schema_completeness,
    zero_domain_knowledge_core,
  ]
  category   query
  types      [Graph, GraphProtocolSchema, OutputFile, AgentExportConfig]
  ports      [CompilerApi]
  consumes   [validation_complete]
  produces   [export_complete]
  requires {
    validation_complete_fired "validation_complete event has fired, confirming the graph is ready for agent export"
  }
  ensures {
    minimal_representation  "Output contains only entity IDs, kinds, titles, and direct relationships"
    schema_version_present  "Output includes a schema_version field identifying the Graph Protocol version"
    export_complete_emitted "export_complete event is emitted after successful export"
  }
  contract   """
    When specforge export --format=brief is invoked, the system MUST produce
    a minimal representation containing only entity IDs, kinds, titles, and
    their direct relationships. This is the lowest-token-cost format for
    agent discovery tasks. The output MUST be valid JSON conforming to the
    Graph Protocol schema. The output MUST include a schema_version field
    identifying the Graph Protocol version.
  """
  verify unit "brief format includes only IDs, kinds, titles, and edges"
  verify unit "brief format is smaller than context format"
  verify unit "output conforms to Graph Protocol schema"
  verify unit "output includes schema_version field"
  verify contract "Export Agent Brief Format: agent brief export holds — validation_complete_fired, minimal_representation, schema_version_present, export_complete_emitted"
  verify unit "the brief export leaves the schema out unless --with-schema is given"
}

behavior export_agent_graph_format "Export Agent Graph Format" {
  invariants [
    graph_traversal_integrity,
    diagnostic_determinism,
    graph_schema_completeness,
    zero_domain_knowledge_core,
  ]
  category   query
  types      [Graph, GraphProtocolSchema, OutputFile, AgentExportConfig]
  ports      [CompilerApi]
  consumes   [validation_complete]
  produces   [export_complete]
  requires {
    validation_complete_fired "validation_complete event has fired, confirming the graph is ready for full-fidelity export"
  }
  ensures {
    full_fidelity_output    "Output contains all nodes, edges, fields, and metadata"
    schema_version_present  "Output includes a schema_version field identifying the Graph Protocol version"
    scope_enforced          "When --scope is specified, only the reachable subgraph is returned"
    invalid_scope_diagnosed "Non-existent scope entity produces E003 and exit code 1"
    export_complete_emitted "export_complete event is emitted after successful export"
  }
  contract   """
    When specforge export --format=graph is invoked, the system MUST produce
    the complete entity graph as JSON conforming to the Graph Protocol schema.
    This is the full-fidelity format containing all nodes, edges, fields, and
    metadata. Unlike specforge render json (which writes files to disk), this
    command writes to stdout for agent consumption. An optional --scope parameter
    MUST allow scoping to a subgraph rooted at a specific entity. If --scope
    references a non-existent entity ID, the system MUST emit an E003
    diagnostic and exit with code 1. --depth (with --scope) limits the hops
    from it, and --kinds keeps only entities of those kinds, the scoped
    entity always kept; a kind the project does not know matches nothing
    and is reported as I020, naming the closest kind; depth without a scope is invalid input about the scope; so are --with-schema and
    --no-schema together.
    specforge.export and the graph resources take the same options. The
    output MUST include a schema_version field identifying the Graph
    Protocol version.
  """
  verify unit "graph format includes all nodes and edges"
  verify unit "graph format includes all fields and metadata"
  verify unit "scoped export returns only reachable subgraph"
  verify unit "non-existent scope entity produces E003 and exit code 1"
  verify unit "depth without a scope is invalid input on every surface"
  verify unit "with_schema and no_schema together are invalid input on every surface"
  verify integration "depth and kinds narrow the export the same way on every surface"
  verify integration "a kind of kinds the project does not know matches nothing and is reported as I020, on every surface"
  verify unit "an export failure carries its code as a constant, never in its message"
  verify unit "output conforms to Graph Protocol schema"
  verify unit "output includes schema_version field"
  verify integration "structural-only graph exports valid JSON with raw keyword strings as entity kinds"
  verify contract "Export Agent Graph Format: agent graph export holds — validation_complete_fired, full_fidelity_output, schema_version_present, scope_enforced, invalid_scope_diagnosed, export_complete_emitted"
}

behavior export_review_document "Export a Review Document" {
  features   [review_export]
  invariants [diagnostic_determinism, zero_domain_knowledge_core]
  category   query
  types      [Graph]
  ports      [CompilerApi]
  contract   """
    When specforge export --format markdown is invoked, or specforge.export or
    specforge.render with format markdown, the system MUST write one Markdown
    document a reviewer reads without tooling: one section per selected
    entity headed by its kind, id and title; each field its extension
    declares normative quoted whole; its other fields in a table; its
    references in a table naming each linked entity's kind and title, a
    reference that does not resolve listed as unresolved; the entities that
    reference it; and its obligations, numbered. Core reads only field
    declarations and names no kind or field. Scope, depth and kinds select
    the sections as for every export format; links name entities outside
    the selection too. The document MUST be deterministic: the scoped
    entity first, then by kind and id, with no timestamp. Prose renders as
    written, with raw HTML, entity references and backslashes escaped
    outside code spans.
  """
  verify unit "a section is headed by its kind, id and title"
  verify unit "a normative field is quoted whole"
  verify unit "links name each linked entity's kind and title, outside the selection too"
  verify unit "a reference that does not resolve is listed as unresolved"
  verify unit "the entities that reference it are listed"
  verify unit "obligations are numbered in declaration order"
  verify unit "prose renders as written, raw HTML escaped outside code spans"
  verify unit "sections are ordered scope first, then by kind and id"
  verify integration "export --format markdown writes the review document of examples/todo-app and examples/shop"
  verify integration "specforge.export answers the same document as specforge export"
}

behavior query_graph_multi_resolution "Query Graph at Multiple Resolutions" {
  invariants [
    graph_traversal_integrity,
    diagnostic_determinism,
    graph_schema_completeness,
    zero_domain_knowledge_core,
  ]
  category   query
  types      [Graph, GraphProtocolSchema, AgentExportConfig]
  ports      [CompilerApi]
  consumes   [validation_complete]
  produces   [graph_queried]
  requires {
    validation_complete_fired "validation_complete event has fired, confirming the graph is finalized and traversable"
  }
  ensures {
    depth_respected            "Subgraph returned contains only entities within the requested hop distance"
    kind_filter_applied        "When --kind is specified, results are restricted to the specified entity kinds"
    graph_protocol_conformance "Output is valid JSON conforming to the Graph Protocol schema with schema_version"
    graph_queried_emitted      "graph_queried event is emitted after query completes"
  }
  contract   """
    When specforge query is invoked with an entity ID and a --depth parameter,
    the system MUST return the subgraph at the requested resolution level.
    Depth 0 returns only the entity itself. Depth 1 (the default) returns
    direct neighbors. Depth N returns all entities within N hops. An
    optional --kind parameter MUST filter results to only include entities
    of the specified kind(s), the queried entity always, keeping an edge
    only when both of its entities are kept. Multiple --kind values MAY be
    specified (e.g., --kind=alpha --kind=beta); a kind the project does not
    know matches nothing and is reported as I020 with the closest kind.
    Kind names are extension-defined; examples use placeholders.
    --format selects an agent format (graph, context, brief) and
    --include-coverage gives each entity its coverage status
    (coverage_status: covered, partial or uncovered), written by the
    export itself, last in each entity. The output MUST be the export of
    the entity's subgraph under the export schema policy: valid JSON
    conforming to the published Graph Protocol schema of its format, with
    or without coverage, with a schema_version field (a graph-format query
    references the published schema). An entity that does not exist MUST
    be E003, naming the closest
    entity, with exit code 1. The CLI and MCP specforge.query MUST return
    the same document for the same arguments. This enables agents to
    request exactly the context slice they need without consuming the full
    graph. Filtering builds on a graph-level node filter that accepts any
    predicate over an entity's kind and fields.
  """
  verify unit "depth 0 returns only the target entity"
  verify unit "depth 1 returns direct neighbors"
  verify unit "depth N returns all entities within N hops"
  verify unit "kind filter restricts results to specified entity kinds"
  verify unit "multiple kind filters combine as union"
  verify unit "an unknown --kind is reported with I020 and the closest kind"
  verify unit "a non-existent entity is E003 naming the closest entity"
  verify unit "--include-coverage gives each entity its coverage status"
  verify unit "output conforms to Graph Protocol schema"
  verify unit "a query with --include-coverage validates against the published schema of its format"
  verify unit "output includes schema_version field"
  verify property "querying same entity at same depth produces identical subgraph"
  verify unit "filter_nodes with field predicate"
  verify unit "an unknown format is refused naming the expected formats"
  verify contract "Query Graph at Multiple Resolutions: multi-resolution graph query holds — validation_complete_fired, depth_respected, kind_filter_applied, graph_protocol_conformance, graph_queried_emitted"
}

// ── Token Economics (Principle 3: agents are first-class consumers) ────

behavior enforce_token_budget "Enforce Token Budget" {
  invariants [
    graph_traversal_integrity,
    diagnostic_determinism,
    token_budget_subgraph_consistency,
    zero_domain_knowledge_core,
  ]
  category   query
  types      [AgentExportConfig, TokenBudgetResult, Graph, OutputFile, ExportResult, TokenBudgetStrategy]
  ports      [CompilerApi]
  consumes   [validation_complete]
  produces   [token_budget_applied]
  requires {
    validation_complete_fired "validation_complete event has fired, confirming the graph is ready for token estimation"
  }
  ensures {
    budget_respected                "Output token count does not exceed the specified --max-tokens budget"
    truncation_metadata_produced    "TokenBudgetResult is included in output metadata when budget is applied"
    valid_subgraph_after_truncation "Remaining subgraph after truncation has no dangling edge references"
    token_budget_applied_emitted    "token_budget_applied event is emitted after budget enforcement completes"
  }
  contract   """
    When specforge export is invoked with --max-tokens, the system MUST
    estimate the output token count before serialization. If the estimate
    exceeds the budget, the system MUST apply a truncation strategy:
    prioritize entities by graph centrality, truncate low-priority entities,
    and include a TokenBudgetResult in the output metadata. The strategy
    field names the approach used; the one strategy is `prioritize`. If no
    --max-tokens is specified, this behavior MUST be skipped.
    The TokenBudgetResult MUST list any truncated entity IDs so agents can
    request them individually via specforge query. When truncating entities
    from the budget, the system MUST remove all edges to and from truncated
    entities before serialization. The remaining subgraph MUST be a valid
    graph with no dangling edge references. The truncated_entities list in
    TokenBudgetResult records which entities were removed. Centrality is
    degree centrality: the number of edges to and from other entities, the
    degree read_views_over_the_project_view counts (an edge from an entity
    to itself counts in neither); neither the strategy nor the metric is
    configurable.

    The graph, context and brief exports are budgeted alike: each lists the
    entities it dropped under `token_budget`, keeps no entity when only its
    envelope and that block fit, and fails with E062 when even those, or an
    embedded schema (which is never cut short), are over the budget. A scoped
    export never drops its scope entity (the export keeps it always): it keeps
    that entity alone when only it, the envelope and that block fit, and fails
    with E062 when even those do not.
  """
  verify unit "output within budget includes all entities"
  verify unit "output exceeding budget truncates low-priority entities"
  verify unit "TokenBudgetResult included in metadata when budget applied"
  verify unit "truncated_entities lists omitted entity IDs"
  verify unit "no --max-tokens skips budget enforcement"
  verify unit "an edge from an entity to itself adds nothing to its centrality"
  verify integration "export with max_tokens produces output within budget and includes metadata"
  verify integration "the graph export honours --max-tokens with the schema left out unless --with-schema is given"
  verify integration "an embedded schema counts toward the token budget"
  verify integration "a budget smaller than the embedded schema fails with E062 instead of truncating the schema"
  verify integration "a budget below one entity yields the envelope with no entities and the truncation marker"
  verify integration "a budget below the empty envelope fails with E062"
  verify unit "a token budget never cuts the scope entity of a scoped export"
  verify unit "a scoped export whose budget cannot hold its scope entity alone fails with E062"
  verify integration "the context and brief exports honour --max-tokens, listing the dropped entities under token_budget"
  verify integration "a context or brief export that cannot fit even without entities, or whose embedded schema is over the budget, fails with E062"
  verify contract "Enforce Token Budget: token budget enforcement holds — validation_complete_fired, budget_respected, truncation_metadata_produced, valid_subgraph_after_truncation, token_budget_applied_emitted"
}
