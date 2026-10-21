// Inference workflow behaviors — AI agent spec inference from codebases
//
// 21 behaviors:
//   - Manifest I/O (3): load, save, compute summary
//   - Session management (3): start, end, mark analyzed
//   - Discovery and staleness (2): discover sources, detect stale entries
//   - MCP tools (3): infer_progress, infer_session, infer_gaps
//   - Guides (1): compute inference guide
//   - MCP prompts (5): the scope read; kind, plan, workflow and file scopes
//   - CLI (2): infer-status, infer-guide
//   - Diagnostics (2): I200, I202

use "types/infer"

defaults behavior {
  features [infer_progress_tracking]
}

// ---------------------------------------------------------------------------
// Manifest I/O
// ---------------------------------------------------------------------------

behavior load_inference_manifest "Load Inference Manifest" {
  types    [InferenceManifest]
  category io
  ensures {
    file_read          "specforge-infer.json is read from project root"
    default_on_missing "returns empty manifest when file does not exist"
    version_checked    "rejects manifests with unsupported version numbers"
    summary_computed   "InferenceSummary is derived from source_index on load, never read from file"
    sessions_read      "sessions are read with the manifest, each with its status"
    never_empty        "a file that cannot be used is E071, never read as an empty manifest"
  }
  contract """
    Load the inference manifest from {project_root}/specforge-infer.json,
    once per operation, through the one reader every operation, the MCP
    tools, the infer prompt and the inferred lint profile share. If the
    file does not exist, return a default empty manifest with version=1,
    empty source_roots, source_index and sessions. If the file exists and
    cannot be used — it cannot be read, is not JSON, does not have the
    manifest's shape (a session's status included), or has an unsupported
    version — refuse with E071 naming why; never read it as empty. Keys the
    manifest does not define are kept. The InferenceSummary MUST be
    computed from source_index on load — it is never persisted in JSON.
  """
  verify unit "load returns default manifest when file is missing"
  verify unit "load deserializes valid specforge-infer.json"
  verify unit "load rejects unsupported version"
  verify unit "load computes summary from source_index"
  verify unit "a session the manifest cannot read refuses the load, and nothing is written"
  verify unit "load refuses an unreadable or invalid manifest with E071"
}

behavior save_inference_manifest "Save Inference Manifest" {
  types    [InferenceManifest]
  category io
  ensures {
    atomic_write        "writes via temp file then rename to prevent corruption"
    summary_excluded    "InferenceSummary is not written to JSON"
    json_formatted      "output is pretty-printed JSON with sorted keys"
    source_index_sorted "source_index serialized as sorted Vec for stable diffs"
    unknown_kept        "keys the manifest does not define are written back as read"
  }
  contract """
    Save the inference manifest to {project_root}/specforge-infer.json.
    It is the one writer of the file: the sessions are written with the
    rest of the manifest. The source_index MUST be sorted by path
    for stable git diffs. The InferenceSummary MUST NOT be written.
    Write to a temporary file in the same directory, then atomically
    rename to the target path. The temporary file is created new: whatever
    is at its name is removed first, a link and never its target, so
    nothing is written through it. A manifest that is itself a link
    leaving the project is unusable (E071) and is not read.
    The JSON MUST be pretty-printed.
  """
  verify unit "save writes valid JSON that can be loaded back"
  verify unit "save does not include summary in JSON output"
  verify unit "save uses atomic write (temp file + rename)"
  verify unit "save serializes source_index sorted by path"
  verify unit "save keeps keys the manifest does not define, at every level"
}

