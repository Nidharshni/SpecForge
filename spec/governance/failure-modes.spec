// Core failure modes — FMEA risk analysis for the compiler engine
//
// Extension-specific failure modes live in their respective extension directories
// under spec/extensions/.

use "invariants/core"
use "invariants/validation"
use "invariants/wasm"

failure_mode incremental_divergence "Incremental Divergence" {
  invariant          incremental_correctness
  threatens_features [product_validation]
  affected_behaviors [rebuild_affected_subgraph, compute_graph_delta, build_in_memory_graph]
  severity           high
  occurrence         occasional
  detection          unlikely
  rpn                84
  cause              "Bug in the red-green rebuild leaves a stale node or edge, or misses a cross-file effect of a change"
  effect             "Incremental build produces different diagnostics than cold rebuild — user sees phantom errors or missed errors"
  mitigation         "Property test comparing incremental result to cold rebuild for randomized file changes"
  post_mitigation {
    severity   high
    occurrence rare
    detection  likely
    rpn        14
  }
  verify unit "Incremental Divergence failure mode is handled"
}

failure_mode string_interning_collision "String Interning Collision" {
  invariant          string_interning_consistency
  affected_behaviors [build_in_memory_graph, maintain_mutable_graph]
  severity           critical
  occurrence         rare
  detection          undetectable
  rpn                48
  cause              "Hash collision in the interning table causes two different strings to share the same key"
  effect             "Two distinct entity IDs compare as equal — phantom duplicate ID errors or missed reference errors"
  mitigation         "Use a collision-resistant hash (lasso uses fx-hash); add debug-mode assertion comparing string values on every key lookup"
  post_mitigation {
    severity   critical
    occurrence rare
    detection  likely
    rpn        16
  }
  verify unit "String Interning Collision failure mode is handled"
}

failure_mode duplicate_id_detection_miss "Duplicate ID Detection Miss" {
  invariant          entity_id_uniqueness
  affected_behaviors [build_in_memory_graph]
  severity           high
  occurrence         unlikely
  detection          moderate
  rpn                42
  cause              "Race condition or ordering bug in parallel file processing skips duplicate detection for entities declared in different files"
  effect             "Two entities with the same ID exist in the graph — unpredictable behavior during validation and rendering"
  mitigation         "Serial ID registration with a global lock; integration tests with deliberately duplicated IDs across files"
  post_mitigation {
    severity   high
    occurrence rare
    detection  certain
    rpn        7
  }
  verify unit "Duplicate ID Detection Miss failure mode is handled"
}

failure_mode import_cycle_detection_miss "Import Cycle Detection Miss" {
  invariant          import_dag
  affected_behaviors [detect_import_cycles, build_in_memory_graph]
  severity           medium
  occurrence         unlikely
  detection          moderate
  rpn                30
  cause              "Topological sort algorithm has a bug that misses cycles in graphs with specific structures (e.g., self-referential imports)"
  effect             "Import cycle goes undetected — infinite loop during resolution or stack overflow"
  mitigation         "Use Tarjan's algorithm with proven correctness; fuzz test with randomly generated import graphs including self-cycles"
  post_mitigation {
    severity   medium
    occurrence rare
    detection  certain
    rpn        5
  }
  verify unit "Import Cycle Detection Miss failure mode is handled"
}

failure_mode diagnostic_drop_under_error_collection "Diagnostic Drop Under Error Collection" {
  invariant  multi_error_collection
  severity   high
  occurrence unlikely
  detection  undetectable
  rpn        60
  cause      "Error in diagnostic collection logic silently drops diagnostics when the bag exceeds an internal limit or encounters an unexpected error type"
  effect     "User misses errors — believes spec is clean when it is not, leading to downstream failures"
  mitigation "Diagnostic bag has no size limit; every code path that produces a diagnostic uses the same collector; integration test asserting diagnostic count matches expected for a known-bad spec"
  post_mitigation {
    severity   high
    occurrence rare
    detection  likely
    rpn        12
  }
  verify unit "Diagnostic Drop Under Error Collection failure mode is handled"
}

