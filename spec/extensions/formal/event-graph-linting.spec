// @specforge/formal event graph linting — structural event flow analysis
//
// Moved from @specforge/software formal-concurrency.spec per 10-expert panel.
// Terminology: "CSP Event Flow Analysis" -> "Event Graph Linting"
// E034: a cycle is mitigated when a member declares sync (ADR 0040)
// W032: "Livelock Risk" -> "Unmitigated Retry Cycle"
// W033: "Starvation Risk" -> "Asymmetric Connectivity Warning"
// All behavior IDs renamed se_ -> fa_

use "extensions/formal/invariants"
use "extensions/formal/types"
use "types/zero-entity-core"

defaults behavior {
  features [fa_event_graph_linting]
}

behavior fa_parse_sync_block "Parse Sync Block" {
  category command
  types    [SyncBlock, DeliverySemantics]
  contract """
    Recognize the sync { } block on event entities as synchronization
    constraints. Supports barrier (behavior references), timeout
    (duration string), and delivery semantics.
  """
  ensures {
    barrier_parsed   "sync block with barrier (list of behavior references) parsed"
    timeout_parsed   "sync block with timeout (duration string with description) parsed"
    delivery_parsed  "sync block with delivery (at_most_once | at_least_once | exactly_once) parsed"
    non_event_warned "sync block on non-event entity produces warning"
  }
  verify unit "sync block with barrier parsed"
  verify unit "sync block with timeout parsed"
  verify unit "sync block with delivery semantics parsed"
  verify unit "sync block on non-event produces warning"
}

behavior fa_build_event_bipartite_graph "Build Event-Behavior Bipartite Graph" {
  category command
  types    [SyncBlock, FormalProtocol]
  contract """
    Construct the event-behavior bipartite graph from Produces and
    Consumes edges. Consumers are derived from Consumes edges only
    (the graph is the sole authority for consumption relationships).
    FollowsProtocol edges from events to protocol entities are
    incorporated — protocol ordering constraints are included as
    additional synchronization edges in the bipartite graph.
    This graph is the input to all event flow analysis sub-passes
    (cycle detection, retry cycle, connectivity, channel checks,
    protocol ordering validation).
  """
  requires {
    edges_built  "graph with Produces/Consumes/FollowsProtocol edges is fully built"
    events_exist "at least one event entity exists in the graph"
  }
  ensures {
    bipartite_nodes      "bipartite graph contains event nodes and behavior nodes as two disjoint sets"
    producers_included   "every behavior with a Produces edge appears as a producer node"
    consumers_from_edges "consumers derived from Consumes edges only (not from entity fields)"
    barrier_edges        "sync block barrier references create additional synchronization edges"
    protocol_edges       "FollowsProtocol edges incorporated — protocol ordering constraints included"
    no_unconnected_nodes "bipartite graph contains no node disconnected from all edges"
  }
  verify unit "bipartite graph built from produces/consumes edges"
  verify unit "consumers derived from Consumes edges, not entity fields"
  verify unit "barrier references create synchronization edges"
  verify unit "FollowsProtocol edges incorporated into bipartite graph"
  verify property "bipartite graph has no unconnected nodes"
  verify property "all producers and consumers are included"
}

behavior fa_detect_unmitigated_cycles "E034: Detect Unmitigated Cycles" {
  category query
  types    [SyncBlock]
  contract """
    Detect event cycles with Tarjan's strongly connected components over
    the event flow graph: a behavior leads to each event it produces, and
    an event to each behavior that consumes it. A component that holds
    two or more behaviors is a cycle. It is mitigated, and passes
    silently, when any behavior or event in it declares a non-empty sync:
    the one mitigation the data model carries (what the sync says is not
    checked). An unmitigated cycle produces one E034 error naming a cycle
    path and suggesting a sync on one of its members. A behavior that
    re-produces an event it consumes, with no other behavior in the
    cycle, is W032 instead. Runs in the event_graph_analyze pass
    (specforge analyze).
  """
  requires {
    bipartite_graph_built "event-behavior bipartite graph is constructed"
  }
  ensures {
    unmitigated_detected "a cycle through two or more behaviors with no sync on any member produces E034 error"
    mitigated_passes     "a cycle with a non-empty sync on any behavior or event in it passes silently"
    non_cycle_passes     "non-circular event dependency produces no diagnostic"
    cycle_path_shown     "E034 names a cycle path and suggests declaring sync on one of its members"
  }
  verify unit "unmitigated circular event dependency detected as E034"
  verify unit "cycle with a sync on one member passes silently"
  verify unit "non-circular event dependency passes"
  verify unit "E034 names a cycle path and suggests sync on a member"
}

behavior fa_detect_unmatched_producers "W029: Unmatched Producers" {
  category query
  contract """
    Detect events that have producers but no consumers (derived from
    Consumes edges). This is a warning rather than an error because
    fire-and-forget events, future consumers, and external system
    consumers are all valid patterns.
  """
  ensures {
    matched_passes     "event with producers and consumers produces no diagnostic"
    unmatched_warned   "event with no consumers produces W029 warning"
    participation_used "an event that participates in a process (participates_in) is used by it and produces no W029"
  }
  verify unit "event with producers and consumers passes"
  verify unit "event with no consumers produces W029"
  verify unit "an event participating in a process counts as used and produces no W029"
}

behavior fa_detect_unbounded_channel "W034: Unbounded Channel Buffer" {
  category query
  types    [SyncBlock]
  contract """
    Detect events that a behavior produces and that declare no sync
    constraint: with no timeout, buffer limit or delivery bound declared,
    nothing bounds how many of the event's messages accumulate under
    load. An event nothing produces carries no messages and is not
    reported. Any non-empty sync counts as a bound; what it says is not
    checked.
  """
  ensures {
    unbounded_detected "a produced event that declares no sync produces W034"
    bounded_passes     "a produced event that declares a sync produces no diagnostic"
  }
  verify unit "event channel with no sync produces W034"
  verify unit "event channel with a sync passes"
}

