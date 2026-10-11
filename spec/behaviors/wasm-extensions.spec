// Extension entity kinds, enhancements, conflicts, queries, contributions,
// collectors, discovery, lock files, and doctor

use "events/wasm-extensions"
use "invariants/extensions"
use "invariants/validation"
use "invariants/wasm"
use "ports/inbound"
use "ports/outbound"
use "types/config"
use "types/errors"
use "types/graph"
use "types/wasm"
use "types/zero-entity-core"

// -- Query Extensions -----

// -- Entity Kind Conflict Prevention -----

// -- Entity Enhancement -----

behavior load_extension_manifest "Load Extension Manifest" {
  features   [extension_manifest]
  invariants [extension_load_order_determinism]
  category   command
  types      [ExtensionDeclaration, ExtensionError]
  ports      [FileSystem]
  produces   [manifest_loaded]
  requires {
    extension_discovered "installed extension has been discovered with a known path to its .wasm binary"
    filesystem_available "FileSystem port is available for reading the binary"
  }
  ensures {
    manifest_loaded_emitted          "manifest_loaded event is emitted once the extension's declaration has been read"
    malformed_manifest_diagnosed     "a declaration that cannot be read fails the extension's load with a diagnostic"
    initialization_sequence_followed "per-extension initialization follows the documented 4-step sequence"
  }
  contract   """
    When the compiler loads an extension, it MUST read the extension's
    declaration from the .wasm binary itself (load_extension_declaration);
    no sidecar file is read. A declaration that cannot be read MUST fail
    the extension's load with a diagnostic (E028). There are no hardcoded
    manifest factory methods — all extensions, including all installed
    extensions, declare themselves. Bundled extensions are loaded from the
    compiler's bundled resources.

    INITIALIZATION SEQUENCE: The guaranteed initialization order is:
      1. handshake — the extension's identity, protocol version and policy
      2. describe — every declared category, once
      3. build_registries_from_declarations — validate the declarations
         and populate the kind, field and edge registries and the rules
      4. register_surface_contributions — populate SurfaceRegistry (CLI
         commands, MCP tools, MCP resources)
    Steps 1-2 run per extension in entry order (the extension load); step 3
    puts the declarations in load order (registry_build_load_order) before
    anything registers; steps 3-4 run once over every loaded declaration,
    in that order.
  """
  verify unit "the declaration is read from the binary"
  verify unit "a declaration that cannot be read produces a diagnostic"
  verify unit "bundled extensions loaded from bundled resources directory"
  verify unit "initialization follows the documented 4-step sequence"
  verify contract "Load Extension Manifest: extension manifest loading holds — extension_discovered, filesystem_available, manifest_loaded_emitted, malformed_manifest_diagnosed, initialization_sequence_followed"
}

behavior register_entity_enhancements "Register Entity Enhancements" {
  features   [entity_enhancement]
  invariants [enhancement_field_uniqueness]
  category   command
  types      [ExtensionDeclaration, EntityEnhancementDescriptor, DynamicEdgeType]
  requires {
    manifests_validated "all extension manifests have been validated and entity kinds registered"
  }
  ensures {
    enhancement_registered_emitted   "enhancement_registered event is emitted after fields are registered in FieldRegistry"
    registration_before_resolve      "registration completes before the resolve phase begins"
    registration_order_deterministic "registration order follows extensions array order in specforge.json"
  }
  contract   """
    When an extension manifest declares entity enhancements, the compiler
    MUST parse the enhancement declarations, validate that the target
    entity kinds exist (an unknown target kind is an I004 info diagnostic), register the field-to-edge mappings in the
    FieldRegistry, and register any dynamic edge types. Registration
    MUST happen before the resolve phase begins. The order of
    registration MUST follow the extensions array order in specforge.json.
    An enhancement that names another extension as the target kind's owner
    is conditional: when that owner is not loaded the enhancement MUST be
    skipped without a diagnostic, because the project does not use it.
    An enhancement field MUST NOT overwrite a field the kind already
    declares.
  """
  produces   [enhancement_registered]
  verify unit "enhancement fields registered in FieldRegistry"
  verify unit "unknown target kind produces I004 info diagnostic"
  verify unit "enhancement of a kind owned by an extension that is not loaded is skipped silently"
  verify unit "an enhancement with verify kinds makes its target kind testable"
  verify unit "enhanced reference fields create graph edges"
  verify unit "enhanced data fields participate in type validation"
  verify unit "registration order follows extensions array"
  verify unit "enhancement field does NOT overwrite existing kind-level field"
  verify contract "Register Entity Enhancements: entity enhancement registration holds — manifests_validated, enhancement_registered_emitted, registration_before_resolve, registration_order_deterministic"
}

// -- Contribution Model -----

