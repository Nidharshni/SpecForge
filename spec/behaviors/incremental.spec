// Incremental compilation behaviors — watch mode and file change handling

use "events/compilation"
use "invariants/core"
use "invariants/validation"
use "invariants/zero-entity-core"
use "ports/inbound"
use "ports/outbound"
use "types/config"
use "types/core"
use "types/diagnostics"
use "types/graph"
use "types/zero-entity-core"

behavior watch_file_system_for_changes "Watch File System for Changes" {
  features   [incremental_compilation]
  invariants [incremental_correctness, watch_mode_response_latency]
  category   command
  types      [FileEntry]
  ports      [FileSystem]
  produces   [file_changed]
  requires {
    watch_mode_active "specforge watch command is active and the file watcher is initialized on the spec root"
  }
  ensures {
    file_changed_emitted "file_changed event is produced for every detected file creation, modification, or deletion"
  }
  contract   """
    When specforge watch is active, the system MUST monitor all .spec files
    under the spec root for changes using the OS file watching API.
    File creation, modification, and deletion MUST each trigger
    recompilation of affected files. A directory created, moved or
    deleted under the spec root changes every source under it, and an edit
    to a symlinked source's target changes that source: the project
    session turns each batch of changed paths into changes
    (classify_project_changes). After any update that
    changes the session's inputs (an environment reload, an edit that names
    a file the checks read) the watcher follows the session's watch roots
    and brings the session up to date with what was written meanwhile
    (bring_session_up_to_date); it does so once at start, before it reports
    ready. A missing directory on the way to an input is watched from its
    nearest existing ancestor.
  """
  verify unit "file modification triggers recompilation"
  verify unit "file creation triggers recompilation"
  verify unit "file deletion triggers recompilation"
  verify integration "watch detects changes within 100ms"
  verify contract "Watch File System for Changes: file system watching holds for the declared obligations"
  verify integration "a specforge.lock change reloads the environment"
  verify integration "a .wasm file no extension loads changes nothing"
  verify integration "after spec_root changes, files under the new spec root are watched"
  verify integration "after an edit names a file outside the watched directories, a change to it is seen"
  verify integration "a file the checks read is seen when it is created in a directory that did not exist"
  verify unit "an edit that names a file outside the watched directories moves the watchers"
  verify unit "an edit that names a file inside the watched directories moves nothing"
  verify unit "after the watchers move, the session catches up on what changed while they did"
  verify unit "a failed move of the watchers is reported and the session still catches up"
  verify unit "what was written between the open and the watchers is applied before ready"
  verify integration "a directory moved inside the spec root leaves watch reporting what check reports"
  verify integration "an edit to a symlinked source's target is seen"
  verify unit "a directory event reaches every source under it"
}

