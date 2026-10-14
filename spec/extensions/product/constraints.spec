// @specforge/product extension constraints — non-functional requirements
//
// Performance, reliability, and usability constraints specific to
// the product entity model validation and query capabilities.

use "extensions/product/behaviors-operations"
use "extensions/product/behaviors-queries"
use "extensions/product/behaviors-registration"
use "extensions/product/behaviors-v1-1"
use "extensions/product/invariants"
use "extensions/product/surfaces-cli"
use "extensions/product/surfaces-shared"

constraint product_validation_latency "Product Validation Latency" {
  description "All product validation rules must complete within 50ms for projects with up to 500 entities across all 9 kinds."
  category    performance
  priority    critical
  metric      """
    Product-specific validation rules (E007, E015, E052, W041-W046, W049,
    W057, W092, W093, I010, I046-I048, I050,
    I053-I055, I057, I059-I062, I066-I070, I080-I083, I086, I087, I089)
    MUST complete in under
    50ms for a project with up to 500 product entities across all 9 kinds.
  """
  constrains  [
    detect_unreferenced_features,
    detect_module_cycles,
    detect_milestone_cycles,
    detect_feature_dependency_cycles,
    detect_unreferenced_journeys,
    detect_unreferenced_modules,
    detect_unreferenced_terms,
    detect_unreferenced_personas,
    detect_unreferenced_channels,
    detect_empty_milestones,
    detect_deliverables_with_no_journeys,
    detect_deliverables_with_no_modules,
    detect_features_with_no_acceptance,
    detect_deliverable_journey_module_gap,
    detect_milestone_feature_module_gap,
    detect_singleton_tags,
    detect_journeys_without_persona,
    detect_journeys_without_channels,
    detect_term_see_also_non_term_refs,
    detect_completed_milestone_without_criteria,
    detect_blocked_milestone_without_dependency,
    detect_journey_deprecated_persona,
    detect_journey_deprecated_channels,
    validate_persona_references,
    validate_channel_references,
    validate_product_vocabularies,
    validate_journey_flow_non_empty,
    validate_milestone_target_date_format,
    pe_validate_persona_fields,
    pe_validate_channel_fields,
    pe_validate_deliverable_completeness,
    pe_validate_milestone_status,
    detect_deliverable_cycles,
    detect_shipped_deliverable_incomplete_milestones,
    detect_deprecated_deliverable_without_reason,
    detect_deferred_feature_without_reason,
    detect_blocked_milestone_without_reason,
    validate_deliverable_version_format,
    validate_module_family_field,
    detect_done_feature_with_incomplete_deps,
    detect_milestone_temporal_inconsistency,
    detect_modules_with_no_features,
    validate_tag_format,
    detect_deprecated_persona_without_reason,
    detect_deprecated_channel_without_reason,
    detect_tag_namespace_collision,
    detect_term_alias_conflicts,
    detect_flow_step_structure,
    detect_transitive_deprecated_persona,
    detect_transitive_deprecated_channel,
    validate_feature_status_transition,
    validate_milestone_status_transition,
    validate_deliverable_status_transition,
    validate_persona_status_transition,
    validate_channel_status_transition,
    detect_deliverable_chain_gap,
    detect_feature_multi_milestone,
    detect_priority_escalation_gap,
    detect_milestone_implicit_ordering,
  ]
  protects    [
    module_dag,
    milestone_dag,
    feature_dag,
    deliverable_dag,
    module_feature_reachability,
    persona_lifecycle_consistency,
    channel_lifecycle_consistency,
    tag_cross_kind_awareness,
    term_alias_uniqueness,
    pe_deliverable_chain_integrity,
  ]
  verify load "benchmark product validation with 500 entities, assert < 50ms"
}

constraint product_cycle_detection_correctness "Product Cycle Detection Correctness" {
  description "Cycle detection for modules, milestones, features, and deliverables must have zero false negatives and zero false positives."
  category    reliability
  priority    critical
  metric      """
    Cycle detection for modules (E007), milestones (E015), and features
    (W045) MUST have zero false negatives — every cycle in the graph MUST
    be detected. Zero false positives — acyclic graphs MUST never produce
    cycle diagnostics. Verified by property-based testing with randomly
    generated dependency graphs including transitive cycles.
  """
  constrains  [
    detect_module_cycles,
    detect_milestone_cycles,
    detect_feature_dependency_cycles,
    detect_deliverable_cycles,
  ]
  protects    [module_dag, milestone_dag, feature_dag, deliverable_dag]
  verify property "cycle detection has zero false negatives on random graphs"
  verify property "cycle detection has zero false positives on acyclic random graphs"
}

