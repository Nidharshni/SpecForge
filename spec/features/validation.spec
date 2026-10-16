// Validation features — core structural validation
// Domain-specific validation (unreferenced entity checks, unused entity warnings, etc.)
// is extension-driven via the declarative validation engine. These features cover
// only core structural validation behaviors.

use "behaviors/error-reporting"
use "behaviors/graph"
use "behaviors/output"
use "behaviors/resolution"
use "behaviors/validation"
use "behaviors/zero-entity-registries"
use "behaviors/zero-entity-validation"

feature reference_resolution "Reference Resolution" {
  status   done
  problem  """
    .spec files reference entities across multiple files via use imports
    and reference lists. Broken imports, circular dependencies, and
    unresolvable cross-extension references must be detected.
  """
  solution """
    The resolver processes use imports, links entity references, detects
    import cycles (W113), and resolves cross-extension references with soft
    resolution (I004 for uninstalled extensions). External ref declarations
    are resolved via provider schemes.
  """
}

feature graph_construction "Graph Construction" {
  status   done
  problem  """
    After resolution, entities and their resolved references must be
    assembled into an in-memory typed entity graph for validation,
    export, and querying.
  """
  solution """
    The graph builder constructs an in-memory directed graph from
    resolved ASTs. Nodes represent entities with kind and fields. Edges
    represent resolved references with registered edge type labels.
  """
}

feature structural_validation "Structural Validation" {
  status   done
  problem  """
    The compiled graph may contain structural inconsistencies: dangling
    references, duplicate IDs, unreferenced structural nodes, and missing
    file references.
  """
  solution """
    Core structural validation checks the graph for: dangling
    reference integrity (the linker's own debug assertion), duplicate
    IDs (E002), unreferenced refs with zero incoming edges (W012), and
    file-reference existence (E016). Domain-specific
    validation is driven by declarative patterns from extensions.
    Note: detect_duplicate_entity_ids runs at all_files_parsed time
    (before graph construction). The remaining structural validators
    run after graph_built. Ordering enforced by validation_pipeline_ordering
    invariant (invariants/validation.spec).
  """
}

feature diagnostic_reporting "Diagnostic Reporting" {
  status   done
  // JSON output path for agent consumption (P3)
  // Note: print_diagnostics_structured and export_diagnostics_as_json also
  // appear in features/output.spec::ci_integration. This is intentional:
  // ci_integration describes the CI/CD output perspective, while this feature
  // describes the validation-diagnostic perspective.
  problem  """
    Validation diagnostics must be formatted with source context, fuzzy
    suggestions for typos, and aggregated into a final summary for
    human and machine consumption.
  """
  solution """
    Diagnostics are formatted with source snippets and underline markers
    pointing to the exact source location. Fuzzy matching suggests
    corrections for misspelled entity IDs. The aggregate summary collects
    all diagnostics with error/warning/info counts. All diagnostics are
    available in both human-readable (ariadne-rendered) and machine-parseable
    (structured JSON via --format=json) formats — agents consume the JSON
    format directly without parsing rendered output (per P3: agents are
    first-class consumers). Two of the six behaviors
    (format_diagnostics_with_source_context and provide_did_you_mean_suggestions)
    run inline during the resolution pass rather than as post-validation
    event consumers.
  """
}

// Note: The four behaviors below (detect_unknown_entity_kinds,
// detect_unknown_entity_fields, two_phase_validate_semantic,
// suggest_missing_extensions) are also
// listed in features/zero-entity-core.spec under zero_entity_bootstrap
// and dynamic_entity_registration. This is intentional: those features
// describe the architectural mechanism (registry-driven validation),
// while this feature describes the validation-pipeline perspective
// (what the user experiences during specforge check). A behavior MAY
// appear in multiple features when it serves multiple user-facing
// capabilities.
feature zero_entity_validation "Zero-Entity Validation" {
  status   done
  problem  """
    Without extension-aware validation, entity keywords and field names
    from uninstalled or misconfigured extensions go undetected, producing
    silent graph corruption or misleading diagnostics.
  """
  solution """
    Group the four zero-entity validation behaviors under a dedicated
    feature: detect_unknown_entity_kinds checks keywords against the
    KindRegistry (E024), detect_unknown_entity_fields checks fields
    against the FieldRegistry (W020), two_phase_validate_semantic
    orchestrates the full Phase 2 semantic validation pass, and
    suggest_missing_extensions attaches help text to E024 diagnostics
    using the candidate vocabulary the registry
    build is handed (the builtins', generated with them, ADR 0122) that maps
    known keywords to their providing extensions. These behaviors execute in Phase 2 after
    registries are populated (see dynamic_entity_registration in
    features/zero-entity-core.spec).

    Bridge: detect_unknown_entity_kinds and detect_unknown_entity_fields
    reuse the same validation infrastructure as the zero-entity-core
    behaviors (behaviors/zero-entity-validation.spec). The KindRegistry
    and FieldRegistry are populated by extension registration behaviors
    (behaviors/zero-entity-registries.spec) and consumed by both
    validation paths.
  """
}