behavior classify_project_changes "Classify Project Changes" {
  features   [incremental_compilation]
  invariants [incremental_correctness]
  category   command
  ports      [FileSystem]
  contract   """
    A project session MUST turn a batch of changed paths into changes by
    comparing what the paths reach with what it last read: the comparison
    bring_session_up_to_date makes, narrowed to the paths. A path under
    the spec root reaches the sources at or under it: for a directory,
    every source discovery finds in it now and every source the session
    read from it; for a file, itself. The spec root, or a directory above
    it, reaches every source. A source is keyed by its path relative to
    the spec root, a symlink by its own name; a symlinked source's target,
    or a directory above it, reaches that source. specforge.json,
    specforge.lock and every extension module the environment loaded (an
    installed extension's extension.wasm, a local .wasm entry) are
    environment inputs; specforge-cache.json, which check-phase passes
    read, and every file a file_reference field or a file_exists rule
    names, which the checks look for, are check inputs; a specforge.json
    in a directory below the spec root marks a nested project, and its
    creation or deletion is a change to every source under that
    directory. Each is reached
    by its own path or by a directory above it, and a missing check
    input's directory also by any file in it. A reached source or input
    is a change when it differs from what the session read (size and
    modification time, and the content when it was read within the
    timestamp granularity); a directory created on the way to a missing
    input's directory outside the root is that input's change, whatever
    the stamps say. Any other path changes nothing. A detached session
    (no project) takes no change from disk: its sources are the buffers
    it holds.
    Watch, the LSP and MCP MUST take changes through the session. What a
    path reaches, which directories watch watches, which files the LSP
    asks its client to report and what the session stamps MUST all derive
    from one set of session inputs, renewed when the environment loads,
    when sources are read and each time the checks run. Watch's watch
    roots and the LSP's watchers MUST cover every input, every
    specforge.json under the spec root included, and every symlinked
    source's target (the LSP spelling each under the project root as
    opened), the LSP MUST ask its client to report every entry
    created or deleted under the project root, and both MUST follow every
    update that changes the inputs. A set of watchers refused once is not
    asked for again in the same follow: the refusal is reported (watch: a
    warning event; the LSP: a WARNING log, after it registers the static
    watchers in its place) and the session still catches up.
  """
  verify unit "a discovered .spec file is a source change keyed relative to the spec root"
  verify unit "specforge.json, specforge.lock and a loaded extension module are environment changes"
  verify unit "a .wasm file no extension loads changes nothing"
  verify unit "specforge-cache.json re-runs the checks without re-parsing"
  verify unit "a file a file_reference field names re-runs the checks"
  verify unit "a file a file_exists rule names re-runs the checks"
  verify unit "an excluded or undiscovered .spec file changes nothing"
  verify unit "a session's watch roots cover every input it classifies"
  verify unit "the LSP's watchers cover every input the session classifies"
  verify integration "the LSP watches a missing referenced file and its directory, spelled under the project root"
  verify unit "an update that names a new file the checks read changes the session's inputs"
  verify integration "the LSP's watchers follow an edit that names a new file the checks read"
  verify unit "a directory created, moved or deleted under the spec root changes every source under it"
  verify unit "a symlinked source is keyed by its link and changes when its target does"
  verify unit "a path whose file is as the session last read it changes nothing"
  verify unit "a detached session takes no change from disk"
  verify unit "a directory moved above an environment or check input changes that input"
  verify unit "a session's watch roots and the LSP's watchers cover every symlinked source's target"
  verify unit "a set of watchers refused once is not asked for again in the same follow"
  verify unit "the LSP reports a refused set of watchers and registers the static ones"
  verify unit "an environment reload that leaves the inputs as they were moves no watchers"
  verify integration "the LSP asks its client to report every directory created or deleted under the project root"
  verify property "the changes of a batch naming every changed path are what the session finds stale"
  verify unit "a nested project's specforge.json appearing or going changes every source under its directory"
  verify unit "the LSP's watchers report every specforge.json under the spec root"
  verify integration "the LSP drops a nested project's files when its specforge.json is created, and takes them back when it goes"
}

behavior bring_session_up_to_date "Bring a Session Up to Date with Disk" {
  features   [incremental_compilation]
  invariants [incremental_correctness]
  category   command
  ports      [FileSystem]
  contract   """
    A project session opened from disk MUST bring itself up to date
    without a file watcher: it compares every discovered source and every
    environment and check input with what it last built from (size and
    modification time; a file modified within the timestamp granularity of
    the last build counts as changed unless its content is unchanged, a
    symlinked source compared through its link) and
    applies exactly those changes (a source that is no source any more, a
    nested project having taken it, leaves the graph, and one that is a
    source again is read): sources by an update, environment
    inputs by an environment reload, check inputs by re-running the
    checks. Afterwards its graph and diagnostics MUST be those a fresh
    compile of the files on disk produces. specforge.json MUST be read
    once per environment load, the extension runtime and the environment
    both built from that read, and every input MUST be stamped before
    anything reads it, the extension runtime included, so a file written
    while the session loads is seen next time. A surface that watches files
    MUST do so each time its watchers move, for what was written while they
    did not watch; a file an editor buffer holds is not stale, whatever
    happened to it on disk (hold_editor_buffers).
  """
  verify unit "an up-to-date session reports no change and re-parses nothing"
  verify unit "edits, creations and deletions since the last build are applied as one update"
  verify unit "a file rewritten within the timestamp granularity of the last build is still seen"
  verify unit "a specforge.lock change reloads the environment"
  verify unit "after bringing itself up to date a session matches a fresh compile"
  verify unit "a specforge.json or module written while the extension runtime loads is seen next time"
  verify integration "after the LSP's watchers move, the session catches up on what changed while they did"
  verify integration "the LSP's catch-up keeps an open buffer"
  verify unit "a nested project appearing below the spec root takes its files out of the session, and its removal puts them back"
}