behavior compute_inference_summary "Compute Inference Summary" {
  types    [InferenceSummary, SourceFileEntry]
  category computation
  ensures {
    files_counted    "files_total reflects source file discovery across all source_roots"
    analyzed_counted "files_analyzed counts entries in source_index"
    entities_summed  "entities_produced sums all source_index entity lists"
  }
  contract """
    Compute the InferenceSummary by aggregating the source_index entries.
    files_total is the count of inference sources
    (discover_inference_sources) across all source_roots. files_analyzed is the count of entries in source_index.
    entities_produced is the sum of all entities_produced arrays across
    source_index entries. needs_review counts entities that appear in
    source_index but have no corresponding entity in the compiled graph.
  """
  verify unit "summary counts match source_index contents"
  verify unit "summary scans all source_roots for files_total"
  verify unit "needs_review detects entities missing from graph"
}

// ---------------------------------------------------------------------------
// Session Management
// ---------------------------------------------------------------------------

behavior start_inference_session "Start Inference Session" {
  types    [InferenceSession, SessionStatus]
  category lifecycle
  ensures {
    session_created "new InferenceSession appended with status=active"
    uuid_assigned   "session_id is a unique UUID"
    single_active   "only one active session allowed at a time"
  }
  contract """
    Create a new InferenceSession with a generated UUID, status=active,
    started_at=now(), and the provided agent identifier. If another session
    already has status=active, return an error — only one active session
    is allowed. Append the session to the manifest's sessions array and
    save the manifest.
  """
  verify unit "start creates session with active status"
  verify unit "start rejects when another session is active"
  verify unit "start assigns unique session ID"
}

behavior end_inference_session "End Inference Session" {
  types    [InferenceSession, SessionStatus]
  category lifecycle
  ensures {
    status_updated "session status set to completed or paused"
    timestamp_set  "ended_at is set to current time"
  }
  contract """
    Find the session by session_id. Set its status to the requested value
    (completed or paused) and ended_at to the current timestamp. Save the
    manifest. If session_id does not exist or is not active, return an error.
  """
  verify unit "end sets status to completed"
  verify unit "end sets ended_at timestamp"
  verify unit "end rejects unknown session_id"
}

behavior mark_source_file_analyzed "Mark Source File as Analyzed" {
  types    [SourceFileEntry]
  category tracking
  ensures {
    entry_created     "SourceFileEntry added to source_index"
    hash_computed     "content_hash is SHA-256 of file contents"
    entities_recorded "entities_produced lists all entity IDs inferred from this file"
    idempotent        "re-analyzing a file updates the existing entry"
    path_recorded     "the file is recorded root-relative with / separators"
    inside_root       "a file outside the project root is refused and nothing is written"
  }
  contract """
    Read the source file at the given path and compute its SHA-256 hash.
    Create or update a SourceFileEntry in the manifest's source_index with
    the path, hash, entities_produced list, and current timestamp. If an
    entry for this path already exists, overwrite it (re-analysis). Save
    the manifest. The path is recorded root-relative with / separators
    (./src/a.rs and src\a.rs are src/a.rs); an absolute path or one with a
    .. component is refused. The same rule applies to the source_roots a
    start sets. Agents should call this AFTER specforge.validate succeeds.
    On validation failure: fix .spec errors, re-validate, then mark.
  """
  verify unit "mark creates new entry for unanalyzed file"
  verify unit "mark updates existing entry on re-analysis"
  verify unit "mark computes SHA-256 content hash"
  verify unit "mark records the path root-relative with / separators"
  verify unit "mark refuses a file outside the project root"
}