constraint product_unreferenced_detection_correctness "Product Unreferenced Detection Correctness" {
  description "Unreferenced detection must fire if and only if an entity has no incoming edge (I010: no edge at all)."
  category    reliability
  priority    critical
  metric      """
    Unreferenced detection for features (W041), journeys (W042), modules (W044),
    personas (I046) and channels (I047) MUST fire if and only if the entity
    has no incoming edge of any type. Unreferenced-term detection (I010)
    MUST fire if and only if the term has no incoming and no outgoing edge.
    Zero false positives when entities have valid references. Zero false
    negatives when entities are truly unreferenced.
  """
  constrains  [
    detect_unreferenced_features,
    detect_unreferenced_journeys,
    detect_unreferenced_modules,
    detect_unreferenced_terms,
    detect_unreferenced_personas,
    detect_unreferenced_channels,
  ]
  protects    [pe_feature_non_testable, pe_persona_non_testable, pe_channel_non_testable]
  verify unit "each unreferenced rule fires iff the entity has no incoming edge, and I010 iff the term has no edge"
}

constraint product_query_correctness "Product Query Correctness" {
  description "All product graph queries must return correct results for all graph topologies, including transitive traversals and reverse queries."
  category    reliability
  priority    critical
  metric      """
    All product graph queries (milestone completion, deliverable traceability,
    journey coverage, feature ordering, milestone timeline, feature
    deliverables, term graph, deliverable completion, persona channels,
    feature dependents, deliverable dependents, deliverable priority, and
    the analytics queries)
    MUST return correct results for all graph topologies. Milestone completion
    ratio MUST be mathematically correct (done_count / total). Deliverable
    traceability MUST include all transitive features via both journey and
    module paths. Dependents queries MUST return the exact inverse of the
    corresponding depends_on references.
  """
  constrains  [
    pe_query_milestone_completion,
    pe_query_deliverable_traceability,
    pe_query_journey_coverage,
    pe_query_feature_ordering,
    pe_query_milestone_timeline,
    pe_query_feature_deliverables,
    pe_query_term_graph,
    pe_query_deliverable_completion,
    pe_query_persona_channels,
    pe_query_feature_dependents,
    pe_query_deliverable_dependents,
    pe_query_deliverable_priority,
    pe_query_persona_features,
    pe_query_feature_impact,
    pe_query_milestone_velocity,
    pe_query_deliverable_personas,
    pe_query_unscheduled_features,
    pe_query_feature_overlap,
    pe_query_persona_coverage_matrix,
    pe_query_critical_path,
  ]
  protects    [deliverable_journey_module_consistency, pe_cross_extension_query_isolation]
  verify unit "milestone completion ratio is mathematically correct"
  verify unit "deliverable traceability includes all transitive features"
  verify unit "feature ordering produces valid topological sort"
  verify unit "reverse queries return exact inverse of forward queries"
  verify unit "term graph respects maxHops boundary"
}

constraint product_standalone_operation "Product Standalone Operation" {
  description "All product queries must operate purely on product entity kinds and edge types without requiring peer extensions."
  category    reliability
  priority    critical
  metric      """
    All product queries operate purely on product entity kinds and edge
    types. No query requires peer extensions. Feature coverage uses the
    FeatureStatus field. No crashes, no false diagnostics, no panics.
  """
  constrains  [pe_query_milestone_completion, pe_query_journey_coverage]
  protects    [pe_feature_non_testable]
  verify unit "all product queries operate without peer extensions"
  verify unit "no panics when product extension is installed alone"
}

constraint product_entity_registration_determinism "Product Entity Registration Determinism" {
  description "Registration of entity kinds, edge types, and field definitions must be deterministic regardless of registration order."
  category    reliability
  priority    critical
  metric      """
    Registration of 9 entity kinds, 20 edge types, and all field
    definitions MUST be deterministic — same manifest input always
    produces same KindRegistry and FieldRegistry state. Registration
    order MUST NOT affect validation outcomes.
  """
  constrains  [
    pe_register_entity_kinds,
    pe_register_edge_types,
    pe_register_field_definitions,
    pe_register_validation_rules,
  ]
  protects    [pe_manifest_nine_entity_kinds, pe_manifest_twenty_edge_types]
  verify property "registration from same manifest produces identical registry state"
}

constraint product_diagnostic_severity_correctness "Product Diagnostic Severity Correctness" {
  description "All product diagnostics must fire at their declared severity level matching their E/W/I code prefix."
  category    reliability
  priority    critical
  metric      """
    All product diagnostics MUST fire at their declared severity level:
    E-codes (E007, E015, E052) are errors, W-codes (W041-W046, W049, W057,
    W092, W093) are warnings, I-codes (I010,
    I046-I048, I050, I053-I055, I057, I059-I062, I066-I070, I080-I083,
    I086, I087, I089) are info. No
    diagnostic may fire at a different severity than its code prefix declares.
  """
  constrains  [pe_register_validation_rules]
  verify unit "E-code diagnostics fire at error level"
  verify unit "W-code diagnostics fire at warning level"
  verify unit "I-code diagnostics fire at info level"
}