behavior call_extension_exports "Call Extension Exports" {
  features   [contribution_based_extensions]
  invariants [extension_isolation, extensions_run_in_their_loading_runtime]
  category   command
  types      [
    CommandInput,
    CommandOutput,
    McpResourceRequest,
    McpResourceContent,
    PassInput,
    PassOutput,
    PassDiagnostic,
    CollectInput,
    CollectOutput,
    ValidatorContext,
    ValidatorVerdict,
    ScanRequest,
    ScanResponse,
    MigrationInput,
    WasmTrapInfo,
  ]
  ports      [WasmRuntime]
  requires {
    extension_loaded "the extension is one the project's runtime loaded"
  }
  ensures {
    one_protocol_type  "each operation's input and answer is one specforge_protocol_types type the host and the SDK share"
    strict_answers     "an answer that does not decode as its protocol type is E028; unknown fields are ignored and absent optional fields take their defaults"
    one_failure        "a trap, an unrouted export, an extension not loaded or a malformed answer is E028 naming the operation, the export and the extension"
    no_silent_failure  "no operation drops a failure: a pass's is a finding, a scanner's is reported, a command's is the command's error"
    runtimes_agree     "an SDK-declared extension answers the same through the in-process runtime as through the component runtime"
    pass_codes_checked "a pass diagnostic whose code its extension may not use is kept and reported (W150)"
  }
  contract   """
    The host performs ten operations on a loaded extension, each one call
    of one export over the WasmRuntime port: handshake and describe (the
    declaration, ADR 0012), a command (cmd__), an MCP tool or resource
    (mcp__), a compiler pass (__pass_<name>, one rule the SDK and the host share), a collector (collect__), a
    custom validator (the rule's wasm_function), a scanner (the analyzer's
    scan export) and a migration hook. Each sends one protocol type as
    JSON and reads one protocol type back (specforge_protocol_types); an
    optional field the host leaves unset is absent, never null. A trap, an
    export the guest does not route, an extension the runtime did not load,
    or an answer that does not decode is E028 naming the operation, the
    export and the extension, with the suggestion to report it to the
    extension's author. What a failure means is the operation's: a check
    pass's is a compile diagnostic, an analyze pass's a finding of that
    pass, a command's its E028 error, a scanner's a reported failure that
    makes the gap report approximate, a collector's the collect error. A
    migration hook's answer is not read.
    A component guest builds its declaration once per instance and answers
    the handshake and every describe from it.

    A pass diagnostic keeps the code and severity the pass gave it; when
    the code is not one the extension may use (its own catalogued code
    at its level, or a third-party code whose prefix states its level),
    the host adds one W150 per code naming the extension and the pass.
    Every diagnostic a rule or a pass of an extension produces names that
    extension as its origin, and a code the extension may not use is not
    titled or explained as its owner's.
  """
  verify unit "every operational payload is one protocol type the host and the SDK share"
  verify unit "an SDK-declared extension answers the same through the in-process runtime as through the component runtime"
  verify unit "both runtimes report an unknown extension, an unrouted export, a guest error and a guest panic as traps"
  verify unit "every extension call encodes its input as the protocol type the SDK decodes"
  verify unit "a guest answers the handshake and every describe from one declaration it builds once"
  verify unit "every extension call decodes the protocol type the SDK encodes"
  verify unit "a call whose export trapped is E028 naming the extension, the operation and the export"
  verify unit "a call whose answer does not decode as its protocol type is E028, never a default"
  verify unit "an unknown field in an answer is ignored and an absent optional field takes its default"
  verify unit "a pass answer may be bare diagnostics or diagnostics with a summary, and its diagnostics come back in canonical order with an entity's span attached"
  verify unit "an analyze pass that traps is reported as an E028 finding of that pass"
  verify unit "a scanner that traps or answers malformed output is reported, not dropped"
  verify unit "a pass, collector, custom rule, scanner or migration hook is declared with its handler, and its export answers through it"
  verify unit "the SDK routes a compiler pass at the export the host calls it by"
  verify unit "a pass input that does not encode fails each pass's call, naming its export, and nothing is sent"
  verify unit "an export the guest's handler answers decodes its input and encodes its answer as a declared handler does"
  verify unit "a pass diagnostic whose code the extension may not use is reported (W150) and kept"
  verify unit "a diagnostic an extension reported names its extension, and a code it may not use is not described as its owner's"
  verify contract "Call Extension Exports: extension calls hold — extension_loaded, one_protocol_type, strict_answers, one_failure, no_silent_failure, runtimes_agree, pass_codes_checked"
}

// -- Check-Phase Passes and the Build Cache -----

behavior run_check_phase_passes "Run Check-Phase Passes" {
  features   [contribution_based_extensions]
  invariants [extension_load_order_determinism]
  category   validation
  types      [ExtensionDeclaration, Diagnostic, WasmTrapInfo]
  ports      [WasmRuntime]
  requires {
    graph_checked "the graph is built and its checks (core validation, the registry checks and the extensions' validation rules) have run"
  }
  ensures {
    runs_every_compile     "a pass declared with phase check runs on every compile, after the graph checks"
    reported_by_check      "its diagnostics join the compile's, with the codes and severities the pass returns, so specforge check, watch, the LSP and MCP report them"
    analyze_only_otherwise "a pass without phase check runs only under analyze, and analyze does not run a check pass"
    declared_order         "check passes run in the order their after/before constraints give, extension by extension in load order"
    trap_is_diagnostic     "a check pass that traps or answers output that does not parse is E028, never a crash"
    entity_span_attached   "a pass diagnostic with no span that names an entity gets that entity's span"
  }
  contract   """
    An extension declares its compiler passes in `__describe passes`
    (name, after, before, phase). A pass declared with `phase: "check"`
    is part of the compile: the shared compiled project runs it after the
    graph checks, so every surface reports what it finds and `specforge
    check` exits 1 on its errors. Which passes are check passes is read
    once, when the extensions load; a compile with none costs nothing.

    The pass export `__pass_<name>` receives the same input an analyze
    pass does (`entities`, `edges`; each entity carries `testable` and
    `exempt`: it owes no obligations of its own, decided from the
    registries), with no `test_results` or
    `proved_claims` (a compile has neither) and with `previous`, the
    statuses of the build cache (read_build_cache). It answers the same
    output: diagnostics, bare or as `{diagnostics, summary}`; the summary
    is ignored. Each diagnostic keeps the code and severity the pass gave
    it (W150 beside it when the extension may not use the code). One
    with no span that carries `entity: "<id>"` gets the span of that
    entity. A trap or an answer that does not parse is E028 naming
    the pass's export and its extension (call_extension_exports); the
    other passes still run.

    A pass with any other phase, or none, runs only under `specforge
    analyze`, which skips check passes: their findings are the compile's.
  """
  produces   []
  verify unit "a pass declared for the check phase runs on every compile"
  verify unit "a check pass's diagnostics are reported by specforge check"
  verify unit "a session reports a check pass's diagnostics after an update"
  verify unit "a pass without the check phase runs only under analyze"
  verify unit "check passes run in their declared after/before order"
  verify unit "a trapping check pass is a diagnostic, not a crash"
  verify unit "a pass diagnostic naming an entity gets that entity's span"
  verify unit "a pass entity carries whether the host found it exempt"
  verify unit "the pass input carries each entity's exemption, which the SDK's PassEntity reads"
}

