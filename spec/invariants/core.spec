// Core compiler invariants — guarantees that must always hold

invariant spec_root_singleton "Spec Root Singleton" {
  guarantee """
    Exactly one project root MUST exist per project: the directory
    holding its specforge.json, located from any path inside the
    project by locate_project_root. The specforge.json file declares
    project identity (name, version), installed extensions, and
    provider configurations; no other file marks or configures a
    project.
  """
  risk      medium
  verify property "a project with exactly one specforge.json is accepted"
}

invariant init_config_validity "Init Config Validity" {
  guarantee """
    Any specforge.json produced by specforge init or modified by
    specforge add MUST be syntactically valid JSON and semantically
    valid per the SpecForgeJsonConfig schema. The generated config
    MUST be parseable by the compiler pipeline without errors. This
    invariant ensures the seconds-to-value principle: init followed
    by check MUST never fail due to malformed configuration.
  """
  risk      high
  verify property "specforge init output is always valid SpecForgeJsonConfig JSON"
  verify unit "specforge init followed by specforge check produces zero config errors"
  verify integration "specforge add or the MCP add_extension tool followed by specforge check produces zero errors"
}

invariant multi_error_collection "Multi-Error Collection" {
  guarantee """
    The compiler MUST collect and report all diagnostics found during a
    compilation pass. It MUST NOT halt on the first error. Every error,
    warning, and info diagnostic MUST be emitted to the user.
  """
  // detect_dangling_references is a post-resolution integrity check (detects
  // resolver bugs), not a user-facing diagnostic emitter — E003 is emitted
  // by link_entity_references during resolution.
  risk      high
  verify property "a file with N errors produces exactly N error diagnostics in one pass"
  verify unit "the compiler does not halt after the first error"
}

invariant string_interning_consistency "String Interning Consistency" {
  guarantee """
    All interned strings MUST resolve to the same symbol for the same input
    within a compilation session. Two equal strings MUST produce the same
    interned key. Comparison by interned key MUST be equivalent to
    comparison by string value.
  """
  risk      high
  verify property "interning the same string twice returns the same key"
  verify unit "comparison by interned key is equivalent to comparison by string value"
}

invariant import_dag "Import DAG" {
  guarantee """
    The use import graph SHOULD form a directed acyclic graph (DAG).
    References resolve across the whole project without use, so a cycle
    changes nothing: the compiler MUST detect circular imports and report
    each with a W113 warning that names the cycle participants, in the
    same order on every run. Files in a cycle are still compiled.
  """
  risk      medium
  verify property "an acyclic import graph is accepted without diagnostics"
  verify unit "a circular import produces W113 naming the cycle participants in a deterministic order"
}

invariant entity_id_uniqueness "Entity ID Uniqueness" {
  guarantee """
    Every entity ID raw string MUST be globally unique across all .spec files
    in a project, regardless of entity kind. Two entities with different kinds
    but the same raw ID are forbidden. The compiler MUST reject duplicate IDs
    with an E002 diagnostic that names both declaration sites.
  """
  risk      high
  verify property "all unique entity IDs across files are accepted"
  verify unit "a duplicate entity ID produces E002 naming both declaration sites"
}

invariant graph_traversal_integrity "Graph Traversal Integrity" {
  guarantee """
    Graph traversal operations (trace, subgraph extraction, delta
    computation) MUST produce complete and deterministic results. Every
    reachable node along a traversal path MUST be included. Traversal
    order MUST be deterministic for identical graph inputs. A traversal
    reads only the edges of the entities it reaches, never every edge of
    the graph for each of them.
  """
  risk      high
  verify property "traversal from any node visits every reachable node exactly once"
  verify unit "identical graph inputs produce identical traversal results"
  verify unit "a directed reach reads only the edges of the entities it reaches"
}

invariant incremental_correctness "Incremental Correctness" {
  guarantee """
    After an incremental recompilation triggered by a file change, the
    in-memory graph MUST be identical to the graph produced by a full
    cold rebuild of the same source files. No stale nodes or edges
    MUST remain from the previous compilation. What a compile reports is
    a function of its environment and its sources (their paths and
    texts): no file it does not hold changes it, so a file discovery
    does not find (excluded, under a skipped directory, outside the spec
    root) is no input. After every update that runs the checks, a
    project session MUST report exactly what a fresh compile of the same
    sources reports, in the same order; an editor update that skips the
    checks while a file does not parse reports no check diagnostic until
    the next update that runs them.
  """
  risk      high
  verify property "incremental recompilation produces the same graph as a full rebuild"
  verify property "a session reports what a fresh compile of the same sources reports, in the same order, after every update that runs the checks"
  verify property "a file parsed with its previous text and tree reads as the same file parsed cold"
  verify unit "no stale nodes or edges remain after incremental recompilation"
  verify unit "a file discovery does not find changes nothing a compile or a session reports"
}

invariant graph_delta_determinism "Graph Delta Determinism" {
  guarantee """
    Given identical previous and current graph states, compute_graph_delta
    MUST produce an identical GraphDelta. All arrays in GraphDelta
    (added_nodes, removed_nodes, modified_nodes, added_edges, removed_edges)
    MUST be sorted by EntityId.raw (lexicographic). The ordering MUST NOT
    depend on hash-map iteration order, filesystem order, or any
    non-deterministic source.
  """
  risk      medium
  verify property "identical graph pairs produce identical GraphDelta across 100 runs"
  verify unit "GraphDelta arrays are sorted by EntityId.raw"
}

invariant graph_schema_completeness "Graph Schema Completeness" {
  guarantee """
    The schema section of any Graph Protocol export MUST include every
    entity kind and edge type registered in the KindRegistry and edge type
    set at the time of export. No registered kind or edge type MUST be
    omitted. The schema MUST accurately reflect the testable, singleton,
    and field definitions for each kind.
  """
  risk      medium
  verify property "schema contains every registered kind and edge type"
  verify unit "newly registered extension kind appears in schema"
}