constraint product_tag_detection_correctness "Product Tag Detection Correctness" {
  description "Singleton tag detection and tag format validation must fire correctly without false positives, completing under 10ms for 1000 tags."
  category    reliability
  priority    high
  metric      """
    Singleton tag detection (I052) MUST fire for tags on exactly one entity
    and suppress for tags on 2+ entities. Levenshtein suggestions MUST NOT
    produce false positives for intentionally short distinct tags. Tag format
    validation (I068) MUST fire for tags not matching [a-z0-9][a-z0-9-]*[a-z0-9].
    Combined tag detection MUST complete in under 10ms for up to 1000 tags
    across 500 entities.
  """
  constrains  [detect_singleton_tags, validate_tag_format]
  protects    [pe_tags_per_entity_kind]
  verify unit "singleton tag fires I052, multi-entity tag suppresses"
  verify unit "tag format validation fires I068 for non-conforming tags"
  verify load "tag detection completes in under 10ms for 1000 tags"
}

constraint product_large_scale_validation "Product Large-Scale Validation" {
  description "Validation rules must complete under 500ms for 5000 entities and under 2s beyond that, with O(V+E) cycle and unreferenced detection."
  category    performance
  priority    high
  metric      """
    Product-specific validation rules SHOULD complete in under 500ms for
    a project with up to 5000 product entities across all 9 kinds. For
    projects exceeding 5000 entities, validation SHOULD complete in under
    2 seconds. Cycle detection algorithms MUST remain O(V+E) regardless
    of graph size. Unreferenced detection MUST remain O(V+E) by leveraging
    pre-computed incoming-edge indexes.
  """
  constrains  [
    detect_module_cycles,
    detect_milestone_cycles,
    detect_feature_dependency_cycles,
    detect_deliverable_cycles,
    detect_unreferenced_features,
    detect_unreferenced_journeys,
    detect_unreferenced_modules,
    detect_unreferenced_personas,
    detect_unreferenced_channels,
    detect_unreferenced_terms,
    detect_singleton_tags,
  ]
  protects    [module_dag, milestone_dag, feature_dag, deliverable_dag]
  verify load "benchmark product validation with 5000 entities, assert < 500ms"
  verify load "benchmark cycle detection with 50000-edge graph, assert O(V+E)"
}

constraint product_large_scale_query "Product Large-Scale Query Latency" {
  description "All product graph queries must complete under 1s for 5000 entities, with feature ordering under 2s."
  category    performance
  priority    high
  metric      """
    All product graph queries SHOULD complete in under 1 second for a
    project with up to 5000 product entities across all 9 kinds.
    Feature ordering (global topological sort) SHOULD complete in under
    2 seconds for 5000 features. Queries on projects exceeding 5000
    entities SHOULD return partial results or a timeout error rather
    than blocking indefinitely.
  """
  constrains  [
    pe_query_milestone_completion,
    pe_query_deliverable_traceability,
    pe_query_journey_coverage,
    pe_query_feature_ordering,
    pe_query_milestone_timeline,
    pe_query_feature_deliverables,
    pe_query_term_graph,
    pe_query_deliverable_completion,
    pe_query_persona_channels,
    pe_query_feature_dependents,
    pe_query_deliverable_dependents,
    pe_query_deliverable_priority,
    pe_query_persona_features,
    pe_query_feature_impact,
    pe_query_milestone_velocity,
    pe_query_deliverable_personas,
  ]
  verify load "benchmark all product queries with 5000 entities, assert < 1s each"
  verify load "benchmark feature ordering with 5000 features, assert < 2s"
}

constraint product_deliverable_lifecycle_correctness "Product Deliverable Lifecycle Correctness" {
  description "Deliverable status, shipped-incomplete-milestone, and deprecated-without-reason validation must fire precisely per their diagnostic rules."
  category    reliability
  priority    critical
  metric      """
    Deliverable status validation MUST refuse (E061) exactly the status
    values outside DeliverableStatus. Shipped
    deliverable incomplete milestone detection (I065) MUST fire for each
    non-completed milestone in a shipped deliverable. Deprecated deliverable
    reason detection (I066) MUST fire for deprecated deliverables with
    empty or missing reason fields.
  """
  constrains  [
    validate_product_vocabularies,
    detect_shipped_deliverable_incomplete_milestones,
    detect_deprecated_deliverable_without_reason,
  ]
  protects    [deliverable_lifecycle_consistency]
  verify unit "E061 fires iff a deliverable status is not a DeliverableStatus value"
  verify unit "I065 fires per incomplete milestone in shipped deliverable"
  verify unit "I066 fires for deprecated deliverable without reason"
}

