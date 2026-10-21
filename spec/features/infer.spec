// Inference workflow features — AI agent spec inference from codebases

use "behaviors/infer"
use "types/infer"

feature infer_progress_tracking "Inference Progress Tracking" {
  status   in_progress
  problem  """
    When an AI agent infers specs from a codebase, there is no way to track
    which source files have been analyzed, which entities were produced, or
    what remains. Without progress tracking, agents re-analyze already-covered
    files, miss uncovered areas, and cannot resume across sessions.
  """
  solution """
    A specforge-infer.json manifest at the project root tracks analyzed source
    files (with content hashes for staleness detection), produced entity IDs,
    and inference sessions. Two MCP tools expose this: specforge.infer_progress
    (read-only) and specforge.infer_session (mutating). The CLI specforge
    infer-status command lets humans view progress. Resume is free: agents
    read the manifest on startup and skip already-analyzed files. The manifest
    supports multiple source_roots for workspace projects.
  """
  priority high
}

feature infer_plan_mode "Inference Plan Mode" {
  status   in_progress
  problem  """
    Agents starting inference on a new codebase don't know the optimal order
    for entity kinds. Inferring events before types leads to broken references.
    Inferring behaviors before types means type entities don't exist yet for
    linking. Without guidance, agents waste time fixing ordering issues.
  """
  solution """
    The specforge://prompts/infer prompt gains two new scopes: scope=plan
    returns a prioritized work plan with kinds ordered by dependency (types
    before behaviors, behaviors before events), each phase including a
    target_spec_directory hint; scope=workflow returns the step-by-step
    agent protocol with tool names and retry patterns.
  """
  priority high
}

feature infer_gap_analysis "Inference Gap Analysis" {
  status   in_progress
  problem  """
    After partial inference, there is no way to measure how much of the
    codebase has been covered by specs. Developers and agents cannot identify
    which modules or directories still need attention, leading to blind spots
    in specification coverage.
  """
  solution """
    The specforge.infer_gaps MCP tool scans source files for public items
    (using regex-based detection for Rust in v1, explicitly marked approximate),
    cross-references them with the anchors manifest and graph entities, and returns per-directory gap
    reports with structured InferenceGap items including name, item_kind, file,
    and line. The CLI surfaces this via specforge infer-status --gaps-detail.
    Real language-specific detection deferred to extension surface commands.
  """
  priority medium
}

feature infer_quality_diagnostics "Inference Quality Diagnostics" {
  status   in_progress
  problem  """
    AI-inferred specs may become stale when source code changes, or may have
    been generated in large batches without human review. Without provenance-
    aware diagnostics, there is no way to identify which inferred specs need
    attention.
  """
  solution """
    Two new info-level diagnostics under the --lint=inferred profile:
    I200 (source anchor stale — source file changed since inference),
    I202 (high inference density — batch-generated file needs review,
    threshold configurable via inference.density_threshold). These fire only
    when specforge-infer.json exists and the lint profile is active.
  """
  priority low
}