behavior discover_inference_sources "Discover Inference Sources" {
  types    [InferenceManifest, SourceFileEntry]
  category computation
  ensures {
    claimed      "a source is a file an enabled analyzer claims by its extension, under a source root"
    shared_walk  "no symlinked directory is entered, and a symlink is a source only when it resolves to a file inside the project root"
    skips_united "the default skipped directories hold whatever the analyzers declare; their excluded_dirs add to them"
    inside_root  "a source root, a file or an indexed path that lands outside the project root is never read"
    one_key      "every source is keyed by the source path rule and listed once"
    nested       "a nested project's files are its own project's sources"
  }
  contract """
    The inference sources are the files under the inference manifest's
    source_roots (the project root when there are none) that an enabled
    analyzer claims by its file extension (the longest extension that
    matches; for one extension, the first analyzer that declares it). The
    walk is the shared discovery policy: no symlinked directory is
    entered; a symlink is a source when it resolves to a file inside the
    project root; no directory is entered whose name starts with a dot,
    that the defaults skip (target, node_modules, dist, build, .git, .hg,
    __pycache__, vendor), that an enabled analyzer's excluded_dirs names,
    or that holds specforge.json below a source root. Each source is keyed
    by the source path rule and listed once, whatever source roots reach
    it. A source root, a file or an indexed path that lands outside the
    project root, by .., an absolute name or a symlink, is not read: such
    a root is not walked and such an indexed path is deleted.
  """
  verify unit "no symlinked directory is entered, so a symlink cycle adds no source"
  verify unit "an analyzer's excluded_dirs add to the default skipped directories"
  verify unit "a file two source roots reach is one source"
  verify unit "no inference source lands outside the project root"
  verify unit "an indexed path outside the project root is deleted, never read"
  verify unit "a nested project's files are not the outer project's inference sources"
}

// ---------------------------------------------------------------------------
// Staleness Detection
// ---------------------------------------------------------------------------

behavior detect_stale_inference_entries "Detect Stale Inference Entries" {
  types    [SourceFileEntry]
  category validation
  ensures {
    hash_compared   "current file hash compared against stored hash"
    stale_flagged   "files with changed content are returned as stale"
    deleted_flagged "files that no longer exist are returned as deleted"
  }
  contract """
    For each entry in source_index, re-read the source file and compute
    its current SHA-256 hash. If the hash differs from content_hash, the
    entry is stale. If the file no longer exists, the entry is deleted.
    Return both lists so the agent or user can decide to re-analyze.
    An indexed path that is not a path inside the project root is deleted;
    its file is never read.
  """
  verify unit "detects stale file when content changes"
  verify unit "detects deleted file"
  verify unit "returns empty lists when nothing changed"
}

// ---------------------------------------------------------------------------
// MCP Tools
// ---------------------------------------------------------------------------

behavior provide_mcp_infer_progress_tool "Provide MCP Infer Progress Tool" {
  types    [InferenceManifest, InferenceSummary, McpInferProgressResult]
  category mcp
  ensures {
    summary_returned  "returns computed InferenceSummary"
    unanalyzed_listed "returns source files not in source_index"
    stale_listed      "returns source files with changed content hash"
    graceful_missing  "returns empty results when specforge-infer.json absent"
  }
  contract """
    Register a read-only MCP tool specforge.infer_progress that loads the
    inference manifest, discovers source files from source_roots, computes
    summary, identifies unanalyzed files (in source_roots but not in
    source_index), and detects stale entries. Returns:
    { summary: InferenceSummary, unanalyzed: [paths], stale: [paths] }.
    If specforge-infer.json does not exist, returns empty summary with all
    source files as unanalyzed. With no project served and none named, the
    tool is the no-project refusal (precondition_failed), as the progress
    operation refuses a view without a root.
  """
  verify unit "returns summary with unanalyzed files"
  verify unit "detects stale files by content hash"
  verify unit "graceful handling when specforge-infer.json is missing"
  verify unit "with no project served, infer_progress is the no-project refusal"
}

behavior provide_mcp_infer_session_tool "Provide MCP Infer Session Tool" {
  types    [InferenceManifest, InferenceSession, SourceFileEntry, McpInferSessionResult]
  category mcp
  ensures {
    start_creates_session "start action creates session and returns ID"
    mark_records_file     "mark_analyzed action records file entry"
    end_completes_session "end action completes session"
    creates_manifest      "creates specforge-infer.json on first write"
  }
  contract """
    Register a mutating MCP tool specforge.infer_session over the one
    session operation (specforge_ops::read::infer::session), with three actions:
    - start: Call start_inference_session, return session_id.
    - mark_analyzed: Call mark_source_file_analyzed with provided
      source_file path and entities_produced list.
    - end: Call end_inference_session with session_id and status.
    All actions read and save the manifest through the call's project,
    atomically. On first write, creates
    specforge-infer.json if it does not exist.
  """
  verify unit "start action creates session and returns ID"
  verify unit "mark_analyzed action records file entry"
  verify unit "end action completes session"
  verify unit "creates manifest on first write"
  verify unit "an unknown action or status is refused with the option table's wording"
}