behavior fa_detect_asymmetric_connectivity "W033: Asymmetric Connectivity Warning" {
  category query
  contract """
    Detect ports with structural patterns that suggest unbalanced
    access: two or more behaviors use the port (their ports field names
    it) and the most-referenced of them has more than three times the
    incoming edges of the least-referenced, a behavior nothing
    references counting as one. This is a structural complexity hint,
    not a formal fairness guarantee.
  """
  ensures {
    asymmetric_detected "port whose users' incoming edge counts differ by more than 3:1 produces W033"
    balanced_passes     "port with balanced users or a single user passes"
    suggestion          "W033 includes suggestion to review access patterns"
  }
  verify unit "port with unbalanced access pattern produces W033"
  verify unit "port with single consumer passes"
}

behavior fa_detect_unmitigated_retry_cycle "W032: Unmitigated Retry Cycle" {
  category query
  types    [SyncBlock]
  contract """
    Detect a behavior that consumes an event and produces the same event
    again, re-triggering itself, when neither the behavior nor the event
    declares a sync constraint (a timeout or backoff) bounding the
    retries.
  """
  ensures {
    retrigger_detected "a behavior producing an event it consumes, with no sync on either, produces W032 warning"
    backoff_passes     "a sync on the behavior or the event produces no diagnostic"
    suggestion         "W032 includes suggestion to declare a sync (timeout or backoff) on the event or the behavior"
  }
  verify unit "re-triggering without backoff detected as W032"
  verify unit "re-triggering with timeout/backoff passes"
}

behavior fa_event_graph_analyze_pass "Event Graph Analyze Compiler Pass" {
  category command
  types    [SyncBlock, FormalProtocol, EventGraphAnalysisReport]
  produces [fa_event_graph_analysis_complete]
  contract """
    The event_graph_analyze compiler pass performs full event flow
    analysis over the event-behavior graph after layering_verify.
  """
  requires {
    layering_verify_done "layering_verify pass has completed"
    graph_constructed    "entity graph with produces/consumes/follows_protocol edges is built"
    timeout_configured   "analysis timeout set from --timeout flag (default: 30s)"
  }
  ensures {
    bipartite_delegated  "event-behavior bipartite graph construction delegated to fa_build_event_bipartite_graph"
    cycle_checked        "Tarjan SCC unmitigated cycle detection runs (E034)"
    unmatched_checked    "unmatched producers detected (W029)"
    retry_cycle_checked  "unmitigated retry cycles detected (W032)"
    connectivity_checked "asymmetric connectivity on ports detected (W033)"
    buffer_checked       "unbounded channel buffers detected (W034)"
    cycle_free_noted     "an event graph with flow edges and no unmitigated cycle is noted once (I009)"
    composition_checked  "process composition cycles detected (E042), one per strongly connected set of processes, naming every process on it"
    timeout_handled      "barrier timeout expiry sets timed_out=true on EventGraphAnalysisReport and emits warning with incomplete sub-analysis count"
    partial_results      "completed sub-analyses included in report; only incomplete ones omitted on timeout"
    timeout_configurable "barrier timeout uses configured value, not hardcoded 30s"
    process_integrated   "process entities integrated into bipartite graph when present"
  }
  verify unit "bipartite graph construction delegated to fa_build_event_bipartite_graph"
  verify unit "unmitigated cycle detection runs on SCC"
  verify unit "pass runs after layering_verify"
  verify unit "barrier timeout sets timed_out=true and emits warning"
  verify unit "partial results included on timeout"
  verify unit "configured timeout overrides default 30s"
}

behavior fa_parse_process_entity "Parse Process Entity" {
  category command
  types    [FormalProcess, ProcessState, CompositionOperator]
  contract """
    Parse process entity declarations. Creates ParticipatesIn edges
    (event -> process) from the alphabet field and ProcessComposition
    edges (process -> process) from composition references. Validates
    that alphabet references are event entities.
  """
  ensures {
    participates_in_edges "ParticipatesIn edges created from alphabet events to process"
    composition_edges     "ProcessComposition edges created between composed processes"
    alphabet_validated    "alphabet entries must reference existing event entities"
    states_parsed         "process states parsed with initial/accepting flags"
  }
  features [fa_process_modeling]
  verify unit "process entity parsed with ParticipatesIn edges from alphabet"
  verify unit "ProcessComposition edges created for composed processes"
  verify unit "alphabet referencing non-event produces error"
  verify unit "process states parsed with initial/accepting flags"
}

behavior fa_integrate_process_with_event_graph "Integrate Process Entities into Event Graph" {
  invariants [fa_process_composition_dag]
  category   command
  types      [FormalProcess, SyncBlock, FormalProtocol]
  contract   """
    Incorporate process-level information into the event-behavior
    bipartite graph. Process alphabet membership implies event
    participation. Events with both a sync block AND a process field are valid
    (dual-mode).
  """
  requires {
    bipartite_built  "event-behavior bipartite graph is constructed (fa_build_event_bipartite_graph)"
    processes_parsed "process entities are parsed (fa_parse_process_entity)"
  }
  ensures {
    process_integrated             "process entities integrated into bipartite graph"
    alphabet_implies_participation "process alphabet membership implies event participation edges"
    dual_mode_valid                "events with both sync block and process field are valid"
  }
  features   [fa_process_modeling]
  verify unit "process entities integrated into bipartite graph"
  verify unit "alphabet membership creates participation edges"
  verify unit "events with both sync block and process field are valid"
}
