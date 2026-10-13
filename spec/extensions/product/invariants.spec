// @specforge/product extension invariants
//
// Runtime guarantees specific to the product entity model:
// entity DAGs, lifecycle consistency, field awareness, and structural properties.

// ════════════════════════════════════════════════════════════════
// Entity DAG Invariants
// ════════════════════════════════════════════════════════════════

invariant module_dag "Module DAG" {
  guarantee """
    The depends_on edges between module entities MUST form a directed
    acyclic graph. Cycles are detected by E007 and rejected.
  """
  risk      high
  verify property "acyclic module dependency graph is accepted"
  verify unit "circular module dependency produces E007"
}

invariant milestone_dag "Milestone DAG" {
  guarantee """
    The depends_on edges between milestone entities MUST form a directed
    acyclic graph. Cycles are detected by E015 and rejected.
  """
  risk      high
  verify property "acyclic milestone dependency graph is accepted"
  verify unit "circular milestone dependency produces E015"
}

invariant feature_dag "Feature DAG" {
  guarantee """
    The depends_on edges between feature entities MUST form a directed
    acyclic graph. Cycles produce a W045 warning.
  """
  risk      medium
  verify property "acyclic feature dependency graph is accepted"
  verify unit "circular feature dependency produces W045"
}

invariant deliverable_dag "Deliverable DAG" {
  guarantee """
    The depends_on edges between deliverable entities MUST form a
    directed acyclic graph. Cycles are detected by E052 and rejected.
  """
  risk      high
  verify property "acyclic deliverable dependency graph is accepted"
  verify unit "circular deliverable dependency produces E052"
}

invariant release_dag "Release DAG" {
  guarantee """
    The depends_on edges between release entities MUST form a directed
    acyclic graph. Cycles produce a W092 warning.
  """
  risk      medium
  verify property "acyclic release dependency graph is accepted"
  verify unit "circular release dependency produces W092"
}

// ════════════════════════════════════════════════════════════════
// Testability Invariants
// ════════════════════════════════════════════════════════════════

invariant pe_feature_non_testable "Feature Non-Testable" {
  guarantee """
    Feature entities MUST have testable=false in the manifest. Features
    are planning constructs tested indirectly through behavior chains.
  """
  risk      medium
  verify unit "feature kind has testable=false"
}

invariant pe_persona_non_testable "Persona Non-Testable" {
  guarantee """
    Persona entities MUST have testable=false in the manifest. Personas
    are reference entities describing user roles, not testable contracts.
  """
  risk      medium
  verify unit "persona kind has testable=false"
}

invariant pe_channel_non_testable "Channel Non-Testable" {
  guarantee """
    Channel entities MUST have testable=false in the manifest. Channels
    are reference entities describing interaction mediums.
  """
  risk      medium
  verify unit "channel kind has testable=false"
}

invariant pe_release_non_testable "Release Non-Testable" {
  guarantee """
    Release entities MUST have testable=false in the manifest. Releases
    are coordination constructs, not testable contracts.
  """
  risk      medium
  verify unit "release kind has testable=false"
}

invariant pe_product_verify_support "Product Verify Support" {
  guarantee """
    No product entity kind supports verify statements: all 9 kinds have
    supportsVerify=false and declare no verify kinds.
  """
  risk      medium
  verify unit "all 9 product kinds have supportsVerify=false"
}

// ════════════════════════════════════════════════════════════════
// Lifecycle Consistency Invariants
// ════════════════════════════════════════════════════════════════

invariant deliverable_lifecycle_consistency "Deliverable Lifecycle Consistency" {
  guarantee """
    Deliverable status transitions MUST follow the declared state machine.
    Shipped deliverables MUST have completed milestones. Deprecated
    deliverables MUST have a documented reason.
  """
  risk      high
  verify unit "deliverable status follows valid transitions"
}

invariant persona_lifecycle_consistency "Persona Lifecycle Consistency" {
  guarantee """
    Deprecated personas MUST have a documented reason field. Persona
    status transitions MUST follow the declared state machine
    (active->deprecated, deprecated is terminal).
  """
  risk      medium
  verify unit "deprecated persona has reason"
  verify unit "persona status follows valid transitions"
}

invariant channel_lifecycle_consistency "Channel Lifecycle Consistency" {
  guarantee """
    Deprecated channels MUST have a documented reason field. Channel
    status transitions MUST follow the declared state machine
    (active->deprecated, deprecated is terminal).
  """
  risk      medium
  verify unit "deprecated channel has reason"
  verify unit "channel status follows valid transitions"
}