behavior provide_mcp_infer_gaps_tool "Provide MCP Infer Gaps Tool" {
  features [infer_gap_analysis]
  types    [InferenceGapReport, DirectoryGaps, InferenceGap, McpInferGapsResult]
  category mcp
  ensures {
    source_scanned   "source files scanned for public items"
    graph_crossed    "public items cross-referenced with the anchors manifest and graph entities"
    anchored_only    "an anchored entity covers only the items its anchors name"
    per_dir_gaps     "gaps computed per directory"
    approximate_flag "output marked approximate when using regex fallback"
  }
  contract """
    Register an MCP tool specforge.infer_gaps that scans source files in
    source_roots for public items. v1 uses regex-based detection for Rust
    (pub fn, pub struct, pub enum, pub trait) — output is explicitly marked
    as approximate. An item is covered when the project's
    specforge-anchors.json anchors it, by its file and its name, to an entity
    the compiled graph declares, or, when no anchor names that entity, when its
    name is the entity's id. An anchor to an entity the graph does not declare
    covers nothing. An anchors manifest that cannot be used is refused with
    E071, as specforge.find_spec_for_source refuses it, before any file is
    scanned. Returns an
    InferenceGapReport with per-directory breakdowns including structured
    InferenceGap items (name, item_kind, file, line). A scanner that fails
    on a file (it traps, or answers what is not a scan response) is
    reported in scan_failures (the file and its E028), never counted as a
    file without public items, and makes the report approximate.
    Items in test files, build scripts, and standard trait impls are excluded.
    With no project served and none named, the tool is the no-project
    refusal (precondition_failed), as the gap analysis refuses a view
    without a root.
  """
  verify unit "scans Rust files for pub items via regex"
  verify unit "excludes test files and build scripts"
  verify unit "returns structured InferenceGap items with file and line"
  verify unit "marks output as approximate"
  verify unit "with no project served, infer_gaps is the no-project refusal"
  verify unit "an item an anchor names for a declared entity is covered"
  verify unit "an anchored entity covers only the items its anchors name"
  verify unit "an unusable anchors manifest refuses the gaps with E071"
  verify unit "every item find_spec_for_source anchors to a declared entity is no gap"
  verify unit "an item whose name is an unanchored entity's id is covered"
  verify unit "an anchor to an entity the graph does not declare covers nothing"
}

// ---------------------------------------------------------------------------
// MCP Prompt Enhancements
// ---------------------------------------------------------------------------