constraint product_surface_schema_completeness "Product Surface Schema Completeness" {
  description "Every product CLI command must have a typed input and a typed json payload, with no untyped JSON allowed."
  category    reliability
  priority    critical
  metric      """
    Every one of the 40 product CLI commands MUST have typed args
    (ProductListFilter for list commands, the entity id positional for
    entity-scoped queries) and a typed json payload: the per-kind list
    result type for list commands, the query payload type types.spec
    names for each query, including bulk_status and health. MCP serves
    the same payload as the auto-promoted tool's result; product declares
    no MCP resources (ADR 0011). No command may return untyped JSON.
  """
  constrains  [
    surface_list_features,
    surface_list_journeys,
    surface_list_deliverables,
    surface_list_milestones,
    surface_list_modules,
    surface_list_terms,
    surface_list_personas,
    surface_list_channels,
    surface_milestone_completion,
    surface_journey_coverage,
    surface_feature_ordering,
    surface_milestone_timeline,
    surface_list_releases,
    surface_deliverable_traceability,
    surface_feature_deliverables,
    surface_term_graph,
    surface_deliverable_completion,
    surface_persona_channels,
    surface_feature_dependents,
    surface_deliverable_dependents,
    surface_deliverable_priority,
    surface_persona_features,
    surface_feature_impact,
    surface_milestone_velocity,
    surface_deliverable_personas,
    surface_feature_overlap,
    surface_channel_features,
    surface_unscheduled_features,
    surface_coverage_matrix,
    surface_critical_path,
    surface_release_completion,
    surface_owner_workload,
    surface_weighted_milestone_completion,
    surface_term_clusters,
    surface_term_density,
    surface_module_dependency_depth,
    surface_module_coupling,
    surface_channel_coverage_matrix,
  ]
  protects    [pe_surface_error_consistency]
  verify unit "every CLI command has typed args and a typed json payload"
  verify unit "no surface returns untyped JSON"
}

constraint product_surface_error_correctness "Product Surface Error Correctness" {
  description "All commands must use exactly two error codes and must never panic on any input including null, empty, or malformed JSON."
  category    reliability
  priority    critical
  metric      """
    All product commands MUST use exactly two error codes:
    ENTITY_NOT_FOUND (exit 1) and INVALID_INPUT (exit 2). The error is
    written to stderr (the ProductSurfaceError object under --format json)
    with nothing on stdout. Over MCP a non-zero exit is an isError tool
    result carrying the same object. No command may panic on any input
    including null, empty string, or malformed JSON.
  """
  constrains  [surface_error_handling]
  protects    [pe_surface_error_consistency]
  verify unit "CLI errors written to stderr: ENTITY_NOT_FOUND exits 1, INVALID_INPUT exits 2"
  verify unit "MCP tool errors are isError results carrying the error object"
  verify unit "no surface panics on malformed input"
}

constraint product_list_pagination_correctness "Product List Pagination Correctness" {
  description "All 9 list commands and the 5 matrix queries must return correct pagination with proper total counts, has_more flags, and input validation."
  category    reliability
  priority    critical
  metric      """
    All 9 list commands and the 5 matrix queries (coverage_matrix,
    channel_coverage_matrix, feature_overlap, owner_workload,
    module_coupling) MUST return correct pagination: total is the count
    after filtering and before paging, has_more is total > offset +
    returned count, limit defaults to 100, offset defaults to 0. A negative
    offset MUST produce INVALID_INPUT (exit 2). Limit is clamped to
    [1, 1000]. There are no cursors (ADR 0011).
  """
  constrains  [
    surface_list_features,
    surface_list_journeys,
    surface_list_deliverables,
    surface_list_milestones,
    surface_list_modules,
    surface_list_terms,
    surface_list_personas,
    surface_list_channels,
    surface_list_releases,
    surface_coverage_matrix,
    surface_channel_coverage_matrix,
    surface_feature_overlap,
    surface_owner_workload,
    surface_module_coupling,
  ]
  protects    [pe_list_pagination_correctness]
  verify unit "default limit is 100, default offset is 0"
  verify unit "negative offset produces INVALID_INPUT error"
  verify unit "limit below 1 is clamped to 1"
  verify unit "limit > 1000 is clamped to 1000"
  verify unit "total reflects filtered count before pagination"
  verify unit "has_more is correct for all edge cases"
}

constraint product_surface_latency "Product Surface Latency" {
  description "CLI commands must complete under 200ms for 500 entities, with surface overhead under 50ms."
  category    performance
  priority    high
  metric      """
    All 40 CLI commands SHOULD complete in under 200ms for a project with
    up to 500 product entities (includes graph query + JSON serialization
    + stdout write); their auto-promoted MCP tools run the same export.
    Surface overhead (serialization + dispatch) MUST
    NOT exceed 50ms beyond the underlying query latency.
  """
  constrains  [
    surface_list_features,
    surface_milestone_completion,
    surface_journey_coverage,
    surface_feature_ordering,
    surface_milestone_timeline,
  ]
  verify load "benchmark CLI commands with 500 entities, assert < 200ms each"
  verify load "benchmark surface overhead, assert < 50ms beyond query latency"
}