failure_mode silent_reference_swallow "Silent Reference Swallow" {
  invariant  reference_resolution_completeness
  severity   critical
  occurrence unlikely
  detection  unlikely
  rpn        64
  cause      "Bug in reference resolution silently skips a reference instead of emitting E001 or I004 — e.g., an early return in a match arm"
  effect     "Broken reference goes undetected — user believes spec is clean when a dangling reference exists, leading to incorrect traceability"
  mitigation "Exhaustive integration test with deliberately broken references for every edge type; fuzzing with random ID mutations"
  post_mitigation {
    severity   critical
    occurrence rare
    detection  likely
    rpn        16
  }
  verify unit "Silent Reference Swallow failure mode is handled"
}

failure_mode spec_root_duplication "Spec Root Duplication" {
  invariant  spec_root_singleton
  severity   medium
  occurrence unlikely
  detection  likely
  rpn        20
  cause      "Bug in spec root detection allows two specforge.json files to coexist without error — e.g., one in the project root and one in a nested directory"
  effect     "Compiler uses unpredictable configuration — wrong extensions, wrong settings for all subsequent compilation"
  mitigation "Unit test: deliberate dual specforge.json files triggers error; project root detection checks for single config before resolution"
  post_mitigation {
    severity   medium
    occurrence rare
    detection  certain
    rpn        5
  }
  verify unit "Spec Root Duplication failure mode is handled"
}

failure_mode non_deterministic_diagnostic_order "Non-Deterministic Diagnostic Order" {
  invariant  diagnostic_determinism
  severity   medium
  occurrence occasional
  detection  unlikely
  rpn        48
  cause      "HashMap iteration order or parallel file processing produces different diagnostic ordering across runs"
  effect     "CI produces flaky results — same spec files yield different diagnostic output, confusing developers and breaking snapshot tests"
  mitigation "Sort diagnostics by (file_path, line, column, code) before emission; property test asserting identical output across 100 runs"
  post_mitigation {
    severity   medium
    occurrence rare
    detection  certain
    rpn        4
  }
  verify unit "Non-Deterministic Diagnostic Order failure mode is handled"
}

failure_mode wasm_extension_crash "Wasm Extension Crash" {
  threatens_features [product_validation]
  invariant          wasm_sandbox_integrity
  severity           high
  occurrence         occasional
  detection          moderate
  rpn                54
  cause              "Extension Wasm module traps during validate() or render() — e.g., out-of-bounds memory access, stack overflow, or unreachable instruction"
  effect             "Extension fails to complete its validation or export pass — diagnostics from that extension are lost, output may be incomplete"
  mitigation         "Wasmtime catches all traps and returns error; compiler wraps call in Result, emits ExtensionError with trap details; remaining extensions continue execution"
  post_mitigation {
    severity   high
    occurrence rare
    detection  likely
    rpn        12
  }
  verify unit "Wasm Extension Crash failure mode is handled"
}

failure_mode wasm_memory_exhaustion "Wasm Memory Exhaustion" {
  threatens_features [wasm_extension_runtime]
  invariant          wasm_sandbox_integrity
  severity           high
  occurrence         occasional
  detection          moderate
  rpn                54
  cause              "An extension grows its linear memory without bound (a leak across calls of a long session, or a hostile guest), up to the 4 GiB a wasm32 instance can address"
  effect             "The host process (CLI, LSP, MCP server) takes gigabytes of memory, slowing or killing the developer's session"
  mitigation         "The runtime's memory limiter holds each instance to its declared max_memory_mb, at most 512 MB; a growth past it traps the call (memory_limit_exceeded, E028) and the extension's next call gets a fresh instance"
  post_mitigation {
    severity   medium
    occurrence rare
    detection  certain
    rpn        6
  }
  verify unit "Wasm Memory Exhaustion failure mode is handled"
}

failure_mode wasm_host_function_timeout "Wasm Host Function Timeout" {
  invariant  wasm_sandbox_integrity
  severity   medium
  occurrence occasional
  detection  likely
  rpn        30
  cause      "specforge.http_get host function makes a request to an unresponsive service — extension blocks waiting for network response"
  effect     "Compilation hangs or takes excessively long — developer experiences unexplained delay"
  mitigation "Enforce timeout on all http_get calls (default 5s); fuel metering caps total execution time per extension; timeout produces diagnostic with URL"
  post_mitigation {
    severity   medium
    occurrence rare
    detection  certain
    rpn        5
  }
  verify unit "Wasm Host Function Timeout failure mode is handled"
}