invariant persona_channel_lifecycle "Persona and Channel Lifecycle" {
  guarantee """
    Persona and channel entities MUST have valid lifecycle states.
    The status field, when present, MUST be a valid enum value.
  """
  risk      medium
  verify unit "persona status is valid PersonaStatus enum"
  verify unit "channel status is valid ChannelStatus enum"
}

invariant milestone_status_consistency "Milestone Status Consistency" {
  guarantee """
    Milestone status MUST be a valid MilestoneStatus enum value.
    Completed milestones MUST have exit criteria. Blocked milestones
    SHOULD have dependencies.
  """
  risk      medium
  verify unit "milestone status is valid and consistent with exit criteria"
}

invariant pe_release_lifecycle_consistency "Release Lifecycle Consistency" {
  guarantee """
    Released releases MUST have all deliverables in shipped status.
    Recalled releases MUST have a documented reason.
  """
  risk      high
  verify unit "released release has all shipped deliverables"
  verify unit "recalled release has reason"
}

invariant pe_release_status_transition "Release Status Transition" {
  guarantee """
    Release status transitions MUST follow the declared state machine:
    planned->in_progress, in_progress->released, released->recalled.
    Recalled is terminal.
  """
  risk      medium
  verify unit "release status follows valid transitions"
}

// ════════════════════════════════════════════════════════════════
// Structural Consistency Invariants
// ════════════════════════════════════════════════════════════════

invariant deliverable_journey_module_consistency "Deliverable Journey-Module Consistency" {
  guarantee """
    Features referenced by a deliverable's journeys MUST be a subset
    of features assigned to the deliverable's modules.
  """
  risk      medium
  verify unit "deliverable journey features are covered by module features"
}

invariant milestone_feature_module_consistency "Milestone Feature-Module Consistency" {
  guarantee """
    Features scheduled in a milestone MUST be reachable from the
    milestone's modules via ModuleContainsFeature edges.
  """
  risk      medium
  verify unit "milestone features are covered by milestone modules"
}

invariant module_feature_reachability "Module-Feature Reachability" {
  guarantee """
    Every module SHOULD have at least one feature assigned via
    ModuleContainsFeature edges.
  """
  risk      low
  verify unit "module with features has reachable feature set"
}

invariant pe_deliverable_chain_integrity "Deliverable Chain Integrity" {
  guarantee """
    The full deliverable->milestone->feature->module chain MUST be
    consistent. Features scheduled in milestones that belong to a
    deliverable MUST be covered by the deliverable's modules.
  """
  risk      medium
  verify unit "end-to-end deliverable chain is consistent"
}

// ════════════════════════════════════════════════════════════════
// Field Awareness Invariants
// ════════════════════════════════════════════════════════════════

invariant pe_ownership_field_awareness "Ownership Field Awareness" {
  guarantee """
    Owner and contributors fields are available on feature, milestone,
    deliverable, and release entity kinds. Other entity kinds
    (module, term, journey, persona, channel) intentionally lack
    ownership fields.
  """
  risk      low
  verify unit "owner field exists on feature, milestone, deliverable, release"
  verify unit "owner field absent on module, term, journey, persona, channel"
}

invariant pe_owner_string_consistency "Owner String Consistency" {
  guarantee """
    Owner strings across entities SHOULD be consistent. Similar owner
    strings within Levenshtein distance 2 produce I085 diagnostics
    suggesting normalization.
  """
  risk      low
  verify unit "similar owner strings are detected"
}

invariant pe_tags_per_entity_kind "Tags Per Entity Kind" {
  guarantee """
    Every product entity kind gets the shared tags field (string[] @optional).
  """
  risk      low
  verify unit "all 9 entity kinds have tags field"
}

invariant tag_cross_kind_awareness "Tag Cross-Kind Awareness" {
  guarantee """
    Tags used on entities of 3 or more different kinds produce I084
    info diagnostics suggesting kind-specific prefixes.
  """
  risk      low
  verify unit "cross-kind tag on 3+ kinds produces I084"
}

invariant term_alias_uniqueness "Term Alias Uniqueness" {
  guarantee """
    Term aliases MUST be unique across all terms (case-insensitive).
    Conflicts between aliases or between an alias and a term ID
    produce W086 warnings.
  """
  risk      medium
  verify unit "duplicate term aliases produce W086"
}

// ════════════════════════════════════════════════════════════════
// Effort and Estimation Invariants
// ════════════════════════════════════════════════════════════════