constraint product_query_latency "Product Query Latency" {
  description "All 12 core product graph queries must complete under 100ms for projects with up to 500 entities."
  category    performance
  priority    high
  metric      """
    All 12 core product graph queries (milestone-completion,
    deliverable-traceability, journey-coverage, feature-ordering,
    milestone-timeline, feature-deliverables, term-graph,
    deliverable-completion, persona-channels, feature-dependents,
    deliverable-dependents, deliverable-priority) SHOULD complete in under 100ms
    for a project with up to 500 product entities across all 9 kinds.
  """
  constrains  [
    pe_query_milestone_completion,
    pe_query_deliverable_traceability,
    pe_query_journey_coverage,
    pe_query_feature_ordering,
    pe_query_milestone_timeline,
    pe_query_feature_deliverables,
    pe_query_term_graph,
    pe_query_deliverable_completion,
    pe_query_persona_channels,
    pe_query_feature_dependents,
    pe_query_deliverable_dependents,
    pe_query_deliverable_priority,
  ]
  verify load "benchmark all 12 core product queries with 500 entities, assert < 100ms each"
}

constraint product_persona_channel_lifecycle_correctness "Product Persona/Channel Lifecycle Correctness" {
  description "Deprecated persona and channel reason detection must fire only when reason is empty or missing, and suppress otherwise."
  category    reliability
  priority    critical
  metric      """
    Deprecated persona reason detection (I069) MUST fire for deprecated
    personas with empty or missing reason fields. Deprecated channel reason
    detection (I070) MUST fire for deprecated channels with empty or missing
    reason fields. Both diagnostics MUST suppress when the reason field is
    present and non-empty. Both diagnostics MUST suppress for non-deprecated
    entities and entities without a status field.
  """
  constrains  [detect_deprecated_persona_without_reason, detect_deprecated_channel_without_reason]
  protects    [persona_lifecycle_consistency, channel_lifecycle_consistency]
  verify unit "I069 fires iff deprecated persona has empty/missing reason"
  verify unit "I070 fires iff deprecated channel has empty/missing reason"
}

constraint product_impact_query_correctness "Product Impact Query Correctness" {
  description "Feature impact and persona feature queries must return complete transitive closures with order-independent results."
  category    reliability
  priority    critical
  metric      """
    Feature impact analysis (pe_query_feature_impact) MUST return the
    complete transitive closure of all affected entities. Persona feature
    traversal (pe_query_persona_features) MUST return the exact union of
    features across all persona journeys. Both queries MUST produce
    identical results regardless of edge traversal order.
  """
  constrains  [pe_query_feature_impact, pe_query_persona_features]
  verify unit "feature impact returns complete transitive closure"
  verify unit "persona features returns exact union across journeys"
  verify property "traversal order does not affect results"
}

constraint product_term_alias_validation_correctness "Product Term Alias Validation Correctness" {
  description "Term alias conflict detection must fire for case-insensitive duplicate aliases with zero false positives or negatives."
  category    reliability
  priority    critical
  metric      """
    Term alias conflict detection (W086) MUST fire if and only if two
    terms share the same alias (case-insensitive) or a term's alias
    matches another term's entity ID (case-insensitive). Zero false
    positives for distinct aliases. Zero false negatives for conflicting
    aliases.
  """
  constrains  [detect_term_alias_conflicts]
  protects    [term_alias_uniqueness]
  verify unit "W086 fires for matching aliases"
  verify unit "W086 suppresses for distinct aliases"
  verify unit "case-insensitive comparison catches 'API' vs 'api'"
}

constraint product_deliverable_persona_correctness "Product Deliverable-Persona Query Correctness" {
  description "Deliverable persona traversal must return deduplicated personas via journey edges with deterministic results."
  category    reliability
  priority    critical
  metric      """
    Deliverable persona traversal (pe_query_deliverable_personas) MUST return
    the exact deduplicated set of personas reachable via deliverable->journey->
    persona edges. via_journey_ids MUST include all intermediate journeys. The
    result MUST be deterministic and identical regardless of edge traversal order.
  """
  constrains  [pe_query_deliverable_personas]
  verify unit "deliverable with journeys returns deduplicated personas"
  verify unit "via_journey_ids includes all intermediate journeys"
  verify property "traversal order does not affect result"
}