behavior write_build_cache "Write the Build Cache" {
  features [ci_integration]
  category command
  types    [Graph, Diagnostic]
  ports    [FileSystem]
  requires {
    project_compiled "the project compiled and its diagnostics are known"
  }
  ensures {
    statuses_recorded "check --cache writes every entity whose kind declares a lifecycle field, with its kind and that field's value, to specforge-cache.json at the project root"
    deterministic     "the file is byte-identical for the same sources"
    opt_in            "check without --cache never writes the file, and no other command or surface writes it"
    clean_builds_only "the file is written only when check exits 0; otherwise the previous file is left as it was"
  }
  contract """
    The build cache is the explicit, opt-in record of the lifecycle states
    of one build, so history rules (status transitions) compare against a
    declared input, never hidden state. `specforge check --cache` writes
    it after the compile, to `specforge-cache.json` beside
    `specforge.json`:
    `{"format": 1, "statuses": {"<entity id>": {"kind": "<kind>",
    "status": "<status>"}}}`, entities sorted by id, pretty-printed with
    a final newline. Only entities whose kind declares a `lifecycle_field`
    (ADR 0009) and that give it a text value are recorded, the value under
    `status` whatever the field is called; no field name is known to the
    cache. The file is replaced atomically (written beside, then
    renamed). A check that exits non-zero (errors, or warnings under
    `--strict`) does not write it: a broken build is not a baseline.
    A --severity filter does not change whether the file is written.
    CI may commit the file to check transitions across builds.
  """
  produces []
  verify unit "check --cache records each entity's kind and lifecycle state"
  verify unit "a kind without a lifecycle field is not recorded"
  verify unit "the cache file is deterministic"
  verify integration "check without --cache never writes the cache"
  verify unit "check --cache with errors leaves the cache untouched"
  verify unit "check --strict --cache with warnings leaves the cache untouched"
  verify unit "a check that passes records the cache and says so"
  verify integration "MCP validate never writes the build cache"
}

behavior read_build_cache "Read the Build Cache" {
  features [ci_integration]
  category validation
  types    [Diagnostic]
  ports    [FileSystem]
  requires {
    check_pass_loaded "at least one check-phase pass is loaded"
  }
  ensures {
    previous_given     "when specforge-cache.json parses, every check pass receives its statuses as previous"
    absent_is_none     "without the file, previous is absent"
    invalid_is_warning "a file that cannot be read or parsed, or declares another format, is W144 and previous is absent"
    passes_only        "the file is read on each compile with a check pass, by every surface, and never otherwise"
  }
  contract """
    When the project root has `specforge-cache.json`, each compile reads
    it as a declared input and hands it to the check passes as
    `previous: {"statuses": {"<entity id>": {"kind", "status"}}}`. With
    no file, `previous` is absent, so history rules stay silent on a
    first build. A file that cannot be read, is not valid JSON, or has a
    `format` other than 1 is a W144 warning and `previous` is absent. The
    file is read on every compile that runs a check pass (check, watch,
    the LSP, MCP), so a cache another command just wrote is seen by the
    next compile; with no check pass loaded, it is not read.
  """
  produces []
  verify unit "check passes receive the cached statuses as previous"
  verify unit "without a cache file previous is absent"
  verify unit "an invalid cache file is W144 and previous is absent"
}

behavior enforce_per_call_site_permissions "Enforce Per-Call-Site Permissions" {
  features   [wasm_host_function_api]
  invariants [wasm_sandbox_integrity]
  category   command
  types      [ExtensionDeclaration, SandboxPolicy]
  ports      [WasmRuntime]
  requires {
    sandbox_policy_ready    "sandbox policy has been computed for the extension"
    contribution_type_known "the contribution type of the current export call site is known"
  }
  ensures {
    contribution_permission_denied_emitted "contribution_permission_denied event is emitted when unauthorized host function is called"
    per_call_site_enforced                 "permissions are enforced per export call site, not per extension"
    unauthorized_calls_rejected            "calls to unauthorized host functions are rejected"
  }
  contract   """
    Host function permissions MUST be enforced per export call site, not
    per extension. An extension's validator export MUST only access query_graph
    and emit_diagnostic. An extension's renderer export MUST additionally
    access emit_file. An extension's provider export MUST additionally access
    http_get. An extension's entity contribution exports MUST only access
    query_graph, add_graph_node, and add_graph_edge. Collector contributions
    MUST only access query_graph and emit_file. An extension's parser
    contribution exports MUST only access emit_diagnostic, add_graph_node,
    add_graph_edge, and read_file. Parsers do NOT get query_graph because
    they run during graph construction, not after (per ADR
    extension_file_parsers). Calls to unauthorized host functions MUST be
    rejected.

    Surface contributions (cmd__, mcp__ exports) are granted no
    capability at all (invariant surface_sandbox_ceiling,
    behaviors/surface-contributions.spec). This behavior covers
    compile-time contribution call sites only.
  """
  produces   [contribution_permission_denied]
  verify unit "validator export limited to query_graph and emit_diagnostic"
  verify unit "renderer export additionally allows emit_file"
  verify unit "provider export additionally allows http_get"
  verify unit "entity contribution export limited to query_graph, add_graph_node, and add_graph_edge"
  verify unit "collector contribution export limited to query_graph and emit_file"
  verify unit "parser contribution export limited to emit_diagnostic, add_graph_node, add_graph_edge, and read_file"
  verify unit "unauthorized host function call is rejected"
  verify contract "Enforce Per-Call-Site Permissions: per-call-site permission enforcement holds — sandbox_policy_ready, contribution_type_known, contribution_permission_denied_emitted, per_call_site_enforced, unauthorized_calls_rejected"
}