invariant pe_effort_weighted_completion "Effort-Weighted Completion" {
  guarantee """
    Weighted milestone completion MUST use configured effort weights.
    Default weights: xs=1, s=2, m=3, l=5, xl=8. Features without
    effort default to the weight of m.
  """
  risk      medium
  verify unit "effort weights are applied correctly"
  verify unit "missing effort defaults to m weight"
}

invariant pe_effort_enum_validity "Effort Enum Validity" {
  guarantee """
    The effort field on features MUST contain a valid Effort enum
    value (xs, s, m, l, xl). Another value is E061.
  """
  risk      low
  verify unit "invalid effort value produces E061"
}

// ════════════════════════════════════════════════════════════════
// Surface and Query Invariants
// ════════════════════════════════════════════════════════════════

invariant pe_surface_error_consistency "Surface Error Consistency" {
  guarantee """
    All product commands MUST use exactly two error codes:
    ENTITY_NOT_FOUND (exit 1) and INVALID_INPUT (exit 2), written to
    stderr with nothing on stdout.
  """
  risk      high
  verify unit "command errors use the two defined error codes"
}

invariant pe_list_pagination_correctness "List Pagination Correctness" {
  guarantee """
    All paged commands (the lists and the matrix queries) MUST return
    correct pagination: total is the count after filters and before
    paging, has_more is total > offset + returned count, limit defaults
    to 100 (clamped to [1, 1000]), offset defaults to 0.
  """
  risk      high
  verify unit "pagination metadata is correct"
}

invariant pe_cross_extension_query_isolation "Cross-Extension Query Isolation" {
  guarantee """
    Product queries MUST NOT traverse edges owned by other extensions.
    Query results MUST be identical regardless of which other
    extensions are installed.
  """
  risk      high
  verify unit "product queries ignore foreign edges"
}

invariant pe_queries_derived_not_standard "Queries Are Derived, Not Standard" {
  guarantee """
    Product queries are a derived convenience layer, NOT part of the
    Graph Protocol standard. Alternative compilers need not implement
    these queries to be compliant.
  """
  risk      low
  verify unit "queries are documented as derived convenience"
}

invariant pe_partial_graph_traversability "Partial Graph Traversability" {
  guarantee """
    Product queries MUST operate on the structural graph regardless
    of validation state. Entities with validation errors MUST still
    be traversable and appear in query results.
  """
  risk      medium
  verify unit "entities with errors appear in query results"
}

invariant pe_rendering_completeness "Rendering Completeness" {
  guarantee """
    All 9 product entity kinds MUST render as standard graph nodes
    in the Graph Protocol JSON output. The core emitter handles all
    entity kinds uniformly.
  """
  risk      medium
  verify unit "all 9 entity kinds appear in export output"
}

// ════════════════════════════════════════════════════════════════
// Temporal and Blocker Invariants
// ════════════════════════════════════════════════════════════════

invariant pe_milestone_temporal_consistency "Milestone Temporal Consistency" {
  guarantee """
    When both start_date and target_date are present on a milestone,
    start_date MUST be on or before target_date. Both fields MUST
    be valid ISO 8601 date format when present.
  """
  risk      medium
  verify unit "start_date <= target_date when both present"
}

invariant pe_blocker_status_consistency "Blocker-Status Consistency" {
  guarantee """
    Blocked milestones SHOULD list blockers entries that document the
    cause of the block; depends_on entries do not replace them. A blocked
    milestone with no blockers produces I060.
  """
  risk      low
  verify unit "blocked milestone without blockers produces I060"
}

// ════════════════════════════════════════════════════════════════
// Migration and Validation Invariants
// ════════════════════════════════════════════════════════════════

invariant pe_migration_backward_compat "Migration Backward Compatibility" {
  guarantee """
    Minor version bumps (1.x) MUST be backward compatible. New fields
    are always optional. Breaking changes require a major version bump
    with a migration hook.
  """
  risk      high
  verify unit "v1 spec files parse under v1.x without errors"
}

invariant pe_validation_deterministic "Validation Deterministic" {
  guarantee """
    Product validation MUST be deterministic: identical inputs (spec
    files + optional cache file) always produce identical diagnostic
    output. No time-dependent diagnostics during specforge check.
  """
  risk      high
  verify unit "same inputs produce same diagnostics"
}

invariant pe_release_version_uniqueness "Release Version Uniqueness" {
  guarantee """
    Release version strings MUST be unique across all release entities.
    Duplicate versions produce I091 info diagnostics.
  """
  risk      medium
  verify unit "duplicate release versions produce I091"
}