behavior compute_inference_guide "Compute Inference Guide" {
  features   [infer_plan_mode]
  invariants [zero_domain_knowledge_core]
  types      [McpInferPromptResult, McpInferOverview, McpInferGuideKind]
  category   query
  ensures {
    guides_merged     "a kind's guide is its extension's, then the project's own for it"
    fields_listed     "every field registered on the kind, by name, with its type"
    example_typed     "the example writes each field the way its type is written"
    project_directory "the spec directory is the project's spec root"
  }
  contract   """
    The inference guide (specforge_ops::read::infer::guide, kind_guide) MUST give,
    for every kind a loaded extension declares (once, as the extension the
    kind registry registered it for declares it), in declaration order:
    its keyword, extension and description; every field the registry
    build registered on it, sorted by name, with its type, whether it is
    required, its description and, for an enum, its values and the value an
    absent one counts as; its guide, the extension's
    inference_guide followed by a blank line, "**Project-specific:**" and
    the project's own guide for the kind (inference.<kind> in
    specforge.json), either alone when the other is absent, empty when
    neither is; an example entity writing its required fields and its
    first three optional ones, each the way its type is written ("..." for
    a string, 0, true, an enum's declared default else its first value, ["item1", "item2"], ref_id,
    [ref_1, ref_2], { } for a block); and its entities, in id order. The
    project's guide adds the loaded extensions, the entity count of every
    written kind, the global conventions (inference.global) and the spec
    directory, the spec root relative to the root. The infer prompt's
    overview, kind and file scopes, specforge infer-guide and the LSP's
    keyword completion render it.
  """
  verify unit "a kind's guide is its extension's guide, then the project's guide for the kind under Project-specific"
  verify unit "a kind only the project guides has the project's guide alone, and one nobody guides an empty guide"
  verify unit "a kind guide lists every field registered on the kind, by name, with its type"
  verify unit "a guide field lists an enum's values and what an absent value counts as"
  verify unit "the example entity writes its required fields and three optional ones, each the way its type is written"
  verify unit "a kind declared by two extensions is guided by the one that registered it"
  verify unit "the guide's spec directory is the project's spec root, relative to its root"
  verify unit "the guide lists each loaded extension, every declared kind in declaration order, and the entities of every written kind"
  verify integration "every builtin kind's example parses and holds no value of the wrong type"
}

behavior read_infer_prompt_scope "Read Infer Prompt Scope" {
  features [infer_plan_mode]
  types    [McpInferPromptResult, McpInferOverview]
  category mcp
  ensures {
    one_scope_one_guide "each scope the prompt knows names one guide; any other is refused on scope"
  }
  contract """
    specforge://prompts/infer reads its scope argument as one of five: none,
    an empty scope or overview (every kind's guide, McpInferOverview),
    kind:<name>, file:<path>, plan and workflow. A kind: or file: with
    nothing after it, and any other value, are refused as invalid input on
    scope; the refusal of an unknown scope names the five. An unknown
    scope is never read as the overview (ADR 0101 D2).
  """
  verify unit "a scope the prompt does not know is refused on scope, naming the scopes it knows"
  verify unit "no scope, an empty scope or scope overview renders every kind's guide"
}

behavior provide_infer_kind_scope "Provide Infer Prompt Kind Scope" {
  features [infer_plan_mode]
  types    [McpInferKindGuide, McpInferGuideField]
  category mcp
  ensures {
    fields_registered "every field registered on the kind is listed: its own, its extension's shared fields and other extensions' enhancement fields"
    types_named       "each field's type is named as the extension protocol names it"
  }
  contract """
    When specforge://prompts/infer is invoked with scope=kind:<name>, render
    the kind's guide (compute_inference_guide): its keyword, extension,
    description, existing entity IDs, fields, guide and example, and the
    validation step naming the validate and analyze tools. A kind no loaded
    extension declares is refused on scope as unknown_kind naming the
    closest declared kind. The payload is tagged with its scope (kind).
  """
  verify unit "the kind scope renders the kind's guide; an undeclared kind is refused on scope with the closest declared kind"
}

behavior provide_infer_plan_scope "Provide Infer Prompt Plan Scope" {
  features [infer_plan_mode]
  types    [McpInferPlan, McpInferKindPriority]
  category mcp
  ensures {
    zero_first        "kinds with no existing entity come first"
    referenced_first  "within each group a kind comes after the kinds its reference fields target"
    analyzed_excluded "already-analyzed files are not listed as unanalyzed"
    directory_default "the target directory defaults to the project's spec root"
    paged             "file lists are pages of at most 50 from the cursor"
  }
  contract """
    When specforge://prompts/infer is invoked with scope=plan, render the
    inference plan (specforge_ops::read::infer::inference_plan): the target
    spec directory (target_spec_directory, else the project's spec root
    relative to its root), the inference progress, the unanalyzed and
    stale source files in pages of at most 50 from cursor, each page
    files only, with each list's total and, unless the page is the last,
    the cursor of the next page; and the kind priorities. The payload is
    tagged scope plan. Kinds with no existing entity come
    first, then the others; within each group a kind comes after the kinds
    its reference fields target, else in declaration order, a cycle broken
    in declaration order. The core names no kind. Already-analyzed files
    are not unanalyzed. If specforge-infer.json exists and cannot be used,
    the plan is refused with E071.
  """
  verify unit "plan lists kinds with no entities first, then each kind after the kinds it references"
  verify unit "plan excludes already-analyzed files"
  verify unit "plan's target directory defaults to the project's spec root"
  verify unit "plan pages the file lists 50 at a time from the cursor"
  verify unit "the last page of a plan has no next cursor, and a page lists only files"
  verify unit "plan refuses a specforge-infer.json it cannot use with E071"
}