// -- Collector Contribution Behaviors -----

// Cross-ref: the collect flow (ADR 0002) spans these behaviors:
// register_collector_contributions → auto_detect_collector →
// approve_collector_command → run_collector_command → dispatch_collector →
// ingest_collector_report. The CLI entry point is `specforge collect`; the
// MCP entry point is provide_mcp_collect_tool (behaviors/mcp-operations.spec).
behavior register_collector_contributions "Register Collector Contributions" {
  features   [test_result_collection]
  invariants [extension_load_order_determinism]
  category   query
  types      [ExtensionDeclaration, CollectorDescriptor, CollectorAutoDetect]
  ports      [WasmRuntime]
  requires {
    manifest_declares_collectors "an enabled extension's handshake raises the collectors flag and its describe payload lists collectors"
  }
  ensures {
    collectors_listed       "every declared collector is listed with its extension, export, detection files, command and report location"
    default_report_location "a collector that names no report location reads .specforge/reports/<name>.json"
  }
  contract   """
    Collectors come from the `collectors` describe payload of each enabled
    extension, in manifest order. A collector names its pure export
    (`collect__<name>`), the project-root files that select it, the argv
    that runs its test runner (elements may contain the `{report}`
    placeholder) and the report file or directory the runner writes,
    relative to the project root. A collector without a report location
    reads `.specforge/reports/<name>.json`.
  """
  produces   [collector_registered]
  verify unit "collector contribution parsed from manifest"
  verify contract "Register Collector Contributions: collector contribution registration holds — manifest_declares_collectors, collectors_listed, default_report_location"
}

// NOTE: auto_detect_collector does not produce an event because dispatch is
// CLI-initiated (specforge collect), not event-driven.
behavior auto_detect_collector "Auto-Detect Collector" {
  features   [test_result_collection]
  invariants [extension_load_order_determinism]
  category   validation
  types      [CollectorDescriptor, CollectorAutoDetect]
  ports      [FileSystem]
  consumes   [collector_registered]
  requires {
    collector_registered_fired "collector_registered event has fired, confirming collectors are available for auto-detection"
  }
  ensures {
    all_matches_selected    "every collector whose detection files exist at the project root is selected"
    runner_flag_selects_one "--runner selects exactly the collector with that name or extension"
    single_collector_used   "a single enabled collector is used without detection"
    no_match_diagnosed      "no collector, no match among several or an unknown --runner is E058, listing the available collectors"
  }
  contract   """
    Without `--runner`, `specforge collect` selects every collector whose
    detection files exist at the project root; the last segment of a
    detection pattern may use `*` wildcards (`vitest.config.*`). A project
    with Rust and TypeScript tests therefore collects from both runners.
    `--runner` names one collector by name or extension. When only one
    collector is enabled, it is selected without detection: detection
    only decides between several. When nothing can be
    selected, the command fails with E058 and lists the collectors that are
    available.
  """
  produces   []
  verify unit "file pattern match selects collector"
  verify unit "wildcards match within the last path segment"
  verify unit "no match emits E058 with available collectors"
  verify integration "a single enabled collector is used without detection"
  verify contract "Auto-Detect Collector: collector auto-detection holds — collector_registered_fired, all_matches_selected, runner_flag_selects_one, single_collector_used, no_match_diagnosed"
}

behavior approve_collector_command "Approve Collector Command" {
  features   [test_result_collection]
  invariants [extension_isolation]
  category   validation
  types      [CollectorDescriptor]
  ports      [FileSystem]
  requires {
    command_declared "the selected collector declares a command"
  }
  ensures {
    consent_outside_project   "approvals are stored in the user-level consent store, never in the project"
    changed_command_reprompts "an approval covers one project, extension, collector and exact argv; any change asks again"
    non_interactive_refuses   "without a terminal, or with JSON output, an unapproved command is refused with E059 unless --yes is passed"
  }
  contract   """
    `specforge collect` runs a collector's command only after the user
    approves it. It shows the extension, the project root and the exact
    command, asks once, and remembers the answer per project, extension,
    collector and argv in `~/.specforge/collector-consent.json`
    (`$SPECFORGE_CONSENT_FILE` overrides). The store lives outside the
    project, so a cloned repository can't approve its own commands. A
    changed command asks again. Without a terminal, or with `--format
    json`, nothing is asked: an unapproved command fails with E059, `--yes`
    runs it without recording an approval, and `--no-run` parses an
    existing report. The MCP server never prompts and runs only approved
    commands.
  """
  produces   []
  verify unit "consent is keyed by project, extension and command"
  verify integration "unapproved command without a terminal fails with E059 and runs nothing"
  verify integration "--yes runs the declared command"
  verify contract "Approve Collector Command: consent holds — command_declared, consent_outside_project, changed_command_reprompts, non_interactive_refuses"
}