behavior hold_editor_buffers "Hold Editor Buffers" {
  features   [incremental_compilation]
  invariants [incremental_correctness]
  category   command
  ports      [FileSystem]
  contract   """
    A project session MUST hold the editor buffers it is given: while it
    holds a buffer, the buffer's text is the truth for its file, whatever
    happens to the file on disk, its deletion included. A changed path and
    bringing the session up to date (bring_session_up_to_date) leave every
    held file out. A buffer whose text is the text the session built its
    file from changes nothing, and an update that changes no file runs no
    check. A buffer of a file that is not a project source is held and
    builds nothing. When the editor releases a buffer, a project source is
    stamped and read from disk through the session's one read, and any
    other file leaves the project; a release that changes nothing runs no
    check, unless the checks were skipped while the buffer did not parse.
    Opening a project in the place of a session and loading the environment
    again keep every held buffer, read in place of its file by their one
    cold build, so the checks run once. The session keeps each held
    buffer's version, so what is reported from it is labelled with the
    version it was computed from. Watch and MCP hold no buffer.
  """
  verify unit "a held buffer is the truth for its file until it is released"
  verify unit "a held buffer's file changed or deleted on disk is not stale"
  verify unit "holding a buffer whose text is its file's runs no check"
  verify unit "releasing a buffer reads its file through the one read and leaves nothing stale"
  verify unit "releasing a buffer outside the project drops its file"
  verify unit "releasing a buffer that did not parse runs the skipped checks"
  verify unit "opening a project in place of a session keeps its buffers and runs the checks once"
  verify unit "an environment reload keeps the held buffers and runs the checks once"
  verify unit "a held buffer that a reload brings into the project is built from its text"
}

behavior invalidate_changed_files "Invalidate Changed Files" {
  features   [incremental_compilation]
  invariants [incremental_correctness, graph_traversal_integrity]
  category   validation
  types      [Graph, Subgraph, FileEntry]
  consumes   [file_changes_coalesced]
  produces   [subgraph_invalidated]
  requires {
    file_changes_coalesced_fired "file_changes_coalesced event has fired, providing a batch of changed files from the debounce stage"
  }
  ensures {
    invalidation_set_computed    "Invalidation set is exactly the changed files"
    subgraph_invalidated_emitted "subgraph_invalidated event is produced with the computed invalidation set"
    unrelated_files_untouched    "Files outside the invalidation set are not re-parsed"
  }
  contract   """
    When a coalesced batch of file changes is received from the debounce
    stage (or editor buffers change, one or several at once, as one
    update), the system MUST compute the
    invalidation set: exactly the changed files. A parse depends only on
    its own file's text, and references resolve across the project
    without use (ADR 0004 D1-a), so an importer of a changed file parses
    the same as before and MUST NOT be re-parsed; the cross-file effects
    of a change (references, duplicates, cycles, imports) are recomputed
    over the whole graph and every cached parse instead. For file
    deletions, all entities declared in the deleted file MUST be removed
    from the graph along with their edges; no re-parse is attempted.
    For file creations, the new file MUST be parsed and its entities
    added to the graph.
  """
  verify unit "only the changed files are re-parsed"
  verify unit "an importer of a changed file is not re-parsed"
  verify unit "unrelated files are not re-parsed"
  verify unit "deleted file entities removed from graph"
  verify unit "new file entities added to graph"
  verify unit "several editor buffers changed at once are one update"
  verify unit "the typing fast path skips the checks while any edited buffer does not parse"
  verify contract "Invalidate Changed Files: file invalidation holds — file_changes_coalesced_fired, invalidation_set_computed, subgraph_invalidated_emitted, unrelated_files_untouched"
}