constraint product_pagination_sort_stability "Pagination Sort Stability" {
  description "Paginated list queries must produce stable, deterministic sort order with entity ID as tie-breaker."
  category    reliability
  priority    critical
  metric      """
    List queries with pagination MUST produce stable sort order across
    consecutive paginated requests when the graph has not been rebuilt.
    Specifically: concatenating pages 0..N MUST produce the same entity
    sequence as a single unpaginated query. Sort order MUST be deterministic
    — entities with identical sort keys MUST be tie-broken by entity ID
    (alphabetical ascending). Cursor-based pagination is not supported in v1;
    offset-based pagination is the only mechanism.
  """
  constrains  [pe_declare_surface_contributions]
  verify unit "page 0 + page 1 concatenation equals unpaginated result"
  verify unit "entities with same priority sorted alphabetically by ID"
  verify unit "sort order stable across repeated identical requests"
  verify property "union of all pages equals full result set with no duplicates"
}

constraint product_cross_extension_integration_correctness "Cross-Extension Integration Correctness" {
  description "Product queries must return identical results with or without peer extensions, enforced by the 20-edge-type allowlist."
  category    reliability
  priority    critical
  metric      """
    When @specforge/software is installed alongside @specforge/product:
    (1) product queries MUST return identical results to standalone operation
    (no foreign edges followed), (2) entity_enhancements from software MUST
    add fields to product entity kinds without affecting query behavior,
    (3) the BehaviorImplementsFeature edge MUST be traversable by software queries but
    invisible to product status results; delivery evidence alone reads it,
    in fields that are absent without recorded evidence (ADR 0039). The
    20-edge-type allowlist MUST be the sole enforcement mechanism for
    status results.
  """
  constrains  [pe_cross_extension_integration]
  verify integration "product queries identical with and without software extension"
  verify integration "entity enhancement adds fields without changing query results"
  verify unit "20-edge-type allowlist rejects BehaviorImplementsFeature edge"
  verify contract "product manifest peer_dependencies is empty"
}

constraint product_status_transition_correctness "Product Status Transition Correctness" {
  description "Status transition validation must catch all invalid transitions and suppress when no build cache exists."
  category    reliability
  priority    critical
  metric      """
    Status transition validation (W087-W091) MUST fire if and only if
    the current status cannot be reached from the prior status via a
    valid transition. Zero false negatives (every invalid transition is
    caught). Zero false positives when a build cache exists and
    transitions are valid. All transition checks MUST be suppressed
    when no build cache file (specforge-cache.json) exists. Terminal
    states MUST have zero outbound transitions.
  """
  constrains  [
    validate_feature_status_transition,
    validate_milestone_status_transition,
    validate_deliverable_status_transition,
    validate_persona_status_transition,
    validate_channel_status_transition,
  ]
  verify unit "valid transitions produce no warnings"
  verify unit "invalid transitions produce correct W-code"
  verify unit "terminal states reject all outbound transitions"
  verify unit "first build without prior state produces no warnings"
}

constraint product_new_query_correctness "Product New Query Correctness" {
  description "Unscheduled features, feature overlap, coverage matrix, and critical path queries must return correct results for all topologies."
  category    reliability
  priority    critical
  metric      """
    All 4 new query methods (queryUnscheduledFeatures, queryFeatureOverlap,
    queryPersonaCoverageMatrix, queryCriticalPath) MUST return correct
    results for all graph topologies. Unscheduled features MUST have zero
    MilestoneDeliversFeature edges. Feature overlap MUST be detected via both
    journey and module paths. Coverage matrix ratios MUST be mathematically
    correct. Critical path MUST be the longest incomplete milestone chain.
  """
  constrains  [
    pe_query_unscheduled_features,
    pe_query_feature_overlap,
    pe_query_persona_coverage_matrix,
    pe_query_critical_path,
  ]
  verify unit "unscheduled features have zero MilestoneDeliversFeature edges"
  verify unit "feature overlap detected via both journey and module paths"
  verify unit "coverage matrix ratios are mathematically correct"
  verify unit "critical path is the longest incomplete chain"
}

constraint product_new_query_latency "Product New Query Latency" {
  description "New query methods must complete under 100ms for 500 entities, with critical path under 200ms and coverage matrix under 500ms at 5000 entities."
  category    performance
  priority    high
  metric      """
    All 4 new query methods SHOULD complete in under 100ms for a project
    with up to 500 product entities. Critical path computation (topological
    sort + longest path) SHOULD complete in under 200ms for up to 5000
    milestones. Coverage matrix computation SHOULD complete in under 500ms
    for up to 5000 entities (it traverses persona→journey→feature for
    every persona).
  """
  constrains  [
    pe_query_unscheduled_features,
    pe_query_feature_overlap,
    pe_query_persona_coverage_matrix,
    pe_query_critical_path,
  ]
  verify load "benchmark new queries with 500 entities, assert < 100ms each"
  verify load "benchmark critical path with 5000 milestones, assert < 200ms"
  verify load "benchmark coverage matrix with 5000 entities, assert < 500ms"
}

