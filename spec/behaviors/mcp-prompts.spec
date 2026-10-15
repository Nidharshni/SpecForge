// MCP Prompt behaviors — Guided prompts for agent workflows
//
// P7 Justification: Core prompts are domain-agnostic graph operations (implement,
// review, trace, explore). They contain zero domain knowledge — they traverse
// generic graph nodes and edges. No extension contributes a prompt today: an
// extension's surfaces are commands, MCP tools and MCP resources.
//
// 5 behaviors: serve, context, review, trace, explore (infer's scopes are in
// behaviors/infer.spec; the exploration and the review they render are in
// behaviors/output.spec)

use "events/mcp"
use "invariants/core"
use "invariants/mcp"
use "ports/inbound"
use "ports/outbound"
use "types/graph"
use "types/mcp"

behavior serve_mcp_prompt "Serve MCP Prompt" {
  features   [mcp_prompts]
  invariants [mcp_structured_error_responses, mcp_served_project_consistency]
  category   query
  types      [McpPromptDescriptor, McpPromptArgument, McpError]
  ports      [McpProtocol, CompilerApi]
  produces   [mcp_prompt_invoked]
  ensures {
    arguments_derived  "listed arguments are exactly those the prompt reads, required exactly when it cannot render without them"
    refusal_structured "a prompt that cannot render answers a JSON-RPC error whose data is an McpError"
    one_layout         "every prompt result is an instruction then a JSON payload, both user messages"
    fresh_graph        "a prompt renders the graph a tool call would see"
  }
  contract   """
    Every core prompt is one module: its name, description, typed arguments,
    the spec type its payload is and its typed payload are stated there
    once; prompts/list derives its arguments from the typed arguments
    prompts/get reads, and the payload is serialized from its type in the
    type's field order, so a listed argument is read
    and a required one is one the prompt cannot render without. MCP sends
    prompt arguments as strings; a count is read from a string. An
    argument the prompt does not declare MUST be refused as invalid input
    naming it (read_mcp_arguments_as_declared). Arguments
    that are not an object are -32602. A prompt that cannot render (an
    invalid argument, an unknown entity, an unusable project file) answers
    a JSON-RPC error, -32602 for input the client can fix and -32603 for a
    server-side failure, whose data is an McpError naming the prompt. A
    rendered prompt is its description and two user messages: the
    instruction, then the JSON payload. prompts/get first brings the
    served project up to date with disk, as tools/call does
    (bring_session_up_to_date). An unknown prompt is -32602 and is not an
    invocation. A prompt or the server's instructions name a tool only as
    tools/list names it, read from the core tool table.
  """
  verify unit "each core prompt lists exactly the arguments its handler reads"
  verify unit "a listed required argument is exactly one the prompt cannot render without"
  verify unit "a prompt refusal is a JSON-RPC error whose data is an McpError naming the prompt"
  verify unit "a missing required prompt argument is -32602 naming the argument"
  verify unit "prompt arguments that are not an object produce -32602 Invalid params"
  verify unit "a numeric prompt argument is read from a string, as MCP sends it"
  verify unit "every prompt result is an instruction then a JSON payload, both user messages"
  verify unit "an unknown prompt records no mcp_prompt_invoked event"
  verify unit "a stateless prompts/get renders without initialize"
  verify unit "every tool a prompt or the server's instructions name is a core tool, named as tools/list names it"
  verify unit "each core prompt is one module: its name, description, arguments, spec type and typed payload are stated there once"
  verify unit "each prompt's payload is the spec type its module names"
}