behavior rebuild_affected_subgraph "Rebuild Affected Subgraph" {
  features   [incremental_compilation]
  invariants [incremental_correctness, graph_traversal_integrity, zero_domain_knowledge_core]
  category   command
  types      [Graph, Subgraph, FileEntry]
  ports      [SourceParser]
  // Sequential dependency: resolve_imports_on_update consumes
  // subgraph_invalidated and produces import_dag_updated. Therefore
  // import_dag_updated always arrives AFTER subgraph_invalidated.
  // This is a correctness gate, not a parallel join barrier.
  consumes   [subgraph_invalidated, import_dag_updated]
  produces   [incremental_rebuild_complete]
  requires {
    subgraph_invalidated "subgraph_invalidated event has fired, providing the set of invalidated files"
    import_dag_updated   "import_dag_updated event has fired, confirming every file's imports were resolved again"
  }
  ensures {
    graph_reflects_reparse "In-memory graph reflects the re-parsed state of all invalidated files"
    stale_removed          "Stale nodes and edges from invalidated files are removed"
    new_added              "New nodes and edges from re-parsed files are added"
    rebuild_event_fired    "incremental_rebuild_complete event fires with accurate rebuilt file and node counts"
  }
  maintains {
    unaffected_subgraph_intact "Nodes and edges from non-invalidated files remain unchanged throughout rebuild"
  }
  contract   """
    After re-parsing invalidated files using SourceParser.parseIncremental,
    the system MUST remove stale nodes and edges from the graph using
    the mutable graph interface (per [maintain_mutable_graph]), then add
    new nodes from the re-parsed ASTs (each ID going to its first
    declaration in path order, as in a cold build) and re-link references
    over the whole graph. The result MUST be identical to a full cold
    rebuild — identical means same node set, same edge set, same field
    values, same diagnostic set. With --verify-incremental, and always in
    a debug build (watch, the LSP and MCP alike), each rebuild is compared
    with a full cold build of the same parses: its nodes, edges and
    graph-build diagnostics, in order. Its delta is checked by
    validate_delta_correctness. The cold build and the rebuild are one
    graph build (ADR 0032): a cold build applies every file at once. The
    rebuild MUST operate on generic entity nodes — it MUST NOT contain
    logic specific to any entity kind. All kind-specific validation is
    deferred to the extension validation phase after the subgraph is
    rebuilt.
  """
  verify unit "stale nodes are removed"
  verify unit "new nodes are added"
  verify property "incremental rebuild equals cold rebuild"
  verify unit "debug --verify-incremental performs cold rebuild comparison"
  verify unit "a rebuild whose diagnostics differ from a cold build is reported"
  verify contract "Rebuild Affected Subgraph: affected subgraph rebuild holds — subgraph_invalidated, import_dag_updated, graph_reflects_reparse, stale_removed, new_added, rebuild_event_fired, unaffected_subgraph_intact"
}