constraint product_chain_validation_correctness "Product Chain Validation Correctness" {
  description "Chain gap, multi-milestone, priority escalation, and implicit ordering diagnostics must fire precisely per their trigger conditions."
  category    reliability
  priority    high
  metric      """
    End-to-end chain validation (I076) MUST fire for every uncovered
    feature in the deliverable→milestone→feature→module chain. Feature
    multi-milestone detection (I077) MUST fire for features in 2+
    milestones. Priority escalation (I078) MUST fire only when both
    feature and milestone have explicit priority and the feature's is
    higher. Implicit ordering (I079) MUST fire only for milestones
    with target_date fields.
  """
  constrains  [
    detect_deliverable_chain_gap,
    detect_feature_multi_milestone,
    detect_priority_escalation_gap,
    detect_milestone_implicit_ordering,
  ]
  protects    [pe_deliverable_chain_integrity]
  verify unit "I076 fires per uncovered feature"
  verify unit "I077 fires for features in 2+ milestones"
  verify unit "I078 requires explicit priority on both sides"
  verify unit "I079 requires target_date on both milestones"
}

constraint product_transitive_deprecated_correctness "Product Transitive Deprecated Reference Correctness" {
  description "Transitive deprecated persona and channel detection must fire on deliverables referencing journeys with deprecated entities."
  category    reliability
  priority    high
  metric      """
    Transitive deprecated persona detection (I073) and transitive deprecated
    channel detection (I074) SHOULD fire on deliverables that reference
    journeys using deprecated personas or channels. SHOULD suppress when all
    journey personas and channels are active or have no status.
  """
  constrains  [detect_transitive_deprecated_persona, detect_transitive_deprecated_channel]
  verify unit "I073 fires for deliverable->journey->deprecated persona"
  verify unit "I074 fires for deliverable->journey->deprecated channel"
  verify unit "both suppress for active references"
}

// ---------------------------------------------------------------------------
// v1.1 constraints — release, ownership, effort
// ---------------------------------------------------------------------------

constraint product_release_validation_latency "Release Validation Latency" {
  description "Release entity validation including cycle detection and version format checks must complete under 50ms for 500 entities."
  category    performance
  priority    high
  metric      """
    Release entity validation (cycle detection, lifecycle consistency, version
    format, date format) MUST complete in under 50ms for 500 entities.
  """
  constrains  [
    detect_release_dependency_cycles,
    detect_release_version_not_semver,
    detect_invalid_release_date,
  ]
  verify load "release validation under 50ms with 500 entities"
}

constraint product_owner_query_latency "Owner Workload Query Latency" {
  description "The owner workload query scanning features, milestones, deliverables, and releases must complete under 100ms for 500 entities."
  category    performance
  priority    high
  metric      """
    The owner workload query MUST complete in under 100ms for 500 entities.
    The query scans all features, milestones, deliverables, and releases.
  """
  constrains  [pe_query_owner_workload]
  verify load "owner-workload query under 100ms with 500 entities"
}

constraint product_weighted_completion_latency "Weighted Completion Query Latency" {
  description "The weighted milestone completion query must complete under 100ms for milestones with up to 200 features."
  category    performance
  priority    high
  metric      """
    The weighted milestone completion query MUST complete in under 100ms
    for milestones with up to 200 features.
  """
  constrains  [pe_query_weighted_milestone_completion]
  verify load "weighted-milestone-completion under 100ms with 200 features"
}

constraint product_release_query_latency "Release Query Latency" {
  description "The release completion query must complete under 100ms for 500 entities."
  category    performance
  priority    high
  metric      """
    The release completion query MUST complete in under 100ms for 500
    entities. A release's deliverables and milestones are core's
    `specforge query <release> --depth 1` (ADR 0011).
  """
  constrains  [pe_query_release_completion]
  verify load "release-completion query under 100ms with 500 entities"
}

constraint product_release_cycle_detection_correctness "Release Cycle Detection Correctness" {
  description "Release dependency cycle detection must have zero false negatives and zero false positives using Tarjan's algorithm."
  category    reliability
  priority    critical
  metric      """
    Release dependency cycle detection MUST have zero false negatives
    and zero false positives. Uses same Tarjan algorithm as other DAGs.
  """
  constrains  [detect_release_dependency_cycles]
  protects    [release_dag]
  verify property "every release cycle produces W092"
  verify property "no false positive W092 on acyclic release graph"
}

constraint product_ownership_field_consistency "Ownership Field Consistency" {
  description "Missing owner detection must fire only for ownable entity kinds and never for journeys, modules, terms, personas, or channels."
  category    reliability
  priority    high
  metric      """
    I080 MUST fire for every feature, milestone, deliverable, and release
    without an owner field. I080 MUST NOT fire for entity kinds that do
    not support owner (journey, module, term, persona, channel).
  """
  constrains  [detect_missing_owner]
  protects    [pe_ownership_field_awareness]
  verify property "I080 fires for all ownable entities without owner"
  verify property "I080 does not fire for non-ownable entity kinds"
}