behavior run_collector_command "Run Collector Command" {
  features   [test_result_collection]
  invariants [extension_isolation]
  category   command
  types      [CollectorDescriptor]
  ports      [FileSystem]
  requires {
    command_approved "approve_collector_command allowed the command"
  }
  ensures {
    report_inside_project  "a report location outside the project (absolute or with ..) is refused with E058"
    stale_report_ignored   "an earlier run's report is never read: a report file is removed before the command starts, and only report-directory files written during the run are read"
    report_path_exported   "the command runs in the project root with SPECFORGE_REPORT set to the absolute report path and {report} expanded"
    failing_tests_recorded "a non-zero exit is not an error when a report was written; no report is E045"
  }
  contract   """
    The host, not the extension, runs the approved command: extensions stay
    pure wasm with no process access. The report location must stay inside
    the project. So that an old report can't pass for a new run, the host
    removes a report file before running, and reads only the `*.json` files
    of a report directory that were written after the command started (it
    deletes nothing there: other tools may keep files in it). The command runs
    in the project root with stdin closed, `{report}` expanded in its
    arguments and `SPECFORGE_REPORT` set to the absolute report path. Its
    output goes to the terminal, to stderr under `--format json`, and
    nowhere under MCP. Test runners exit non-zero when tests fail, so the
    exit status only matters when no report was written, which is E045.
    A collector that declares `capture: "stdout"` gets the command's
    standard output too: it still reaches the terminal (or stderr) as it
    is produced, and is also written to `<collector>.stdout.txt` inside a
    report directory (next to a report file), which `--no-run` reads back.
    A captured output counts as a report. Any other `capture` value is
    E058.
  """
  produces   []
  verify unit "report path must stay inside the project"
  verify unit "run ignores stale reports and sets the report env"
  verify unit "command line expands the report placeholder"
  verify unit "a captured stdout is kept in every output mode"
  verify unit "an unknown capture is refused"
  verify contract "Run Collector Command: collector command execution holds — command_approved, report_inside_project, stale_report_ignored, report_path_exported, failing_tests_recorded"
}

behavior dispatch_collector "Dispatch Collector" {
  features   [test_result_collection]
  invariants [wasm_sandbox_integrity, extension_isolation]
  category   query
  types      [CollectorDescriptor, CollectInput, CollectOutput, WasmTrapInfo]
  ports      [WasmRuntime, FileSystem]
  requires {
    report_available "the collector's report exists: a file, or a directory of *.json files"
  }
  ensures {
    collector_dispatched_emitted "collector_dispatched event is emitted after the export returns"
    report_files_passed          "the export receives every report file with its project-relative path and text"
    traps_reported               "a trap or malformed answer is E028"
  }
  contract   """
    The host reads the report (the file itself, or every `*.json` file
    directly inside a report directory, in path order) and passes the files
    to the collector's pure export as the protocol's CollectInput
    (`{"reports": [{"path", "content"}], "stdout"?}`, `stdout` being the
    captured standard output when the collector declares one, absent
    otherwise), and reads its answer as the protocol's CollectOutput
    (specforge_protocol_types, through call_extension_exports).
    The export answers with test results grouped by entity:
    `{"entity_results": [{"entity_id", "test_results": [{"name", "status",
    "verify"?, "duration_ms"?}]}], "unlinked"?: [{"name", "path",
    "status"}]}`, where status is passed, failed or skipped and `unlinked`
    lists tests the report doesn't link to an entity. A trap or an answer
    that is not a CollectOutput (a test without a name, an empty object) is
    E028 naming the collector, never empty results. `--no-run`
    and `--report` skip running and dispatch an existing report.
  """
  produces   [collector_dispatched]
  verify unit "reads a file or every json file in a directory"
  verify unit "the collector receives a CollectInput and answers a CollectOutput, and an answer that is not one is an error naming the collector"
  verify contract "Dispatch Collector: collector dispatch holds — report_available, collector_dispatched_emitted, report_files_passed, traps_reported"
}

behavior ingest_collector_report "Ingest Collector Report" {
  features   [test_result_collection]
  invariants [collector_output_conformance]
  category   query
  types      [CollectOutput, Graph]
  ports      [FileSystem]
  consumes   [collector_dispatched]
  requires {
    collector_dispatched_fired "collector_dispatched event has fired with the export's answer"
    graph_available            "the compiled graph is available to check entity IDs"
  }
  ensures {
    collector_report_ingested_emitted "collector_report_ingested event is emitted after the report is written"
    runner_results_replaced           "a runner's earlier results are replaced; other runners' results are kept"
    unknown_entities_warned           "results for undeclared entities are dropped with W115"
    skipped_not_recorded              "skipped tests are counted but not recorded"
    merged_report_written             "the merged results are written to specforge-report.json, which analyze reads by default"
  }
  contract   """
    Each runner's answer is merged into `specforge-report.json`: the
    runner's earlier results are removed and its new ones added, so several
    runners can report on one project. Every recorded test carries its
    name, `pass` or `fail`, the `verify` obligation it names and the runner
    that recorded it. Results for entities the graph doesn't declare are
    dropped with a W115 warning. Skipped tests prove nothing, so they're
    counted but not recorded. `specforge analyze` reads the written report
    without `--test-results`.
  """
  produces   [collector_report_ingested]
  verify unit "merge replaces only the same runner"
  verify unit "the entities results may name are the entity snapshot's, with their obligation texts"
  verify integration "collect then analyze scores the recorded tests"
  verify contract "Ingest Collector Report: collector report ingestion holds — collector_dispatched_fired, graph_available, collector_report_ingested_emitted, runner_results_replaced, unknown_entities_warned, skipped_not_recorded, merged_report_written"
}