behavior emit_incremental_diagnostics "Emit Incremental Diagnostics" {
  features   [incremental_compilation]
  invariants [
    multi_error_collection,
    incremental_correctness,
    diagnostic_determinism,
    zero_domain_knowledge_core,
    watch_mode_response_latency,
  ]
  category   command
  types      [DiagnosticBag, DiagnosticsDelta]
  consumes   [incremental_rebuild_complete, graph_delta_computed, incremental_validators_dispatched]
  produces   [incremental_diagnostics_complete]
  requires {
    incremental_rebuild_complete_fired      "incremental_rebuild_complete event has fired, confirming subgraph rebuild is done"
    graph_delta_computed_fired              "graph_delta_computed event has fired, providing the diff between old and new graph"
    incremental_validators_dispatched_fired "incremental_validators_dispatched event has fired, confirming extension validators have run"
  }
  ensures {
    diagnostics_refreshed           "Diagnostics from invalidated files are replaced with fresh validation results"
    unchanged_diagnostics_preserved "Diagnostics from non-invalidated files remain unchanged"
    incremental_diagnostics_emitted "incremental_diagnostics_complete event fires with the merged diagnostic bag"
  }
  maintains {
    non_invalidated_diagnostics_stable "Diagnostics the change does not affect are not modified"
  }
  // These three events form a sequential chain, not a parallel fan-in:
  // incremental_rebuild_complete → graph_delta_computed → incremental_validators_dispatched
  // Listing all three as consumed events is a completeness declaration,
  // not a parallel join. The behavior activates on the last event.
  contract   """
    After incremental rebuild, the system MUST re-validate the affected
    subgraph and emit updated diagnostics. This behavior MUST wait for
    both compute_graph_delta and dispatch_incremental_validators to
    complete (via graph_delta_computed and
    incremental_validators_dispatched respectively) before emitting.
    Extension diagnostics MUST be collected into the final bag only
    after both prerequisites are satisfied. Re-validation MUST include all
    registered core validation passes and all extension-contributed
    validation passes; they run over the whole patched graph, since
    references cross files without use. The diagnostic set MUST be the
    one a full rebuild reports: diagnostics from invalidated files are
    replaced with fresh results, and a diagnostic in another file changes
    only when the change affects it (an unresolved reference to an
    entity that went away, a duplicate, a cycle) — if a change to file A
    does not affect file B, B's diagnostics MUST remain unchanged. This
    behavior orchestrates the diagnostic collection, not the extension
    invocation directly.
    Each update MUST also state the diagnostics it added and removed
    (DiagnosticsDelta), each compared whole, as every surface presents it:
    code, severity, message, span, suggestion, data and origin. A
    diagnostic that moved or changed in any part is removed and added;
    identical diagnostics count one by one; the same diagnostics in
    another order are no change.
  """
  verify unit "diagnostics from changed files are refreshed"
  verify unit "diagnostics from unchanged files are preserved"
  verify unit "total diagnostic set matches full rebuild"
  verify unit "a diagnostic is unchanged in a delta only when every presented part is"
  verify unit "an update reports the diagnostics it added and removed"
  verify unit "a diagnostic without a span is unchanged when only its entities move"
  verify performance "file change to diagnostics emitted within 100ms"
  verify contract "Emit Incremental Diagnostics: incremental diagnostics holds for the declared obligations"
}

behavior debounce_file_changes "Debounce File Changes" {
  features   [incremental_compilation]
  invariants [incremental_correctness, diagnostic_determinism, watch_mode_response_latency]
  category   command
  types      [FileEntry, CompilerConfig]
  consumes   [file_changed]
  produces   [file_changes_coalesced]
  requires {
    file_changed_fired "At least one file_changed event has been received from the file watcher"
  }
  ensures {
    coalesced_batch_produced          "file_changes_coalesced event fires with the union of all changed files within the debounce window"
    redundant_recompilation_prevented "Multiple rapid changes to the same file result in a single recompilation"
  }
  contract   """
    When multiple file_changed events arrive in rapid succession (e.g.,
    save-all or editor reformatting), the system MUST coalesce them into a
    single invalidation batch. A debounce window of 50ms MUST be applied,
    by one rule watch and the LSP share: the system MUST wait until no new
    changes arrive within the window before emitting a
    file_changes_coalesced event. The coalesced batch MUST include the
    union of all changed files within the debounce window, under every
    directory watch watches: one burst is one batch however many watched
    directories it touches.
  """
  verify unit "rapid successive changes coalesced into single batch"
  verify unit "debounce window prevents redundant recompilation"
  verify unit "coalesced batch includes union of all changed files"
  verify unit "changes under different watched directories within the window join one batch"
  verify unit "single isolated change triggers after debounce window"
  verify unit "each change restarts the quiet window"
  verify contract "Debounce File Changes: file change debouncing holds — file_changed_fired, coalesced_batch_produced, redundant_recompilation_prevented"
}