constraint product_effort_weight_correctness "Effort Weight Correctness" {
  description "Weighted milestone completion must use the fixed Fibonacci effort weights with features defaulting to medium when unspecified."
  category    reliability
  priority    critical
  metric      """
    Weighted milestone completion MUST use the effort scale's weights:
    xs=1, s=2, m=3, l=5, xl=8 (Fibonacci-inspired). The weights define
    the scale and are not configurable (ADR 0011). Features without
    effort MUST default to the weight of m (3).
  """
  constrains  [pe_query_weighted_milestone_completion]
  protects    [pe_effort_weighted_completion]
  verify property "each effort level maps to its Fibonacci weight"
  verify property "missing effort defaults to m weight"
}

// ---------------------------------------------------------------------------
constraint product_query_complexity "Product Query Complexity Bounds" {
  description "Multi-hop traversal queries must complete under 500ms for 5000 entities, bounded by max_hops to prevent unbounded traversal."
  category    performance
  priority    high
  metric      """
    Multi-hop traversal queries (queryPersonaCoverageMatrix,
    queryFeatureImpact, queryCriticalPath) SHOULD complete in under 500ms
    for projects with up to 5000 entities. Queries SHOULD be bounded by
    max_hops (default 5) to prevent unbounded traversal. The complexity
    of queryPersonaCoverageMatrix is O(P*J*F) where P=personas, J=journeys,
    F=features.
  """
  constrains  [pe_query_persona_coverage_matrix, pe_query_feature_impact, pe_query_critical_path]
  verify property "coverage matrix completes under 500ms at 5000 entities"
  verify property "feature impact bounded by max_hops=5"
}

// ---------------------------------------------------------------------------
// Scalability tiers — performance expectations beyond 500 entities
// ---------------------------------------------------------------------------

constraint product_scalability_tiers "Product Scalability Tiers" {
  description "Product operations must degrade gracefully across three entity-count tiers with no superlinear degradation beyond 10000 entities."
  category    performance
  priority    high
  metric      """
    Product extension operations MUST degrade gracefully as entity count
    increases. Performance expectations are defined in three tiers:

    Tier 1 (<=500 entities): validation <50ms, queries <100ms.
    Tier 2 (501-2000 entities): validation <200ms, queries <500ms.
    Tier 3 (2001-10000 entities): validation <1s, queries <2s.

    Beyond 10,000 entities, no hard guarantees apply but the extension MUST
    NOT exhibit superlinear (O(n^2) or worse) degradation for any operation.
    Pagination becomes critical at Tier 3 — queries returning result sets
    >1000 items MUST support pagination.

    Cycle detection (Tarjan's) is O(V+E) at all tiers. Coverage matrix
    (O(P*J*F)) is the highest-complexity query and MAY exceed 2s at Tier 3.
    Critical path computation is O(V+E) via topological sort and does not
    degrade significantly.
  """
  constrains  [
    pe_query_persona_coverage_matrix,
    pe_query_feature_impact,
    pe_query_critical_path,
    pe_query_feature_ordering,
    pe_query_module_coupling,
    pe_query_owner_workload,
  ]
  verify load "validation completes under 200ms at 2000 entities"
  verify load "validation completes under 1s at 10000 entities"
  verify load "queries complete under 500ms at 2000 entities"
  verify load "queries complete under 2s at 10000 entities"
  verify load "no operation exhibits superlinear degradation from 500 to 10000 entities"
}

constraint product_query_pagination_required "Query Pagination for Large Result Sets" {
  description "Matrix and analytics queries must page with the list commands' offset and limit to enable incremental consumption by AI agents."
  category    usability
  priority    high
  metric      """
    Matrix and analytics queries (coverage_matrix, module_coupling,
    owner_workload, feature_overlap, channel_coverage_matrix) MUST page
    like the list commands: --limit (default 100, clamped to [1, 1000])
    and --offset (default 0), with total, offset, limit and has_more in
    the payload. There are no cursors: with no graph revision a cursor
    detects nothing an offset does not (ADR 0011).

    This enables AI agents to consume large result sets incrementally
    without exceeding token budgets.
  """
  constrains  [
    pe_query_persona_coverage_matrix,
    pe_query_module_coupling,
    pe_query_owner_workload,
    pe_query_feature_overlap,
    pe_query_channel_coverage_matrix,
  ]
  verify unit "matrix query with limit=10 returns at most 10 entries"
  verify unit "offset=limit retrieves the second page"
  verify unit "limit>1000 is clamped to 1000"
}