failure_mode peer_dependency_version_mismatch "Peer Dependency Version Mismatch" {
  invariant  peer_dependency_satisfaction
  severity   high
  occurrence unlikely
  detection  likely
  rpn        24
  cause      "Extension A declares peer dependency on Extension B >=2.0, but Extension B version 1.x is installed — semver range check fails"
  effect     "Extension initialization fails — entity types from dependent extension are unavailable, soft references degrade silently"
  mitigation "Hard error on unsatisfied peer dependencies at startup; diagnostic includes installed vs required version; specforge add checks peers before installing"
  post_mitigation {
    severity   high
    occurrence rare
    detection  certain
    rpn        6
  }
  verify unit "Peer Dependency Version Mismatch failure mode is handled"
}

failure_mode wasm_compile_cache_corruption "Wasm Compile Cache Corruption" {
  invariant  wasm_compile_cache_integrity
  severity   medium
  occurrence unlikely
  detection  unlikely
  rpn        40
  cause      "A cached compilation artifact is corrupted — e.g., interrupted write, disk error, or engine/platform change after an OS upgrade"
  effect     "Cache lookup misses or fails to deserialize — without engine validation this could serve wrong code; with it, the cost is only a lost cache entry"
  mitigation "Cache entries are keyed by component bytes and engine config and validated by the runtime engine; any corrupt or mismatched entry falls back to fresh compilation"
  post_mitigation {
    severity   low
    occurrence rare
    detection  certain
    rpn        2
  }
  verify unit "Wasm Compile Cache Corruption failure mode is handled"
}

failure_mode circular_peer_dependency "Circular Peer Dependency" {
  invariant  peer_dependency_satisfaction
  severity   high
  occurrence unlikely
  detection  likely
  rpn        24
  cause      "Extension A requires Extension B as a peer, and B requires A (or a longer chain closes back): no load order puts each after its required peers"
  effect     "Without detection the order silently breaks 'dependencies first', and first-wins registration (E026, W018, enhancements, surfaces) follows whichever was listed first"
  mitigation "The registry build finds each strongly connected set of required peers and reports it once as E027 naming its extensions, in every compile (check, the LSP, MCP, doctor); the members still load, together, in entry order. Optional peers that would close a cycle are only a preference and are dropped from the order (ADR 0041)"
  post_mitigation {
    severity   high
    occurrence rare
    detection  certain
    rpn        6
  }
  verify unit "Circular Peer Dependency failure mode is handled"
}

failure_mode manifest_schema_mismatch "Manifest Schema Mismatch" {
  threatens_features [product_entity_registration]
  invariant          peer_dependency_satisfaction
  severity           medium
  occurrence         unlikely
  detection          moderate
  rpn                30
  cause              "Extension built against an outdated manifest schema — field names and semantics differ between versions"
  effect             "Manifest fields misinterpreted — entity registrations wrong, peer dependencies ignored, sandbox policy defaults applied instead of declared values"
  mitigation         "the handshake's protocol version is checked at load time: another major version fails the extension's load (E028); describe keys the protocol does not define produce W138; a sandbox declaration the host does not honour produces W153"
  post_mitigation {
    severity   medium
    occurrence rare
    detection  certain
    rpn        5
  }
  verify unit "Manifest Schema Mismatch failure mode is handled"
}

failure_mode host_function_type_violation "Host Function Type Safety Violation" {
  invariant  host_function_type_safety
  severity   critical
  occurrence unlikely
  detection  moderate
  rpn        48
  cause      "Extension sends malformed or unexpected data through a host function — e.g., invalid JSON to specforge.add_graph_node, wrong schema to specforge.emit_diagnostic"
  effect     "Host processes corrupted data — wrong graph nodes added, invalid diagnostics emitted, graph corruption possible"
  mitigation "Schema validation on every host function input; malformed data returns ExtensionError to extension; integration tests with deliberately malformed extension inputs"
  post_mitigation {
    severity   critical
    occurrence rare
    detection  certain
    rpn        8
  }
  verify unit "Host Function Type Safety Violation failure mode is handled"
}