behavior resolve_imports_on_update "Resolve Imports on Every Update" {
  features   [incremental_compilation]
  // Runs synchronously before rebuild_affected_subgraph — every file's
  // imports are resolved again before any subgraph rebuild begins.
  category   command
  invariants [import_dag, incremental_correctness]
  types      [Graph, FileEntry]
  consumes   [subgraph_invalidated]
  produces   [import_dag_updated]
  requires {
    subgraph_invalidated_fired "subgraph_invalidated event has fired, identifying the set of files to re-parse"
  }
  ensures {
    import_dag_updated_emitted "import_dag_updated event fires after every cached file's use imports are resolved again"
    cycle_detection_rerun      "Import cycle detection (W113) has been re-run across the full import graph"
  }
  contract   """
    After every update, the system MUST resolve the use imports of every
    file again, over the cached parses and the keys of the sources the
    session holds (no file is re-read, re-parsed or looked up for it), so
    the import diagnostics (E025, I004, W113, W027) are the ones a full
    rebuild reports. A source that could not be read stays E025 until it
    is readable or gone. The import graph is rebuilt rather than patched:
    an added or removed import, a source an import names created or
    deleted, and a cycle closed or broken anywhere are all seen on the
    update that causes them; a file that is no source (excluded, under a
    skipped directory) changes nothing when it is created or deleted.
    References resolve across the project without use (ADR 0004 D1-a),
    so the import graph decides no re-parse.
  """
  verify unit "an added use import is resolved on the next update"
  verify unit "a removed use import no longer reports"
  verify unit "cycle detection re-runs after an update"
  verify unit "import diagnostics after an update match a full rebuild"
  verify unit "an unreadable source stays E025 after an update of another file"
  verify contract "Resolve Imports on Every Update: import resolution after each update holds — subgraph_invalidated_fired, import_dag_updated_emitted, cycle_detection_rerun"
}

// ── Incremental Graph Delta ───────────────────────────────────