behavior resolve_test_conventions "Resolve Test Naming Conventions" {
  features   [test_result_collection]
  invariants [collector_output_conformance]
  category   query
  consumes   [collector_dispatched]
  requires {
    unlinked_returned "the collector returned the tests its report doesn't link"
  }
  ensures {
    double_underscore_linked "entity_id__slug links the test to that entity"
    module_linked            "otherwise the innermost module named after an entity links it"
    obligation_by_slug       "the rest of the name proves the one obligation whose slug it is"
    ambiguity_warned         "a name that splits into several entities is W137 and not linked"
    unresolved_silent        "a test no convention links is left out without a diagnostic"
  }
  contract   """
    A test the collector's report doesn't link (a plain `#[test]` seen in
    libtest's output, say) is linked by its name before the merge. When
    the part of the test's own name before a `__` is a declared entity ID,
    the test belongs to that entity and the rest of the name is checked
    against its obligations; otherwise the innermost enclosing module
    named after an entity takes it, with the whole name checked. The test
    proves the obligation whose slug equals the name's slug, when exactly
    one does, and otherwise the entity alone. A name whose `__` splits
    give more than one declared entity is W137 and stays unlinked. Tests
    no rule links are dropped silently, since plain tests are the norm.
    `specforge collect` reports how many tests it linked this way.
  """
  produces   []
  verify unit "a double underscore splits the entity from the obligation"
  verify unit "an entity with single underscores is not split"
  verify unit "the innermost module named after an entity links its tests"
  verify unit "a double underscore takes precedence over the module"
  verify unit "an obligation matches only when its slug is unique"
  verify unit "a name that splits into several entities is W137"
  verify unit "a test no convention links is left out silently"
  verify integration "collect links plain tests by naming convention"
  verify integration "a plain cargo test proves obligations by naming convention"
}

behavior slug_obligation_text "Slug an Obligation Text" {
  features   [test_result_collection]
  invariants [collector_output_conformance]
  category   query
  ensures {
    one_algorithm "the host and the specforge-test crate slug a text the same way"
  }
  contract   """
    A test named after an obligation carries the obligation's slug:
    `<=`, `>=`, `<` and `>` become `lte`, `gte`, `lt` and `gt`, spaces
    become underscores, ASCII letters are lowercased, every other character
    outside `[a-z0-9_]` is dropped, runs of underscores collapse to one and
    leading or trailing underscores are trimmed. The host and the
    `specforge-test` crate each carry this algorithm and are held to the
    same test vectors, so a name that links in one links in the other.
  """
  verify unit "slug matches the shared test vectors"
}

// -- Discovery & Configuration -----

behavior run_doctor_check "Run Doctor Check" {
  features   [entity_enhancement]
  // Doctor REPORTS on invariant violations — it does not ENFORCE them.
  // Enforcement is done by the behaviors listed in each invariant's enforced_by.
  category   validation
  invariants [diagnostic_determinism]
  types      [ExtensionDeclaration, EntityEnhancementDescriptor]
  ports      [FileSystem]
  consumes   [enhancement_registered]
  requires {
    enhancement_registered_fired "enhancement_registered event has fired, confirming FieldRegistry is built"
    filesystem_available         "FileSystem port is available for reading extension manifests"
  }
  ensures {
    doctor_check_completed_emitted "doctor_check_completed event is emitted after all checks finish"
    report_produced                "human-readable report listing extensions, enhancements, and conflicts is produced"
    json_output_supported          "--format json produces machine-readable JSON output for CI integration"
  }
  contract   """
    When specforge doctor is invoked, the system MUST load all extension
    manifests, build the FieldRegistry, detect all conflicts, and
    produce a human-readable report listing installed extensions, their
    enhancements, any conflicts with actionable resolution suggestions,
    and additional checks (shadowed fields, unknown target entities,
    edge label conflicts). An enabled extension that fails to load (E028:
    not installed; E070: its binary is not the one the lock pins) MUST be
    reported as an error. Each problem MUST be one finding that says what it
    is about (the config, the lock, a binary, a load, a conflict, a
    shadowing, a peer, the toolchain); a missing or changed installed binary
    is one binary finding, whose remediation is the command that reinstalls
    it as its lock entry records it. The report MUST carry its verdict (no
    error-level finding), and specforge doctor --format json and
    specforge.doctor MUST return the same report (the CLI adds the user's
    credentials). Each peer requirement the compile reports unsatisfied
    (E027, E073) MUST be an error finding with the remedy its diagnostic
    suggests: doctor and check judge peers by one rule over the loaded
    extensions (ADR 0041), so a peer a builtin or a .wasm file entry
    satisfies is not reported. Doctor MUST list the extensions the
    extensions listing lists, in its order, each with the version, source
    and status the listing gives it. Run in a
    directory without specforge.json, doctor MUST report a warning finding
    config_missing naming the directory; the run stays healthy. A
    specforge.lock that exists but cannot be read (E033) MUST be reported
    as an error finding. A
    remediation that names a command MUST name one the user can run as
    written. A finding whose diagnostic offers no
    suggestion of its own MUST quote the catalogue's explanation of its
    code. specforge doctor --format json MUST produce machine-readable
    JSON output for CI integration.
  """
  produces   [doctor_check_completed]
  verify unit "doctor lists all installed extensions with enhancement counts"
  verify unit "doctor lists all enhancements grouped by entity kind"
  verify unit "doctor reports conflicts with resolution suggestions"
  verify unit "doctor detects shadowed grammar-level constructs"
  verify unit "doctor --format json produces valid JSON output"
  verify unit "doctor reports an extension that fails to load (E028, E070) as an error"
  verify unit "doctor reports a missing or changed installed binary once, with the remedy its load gives"
  verify unit "doctor reports the peer requirements check reports, with the remedy each suggests"
  verify unit "a peer a builtin satisfies is not reported"
  verify unit "a finding without its own suggestion quotes the catalogued explanation"
  verify unit "doctor lists the extensions the extensions listing lists, each with its version, source and status"
  verify unit "doctor in a directory without specforge.json reports config_missing as a warning"
  verify unit "a lock file that cannot be read is an error finding naming E033"
  verify unit "doctor states each finding once, saying what it is about, and its verdict; the CLI and MCP return one report"
  verify contract "Run Doctor Check: doctor check holds — enhancement_registered_fired, filesystem_available, doctor_check_completed_emitted, report_produced, json_output_supported"
}