failure_mode entity_kind_collision_undetected "Entity Kind Collision Undetected" {
  threatens_features [product_entity_registration]
  invariant          entity_kind_uniqueness
  severity           high
  occurrence         unlikely
  detection          likely
  rpn                28
  cause              "Two extensions register the same entity kind name but the KindRegistry fails to detect the collision — e.g., race condition or case-insensitive match not checked"
  effect             "One extension's entity kind silently shadows the other — entities parsed incorrectly, wrong validation rules applied, corrupted graph"
  mitigation         "The registry build checks every kind registration against the kinds earlier-loaded extensions registered; a duplicate is E026 and the first registration wins; property-based tests with random kind name combinations"
  post_mitigation {
    severity   high
    occurrence rare
    detection  certain
    rpn        7
  }
  verify unit "Entity Kind Collision Undetected failure mode is handled"
}

failure_mode registry_unavailability "Registry Unavailability" {
  threatens_features [product_entity_registration]
  invariant          registry_integrity
  severity           medium
  occurrence         occasional
  detection          likely
  rpn                24
  cause              "Registry endpoint is unreachable — DNS failure, network timeout, authentication error, or registry service outage"
  effect             "Extension installation or upgrade fails — developer cannot add new extensions or update existing ones"
  mitigation         "Configurable timeout (default 10s) with retry guidance in diagnostic; offline fallback to local cache; diagnostic includes registry URL and HTTP status"
  post_mitigation {
    severity   medium
    occurrence unlikely
    detection  certain
    rpn        8
  }
  verify unit "Registry Unavailability failure mode is handled"
}

failure_mode collector_output_malformation "Collector Output Malformation" {
  threatens_features [product_health_metric]
  invariant          collector_output_conformance
  severity           medium
  occurrence         unlikely
  detection          moderate
  rpn                30
  cause              "Collector extension produces output that does not conform to specforge-report/v1 schema — e.g., missing entries array, invalid entity IDs, wrong schema version"
  effect             "Coverage ingestion fails or produces incorrect results — developer sees wrong coverage statistics or missing test mappings"
  mitigation         "Schema validation on all collector output before ingestion; malformed output produces ExtensionError with specific field-level details; partial ingestion of valid entries with warnings for invalid ones"
  post_mitigation {
    severity   medium
    occurrence rare
    detection  certain
    rpn        5
  }
  verify unit "Collector Output Malformation failure mode is handled"
}

failure_mode extension_initialization_failure "Extension Initialization Failure" {
  invariant  extension_isolation
  severity   high
  occurrence occasional
  detection  likely
  rpn        36
  cause      "Extension .wasm module missing or exporting wrong initialize() signature — e.g., built with incompatible PDK version"
  effect     "Extension entities not registered — references to extension entities produce E001 instead of I004, misleading developers into thinking entities are misspelled"
  mitigation "Detect missing/wrong initialize() at loadModule phase before any export calls; transition extension to failed state; emit diagnostic with PDK version hint; continue loading remaining extensions"
  post_mitigation {
    severity   high
    occurrence rare
    detection  certain
    rpn        6
  }
  verify unit "Extension Initialization Failure failure mode is handled"
}

// C12-01: the operational build assumption the ADRs record only in prose.
// A fresh clone cannot run wasm-backed extensions until the embedded
// builtin .wasm blobs exist — record the failure mode so the compiled
// spec carries what docs forget.
failure_mode fresh_clone_wasm_bootstrap_missing {
  severity   high
  occurrence certain
  detection  certain
  rpn        60
  cause      "Repository is cloned fresh and the embedded builtin .wasm blobs (extensions/*/src/*.wasm) have not been built — the wasm32-wasip2 target was never installed or the bootstrap step was skipped."
  effect     "Extension loading fails at startup; wasm-backed behaviors (registry populate, describe fetch, custom validation rules) are unavailable, and builds depending on them error instead of degrading."
  mitigation "Bootstrap script builds the four builtin .wasm blobs (extensions/*/src) for wasm32-wasip2 before first run; compile-cache warm path documents the requirement; CI builds the blobs ahead of workspace tests."
  post_mitigation {
    severity   low
    occurrence unlikely
    detection  certain
    rpn        6
  }
}
