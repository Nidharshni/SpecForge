// @specforge/formal analyze commands — CLI commands for formal analysis
//
// Moved from @specforge/software analyze-commands.spec per 10-expert panel.
// All behavior IDs renamed se_ -> fa_
// Pass names updated: contract_check -> condition_check, refinement_verify ->
// layering_verify, process_analyze -> event_graph_analyze

use "extensions/formal/types"
use "types/zero-entity-core"

behavior fa_analyze_conditions "Analyze Conditions Command" {
  category command
  types    [RequiresBlock, EnsuresBlock, ConditionEntry]
  contract """
    The specforge analyze conditions command MUST run the condition_check
    pass in isolation and report results. Output MUST include: count
    of behaviors with conditions, precondition satisfiability results,
    postcondition reachability results, layering compliance results.
  """
  requires {
    graph_available "compiled entity graph is available"
  }
  ensures {
    condition_report "report shows condition analysis for all behaviors with requires/ensures"
    json_output      "with --format json, output is machine-readable JSON"
    strict_exit_code "with --strict flag, any violation exits non-zero"
  }
  features [fa_analyze_commands]
  verify unit "analyze conditions reports all behaviors with requires/ensures"
  verify unit "analyze conditions --format json produces JSON output"
  verify unit "analyze conditions --strict exits non-zero on violation"
}

behavior fa_analyze_layering "Analyze Layering Command" {
  category command
  types    [RefinementChain]
  contract """
    The specforge analyze layering command MUST run the
    layering_verify pass in isolation and report results.
  """
  ensures {
    layering_report "report shows all layering chains with completeness status"
    json_output     "with --format json, output is machine-readable JSON"
  }
  features [fa_analyze_commands]
  verify unit "analyze layering reports all abstract/concrete chains"
  verify unit "analyze layering --format json produces JSON output"
}

behavior fa_analyze_event_graph "Analyze Event Graph Command" {
  category command
  types    [SyncBlock]
  contract """
    The specforge analyze event-graph command MUST run the
    event_graph_analyze pass in isolation and report results.
  """
  ensures {
    event_graph_report "report shows cycles, retry cycles, unmatched producers, channel mismatches"
    json_output        "with --format json, output is machine-readable JSON"
  }
  features [fa_analyze_commands]
  verify unit "analyze event-graph reports cycles and retry cycles"
  verify unit "analyze event-graph --format json produces JSON output"
}

behavior fa_analyze_all "Analyze All Command" {
  category command
  types    [FormalAnalysisResult]
  contract """
    The specforge analyze all command MUST run condition_check,
    layering_verify, and event_graph_analyze in pipeline order and
    produce a unified report.
  """
  requires {
    graph_available "compiled entity graph is available"
  }
  ensures {
    unified_report   "report combines condition, layering, and event graph analysis"
    json_output      "with --format json, output is machine-readable JSON"
    strict_exit_code "with --strict flag, any violation exits non-zero (for CI)"
  }
  features [fa_analyze_commands]
  verify unit "analyze all runs all three passes in pipeline order"
  verify unit "analyze all --strict exits non-zero on any violation"
  verify unit "analyze all --format json produces unified JSON report"
}