behavior compute_graph_delta "Compute Graph Delta" {
  features   [incremental_graph_deltas]
  invariants [
    incremental_correctness,
    graph_traversal_integrity,
    diagnostic_determinism,
    graph_delta_determinism,
  ]
  category   query
  types      [Graph, GraphDelta, NodeChange, ModifiedNodeChange, EdgeChange]
  consumes   [incremental_rebuild_complete]
  produces   [graph_delta_computed]
  requires {
    previous_graph_available "Previous graph snapshot is available for comparison"
    new_graph_available      "Newly compiled graph is available for comparison"
  }
  ensures {
    complete_diff      "GraphDelta is a complete symmetric diff of the two graphs"
    deterministic_sort "All arrays in GraphDelta are sorted by EntityId.raw (lexicographic)"
  }
  maintains {
    delta_equivalence "Applying the delta to the previous graph produces a state identical to the new graph"
  }
  contract   """
    After an incremental rebuild completes, the system MUST diff the
    previous graph state against the new graph state to produce a
    GraphDelta. The delta MUST enumerate all added nodes, removed nodes,
    modified nodes (with what changed: the fields whose values differ, by
    name, and apart from them which of its kind, title, methods, obligations
    and outgoing edges differ, so a field named kind or methods is never read
    as the aspect), added edges, removed edges, and the list of affected
    files. Source positions are ignored: an entity that only moved is not
    modified. The update lists it apart, as a moved node: its id and kind
    and where it is declared now (file and start line, what every graph
    document writes); a change of column or content alone moves nothing.
    added_nodes, removed_nodes, and modified_nodes MUST be
    sorted by EntityId.raw to guarantee deterministic output. The delta
    MUST be computed before any subscribers are notified. One delta
    serves every reader: watch prints it, MCP notifies it, and the
    post-migration check (validate_post_migration_integrity) keeps a
    migration only when the delta from the graph before it to the graph
    after is empty.
  """
  verify unit "added nodes appear in delta"
  verify unit "removed nodes appear in delta"
  verify unit "modified nodes list changed fields"
  verify unit "added and removed edges appear in delta"
  verify unit "affected files listed in delta"
  verify unit "shifted expression positions are not a modification"
  verify unit "a field named like an aspect is a changed field, never the aspect"
  verify unit "an entity that only moved is a moved node, not a modification"
  verify unit "a change of column or content alone moves no node"
  verify unit "a verify statement edit is the obligations aspect, not a changed field"
  verify unit "an incremental build compares each node's obligations"
  verify contract "Compute Graph Delta: graph delta computation holds — previous_graph_available"
}

// Not built (ADR 0006): every check runs over the whole graph after each
// update. The obligations stay unproven until extensions receive deltas.
behavior dispatch_incremental_validators "Dispatch Incremental Validators" {
  features   [incremental_graph_deltas]
  invariants [incremental_correctness, diagnostic_determinism, zero_domain_knowledge_core]
  category   command
  types      [GraphDelta, Graph, EntityKindDescriptor]
  ports      [WasmRuntime]
  consumes   [graph_delta_computed]
  produces   [incremental_validators_dispatched]
  requires {
    delta_computed "graph_delta_computed event has fired and GraphDelta is available"
  }
  ensures {
    all_validators_invoked "All extension validators invoked with appropriate input (delta or full graph)"
    event_produced         "incremental_validators_dispatched event produced on completion"
  }
  maintains {
    topological_order "Dispatch follows topological extension order regardless of delta content"
  }
  contract   """
    After a graph delta is computed, the system MUST dispatch validation
    to extensions. Extensions that declare incremental=true in their
    manifest MUST receive only the GraphDelta. Extensions without
    incremental support MUST receive the full graph for re-validation.
    The incremental: false flag is per-kind, not per-extension. When a
    delta contains entities of a kind marked incremental: false, the
    dispatcher MUST invoke the owning extension with the full graph for
    those entities, even if other kinds from the same extension support
    incremental validation. Dispatch MUST follow the topological
    extension order.
  """
  verify unit "incremental extension receives delta only"
  verify unit "non-incremental extension receives full graph"
  verify unit "dispatch follows topological order"
  verify unit "kind with incremental=false triggers full graph validation for that kind"
  verify unit "mixed incremental and non-incremental kinds dispatch separately"
  verify contract "Dispatch Incremental Validators: incremental dispatch holds — all_validators_invoked"
}