// -- Extension Source Resolution -----

behavior parse_extension_specifier "Parse Extension Specifier" {
  features   [wasm_extension_installation]
  invariants [registry_integrity]
  category   command
  types      [ExtensionSpecifier, ExtensionSource, ExtensionError, PackageName, VersionRequirement]
  requires {
    specifier_string_provided "a raw extension specifier string is provided for parsing"
  }
  ensures {
    extension_specifier_parsed_emitted "extension_specifier_parsed event is emitted with structured source descriptor"
    invalid_specifier_diagnosed        "invalid specifiers produce ExtensionError diagnostic with expected format"
  }
  contract   """
    The system MUST read an add argument once into one source: a builtin's
    name, a local path (ending in .wasm, or starting with ./, ../ or /), a
    git+ URL, or a package reference @scope/name[@requirement], where the
    name is a PackageName and the requirement a VersionRequirement (latest
    when absent). An argument that is none of these MUST be refused with
    E054 before any registry is asked; a requirement that is not one MUST be
    refused with R-RES-003 before any registry is asked. No other code may
    split name@requirement.
  """
  produces   [extension_specifier_parsed]
  verify unit "@scope/name@version parsed as registry source"
  verify unit "./path parsed as local source"
  verify unit "git:url#ref parsed as git source"
  verify unit "invalid specifier produces ExtensionError"
  verify unit "each add argument reads as one extension source"
  verify unit "a package name is @scope/name or a local name, and always a relative path inside its directory"
  verify unit "a package name crosses a registry URL as one segment"
  verify unit "a version requirement is latest, one version or a SemVer requirement"
  verify contract "Parse Extension Specifier: extension specifier parsing holds — specifier_string_provided, extension_specifier_parsed_emitted, invalid_specifier_diagnosed"
}

behavior resolve_extension_source "Resolve Extension Source" {
  features   [wasm_extension_installation]
  invariants [registry_integrity]
  category   query
  types      [ExtensionDeclaration, ExtensionSpecifier, ExtensionSource, ExtensionError]
  ports      [FileSystem, RegistryClient]
  consumes   [extension_specifier_parsed]
  requires {
    extension_specifier_parsed_fired "extension_specifier_parsed event has fired, confirming structured source descriptor is available"
    source_ports_available           "FileSystem and RegistryClient ports are available for resolution"
  }
  ensures {
    extension_source_resolved_emitted "extension_source_resolved event is emitted with concrete manifest and .wasm binary"
    resolution_failure_diagnosed      "resolution failures produce ExtensionError diagnostic"
  }
  contract   """
    Given a parsed extension specifier, the system MUST resolve it to a
    concrete manifest and .wasm binary. Registry sources MUST query the
    registry API. Local sources MUST read from the filesystem. Git sources
    MUST clone or fetch the repository at the specified ref. Resolution
    failures MUST produce a ExtensionError diagnostic.
  """
  produces   [extension_source_resolved]
  verify unit "registry source resolves via registry API"
  verify unit "local source resolves from filesystem"
  verify unit "git source resolves from repository"
  verify unit "resolution failure produces ExtensionError"
  verify contract "Resolve Extension Source: extension source resolution holds — extension_specifier_parsed_fired, source_ports_available, extension_source_resolved_emitted, resolution_failure_diagnosed"
}

// -- Lock File Management -----

behavior write_lock_file "Write Lock File" {
  features   [wasm_lock_management]
  invariants [extension_load_order_determinism, registry_integrity]
  category   command
  types      [ExtensionDeclaration, LockFile, LockFileEntry]
  ports      [FileSystem]
  requires {
    extensions_resolved  "all extensions have been resolved with exact versions and wasm hashes"
    filesystem_available "FileSystem port is available for writing lock file"
  }
  ensures {
    lock_file_written_emitted "lock_file_written event is emitted after successful write"
    output_deterministic      "same inputs always produce byte-identical lock file output"
    write_atomic              "lock file is written atomically to prevent corruption"
  }
  contract   """
    After resolving all extensions, the system MUST write a specforge.lock
    file containing the exact resolved version and SHA256 wasm_hash for
    each installed extension. The lock file format MUST be deterministic —
    same inputs always produce byte-identical output. The resolved_at
    timestamp is metadata excluded from the determinism guarantee. The
    lock file MUST be written atomically to prevent corruption.
  """
  produces   [lock_file_written]
  verify unit "lock file contains exact versions and wasm hashes"
  verify unit "lock file output is deterministic"
  verify unit "lock file written atomically"
  verify contract "Write Lock File: lock file writing holds — extensions_resolved, filesystem_available, lock_file_written_emitted, output_deterministic, write_atomic"
}