behavior provide_infer_workflow_scope "Provide Infer Prompt Workflow Scope" {
  types    [InferenceManifest, McpInferWorkflow]
  category mcp
  ensures {
    protocol_taught   "agent receives step-by-step inference protocol"
    tool_names_listed "the tools the protocol names are listed, as tools/list names them"
    retry_documented  "retry pattern for validation failures documented"
  }
  contract """
    When specforge://prompts/infer is invoked with scope=workflow, return
    the step-by-step agent protocol, naming each tool as tools/list names
    it:
    1. Call specforge.infer_session with action=start
    2. Call specforge.infer_progress to get unanalyzed files
    3. Read source files, write .spec files
    4. Call specforge.validate to check errors, fix any errors
    5. Call specforge.infer_session with action=mark_analyzed
    6. Repeat steps 2-5 until satisfied
    7. Call specforge.infer_session with action=end
    Include retry pattern: if validate fails, fix .spec -> re-validate
    -> then mark. Never mark before validation passes. The payload lists
    the tools the protocol names and the declared kinds. It is tagged with
    its scope (workflow).
  """
  verify unit "workflow returns step-by-step protocol"
  verify unit "the workflow lists the tools it names, as the tool table names them"
  verify unit "workflow documents retry pattern"
}

behavior provide_infer_file_scope "Provide Infer Prompt File Scope" {
  features [infer_plan_mode]
  category mcp
  types    [McpSpecForSourceResult, McpInferFileGuide, McpInferFileKind]
  ensures {
    existing_listed "entities that belong to the file are listed so the agent does not duplicate them"
    one_answer      "the same entities specforge.find_spec_for_source finds for that file"
  }
  contract """
    When specforge://prompts/infer is invoked with scope=file:<path>,
    list the entities that belong to that source file under the one
    file rule MCP navigation and the LSP share, with each one's kind,
    line and symbol, and each kind's guide
    (compute_inference_guide). The list is the one
    specforge.find_spec_for_source returns for the same path. With no
    project served the file scope names a file of no project: it is the
    no-project refusal (precondition_failed), as every core prompt read
    that names a file is. The payload is tagged with its scope (file).
  """
  verify unit "file scope lists the entities find_spec_for_source finds for the same file"
  verify unit "an unanchored file lists no existing entities"
  verify unit "with no project served, the file scope is the no-project refusal"
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

behavior provide_infer_status_cli "Provide CLI Infer-Status Command" {
  types    [InferenceManifest, InferenceSummary]
  category cli
  ensures {
    summary_displayed "prints summary table: files analyzed / total, entities produced"
    session_history   "prints session history with timestamps"
    json_format       "supports --format json for machine output"
    gaps_flag         "supports --gaps to list unanalyzed source files grouped by directory"
    stale_flag        "supports --stale to list files with changed content"
    graceful_missing  "prints helpful message when specforge-infer.json is absent"
  }
  contract """
    Register a CLI subcommand 'infer-status' that reads specforge-infer.json
    and displays inference progress. Default output is a human-readable
    summary table. --format json produces machine-readable output: the
    document specforge.infer_progress answers with, plus
    unanalyzed_by_directory with --gaps and gap_analysis (the
    specforge.infer_gaps report) with --gaps-detail. --gaps lists unanalyzed
    source files grouped by directory with counts per directory. --stale
    lists files whose content changed since last analysis. If
    specforge-infer.json does not exist, print a message directing the user
    to the infer prompt. The JSON document and specforge.infer_progress
    list the sessions (session_id, agent, status, started_at, ended_at).
  """
  verify unit "displays summary table"
  verify unit "--format json produces valid JSON"
  verify unit "--format json includes the gaps --gaps and --gaps-detail ask for"
  verify unit "--gaps lists unanalyzed files grouped by directory"
  verify unit "--stale lists files with changed content"
  verify unit "missing manifest shows helpful message"
  verify unit "an unusable manifest is refused with E071"
  verify unit "prints the sessions the manifest records, with their timestamps"
}

behavior provide_infer_guide_cli "Provide CLI Infer-Guide Command" {
  features [infer_plan_mode]
  category cli
  ensures {
    overview_printed "without a kind, every declared kind's guide is printed"
    kind_printed     "with a kind, the kind's guide, fields, existing entities and example are printed"
    json_format      "--format json prints the infer prompt's guide data"
  }
  contract """
    Register a CLI subcommand 'infer-guide [KIND]' that renders the
    inference guide (compute_inference_guide) of the project compiled at
    --path: without KIND every declared kind's guide, the conventions and
    the spec directory; with KIND that kind's guide. --format json prints
    the infer prompt's overview or kind-scope data, without the prompt's
    output_format and validation text. An undeclared KIND is unknown_kind
    naming the closest declared kind, exit 1.
  """
  verify integration "specforge infer-guide --format json is the infer prompt's guide data"
  verify integration "an undeclared kind is unknown_kind naming the closest declared kind, exit 1"
}

// ---------------------------------------------------------------------------
// Inference Quality Diagnostics
// ---------------------------------------------------------------------------

behavior detect_stale_source_anchor "I200: Stale Source Anchor" {
  features [infer_quality_diagnostics]
  types    [SourceFileEntry]
  category diagnostics
  ensures {
    hash_checked       "source file content hash compared with manifest entry"
    diagnostic_emitted "I200 emitted when hash differs"
  }
  contract """
    When --lint=inferred is enabled and specforge-infer.json exists, for
    each entity ID that appears in the inference manifest's source_index,
    check if the source file's current SHA-256 hash matches the stored
    content_hash. If it differs, emit I200 on the entity with message
    'source file {path} changed since entity was inferred — consider
    re-inferring'. Only fires when the manifest is present and the lint
    profile is active. If the manifest exists and cannot be used, the
    profile reports E071 instead, an error.
  """
  verify unit "I200 fires when source file content changed"
  verify unit "I200 silent when hash matches"
  verify unit "I200 silent when --lint=inferred not set"
  verify unit "the inferred profile reports a manifest it cannot use as E071"
}

behavior detect_high_inference_density "I202: High Inference Density" {
  features [infer_quality_diagnostics]
  types    [InferenceManifest]
  category diagnostics
  ensures {
    density_computed  "percentage of inferred entities per spec file computed"
    threshold_checked "I202 emitted when density exceeds configurable threshold (default 80%)"
    configurable      "threshold overridable via inference.density_threshold in specforge.json"
  }
  contract """
    When --lint=inferred is enabled and specforge-infer.json exists, for
    each .spec file, count how many of its entities appear in the inference
    manifest. If more than the configured threshold (default 80%, overridable
    via inference.density_threshold in specforge.json) of entities in the
    file were inferred in a single session, emit I202 with message 'spec
    file {path} has high inference density — consider human review'. If
    the manifest exists and cannot be used, the profile reports E071
    instead, an error.
  """
  verify unit "I202 fires when density exceeds threshold"
  verify unit "I202 silent when density is below threshold"
  verify unit "I202 silent when --lint=inferred not set"
  verify unit "I202 threshold is configurable via specforge.json"
}