behavior notify_delta_subscribers "Notify Delta Subscribers" {
  features   [incremental_graph_deltas]
  invariants [incremental_correctness, diagnostic_determinism, graph_traversal_integrity]
  category   command
  types      [GraphDelta, DiagnosticsDelta]
  // MCP notification is handled by notify_graph_delta_via_mcp in behaviors/mcp-server.spec.
  // The LSP is not a delta subscriber: provide_semantic_tokens in behaviors/lsp.spec
  // owns its semantic token refresh.
  consumes   [graph_delta_computed]
  produces   [delta_subscribers_notified]
  requires {
    graph_delta_computed_fired "graph_delta_computed event has fired, providing the GraphDelta for notification"
  }
  ensures {
    affected_files_delivered           "Each update reports its GraphDelta and the files it affects"
    delta_subscribers_notified_emitted "delta_subscribers_notified event fires once the update is reported"
  }
  contract   """
    After every update, the project session MUST report with the update
    the GraphDelta and the files it affects, and what it reported before
    and reports after, from which it computes, when asked, the
    DiagnosticsDelta: the diagnostics the update added and removed
    (emit_incremental_diagnostics). Whoever holds the session delivers
    them: specforge watch prints both (whole under --format json: each
    rebuilt or rechecked line's delta and each rebuilt, rechecked or
    extensions_reloaded line's diagnostics_delta; counts in text), and
    MCP notifies its subscribed clients (notify_graph_delta_via_mcp,
    notify_diagnostics_delta_via_mcp). A DiagnosticsDelta nobody asks for
    is never computed. The session never waits on a subscriber, so a slow
    one cannot delay the compilation pipeline. The LSP does not use these
    deltas: it publishes every file's diagnostics as the editor shows
    them, a spanless one placed at its entity's name
    (emit_live_diagnostics), which a delta of the diagnostics as JSON
    presents them does not describe, and asks its client to refresh
    semantic tokens (provide_semantic_tokens, behaviors/lsp.spec).
  """
  verify unit "an update reports its delta and the files it affects"
  verify unit "watch --format json prints each rebuild's GraphDelta, the one MCP notifies"
  verify unit "watch --format json prints the diagnostics each update added and removed"
  verify contract "Notify Delta Subscribers: delta reporting holds — graph_delta_computed_fired, affected_files_delivered, delta_subscribers_notified_emitted"
}

behavior validate_delta_correctness "Validate Delta Correctness" {
  features   [incremental_graph_deltas]
  invariants [incremental_correctness, graph_delta_determinism]
  category   validation
  types      [Graph, GraphDelta]
  consumes   [graph_delta_computed]
  produces   [delta_validation_failed, delta_validation_passed]
  requires {
    graph_delta_available "graph_delta_computed event has fired and GraphDelta is available for verification"
    debug_mode_active     "Debug build configuration or --verify-incremental CLI flag is active"
  }
  ensures {
    delta_verified           "Delta applied to previous graph produces state identical to new graph, or the discrepancy is reported"
    validation_event_emitted "delta_validation_passed or delta_validation_failed event is produced"
  }
  contract   """
    In debug mode, after computing a graph delta, the system MUST verify
    correctness: the delta MUST equal a full comparison of the previous
    and new graphs, and applying it to the previous graph's node IDs and
    edges MUST yield the new graph's. Any discrepancy MUST be reported
    with the rebuild (specforge watch reports the verification as
    failed) with a descriptive message identifying the inconsistent nodes
    or edges: a node ID or edge mismatch between the applied delta and
    the new graph, or a modified node missing from either graph.

    Debug mode is activated by the compiler's debug build configuration,
    in every project session (watch, the LSP and MCP), or by watch's
    --verify-incremental CLI flag. A divergence is reported with the
    rebuild: watch prints it, the LSP logs it, and a debug build of the
    LSP or MCP stops on it. The CLI flag enables delta validation
    in release builds for CI use. This check MUST be disabled in release
    builds (without --verify-incremental) to avoid performance overhead.
  """
  verify unit "delta applied to old graph equals new graph"
  verify unit "a discrepancy is reported with a message naming what differs"
  verify unit "check disabled in release builds"
  verify integration "a debug build checks each rebuild without the flag"
  verify unit "the LSP and MCP check each rebuild in a debug build"
  verify unit "a rebuild that passes the check is reported as passed"
  verify contract "Validate Delta Correctness: delta correctness validation holds — graph_delta_available, debug_mode_active, delta_verified, validation_event_emitted"
}