behavior provide_mcp_context_prompt "Provide MCP Context Prompt" {
  features   [mcp_prompts]
  invariants [
    graph_traversal_integrity,
    diagnostic_determinism,
    mcp_structured_error_responses,
    mcp_tool_idempotency,
  ]
  category   query
  types      [McpPromptDescriptor, Graph, McpContextPromptResult, McpContextConstraint]
  ports      [McpProtocol, CompilerApi]
  produces   [mcp_prompt_invoked]
  requires {
    graph_available "Compiled graph is available via CompilerApi"
  }
  ensures {
    context_returned       "Structured entity context returned: contract, related entities, verify declarations"
    hints_included         "structural_constraints entities included as additional context even if not directly connected"
    prompt_invoked_emitted "mcp_prompt_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge://prompts/context
    prompt that accepts entity_id (required) and structural_constraints? (optional
    string array of additional entity IDs to include as context). The prompt
    MUST return structured entity context including the entity's contract text,
    every field the entity declares (an invariant's guarantee, a decision's
    rationale, whatever its kind names its text),
    all directly related entities (upstream and downstream), verify
    declarations as verification expectations, and related entities.
    structural_constraints entities are included as additional context even if not
    directly connected. If the entity does not exist, the prompt MUST
    return an error.
  """
  verify unit "specforge://prompts/context returns structured entity context"
  verify unit "response includes contract and related entities"
  verify unit "context includes every field, like an invariant's guarantee"
  verify unit "non-existent entity returns error"
  verify unit "context prompt works with zero extensions installed"
  verify contract "Provide MCP Context Prompt: MCP context prompt holds — graph_available, context_returned, hints_included, prompt_invoked_emitted"
}

behavior provide_mcp_review_prompt "Provide MCP Review Prompt" {
  features   [mcp_prompts]
  invariants [
    graph_traversal_integrity,
    diagnostic_determinism,
    mcp_structured_error_responses,
    mcp_tool_idempotency,
  ]
  category   query
  types      [McpPromptDescriptor, McpCoverageResult, McpReviewPromptResult, McpReviewFinding]
  ports      [McpProtocol, CompilerApi]
  produces   [mcp_prompt_invoked]
  requires {
    graph_available "Compiled graph is available via CompilerApi"
  }
  ensures {
    coverage_analysis_returned "Coverage analysis returned for entity and neighbors up to specified depth"
    gaps_identified            "Entities that declare no obligation, uncovered verify declarations, missing evidence links and unconnected entities identified"
    prompt_invoked_emitted     "mcp_prompt_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge://prompts/review
    prompt that accepts entity_id? (optional; the whole graph when omitted)
    and depth? (optional count, default 1). It renders the review
    (review_coverage_gaps) of its arguments: entity_id ("*" for the whole
    graph), findings (entity_id, severity, message) and coverage_summary
    (the coverage view's rows, as specforge.coverage lists them), after an
    instruction to prioritize uncovered and unconnected entities.
  """
  verify unit "specforge://prompts/review returns coverage analysis"
  verify unit "review coverage matches specforge.coverage obligation by obligation"
  verify contract "Provide MCP Review Prompt: MCP review prompt holds — graph_available, coverage_analysis_returned, gaps_identified, prompt_invoked_emitted"
}

behavior provide_mcp_trace_prompt "Provide MCP Trace Prompt" {
  features   [mcp_prompts]
  invariants [
    graph_traversal_integrity,
    diagnostic_determinism,
    mcp_structured_error_responses,
    mcp_tool_idempotency,
  ]
  category   query
  types      [McpPromptDescriptor, TraceChain, McpTracePromptResult, McpTraceGap]
  ports      [McpProtocol, CompilerApi]
  produces   [mcp_prompt_invoked]
  requires {
    graph_available "Compiled graph is available via CompilerApi"
  }
  ensures {
    gaps_returned            "Identified gaps returned with deterministic gap context"
    affected_entities_listed "Entities affected by the plan listed in response"
    prompt_invoked_emitted   "mcp_prompt_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge://prompts/trace
    prompt that accepts plan (an AgentPlan's JSON text, types/graph.spec) or
    entity_id (trace that one entity's chain), not both: both MUST be refused
    as invalid input on plan, as specforge.trace refuses them.
    The prompt MUST perform gap analysis against the current graph, identify
    entities affected by the plan, flag missing traceability links, and return
    identified gaps with deterministic gap context.
    A plan that is not an AgentPlan MUST be refused as invalid input on plan,
    naming every place it departs from the type (validate_agent_plan).
  """
  verify unit "specforge://prompts/trace identifies gaps in plan"
  verify unit "response returns identified gaps with gap context"
  verify unit "affected entities are listed"
  verify unit "malformed plan JSON returns validation error"
  verify unit "unverified entities are the ones the trace reaches that count toward coverage and are not proven"
  verify unit "the trace prompt refuses an entity_id given with a plan, as invalid input on plan"
  verify contract "Provide MCP Trace Prompt: MCP trace prompt holds — graph_available, gaps_returned, affected_entities_listed, prompt_invoked_emitted"
}

behavior provide_mcp_explore_prompt "Provide MCP Explore Prompt" {
  features   [mcp_prompts]
  invariants [
    graph_traversal_integrity,
    diagnostic_determinism,
    mcp_structured_error_responses,
    mcp_tool_idempotency,
  ]
  category   query
  types      [McpPromptDescriptor, Graph, McpExplorePromptResult, McpRelationshipPath]
  ports      [McpProtocol, CompilerApi]
  produces   [mcp_prompt_invoked]
  requires {
    graph_available "Compiled graph is available via CompilerApi"
  }
  ensures {
    exploration_returned   "Guided exploration returned: starting points, high-connectivity entities, unconnected entities"
    bfs_from_entity        "When entity_id provided, BFS traversal starts from that node"
    prompt_invoked_emitted "mcp_prompt_invoked event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge://prompts/explore
    prompt that accepts entity_id? (optional starting point), kind? (optional
    entity kind) and depth? (optional count; unbounded when omitted). It
    renders the exploration (explore_the_graph) of its arguments:
    matching_entities, relationship_paths, starting_points,
    high_connectivity, unconnected and notices, after an instruction to
    start from the most connected entities and investigate unconnected
    ones. If entity_id names no entity, the prompt MUST return an error.
  """
  verify unit "specforge://prompts/explore returns exploration starting points"
  verify unit "unknown entity_id returns error"
  verify contract "Provide MCP Explore Prompt: MCP explore prompt holds — graph_available, exploration_returned, bfs_from_entity, prompt_invoked_emitted"
}