invariant schema_version_backward_compatibility "Schema Version Backward Compatibility" {
  guarantee """
    The previous major version of the Graph Protocol schema MUST remain
    readable by the current compiler. Breaking changes to the schema MUST
    only occur on major version increments. The compiler MUST be capable of
    loading and interpreting Graph Protocol JSON produced by any version
    within the same major version range.
  """
  risk      high
  verify property "Graph Protocol JSON from previous major version is readable"
  verify unit "breaking change on minor version increment is rejected"
}

invariant watch_mode_response_latency "Watch Mode Response Latency" {
  guarantee """
    File-change-to-diagnostics in watch mode MUST complete within 100ms
    for single-file changes.
  """
  risk      medium
  verify performance "single-file change produces diagnostics within 100ms"
}

invariant token_budget_subgraph_consistency "Token Budget Subgraph Consistency" {
  guarantee """
    When enforce_token_budget truncates entities from export output, the
    remaining subgraph MUST NOT contain dangling edges referencing truncated
    nodes. Truncated entity IDs MUST be listed in the truncated_entities
    response field.
  """
  risk      medium
  verify property "truncated subgraph contains no dangling edges"
  verify unit "truncated_entities field lists all omitted entity IDs"
}

invariant query_file_grammar_consistency "Query File Grammar Consistency" {
  guarantee """
    All .scm query files (highlights.scm, folds.scm, indents.scm) MUST
    remain valid and consistent with the tree-sitter grammar after any
    grammar change. Node names, capture names, and pattern structures in
    query files MUST reference grammar rules that exist in the current
    grammar version. A grammar change that adds, removes, or renames a
    rule MUST trigger review of all .scm files for broken references.
  """
  risk      medium
  verify integration "highlights.scm loads without error against current grammar"
  verify integration "folds.scm loads without error against current grammar"
  verify integration "indents.scm loads without error against current grammar"
}

invariant cst_vocabulary_grammar_consistency "CST Vocabulary Grammar Consistency" {
  guarantee """
    The node kinds and field names the parser and the formatter read from
    tree-sitter's concrete syntax tree MUST be named once, by the grammar
    crate (tree_sitter_specforge::kind and tree_sitter_specforge::field),
    and MUST be exactly the named node kinds and field names of the
    current grammar: a grammar change that adds, removes or renames a rule
    or a field fails a test until the vocabulary follows it (ADR 0038).
  """
  risk      medium
  verify unit "every named node kind and field name of the grammar has one constant, and every constant names one"
}

invariant dry_run_side_effect_freedom "Dry-Run Side-Effect Freedom" {
  guarantee """
    Any command invoked with --dry-run or --check MUST NOT modify any files
    on disk. These flags guarantee read-only execution. If a code path
    reachable from a dry-run context writes to the filesystem, it is a P0
    bug. This applies to specforge migrate --dry-run, specforge add
    --dry-run, specforge remove --dry-run, specforge format --check,
    specforge format --diff, and any future commands that support dry-run mode.
    Over MCP a preview flag is read by its type: dry_run, check or diff sent as
    the string "true" is a preview.
  """
  risk      high
  verify unit "--dry-run produces output without modifying files"
  verify unit "--check produces output without modifying files"
  verify unit "an MCP dry run, check or diff asked for with the string true writes nothing"
  verify integration "a dry run of an extension already present is told from the add, on the command line"
  verify integration "every preview's document says dry_run, and every run's says it did not preview"
  verify property "no file write operations occur during dry-run execution"
}

invariant document_project_paths "Document Project Paths" {
  guarantee """
    A field of a surface document that names a file or directory an
    operation opened by path or wrote MUST be relative to the root of the
    project the call acted on, with / between components; a path outside
    that root (only the CLI can name one) is absolute and canonical. A
    document's anchor directory (init's project_root, watch's spec_root) is
    absolute. A location the graph declares (a span, a navigation result,
    an export node, rename's plan) is named by its source key, relative to
    the spec root. The CLI and MCP print the same bytes for one run,
    whatever the working directory or the spelling of the root.
  """
  risk      medium
  verify integration "the CLI and MCP documents of one run are byte-equal, whatever the working directory or root spelling"
}

invariant source_span_completeness "Source Span Completeness" {
  guarantee """
    Every AST node produced by the parser MUST carry a valid SourceSpan
    with start and end positions that accurately reflect the original
    source text. No AST node MUST have a zero-length span unless it
    represents a synthetic node inserted by error recovery.
  """
  risk      high
  verify property "all AST nodes have non-zero source spans"
  verify unit "source spans survive error recovery"
}

invariant typed_edit_canonical_form "Typed Edit Canonical Form" {
  guarantee """
    A value a typed edit writes MUST be in the form its field's declaration
    decides, so a typed edit never produces W159 or W020, and never writes a
    field that restates its file's default (W161). Every block a typed edit
    touches MUST be format-clean afterwards, laid out with the project's
    format configuration, its multi-line values included, and no byte outside
    the touched blocks changes.
  """
  risk      high
  verify property "every block a typed edit writes is unchanged by format"
  verify unit "a spelled value reads back and is left as it is by format and W159"
}

invariant typed_edit_atomicity "Typed Edit Atomicity" {
  guarantee """
    A typed edit MUST NOT be written when the edited project reports an error
    the project did not report before (E080). Its files are written as one
    project change: every file or none, and a file changed since the project
    was compiled refuses the whole plan.
  """
  risk      high
  verify property "a refused or failed typed edit leaves every file as it was"
}

// Embedding invariants moved to spec/extensions/embeddings/invariants.spec