behavior read_lock_file "Read Lock File" {
  features   [wasm_lock_management]
  invariants [extension_load_order_determinism]
  category   command
  types      [ExtensionDeclaration, LockFile, LockFileEntry, ExtensionError]
  ports      [FileSystem]
  consumes   [all_files_parsed]
  requires {
    all_files_parsed_fired "all_files_parsed event has fired, confirming spec files are parsed"
    filesystem_available   "FileSystem port is available for reading lock file"
  }
  ensures {
    lock_file_read_emitted  "lock_file_read event is emitted after lock file is processed"
    locked_versions_used    "locked versions are used instead of resolving from sources when lock file exists"
    malformed_lock_reported "a lock file that exists and can't be read is E033: nothing is known to be installed"
  }
  contract   """
    When a specforge.lock file exists, the system MUST use the versions and
    hashes it records. A lock file that exists and can't be read or parsed is
    E033: no installed extension loads from it (each is E028 naming the
    lock), and add, update and remove refuse to change what is installed
    until it is fixed or deleted, so it is never silently replaced.
  """
  produces   [lock_file_read]
  verify unit "locked versions used when lock file exists"
  verify unit "missing lock entry triggers resolution"
  verify unit "an unreadable lock file is E033 and is never replaced by a change"
  verify contract "Read Lock File: lock file reading holds — all_files_parsed_fired, filesystem_available, lock_file_read_emitted, locked_versions_used, malformed_lock_reported"
}

// ── Extension Update ──────────────────────────────────────────

behavior update_all_extensions "Update All Extensions" {
  features   [wasm_extension_maintenance]
  invariants [
    wasm_sandbox_integrity,
    peer_dependency_satisfaction,
    wasm_compile_cache_integrity,
    extension_operation_atomicity,
  ]
  category   command
  types      [ExtensionDeclaration, LockFileEntry, ExtensionError]
  ports      [WasmRuntime]
  requires {
    extensions_installed "at least one extension is installed with a valid manifest"
    registries_reachable "configured registries are reachable for version checks"
  }
  ensures {
    batch_update_completed_emitted "batch_update_completed event is emitted after all upgrades are applied"
    semver_constraints_respected   "upgrades respect semver constraints, major bumps skipped without --major"
    lock_hashes_refreshed          "updated lock entries record the new binary hashes"
    atomic_rollback_on_failure     "if any upgrade fails, all changes are rolled back"
  }
  contract   """
    When specforge update is invoked, the system MUST check all installed
    extensions for newer versions by querying their configured registries.
    The system MUST upgrade each extension to the latest compatible version
    respecting semver constraints. Major version bumps MUST be skipped unless
    --major is specified. After upgrading, the system MUST refresh the lock
    file with the new binary hashes. Peer dependency
    conflicts introduced by upgrades MUST be detected and reported before
    applying changes. Every upgrade is judged against the lock as the
    whole update leaves it, whatever order the lock lists the extensions
    in: an upgraded extension's peer ranges against the versions its peers
    are left at, those this update moves included, and every locked
    extension's ranges against the versions this update locks. If any
    upgrade fails, the system MUST roll back all
    changes and report the failure. A write that fails while the checked
    upgrades are applied is the update's own failure, naming the file it
    could not write (E032) or the lock (E033); every file is put back, and no
    extension is reported updated. Only registry installs are updated: an
    extension installed from a local file (source local:<path>) MUST NOT be
    replaced from a registry. When a registry-installed extension is
    to be updated and no registry is configured in specforge.json, update
    MUST make no network call and MUST fail with E063, whose suggestion
    names the registries key.
    Under --format json it prints the update document: ok (nothing
    failed, so the update was written), each locked extension in lock
    order with its status (updated, with its publisher's acceptance and
    key id when signed; held_back when another extension failed;
    up_to_date; not_from_registry; failed, with the failure's code,
    message and suggestion), and the batch_update_completed payload. A
    failed update prints the same document and exits 1.
  """
  produces   [batch_update_completed]
  verify unit "with no registry configured, update makes no network call and reports how to configure one"
  verify unit "newer versions detected from registry"
  verify unit "semver-compatible upgrades applied"
  verify unit "major version skipped without --major flag"
  verify unit "lock file records new binary hashes after update"
  verify unit "peer dependency conflicts detected before applying"
  verify unit "every upgrade is judged against the lock as the whole update leaves it, whatever order the lock lists them in"
  verify unit "failed upgrade rolls back all changes"
  verify unit "a write that fails while applying fails the update, naming what it could not write, and nothing is applied"
  verify integration "update never replaces a locally installed extension from a registry"
  verify unit "the update document lists every locked extension's status, held back when another failed"
  verify integration "update --format json prints every locked extension's status and the batch_update_completed payload"
  verify contract "Update All Extensions: batch extension update holds — extensions_installed, registries_reachable, batch_update_completed_emitted, semver_constraints_respected, lock_hashes_refreshed, atomic_rollback_on_failure"
}
