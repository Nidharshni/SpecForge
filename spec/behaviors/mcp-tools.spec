// MCP Tool behaviors — Core tools and navigation tools
//
// 15 behaviors:
//   - Core Tools (9): query, validate, export, trace, search, schema, coverage, stats, analyze
//   - Navigation Tools (5): inspect, find_definition, find_references, outline, suggest_fixes
//   - Dynamic kinds (1): specforge.list and the entities-by-kind resource

use "events/mcp"
use "invariants/core"
use "invariants/mcp"
use "invariants/validation"
use "ports/inbound"
use "ports/outbound"
use "types/diagnostics"
use "types/graph"
use "types/mcp"
use "types/output"

// ---------------------------------------------------------------------------
// Core Tools
// ---------------------------------------------------------------------------

behavior provide_mcp_query_tool "Provide MCP Query Tool" {
  features   [mcp_core_tools]
  invariants [
    graph_traversal_integrity,
    diagnostic_determinism,
    mcp_structured_error_responses,
    mcp_tool_idempotency,
  ]
  category   query
  types      [Graph, AgentExportConfig, McpToolDescriptor]
  ports      [McpProtocol, CompilerApi]
  produces   [mcp_tool_invoked]
  requires {
    graph_available "Compiled graph is available via CompilerApi"
  }
  ensures {
    subgraph_returned      "Subgraph rooted at entityId returned up to requested depth"
    unknown_kinds_reported "Unknown kind values silently filtered with I-level diagnostic in metadata"
    tool_invoked_emitted   "mcp_tool_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge.query tool that
    accepts entityId (required), depth? (optional integer), kind?[] (optional
    array of entity kinds to include), include_coverage? (optional boolean),
    and format? (optional: graph|context|brief). The tool MUST return the
    subgraph rooted at the specified entity up to the requested depth. When
    kind is specified, only nodes matching those kinds MUST be included.
    Unknown kind values in the kind[] array MUST be silently filtered out
    and an I-level diagnostic MUST be included in the response metadata
    listing the unrecognized kinds. When format is specified, the output
    MUST use that serialization format. An unknown format MUST be an
    invalid-input error on format naming the expected formats (graph,
    context, brief). If entityId does not exist, the tool MUST return an
    error response. The result MUST be the document specforge query prints
    for the same arguments; at the default depth in the graph format it is
    the document specforge://graph/{entityId} serves.
  """
  verify unit "specforge.query tool returns subgraph for valid entityId"
  verify unit "depth parameter limits traversal depth"
  verify unit "kind filter restricts returned node types"
  verify unit "format parameter changes output serialization"
  verify unit "non-existent entityId returns error response"
  verify unit "an unknown format is an invalid-input error naming the expected formats"
  verify unit "include_coverage parameter includes coverage status in response"
  verify unit "the default query is the document specforge://graph/{entityId} serves"
  verify contract "Provide MCP Query Tool: MCP query tool holds — graph_available, subgraph_returned, unknown_kinds_reported, tool_invoked_emitted"
  verify unit "unknown tool returns error"
}

// Idempotency here means result equivalence: the same input always produces
// the same output. It does NOT imply execution caching — each invocation
// performs a full compilation pass.
behavior provide_mcp_validate_tool "Provide MCP Validate Tool" {
  features   [mcp_core_tools]
  invariants [
    multi_error_collection,
    diagnostic_determinism,
    mcp_structured_error_responses,
    mcp_tool_idempotency,
  ]
  category   validation
  types      [DiagnosticBag, McpToolDescriptor]
  ports      [McpProtocol, CompilerApi]
  produces   [mcp_tool_invoked]
  requires {
    compiler_api_available "CompilerApi port is available for triggering compilation"
  }
  ensures {
    diagnostics_returned      "All diagnostics matching filter returned with severity, message, file path, and line number"
    strict_promotion_enforced "When strict is true, warnings promoted to errors in response"
    tool_invoked_emitted      "mcp_tool_invoked event emitted"
    verdict_in_meta           "_meta[\"specforge/check\"] carries ok and the error, warning and info counts of everything reported, and how many diagnostics are shown"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge.validate tool
    that brings the served project up to date with disk (its diagnostics
    are what specforge check reports) and returns validation results as
    Graph Protocol diagnostics. The tool accepts path? (optional: the
    project; another project is opened for the call only),
    severity_filter? (optional: error, warning or info, matched ignoring
    case; any other value is invalid input), strict? (optional boolean,
    treat warnings as errors), lint? (optional list of inferred and
    pedantic; any other name is invalid input).
    The response MUST include all diagnostics matching the filter with their
    severity, message, file path, and line number, and each diagnostic whose
    code is catalogued MUST carry the catalogue's title for it (null for any
    other code). When strict is true,
    warnings MUST be promoted to errors in the response.

    A run that finds errors is a successful call (isError false, ADR 0004
    D4-a); whether the check passed is in _meta["specforge/check"].ok,
    over every reported diagnostic whatever severity_filter shows. The
    tool never writes the build cache.
  """
  verify unit "each catalogued diagnostic carries its title"
  verify unit "specforge.validate tool triggers compilation"
  verify unit "response includes all diagnostics as Graph Protocol diagnostics"
  verify unit "severity_filter restricts returned diagnostics"
  verify unit "strict mode promotes warnings to errors"
  verify integration "validate with lint profiles reports what specforge check reports with the same profiles"
  verify unit "validate reports files changed on disk since the last call"
  verify unit "an unknown severity_filter or lint profile is invalid input"
  verify unit "the verdict on every reported diagnostic rides in _meta, whatever severity_filter shows"
  verify contract "Provide MCP Validate Tool: MCP validate tool holds — compiler_api_available, diagnostics_returned, strict_promotion_enforced, tool_invoked_emitted, verdict_in_meta"
}

behavior provide_mcp_export_tool "Provide MCP Export Tool" {
  features   [mcp_core_tools]
  invariants [
    graph_traversal_integrity,
    graph_schema_completeness,
    diagnostic_determinism,
    mcp_structured_error_responses,
    mcp_tool_idempotency,
  ]
  category   query
  types      [Graph, AgentExportConfig, OutputFile, McpToolDescriptor]
  ports      [McpProtocol, CompilerApi]
  produces   [mcp_tool_invoked]
  requires {
    graph_available "Compiled graph is available via CompilerApi"
  }
  ensures {
    format_produced       "Graph returned in requested agent-optimized format conforming to Graph Protocol schema"
    token_budget_enforced "When max_tokens specified, output truncated to fit budget prioritizing high-connectivity nodes"
    tool_invoked_emitted  "mcp_tool_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge.export tool that
    accepts format (required: context|brief|graph), scope? (optional entityId
    to restrict to subgraph), max_tokens? (optional integer token budget),
    with_schema? and no_schema? (optional booleans), depth? (with scope,
    hops from the scoped entity), kinds? (entity kinds to keep) and
    schema_version? (as specforge export --schema-version). The tool MUST return the
    graph in the requested agent-optimized format. When max_tokens is
    specified, the output MUST be truncated to fit within the budget,
    prioritizing high-connectivity nodes. The output MUST conform to the
    Graph Protocol schema. The tool MUST produce the export specforge export
    produces for the same arguments, through the same function and schema
    policy: a full graph export is Graph Protocol 2.0 with the schema
    embedded, a scoped one carries a schema_ref, and context, brief and a
    budgeted export leave the schema out unless with_schema is true;
    no_schema leaves it out of a graph export (format 1.0). with_schema and
    no_schema exclude each other: both MUST be refused as invalid input on
    with_schema, as specforge export refuses --with-schema with
    --no-schema. The embedded
    schema carries the version specforge export computes against the
    project's schema cache; the tool only reads the cache.
  """
  verify unit "specforge.export takes the options specforge export takes"
  verify unit "specforge.export tool returns graph in requested format"
  verify unit "scope parameter restricts to subgraph"
  verify unit "max_tokens truncates output to fit token budget"
  verify unit "a token budget too small for the export is invalid_input carrying E062"
  verify unit "all three formats (context, brief, graph) supported"
  verify contract "Provide MCP Export Tool: MCP export tool holds — graph_available, format_produced, token_budget_enforced, tool_invoked_emitted"
  verify unit "unknown format returns error"
  verify integration "the graph export is the document specforge export --format graph writes, Graph Protocol 2.0 with the schema embedded"
  verify integration "with_schema embeds the schema in a context, brief or budgeted export, and no_schema leaves it out of a graph export"
  verify integration "the embedded schema carries the version specforge export computes against the schema cache, which the tool leaves as it is"
  verify integration "specforge export and specforge.export both refuse with_schema with no_schema"
  verify unit "with_schema and no_schema together are invalid input on with_schema"
}

behavior provide_mcp_trace_tool "Provide MCP Trace Tool" {
  features   [mcp_core_tools]
  invariants [
    graph_traversal_integrity,
    reference_resolution_completeness,
    diagnostic_determinism,
    mcp_structured_error_responses,
    mcp_tool_idempotency,
  ]
  category   query
  types      [
    Graph,
    TraceChain,
    TraceLink,
    McpTraceResult,
    McpTraceChainResult,
    McpTracePlanResult,
    McpToolDescriptor,
  ]
  ports      [McpProtocol, CompilerApi]
  produces   [mcp_tool_invoked]
  requires {
    graph_available "Compiled graph is available via CompilerApi"
  }
  ensures {
    trace_result_returned "TraceChain or McpTracePlanResult returned depending on input parameter"
    gaps_identified       "Missing links flagged in the trace document"
    tool_invoked_emitted  "mcp_tool_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge.trace tool that
    accepts entityId? (optional) and plan? (optional: an AgentPlan, or its JSON text). When entityId
    is provided, the tool MUST delegate to compute_traceability_chain to traverse
    the graph upstream and downstream from the entity and return the document
    `specforge trace <entity> --format json` writes: the TraceChain with its
    TraceLink entries and its missing links, the expected edges the entity
    lacks. It carries no other gap list. When plan is provided, the tool
    MUST perform the plan check (validate_agent_plan) and return its document,
    a McpTracePlanResult: ok, the affected entities and the gaps. At least one of entityId
    or plan MUST be provided; otherwise the tool MUST return an error.
    entityId and plan are two requests: a call with both MUST be refused as
    invalid input on plan, as specforge trace refuses an entity with --plan.
  """
  verify unit "specforge.trace tool returns traceability chain for valid entityId"
  verify unit "plan parameter triggers gap analysis"
  verify unit "trace without entity_id or plan returns error"
  verify unit "non-existent entityId returns error response"
  verify unit "response includes upstream and downstream links"
  verify unit "missing links flagged in trace output"
  verify unit "a trace call with both entity_id and plan is invalid input on plan"
  verify contract "Provide MCP Trace Tool: MCP trace tool holds — graph_available, trace_result_returned, gaps_identified, tool_invoked_emitted"
}

behavior provide_mcp_search_tool "Provide MCP Search Tool" {
  features   [mcp_core_tools]
  invariants [
    graph_traversal_integrity,
    diagnostic_determinism,
    mcp_structured_error_responses,
    mcp_tool_idempotency,
  ]
  category   query
  types      [Graph, McpSearchResults, McpSearchResult, McpToolDescriptor]
  ports      [McpProtocol, CompilerApi]
  produces   [mcp_tool_invoked]
  requires {
    graph_available "Compiled graph is available via CompilerApi"
  }
  ensures {
    filtered_results_returned "Search results returned combining all filters with AND semantics"
    unknown_kinds_reported    "Unknown kind values silently filtered with I-level diagnostic in metadata"
    tool_invoked_emitted      "mcp_tool_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge.search tool that
    accepts query (required; empty matches every entity), kinds?[], field?
    and value? (together: only entities whose field's text, by the
    one field-text rule, contains the value, ignoring case: a list item by
    item, an expression in its written form, a block by its keys; one without the other MUST be an invalid-input
    error naming the missing one), references? (an entity id: only
    entities that reference it), and limit? (default 20). The tool MUST combine these filters with
    AND semantics. Unknown kind values in the kinds[] array MUST be
    silently filtered out and an I-level diagnostic MUST be included in the
    response metadata listing the unrecognized kinds. Fuzzy text search
    MUST use the same algorithm as LSP workspaceSymbol and completion:
    exact, prefix, substring, field text (search only), then Jaro-Winkler
    similarity of at least 0.8 over ID and title. An empty query with no
    filters MUST return all entities up to the limit. The result is an
    object whose results are the hits, McpSearchResult items, in rank order.
  """
  verify unit "text search finds entities matching by name or contract"
  verify unit "kind filter restricts results to matching entity kinds"
  verify unit "field and value filter matches entity fields"
  verify unit "field without value, or value without field, is an invalid-input error"
  verify unit "the field filter reads a field's text: a list by its items, an expression by its written form, a block by its keys; never the export's JSON"
  verify unit "limit caps the number of returned results"
  verify unit "empty query returns all entities up to limit"
  verify unit "references filter returns entities referencing target"
  verify unit "the references filter combines with the other filters"
  verify integration "search ranks exactly as LSP workspaceSymbol and completion rank"
  verify contract "Provide MCP Search Tool: MCP search tool holds — graph_available, filtered_results_returned, unknown_kinds_reported, tool_invoked_emitted"
  verify unit "missing query returns error"
  verify unit "the result is an object holding the hits as results, with structuredContent"
}

behavior provide_mcp_explain_tool "Provide MCP Explain Tool" {
  features   [mcp_core_tools]
  invariants [diagnostic_determinism, mcp_structured_error_responses, mcp_tool_idempotency]
  category   query
  types      [McpExplainResult, McpToolDescriptor]
  ports      [McpProtocol]
  produces   [mcp_tool_invoked]
  requires {
    code_given "The caller names a diagnostic code"
  }
  ensures {
    entry_returned       "The catalogued entry for the code is returned: title, owner, level, explanation and docs link"
    tool_invoked_emitted "mcp_tool_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge.explain tool
    that takes code (case-insensitive) and returns what specforge explain
    prints for it, as data: the code, its title, its owner (core or the
    emitting extension), its level, its explanation and the link to its
    section of docs/diagnostics.md. A retired code returns retired true and
    the entry of the code that replaced it, if any. It is the operation
    specforge explain runs (explain_diagnostic_code). A code the catalogue
    neither has nor retired MUST be an invalid_input error naming the code
    argument, its suggestion the code forms.
  """
  verify unit "specforge.explain returns the catalogued title, owner, level, explanation and docs link"
  verify unit "a retired code names the code that replaced it"
  verify unit "an uncatalogued code is an invalid_input error"
}

behavior provide_mcp_schema_tool "Provide MCP Schema Tool" {
  features   [mcp_core_tools]
  invariants [
    graph_schema_completeness,
    diagnostic_determinism,
    mcp_structured_error_responses,
    mcp_tool_idempotency,
  ]
  category   query
  types      [GraphProtocolSchema, McpToolDescriptor]
  ports      [McpProtocol, CompilerApi]
  produces   [mcp_tool_invoked]
  requires {
    graph_available "Compiled graph is available via CompilerApi"
  }
  ensures {
    schema_returned      "GraphProtocolSchema returned, optionally filtered by kind"
    tool_invoked_emitted "mcp_tool_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge.schema tool that
    accepts kind? (optional entity kind), include_edges? (optional boolean,
    default true), and include_validation_rules? (optional boolean, default
    false). The tool MUST return the GraphProtocolSchema, optionally filtered
    to a single entity kind. When include_edges is false, edge type definitions
    MUST be omitted. When include_validation_rules is true, the response MUST
    include declared validation rules from loaded extensions.
    The schema MUST carry the version `specforge export` would give it,
    computed against the project root's schema cache, which the tool only
    reads. A kind no loaded extension declares MUST be an invalid-input
    error on kind naming the closest known kind, as `specforge schema
    --kind` refuses it.
  """
  verify unit "specforge.schema returns full GraphProtocolSchema"
  verify unit "an unknown kind is an invalid-input error naming the closest kind"
  verify unit "kind filter restricts schema to single entity kind"
  verify unit "include_edges false omits edge type definitions"
  verify unit "include_validation_rules true includes validation rules"
  verify contract "Provide MCP Schema Tool: MCP schema tool holds — graph_available, schema_returned, tool_invoked_emitted"
}

behavior provide_mcp_coverage_tool "Provide MCP Coverage Tool" {
  features   [mcp_core_tools]
  invariants [
    graph_traversal_integrity,
    diagnostic_determinism,
    mcp_structured_error_responses,
    mcp_tool_idempotency,
    testable_entity_classification,
  ]
  category   query
  types      [McpCoverageResults, McpCoverageResult, McpToolDescriptor, CoverageStatus]
  ports      [McpProtocol, CompilerApi]
  produces   [mcp_tool_invoked]
  requires {
    graph_available "Compiled graph is available via CompilerApi"
  }
  ensures {
    coverage_returned     "Coverage status per entity returned including verify count and evidence count"
    testability_respected "Testability determined by extension manifests, not hardcoded"
    tool_invoked_emitted  "mcp_tool_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge.coverage tool
    that accepts entity_id? (optional, single entity), kind? (optional, filter
    by entity kind), and status_filter? (optional: covered|uncovered|partial).
    Status values are structurally computed from the entity's verify declarations
    and collected evidence — no extension input is required to determine coverage
    status (P2). An obligation is proven when a passing recorded test names its
    verify text, the rule analyze coverage applies: an entity is covered only
    when every obligation is proven and no recorded test fails, so it never
    reports covered while analyze reports A015 or A014. Each result MUST carry
    the obligation count, the proven count, and the unproven verify texts. A verify
    property obligation counts as proven only through a passing test, as in
    analyze coverage without --prove: the SMT discharge --prove adds is not
    run per call. The tool MUST return an object whose entities are the coverage status
    per entity, including verify count, linked evidence count, and evidence status from
    specforge-report.json if available.
    A specforge-report.json that exists but cannot be read or parsed MUST be
    an error, an isError result carrying an McpError, as the CLI refuses it;
    it is never read as a project with no recorded tests. A report the
    system refuses to read is permission_denied, on this tool and every tool
    that reads it.
    When no filters are provided, the tool MUST return coverage for every
    entity that counts toward coverage: an entity of a kind the extension
    manifests declare testable, less the entities W004 exempts that
    declare no obligations (union types, abstract entities, governance
    kinds), the entities stats counts as testable. An entity_id filter
    returns that entity whether it counts or not; each result says
    whether the entity is exempt. A status_filter other than covered,
    uncovered or partial MUST be an invalid-input error on status_filter
    naming the closest status.
  """
  verify unit "specforge.coverage returns coverage for all testable entities"
  verify unit "the result is an object holding the rows as entities"
  verify unit "a report the OS refuses to read is a permission_denied error on every tool that reads it"
  verify unit "with no filters the rows are the entities that count toward coverage"
  verify unit "an exempt entity named by entity_id is returned with exempt true"
  verify unit "an unknown status_filter is an invalid-input error naming the closest status"
  verify unit "entity_id filter returns single entity coverage"
  verify unit "kind filter restricts to matching entity kinds"
  verify unit "status_filter restricts to matching coverage status"
  verify unit "an entity with an unproven obligation is partial, not covered"
  verify unit "a failing recorded test keeps an entity from being covered"
  verify unit "a field named verify does not hide an entity's verify statements"
  verify unit "a malformed specforge-report.json is an error result, not an empty report"
  verify integration "specforge.coverage reports covered exactly for the entities analyze coverage proves"
  verify integration "with no filters the coverage rows are the entities stats counts as testable"
  verify contract "Provide MCP Coverage Tool: MCP coverage tool holds — graph_available, coverage_returned, testability_respected, tool_invoked_emitted"
}

behavior provide_mcp_stats_tool "Provide MCP Stats Tool" {
  features   [mcp_core_tools]
  invariants [
    graph_traversal_integrity,
    diagnostic_determinism,
    mcp_structured_error_responses,
    mcp_tool_idempotency,
  ]
  category   query
  types      [McpStatsResult, McpToolDescriptor, McpEntityCount, McpDiagnosticSummary]
  ports      [McpProtocol, CompilerApi]
  produces   [mcp_tool_invoked]
  requires {
    graph_available "Compiled graph is available via CompilerApi"
  }
  ensures {
    stats_returned         "Aggregate statistics returned: entity counts, edge count, coverage, unconnected entities, diagnostics"
    latest_state_reflected "Response reflects the latest compilation state"
    tool_invoked_emitted   "mcp_tool_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge.stats tool with
    no required parameters. The tool MUST return aggregate statistics about the
    current graph: entity counts by kind, total edge count, the declared
    percentage (declared_pct), the proof
    percentage (proof_pct, absent without recorded test results), the unconnected entity
    count (unconnected_count), and a diagnostic summary (counts by severity). A
    specforge-report.json that exists but cannot be read is an error result. The
    response MUST reflect the latest compilation state.
  """
  verify unit "specforge.stats returns entity counts by kind"
  verify unit "response includes coverage percentage"
  verify integration "response includes the declared and proof percentages"
  verify unit "response includes the unconnected entity count"
  verify unit "response includes diagnostic summary by severity"
  verify contract "Provide MCP Stats Tool: MCP stats tool holds — graph_available, stats_returned, latest_state_reflected, tool_invoked_emitted"
}

// ---------------------------------------------------------------------------
// Navigation Tools
// ---------------------------------------------------------------------------

behavior provide_mcp_inspect_tool "Provide MCP Inspect Tool" {
  features   [mcp_navigation_tools]
  invariants [
    graph_traversal_integrity,
    diagnostic_determinism,
    mcp_structured_error_responses,
    mcp_tool_idempotency,
    testable_entity_classification,
  ]
  category   query
  types      [McpInspectResult, McpToolDescriptor]
  ports      [McpProtocol, CompilerApi]
  produces   [mcp_tool_invoked]
  requires {
    graph_available "Compiled graph is available via CompilerApi"
  }
  ensures {
    entity_details_returned "Full entity details returned: kind, fields, contract, references, verify, standing, coverage, diagnostics"
    tool_invoked_emitted    "mcp_tool_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge.inspect tool that
    accepts entity_id (required). The tool MUST return full entity details
    including kind, fields, contract text, references, verify declarations,
    coverage status, and related diagnostics. Its testable field is the
    testability of the entity's kind, the standing the LSP hover shows; a
    separate declared field says whether the entity declares at least one
    verify obligation, and exempt whether its kind is testable but it owes
    no obligations and declares none, so it does not count toward
    coverage (specforge.coverage's row says the same). obligated says
    whether its kind must declare obligations (a no_verify_statements rule
    targets it), the reason an exempt entity is exempt. source_extension
    names the extension that declares its kind, absent when no loaded
    extension does. The fields MUST include every
    field the entity declares, whatever its kind names them (an invariant's
    guarantee, a decision's rationale), not just contract. The coverage status
    MUST count the recorded test results in specforge-report.json exactly as
    specforge.coverage does. References are split by direction:
    referenced_by (incoming) and refers_to (outgoing). The related diagnostics,
    in the shape specforge.validate reports them, are those about the entity: the entities a diagnostic's data names, or,
    when its data names none, the innermost entity whose source span holds
    the diagnostic's span. A diagnostic's message is never read; an entity
    whose ID is a prefix of another's never collects the other's
    diagnostics. LSP equivalence: this tool and textDocument/hover render
    one read view of the entity, so its kind, standing, references,
    coverage and diagnostics are the same on both. If the entity does not
    exist, the tool MUST return an error response.
  """
  verify unit "specforge.inspect returns full entity details"
  verify unit "response includes referenced_by, refers_to and verify declarations"
  verify unit "an untitled entity's reply has no title"
  verify unit "the entity's diagnostics are in the shape validate reports them"
  verify unit "non-existent entity returns error response"
  verify unit "response includes every field, like an invariant's guarantee"
  verify unit "coverage status matches specforge.coverage obligation by obligation"
  verify unit "diagnostics are the entity's own, not those of an entity whose ID contains it"
  verify unit "a spanless diagnostic belongs to the entities its data names, never to one its message quotes"
  verify unit "testable is the kind's testability and declared says whether the entity has obligations"
  verify unit "exempt says the entity does not count toward coverage, as specforge.coverage's row says"
  verify unit "obligated says whether the entity's kind must declare obligations"
  verify unit "source_extension names the extension that declares the entity's kind"
  verify contract "Provide MCP Inspect Tool: MCP inspect tool holds — graph_available, entity_details_returned, tool_invoked_emitted"
}

behavior provide_mcp_find_definition_tool "Provide MCP Find Definition Tool" {
  features   [mcp_navigation_tools]
  invariants [
    graph_traversal_integrity,
    diagnostic_determinism,
    mcp_structured_error_responses,
    mcp_tool_idempotency,
  ]
  category   query
  types      [McpDefinitionResult, McpToolDescriptor]
  ports      [McpProtocol, CompilerApi]
  produces   [mcp_tool_invoked]
  requires {
    graph_available "Compiled graph is available via CompilerApi"
  }
  ensures {
    source_location_returned "Source location returned including file path, line number, and column"
    name_position            "the position is the entity's name"
    tool_invoked_emitted     "mcp_tool_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge.find_definition
    tool that accepts entity_id (required). The tool MUST return the source
    location where the entity is defined, including file path, line number,
    and column. LSP equivalence: this tool mirrors textDocument/definition
    (gotoDefinition), returning the same source location an IDE navigates to
    but over the MCP transport. If the entity does not exist, the tool MUST
    return an error.
  """
  verify unit "specforge.find_definition returns file, line, and column"
  verify unit "non-existent entity returns error response"
  verify contract "Provide MCP Find Definition Tool: MCP find definition tool holds — graph_available, source_location_returned, tool_invoked_emitted"
}

behavior provide_mcp_find_references_tool "Provide MCP Find References Tool" {
  features   [mcp_navigation_tools]
  invariants [
    graph_traversal_integrity,
    diagnostic_determinism,
    mcp_structured_error_responses,
    mcp_tool_idempotency,
  ]
  category   query
  types      [McpReferenceResult, McpToolDescriptor, McpReferenceLocation]
  ports      [McpProtocol, CompilerApi]
  produces   [mcp_tool_invoked]
  requires {
    graph_available "Compiled graph is available via CompilerApi"
  }
  ensures {
    references_returned         "All reference locations returned with entity id, file path, line, and column"
    empty_list_for_unreferenced "Entity with no references returns empty list, not an error"
    tool_invoked_emitted        "mcp_tool_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge.find_references
    tool that accepts entity_id (required), direction? ("incoming", the
    default; "outgoing"; "both") and include_declaration? (default false).
    The tool MUST return each occurrence: the referencing entity's id, the
    referenced entity's id, the field, its role (declaration or reference)
    and the identifier token's source span. LSP equivalence: with the
    defaults it returns the occurrences textDocument/references returns
    with includeDeclaration false. An entity with no references MUST return
    an empty list, not an error.
  """
  verify unit "specforge.find_references returns all reference locations"
  verify unit "entity with no references returns empty list"
  verify unit "non-existent entity returns error response"
  verify unit "direction and include_declaration select which occurrences are returned"
  verify integration "find_references and the LSP's references answer the same occurrences"
  verify contract "Provide MCP Find References Tool: MCP find references tool holds — graph_available, references_returned, empty_list_for_unreferenced, tool_invoked_emitted"
}

behavior provide_mcp_find_implementation_tool "Provide MCP Find Implementation Tool" {
  features   [mcp_navigation_tools]
  invariants [diagnostic_determinism, mcp_structured_error_responses, mcp_tool_idempotency]
  category   query
  types      [McpImplementationResult, McpToolDescriptor]
  ensures {
    anchors_listed   "every anchor of the entity in specforge-anchors.json, in manifest order"
    empty_when_none  "an entity with no anchor, or no anchors manifest, has no implementations"
    unusable_refused "an anchors manifest that cannot be used is E071"
  }
  contract   """
    In MCP server mode, the system MUST register a
    specforge.find_implementation tool that accepts entity_id (required)
    and returns {entity_id, implementations, count}: each anchor of the
    entity in the project's specforge-anchors.json (file, line,
    symbol_name, item_kind, scanner), in manifest order, from the one
    anchor lookup navigation owns. No anchors manifest is an empty list.
    An anchors manifest that cannot be read or parsed is refused with
    E071.
  """
  verify unit "find_implementation lists every anchor of the entity, in manifest order"
  verify unit "an entity with no anchor has no implementations, and no anchors manifest is none"
}

behavior provide_mcp_outline_tool "Provide MCP Outline Tool" {
  features   [mcp_navigation_tools]
  invariants [
    graph_traversal_integrity,
    diagnostic_determinism,
    mcp_structured_error_responses,
    mcp_tool_idempotency,
  ]
  category   query
  types      [McpOutlineResult, McpOutlineEntry, McpOutlineChild, McpToolDescriptor]
  ports      [McpProtocol, CompilerApi]
  produces   [mcp_tool_invoked]
  requires {
    graph_available "Compiled graph is available via CompilerApi"
  }
  ensures {
    outline_returned     "All entities in file returned as McpOutlineEntry with id, kind, name, line range, and children"
    tool_invoked_emitted "mcp_tool_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge.outline tool that
    accepts file (required). The tool MUST return an object whose entries are
    the entities defined in the file, McpOutlineEntry items, including entity id, kind, name, line
    range, the range of its name, and any nested children (an entity's
    method members). LSP equivalence: this tool mirrors
    textDocument/documentSymbol, returning the same outline structure an IDE
    shows in its symbol navigator but over the MCP transport. If the file
    does not exist, the tool MUST return an error. With no project served, no
    file is a project's: the tool MUST return the no-project refusal
    (precondition_failed), whatever the server's working directory holds.
  """
  verify unit "specforge.outline returns all entities defined in file"
  verify unit "the result is an object holding the outline as entries"
  verify unit "nested entries included for complex entities"
  verify unit "non-existent file returns error response"
  verify contract "Provide MCP Outline Tool: MCP outline tool holds — graph_available, outline_returned, tool_invoked_emitted"
  verify unit "outline entries sorted by line number"
  verify unit "a file under the spec root with no entities has an empty outline"
  verify unit "with no project served, outline is the no-project refusal"
  verify unit "sorted by line number"
}

behavior provide_mcp_suggest_fixes_tool "Provide MCP Suggest Fixes Tool" {
  features   [mcp_navigation_tools]
  invariants [
    graph_traversal_integrity,
    diagnostic_determinism,
    mcp_structured_error_responses,
    mcp_tool_idempotency,
  ]
  category   query
  types      [McpFixSuggestions, McpFixSuggestion, McpToolDescriptor]
  ports      [McpProtocol, CompilerApi]
  produces   [mcp_tool_invoked]
  requires {
    graph_available "Compiled graph is available via CompilerApi"
  }
  ensures {
    fixes_returned       "Applicable fix suggestions returned as McpFixSuggestion items with title, edits, and diagnostic"
    empty_for_clean      "Clean entity with no diagnostics returns empty list"
    tool_invoked_emitted "mcp_tool_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge.suggest_fixes tool
    that accepts entity_id? (optional), file_path? (optional), and
    diagnostic_code? (optional). When all three parameters are omitted, the
    system MUST return all fix suggestions for the current project. The tool
    MUST return an object whose fixes are the applicable fix suggestions, McpFixSuggestion items, each
    including a title, edit operations, and the diagnostic it resolves.
    LSP equivalence: this tool mirrors textDocument/codeAction, returning
    the same quick-fix suggestions an IDE offers but over the MCP
    transport. Each suggestion is a fix the LSP offers as a code action for
    the same diagnostic or entity, with the same title and the same edits
    (file_path, range, new_text); a diagnostic whose data names no fix
    contributes none (its suggestion text stays on the diagnostic). Fixes
    read the diagnostic's data, never its message. A clean entity with no
    diagnostics MUST return no fixes.
  """
  verify unit "specforge.suggest_fixes returns applicable fix suggestions"
  verify unit "the result is an object holding the fixes"
  verify unit "clean entity with no diagnostics returns empty list"
  verify unit "diagnostic_code filter restricts to matching diagnostics"
  verify integration "every suggestion carries the edits the LSP's code action applies"
  verify contract "Provide MCP Suggest Fixes Tool: MCP suggest fixes tool holds — graph_available, fixes_returned, empty_for_clean, tool_invoked_emitted"
}

behavior provide_mcp_analyze_tool "Provide MCP Analyze Tool" {
  features   [mcp_core_tools]
  invariants [diagnostic_determinism, mcp_structured_error_responses, mcp_tool_idempotency]
  category   query
  types      [McpAnalyzeResult, McpToolDescriptor]
  ports      [McpProtocol, CompilerApi]
  produces   [mcp_tool_invoked]
  requires {
    graph_available "Compiled graph is available via CompilerApi"
  }
  ensures {
    passes_run           "the requested analysis pass, or every pass, runs over the compiled project"
    results_structured   "each pass's findings and summary are returned with an overall ok flag"
    tool_invoked_emitted "mcp_tool_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge.analyze tool
    that runs the same analysis passes as `specforge analyze`: the core
    `contracts` pass and every extension-owned pass (such as
    @specforge/testing's coverage), or only the one named by `pass`. It
    accepts `strict` (warnings become errors), `min` (a proof-coverage
    minimum in percent: below it the analysis is not ok, E048; it needs
    test results and the coverage pass, as `specforge analyze --min`),
    `test_results` (a specforge-report.json path)
    and `path` (another project, opened for the call only, its extension
    passes run in the one runtime it was opened with). With no project
    served and no `path` there is nothing to analyze: the call MUST be an
    isError result with a no-project McpError. Without `test_results` it MUST read the
    project's own specforge-report.json when one exists, as the CLI does,
    so proof coverage never silently drops. A test report that cannot be read
    or parsed, the project's own or the one `test_results` names, MUST be an
    isError result carrying an McpError, as the CLI exits 2 on it. A
    test_results file that does not exist is file_not_found. A relative
    test_results names a file under the project's root. The result MUST list each pass with its
    findings and summary, plus an `ok` flag that is false when any finding
    is an error or the coverage gate is below its minimum or unjudged,
    and, with `min`, a `gate` object saying where the gate landed. An extension pass that traps or answers what does not
    parse is one E028 finding of that pass (its summary marks it
    `failed`), so the analysis is not ok. The tool does not run the prove
    pass, so extension passes
    MUST receive no proved claims, as `specforge analyze` without --prove.
    The tool runs through the shared analyze operation of specforge-ops, the
    one the CLI runs: a `pass` that is not `all`, `coverage`, `contracts` or a
    declared `<extension>:<pass>` MUST be an invalid-input error on `pass`
    listing the available passes, and `strict` MUST be applied once over
    every pass. A `path` naming another project analyzes that project for the
    call and leaves the served project untouched. When the test report holds
    records for entities the graph does not know, the result MUST carry a
    top-level `stray_records` list of `{entity_id, near}` (stray test records), outside the passes and
    never promoted by `strict`; the field is absent when there are none.
  """
  verify unit "analyze reads the project's specforge-report.json by default"
  verify unit "a malformed test report is an error result"
  verify unit "a test_results file that does not exist is a file_not_found error"
  verify unit "a relative test_results names a file under the project's root"
  verify unit "extension passes receive no proved claims, as specforge analyze without --prove"
  verify unit "an unknown or undeclared pass is an invalid-input error listing the available passes"
  verify unit "strict promotes warnings and clears ok"
  verify unit "analyzing another project leaves the served project untouched"
  verify unit "analyze with no project served and no path is a no-project error"
  verify unit "stray test records come back as an optional stray_records field"
  verify unit "specforge.analyze takes min and returns the run verdict and where the gate landed"
  verify contract "Provide MCP Analyze Tool: MCP analyze tool holds — graph_available, passes_run, results_structured, tool_invoked_emitted"
}

behavior provide_mcp_entities_by_kind "List Entities by Kind over MCP" {
  features   [mcp_core_tools]
  invariants [graph_traversal_integrity, mcp_tool_idempotency]
  category   query
  types      [McpListResult, McpListedEntity, McpToolDescriptor]
  ports      [McpProtocol, CompilerApi]
  produces   [mcp_tool_invoked]
  requires {
    graph_available "Compiled graph is available via CompilerApi"
  }
  ensures {
    tool_lists_by_kind     "specforge.list returns the entities of a kind, or all entities without one"
    tool_filters_fields    "specforge.list keeps only the entities whose fields hold every value its where object names"
    tool_pages             "specforge.list returns the entities sorted by id, paged by offset and limit"
    resource_lists_by_kind "specforge://entities/{kind} returns the entities of that kind"
    unknown_kind_empty     "an unknown kind yields an empty list and an I-level diagnostic, not an error"
  }
  contract   """
    Because entity kinds come from extensions, MCP clients need a way to
    enumerate entities without knowing the kinds in advance. The system
    MUST register a specforge.list tool (optional `kind`) and a
    specforge://entities/{kind} resource template. Both MUST return each
    matching entity's id, kind and title: the tool as an object whose
    entities are those rows, the resource as an array of them. An unknown
    kind MUST yield an empty list, and the tool's response metadata MUST carry I020 naming
    the closest kind. The tool MUST also accept a `where` object (field name to
    the value the field holds, any kind's fields, no field known to core)
    and `offset`/`limit`, applied to the entities sorted by id.
    Extensions that list their own kinds their way contribute commands,
    auto-promoted to tools (`specforge.product.features`).
  """
  verify unit "specforge.list returns entities filtered by kind"
  verify unit "the result is an object holding the entities"
  verify unit "specforge.list keeps the entities whose fields hold the where values"
  verify unit "specforge.list pages the entities sorted by id with offset and limit"
  verify unit "specforge.list returns empty for unknown kind"
  verify unit "specforge.list reports an unknown kind with I020"
  verify unit "the entities resource lists what specforge.list lists for the kind"
  verify unit "entity-by-kind resource returns entities"
  verify unit "specforge.list tool appears in tool list"
  verify unit "entities resource template in resource template list"
}
