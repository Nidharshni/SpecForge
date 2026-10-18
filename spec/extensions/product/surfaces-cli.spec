// CLI surface contributions — list commands, query commands, and planning insights
//
// This file specifies the CLI commands the @specforge/product extension
// contributes: 40 commands (40 built, 0 to build). Each is a cmd__product_<id>
// export the CLI runs as `specforge product <id>` (underscores spelled as
// dashes) and MCP auto-promotes to the tool `specforge.product.<id>`. The
// extension declares no MCP resources. One-hop navigation (the milestones of a
// feature, the deliverables of a module, ...) is core's `specforge query <id>
// --depth 1 --kind <kind>` / `specforge.query` (ADR 0011). See
// surfaces-shared.spec for the conventions every command follows.

use "extensions/product/behaviors-operations"
use "extensions/product/behaviors-queries"
use "extensions/product/behaviors-registration"
use "extensions/product/behaviors-v1-1"
use "extensions/product/features"
use "extensions/product/types"

defaults behavior {
  features [pe_surface_contributions]
}

// ════════════════════════════════════════════════════════════════
// CLI List Commands (8 + 1 v1.1) — entity listing with filter/pagination
// ════════════════════════════════════════════════════════════════

behavior surface_list_features "Surface: List Features" {
  category command
  types    [ProductListFilter, FeatureListResult, FeatureListEntry, ProductSurfaceError]
  contract """
    The product:features CLI command MUST list all feature entities in
    the product graph, returning a paginated FeatureListResult. Accepts
    optional --status, --priority, --tags (comma-separated; a feature
    matches when its tags intersect them), --limit (default 100),
    --offset (default 0), --sort-by (default "id"), and --sort-order
    (default "asc") flags, and the host's --format.
    Wasm export: cmd__product_features.
    MCP tool: specforge.product.features.
  """
  ensures {
    returns_list       "stdout is a valid FeatureListResult JSON object"
    pagination_correct "total reflects the filtered count before paging; has_more is true iff offset + entries.length < total"
    status_filter      "when --status is set, only features with matching FeatureStatus are returned"
    priority_filter    "when --priority is set, only features with matching Priority are returned"
    tags_filter        "when --tags is set, only features whose tags intersect the filter set are returned"
    sort_applied       "entries are sorted by sort_by field in sort_order direction"
    limit_respected    "entries.length <= limit"
    empty_result       "project with no features returns empty list with total=0"
    human_format       "under --format human, stdout is a table with a header row and columns: id, title, status, priority"
    exit_zero          "exit code 0 on success"
  }
  verify unit "list features returns paginated FeatureListResult"
  verify unit "status filter reduces result set"
  verify unit "priority filter reduces result set"
  verify unit "tags filter intersects correctly"
  verify unit "pagination offset and limit are respected"
  verify unit "empty project returns total=0 and empty list"
  verify unit "human format is a table with a header row"
}

behavior surface_list_journeys "Surface: List Journeys" {
  category command
  types    [ProductListFilter, JourneyListResult, JourneyListEntry, ProductSurfaceError]
  contract """
    The product:journeys CLI command MUST list all journey entities in
    the product graph, returning a paginated JourneyListResult. Accepts
    the standard list flags (--status, --priority, --tags, --limit,
    --offset, --sort-by, --sort-order) plus --persona to
    filter by persona ID.
    Wasm export: cmd__product_journeys.
    MCP tool: specforge.product.journeys.
  """
  ensures {
    returns_list       "stdout is a valid JourneyListResult JSON object"
    persona_filter     "when --persona is set, only journeys referencing that persona (via JourneyTargetsPersona edge) are returned"
    pagination_correct "total reflects the filtered count before paging; has_more is true iff offset + entries.length < total"
    channel_count      "each entry's channel_count reflects the number of JourneyUsesChannel edges"
    feature_count      "each entry's feature_count reflects the number of JourneyExercisesFeature edges"
    exit_zero          "exit code 0 on success"
  }
  verify unit "list journeys returns paginated JourneyListResult"
  verify unit "persona filter reduces result set to matching journeys"
  verify unit "channel_count and feature_count are accurate per entry"
}

behavior surface_list_deliverables "Surface: List Deliverables" {
  category command
  types    [ProductListFilter, DeliverableListResult, DeliverableListEntry, ProductSurfaceError]
  contract """
    The product:deliverables CLI command MUST list all deliverable entities
    in the product graph, returning a paginated DeliverableListResult.
    Accepts the standard list flags plus --artifact-type to filter by
    ArtifactType.
    Wasm export: cmd__product_deliverables.
    MCP tool: specforge.product.deliverables.
  """
  ensures {
    returns_list         "stdout is a valid DeliverableListResult JSON object"
    artifact_type_filter "when --artifact-type is set, only deliverables with matching ArtifactType are returned"
    status_filter        "when --status is set, only deliverables with matching DeliverableStatus are returned"
    journey_count        "each entry's journey_count reflects the number of DeliverableSupportsJourney edges"
    module_count         "each entry's module_count reflects the number of DeliverableContainsModule edges"
    exit_zero            "exit code 0 on success"
  }
  verify unit "list deliverables returns paginated DeliverableListResult"
  verify unit "artifact-type filter reduces result set"
  verify unit "journey_count and module_count are accurate per entry"
}

behavior surface_list_milestones "Surface: List Milestones" {
  category command
  types    [ProductListFilter, MilestoneListResult, MilestoneListEntry, ProductSurfaceError]
  contract """
    The product:milestones CLI command MUST list all milestone entities
    in the product graph, returning a paginated MilestoneListResult.
    Accepts the standard list flags.
    Wasm export: cmd__product_milestones.
    MCP tool: specforge.product.milestones.
  """
  ensures {
    returns_list    "stdout is a valid MilestoneListResult JSON object"
    status_filter   "when --status is set, only milestones with matching MilestoneStatus are returned"
    priority_filter "when --priority is set, only milestones with matching Priority are returned"
    feature_count   "each entry's feature_count reflects the number of MilestoneDeliversFeature edges"
    exit_zero       "exit code 0 on success"
  }
  verify unit "list milestones returns paginated MilestoneListResult"
  verify unit "status filter reduces result set"
  verify unit "feature_count is accurate per entry"
}

behavior surface_list_modules "Surface: List Modules" {
  category command
  types    [ProductListFilter, ModuleListResult, ModuleListEntry, ProductSurfaceError]
  contract """
    The product:modules CLI command MUST list all module entities in the
    product graph, returning a paginated ModuleListResult. Accepts the
    standard list flags plus --family to filter by module family.
    Wasm export: cmd__product_modules.
    MCP tool: specforge.product.modules.
  """
  ensures {
    returns_list  "stdout is a valid ModuleListResult JSON object"
    family_filter "when --family is set, only modules with matching family are returned"
    feature_count "each entry's feature_count reflects the number of ModuleContainsFeature edges"
    depends_on    "each entry's depends_on lists outgoing ModuleDependsOn target IDs"
    exit_zero     "exit code 0 on success"
  }
  verify unit "list modules returns paginated ModuleListResult"
  verify unit "family filter reduces result set"
  verify unit "feature_count and depends_on are accurate per entry"
}

behavior surface_list_terms "Surface: List Terms" {
  category command
  types    [ProductListFilter, TermListResult, TermListEntry, ProductSurfaceError]
  contract """
    The product:terms CLI command MUST list all term entities in the
    product graph, returning a paginated TermListResult. Accepts the
    standard list flags.
    Wasm export: cmd__product_terms.
    MCP tool: specforge.product.terms.
  """
  ensures {
    returns_list "stdout is a valid TermListResult JSON object"
    alias_count  "each entry's alias_count reflects the length of the aliases field"
    definition   "each entry includes the full definition string"
    exit_zero    "exit code 0 on success"
  }
  verify unit "list terms returns paginated TermListResult"
  verify unit "alias_count is accurate per entry"
}

behavior surface_list_personas "Surface: List Personas" {
  category command
  types    [ProductListFilter, PersonaListResult, PersonaListEntry, ProductSurfaceError]
  contract """
    The product:personas CLI command MUST list all persona entities in
    the product graph, returning a paginated PersonaListResult. Accepts
    the standard list flags plus --technical-level to filter by
    TechnicalLevel.
    Wasm export: cmd__product_personas.
    MCP tool: specforge.product.personas.
  """
  ensures {
    returns_list           "stdout is a valid PersonaListResult JSON object"
    technical_level_filter "when --technical-level is set, only personas with matching TechnicalLevel are returned"
    journey_count          "each entry's journey_count reflects the number of reverse JourneyTargetsPersona edges"
    exit_zero              "exit code 0 on success"
  }
  verify unit "list personas returns paginated PersonaListResult"
  verify unit "technical-level filter reduces result set"
  verify unit "journey_count is accurate per entry"
}

behavior surface_list_channels "Surface: List Channels" {
  category command
  types    [ProductListFilter, ChannelListResult, ChannelListEntry, ProductSurfaceError]
  contract """
    The product:channels CLI command MUST list all channel entities in
    the product graph, returning a paginated ChannelListResult. Accepts
    the standard list flags plus --interaction-model to filter by
    InteractionModel.
    Wasm export: cmd__product_channels.
    MCP tool: specforge.product.channels.
  """
  ensures {
    returns_list             "stdout is a valid ChannelListResult JSON object"
    interaction_model_filter "when --interaction-model is set, only channels with matching InteractionModel are returned"
    journey_count            "each entry's journey_count reflects the number of reverse JourneyUsesChannel edges"
    exit_zero                "exit code 0 on success"
  }
  verify unit "list channels returns paginated ChannelListResult"
  verify unit "interaction-model filter reduces result set"
  verify unit "journey_count is accurate per entry"
}

// ════════════════════════════════════════════════════════════════
// CLI Query Commands — graph queries with typed I/O
// ════════════════════════════════════════════════════════════════

behavior surface_milestone_completion "Surface: Milestone Completion" {
  category command
  types    [MilestoneCompletionInput, MilestoneCompletionPayload, ProductSurfaceError]
  contract """
    The product:milestone-completion CLI command MUST accept a positional
    milestone argument (the entity id) and return a MilestoneCompletionPayload JSON
    object on stdout. Delegates to pe_query_milestone_completion.
    Wasm export: cmd__product_milestone_completion.
    MCP tool: specforge.product.milestone_completion.
    MCP tool input schema:
      { "milestone": { "type": "string", "description": "Entity ID of the milestone" } }
    Exit code 0 on success, 1 on entity-not-found.
  """
  ensures {
    delegates         "delegates to pe_query_milestone_completion with the provided milestone id"
    json_output       "stdout is a valid MilestoneCompletionPayload JSON: milestone_id, total_features, done_count, completion_ratio, done_features, evidence, and with recorded evidence proven_count, proven_ratio, proven_features and feature_evidence"
    not_found_error   "missing milestone returns ProductSurfaceError with suggestion and exit code 1"
    human_format      "under --format human, stdout shows milestone_id, done/total and the ratio as a percentage, the proven share (or why there is none) and each feature's status and evidence"
    exit_zero_success "exit code 0 when query succeeds"
    exit_one_error    "exit code 1 when the entity is not found"
  }
  verify unit "milestone-completion returns MilestoneCompletionPayload JSON"
  verify unit "missing milestone ID returns error with suggestion"
  verify unit "human format shows ratio as percentage"
  verify unit "human format shows the proven share and each feature's evidence beside its status"
  verify unit "exit code 0 on success, 1 on error"
}

behavior surface_journey_coverage "Surface: Journey Coverage" {
  category command
  types    [JourneyCoverageInput, JourneyCoveragePayload, ProductSurfaceError]
  contract """
    The product:journey-coverage CLI command MUST accept a positional
    journey argument (the entity id) and return a JourneyCoveragePayload JSON object
    on stdout. Delegates to pe_query_journey_coverage.
    Wasm export: cmd__product_journey_coverage.
    MCP tool: specforge.product.journey_coverage.
    MCP tool input schema:
      { "journey": { "type": "string", "description": "Entity ID of the journey" } }
    Exit code 0 on success, 1 on entity-not-found.
  """
  ensures {
    delegates         "delegates to pe_query_journey_coverage with the provided journey id"
    json_output       "stdout is a valid JourneyCoveragePayload JSON: journey_id, total_features, covered_count, uncovered_features"
    not_found_error   "missing journey returns ProductSurfaceError with suggestion and exit code 1"
    human_format      "under --format human, stdout shows journey_id, covered/total and the uncovered list"
    exit_zero_success "exit code 0 when query succeeds"
    exit_one_error    "exit code 1 when the entity is not found"
  }
  verify unit "journey-coverage returns JourneyCoveragePayload JSON"
  verify unit "missing journey ID returns error with suggestion"
  verify unit "exit code 0 on success, 1 on error"
}

behavior surface_feature_ordering "Surface: Feature Ordering" {
  category command
  types    [FeatureOrderingPayload, ProductSurfaceError]
  contract """
    The product:feature-ordering CLI command takes no positional arguments
    and returns a FeatureOrderingPayload JSON object on stdout. Delegates
    to pe_query_feature_ordering. This is a global query.
    Wasm export: cmd__product_feature_ordering.
    MCP tool: specforge.product.feature_ordering.
    MCP tool input schema: {} (no parameters).
    Exit code 0 on success.
  """
  ensures {
    delegates         "delegates to pe_query_feature_ordering"
    json_output       "stdout is a valid FeatureOrderingPayload JSON: sorted_features, has_cycles, cycle_members"
    no_args           "command takes no positional entity ID argument"
    human_format      "under --format human, stdout is a numbered feature list with cycle members flagged"
    exit_zero_success "exit code 0 when query succeeds (even if cycles exist)"
  }
  verify unit "feature-ordering returns FeatureOrderingPayload JSON"
  verify unit "cycles present in output does not cause exit code 1"
  verify unit "empty feature graph returns empty sorted list"
}

behavior surface_milestone_timeline "Surface: Milestone Timeline" {
  category command
  types    [
    MilestoneTimelineInput,
    MilestoneTimelinePayload,
    MilestoneTimelineEntry,
    ProductSurfaceError,
  ]
  contract """
    The product:milestone-timeline CLI command takes an optional --as-of
    arg (YYYY-MM-DD, defaults to the today the host passes, the UTC date
    of the call) and
    returns a MilestoneTimelinePayload JSON object on stdout. Delegates
    to pe_query_milestone_timeline. Overdue detection is query-time
    only — it does NOT emit I058 diagnostics during specforge check.
    Wasm export: cmd__product_milestone_timeline.
    MCP tool: specforge.product.milestone_timeline.
    MCP tool input schema:
      { "as_of": { "type": "string", "description": "Date for overdue calculation, YYYY-MM-DD (default: the host's today)" } }
    Exit code 0 on success.
  """
  ensures {
    delegates                 "delegates to pe_query_milestone_timeline with optional --as-of"
    json_output               "stdout is a valid MilestoneTimelinePayload JSON: milestones[], overdue_count"
    entry_fields              "each MilestoneTimelineEntry has: milestone_id, target_date, status, is_overdue, priority"
    date_default              "when --as-of is omitted, the today the host passes is used"
    no_validation_side_effect "does NOT emit I058 diagnostics — query-time only"
    human_format              "under --format human, stdout is a chronological table with overdue markers"
    exit_zero_success         "exit code 0 when query succeeds"
  }
  verify unit "milestone-timeline returns MilestoneTimelinePayload JSON"
  verify unit "as-of flag overrides current date for overdue calculation"
  verify unit "human format marks overdue milestones"
}

// ════════════════════════════════════════════════════════════════
// CLI Query Commands — traceability, rollups and dependency queries
// ════════════════════════════════════════════════════════════════

behavior surface_deliverable_traceability "Surface: Deliverable Traceability" {
  category command
  types    [DeliverableTraceabilityPayload, ProductSurfaceError]
  contract """
    The product:deliverable-traceability CLI command MUST accept a positional
    deliverable argument (the entity id) and return a DeliverableTraceabilityPayload.
    Delegates to pe_query_deliverable_traceability.
    Wasm export: cmd__product_deliverable_traceability.
    MCP tool: specforge.product.deliverable_traceability.
  """
  ensures {
    delegates       "delegates to pe_query_deliverable_traceability"
    json_output     "stdout is a valid DeliverableTraceabilityPayload JSON"
    not_found_error "missing deliverable returns ProductSurfaceError with suggestion and exit code 1"
    exit_code       "exit 0 on success, 1 on ENTITY_NOT_FOUND, 2 on INVALID_INPUT (surface_error_handling)"
  }
  verify unit "deliverable-traceability returns DeliverableTraceabilityPayload JSON"
  verify unit "missing deliverable ID returns error with suggestion"
}

behavior surface_feature_deliverables "Surface: Feature Deliverables" {
  category command
  types    [FeatureDeliverablePayload, ProductSurfaceError]
  contract """
    The product:feature-deliverables CLI command MUST accept a positional
    feature argument (the entity id) and return a FeatureDeliverablePayload.
    Delegates to pe_query_feature_deliverables.
    Wasm export: cmd__product_feature_deliverables.
  """
  ensures {
    delegates       "delegates to pe_query_feature_deliverables"
    json_output     "stdout is a valid FeatureDeliverablePayload JSON"
    not_found_error "missing feature returns ProductSurfaceError with suggestion and exit code 1"
    exit_code       "exit 0 on success, 1 on ENTITY_NOT_FOUND, 2 on INVALID_INPUT (surface_error_handling)"
  }
  verify unit "feature-deliverables returns FeatureDeliverablePayload JSON"
  verify unit "missing feature ID returns error with suggestion"
}

behavior surface_term_graph "Surface: Term Graph" {
  category command
  types    [TermGraphPayload, ProductSurfaceError]
  contract """
    The product:term-graph CLI command MUST accept a positional term
    argument (the entity id) and optional --max-hops flag (default 1, max 5). Returns a
    TermGraphPayload. Delegates to pe_query_term_graph.
    Wasm export: cmd__product_term_graph.
  """
  ensures {
    delegates       "delegates to pe_query_term_graph with the term id and optional --max-hops"
    json_output     "stdout is a valid TermGraphPayload JSON"
    max_hops_cap    "maxHops > 5 is clamped to 5 without error"
    not_found_error "missing term returns ProductSurfaceError with suggestion and exit code 1"
    exit_code       "exit 0 on success, 1 on ENTITY_NOT_FOUND, 2 on INVALID_INPUT (surface_error_handling)"
  }
  verify unit "term-graph returns TermGraphPayload JSON"
  verify unit "max-hops flag is respected and capped at 5"
  verify unit "missing term ID returns error with suggestion"
}

behavior surface_deliverable_completion "Surface: Deliverable Completion" {
  category command
  types    [DeliverableCompletionPayload, ProductSurfaceError]
  contract """
    The product:deliverable-completion CLI command MUST accept a positional
    deliverable argument (the entity id) and return a DeliverableCompletionPayload.
    Delegates to pe_query_deliverable_completion.
    Wasm export: cmd__product_deliverable_completion.
  """
  ensures {
    delegates       "delegates to pe_query_deliverable_completion"
    json_output     "stdout is a valid DeliverableCompletionPayload JSON"
    not_found_error "missing deliverable returns ProductSurfaceError with suggestion and exit code 1"
    exit_code       "exit 0 on success, 1 on ENTITY_NOT_FOUND, 2 on INVALID_INPUT (surface_error_handling)"
  }
  verify unit "deliverable-completion returns DeliverableCompletionPayload JSON"
  verify unit "missing deliverable ID returns error with suggestion"
}

behavior surface_persona_channels "Surface: Persona Channels" {
  category command
  types    [PersonaChannelPayload, ProductSurfaceError]
  contract """
    The product:persona-channels CLI command MUST accept a positional
    persona argument (the entity id) and return a PersonaChannelPayload.
    Delegates to pe_query_persona_channels.
    Wasm export: cmd__product_persona_channels.
  """
  ensures {
    delegates       "delegates to pe_query_persona_channels"
    json_output     "stdout is a valid PersonaChannelPayload JSON"
    not_found_error "missing persona returns ProductSurfaceError with suggestion and exit code 1"
    exit_code       "exit 0 on success, 1 on ENTITY_NOT_FOUND, 2 on INVALID_INPUT (surface_error_handling)"
  }
  verify unit "persona-channels returns PersonaChannelPayload JSON"
  verify unit "missing persona ID returns error with suggestion"
}

behavior surface_feature_dependents "Surface: Feature Dependents" {
  category command
  types    [FeatureDependentPayload, ProductSurfaceError]
  contract """
    The product:feature-dependents CLI command MUST accept a positional
    feature argument (the entity id) and return a FeatureDependentPayload.
    Delegates to pe_query_feature_dependents.
    Wasm export: cmd__product_feature_dependents.
  """
  ensures {
    delegates       "delegates to pe_query_feature_dependents"
    json_output     "stdout is a valid FeatureDependentPayload JSON"
    not_found_error "missing feature returns ProductSurfaceError with suggestion and exit code 1"
    exit_code       "exit 0 on success, 1 on ENTITY_NOT_FOUND, 2 on INVALID_INPUT (surface_error_handling)"
  }
  verify unit "feature-dependents returns FeatureDependentPayload JSON"
  verify unit "missing feature ID returns error with suggestion"
}

behavior surface_deliverable_dependents "Surface: Deliverable Dependents" {
  category command
  types    [DeliverableDependentPayload, ProductSurfaceError]
  contract """
    The product:deliverable-dependents CLI command MUST accept a positional
    deliverable argument (the entity id) and return a DeliverableDependentPayload.
    Delegates to pe_query_deliverable_dependents.
    Wasm export: cmd__product_deliverable_dependents.
  """
  ensures {
    delegates       "delegates to pe_query_deliverable_dependents"
    json_output     "stdout is a valid DeliverableDependentPayload JSON"
    not_found_error "missing deliverable returns ProductSurfaceError with suggestion and exit code 1"
    exit_code       "exit 0 on success, 1 on ENTITY_NOT_FOUND, 2 on INVALID_INPUT (surface_error_handling)"
  }
  verify unit "deliverable-dependents returns DeliverableDependentPayload JSON"
  verify unit "missing deliverable ID returns error with suggestion"
}

behavior surface_deliverable_priority "Surface: Deliverable Priority" {
  category command
  types    [DeliverablePriorityPayload, ProductSurfaceError]
  contract """
    The product:deliverable-priority CLI command MUST accept a positional
    deliverable argument (the entity id) and return a DeliverablePriorityPayload.
    Delegates to pe_query_deliverable_priority.
    Wasm export: cmd__product_deliverable_priority.
  """
  ensures {
    delegates       "delegates to pe_query_deliverable_priority"
    json_output     "stdout is a valid DeliverablePriorityPayload JSON"
    null_priority   "deliverable with no milestones/journeys returns priority=null"
    not_found_error "missing deliverable returns ProductSurfaceError with suggestion and exit code 1"
    exit_code       "exit 0 on success, 1 on ENTITY_NOT_FOUND, 2 on INVALID_INPUT (surface_error_handling)"
  }
  verify unit "deliverable-priority returns DeliverablePriorityPayload JSON"
  verify unit "missing deliverable ID returns error with suggestion"
}

behavior surface_persona_features "Surface: Persona Features" {
  category command
  types    [PersonaFeaturePayload, ProductSurfaceError]
  contract """
    The product:persona-features CLI command MUST accept a positional
    persona argument (the entity id) and return a PersonaFeaturePayload via multi-hop
    persona->journey->feature traversal. Delegates to pe_query_persona_features.
    Wasm export: cmd__product_persona_features.
  """
  ensures {
    delegates       "delegates to pe_query_persona_features"
    json_output     "stdout is a valid PersonaFeaturePayload JSON"
    not_found_error "missing persona returns ProductSurfaceError with suggestion and exit code 1"
    exit_code       "exit 0 on success, 1 on ENTITY_NOT_FOUND, 2 on INVALID_INPUT (surface_error_handling)"
  }
  verify unit "persona-features returns PersonaFeaturePayload JSON"
  verify unit "missing persona ID returns error with suggestion"
}

behavior surface_feature_impact "Surface: Feature Impact" {
  category command
  types    [FeatureImpactPayload, ProductSurfaceError]
  contract """
    The product:feature-impact CLI command MUST accept a positional
    feature argument (the entity id) and return a FeatureImpactPayload with transitive
    impact analysis. Delegates to pe_query_feature_impact. Each list
    names the referencing entities sorted by id, whatever order the
    references were declared in (product_impact_query_correctness).
    Wasm export: cmd__product_feature_impact.
  """
  ensures {
    delegates       "delegates to pe_query_feature_impact"
    json_output     "stdout is a valid FeatureImpactPayload JSON"
    not_found_error "missing feature returns ProductSurfaceError with suggestion and exit code 1"
    exit_code       "exit 0 on success, 1 on ENTITY_NOT_FOUND, 2 on INVALID_INPUT (surface_error_handling)"
  }
  verify unit "feature-impact returns FeatureImpactPayload JSON"
  verify unit "missing feature ID returns error with suggestion"
}

behavior surface_milestone_velocity "Surface: Milestone Velocity" {
  category command
  types    [MilestoneVelocityPayload, ProductSurfaceError]
  contract """
    The product:milestone-velocity CLI command MUST accept a positional
    milestone argument (the entity id) and an optional --as-of date
    (YYYY-MM-DD, default the today the host passes) and return a
    MilestoneVelocityPayload. Delegates to pe_query_milestone_velocity.
    Wasm export: cmd__product_milestone_velocity.
  """
  ensures {
    delegates       "delegates to pe_query_milestone_velocity"
    json_output     "stdout is a valid MilestoneVelocityPayload JSON"
    not_found_error "missing milestone returns ProductSurfaceError with suggestion and exit code 1"
    exit_code       "exit 0 on success, 1 on ENTITY_NOT_FOUND, 2 on INVALID_INPUT (surface_error_handling)"
  }
  verify unit "milestone-velocity returns MilestoneVelocityPayload JSON"
  verify unit "missing milestone ID returns error with suggestion"
}

behavior surface_deliverable_personas "Surface: Deliverable Personas" {
  category command
  types    [DeliverablePersonaPayload, ProductSurfaceError]
  contract """
    The product:deliverable-personas CLI command MUST accept a positional
    deliverable argument (the entity id) and return a DeliverablePersonaPayload.
    Delegates to pe_query_deliverable_personas.
    Wasm export: cmd__product_deliverable_personas.
  """
  ensures {
    delegates       "delegates to pe_query_deliverable_personas"
    json_output     "stdout is a valid DeliverablePersonaPayload JSON"
    not_found_error "missing deliverable returns ProductSurfaceError with suggestion and exit code 1"
    exit_code       "exit 0 on success, 1 on ENTITY_NOT_FOUND, 2 on INVALID_INPUT (surface_error_handling)"
  }
  verify unit "deliverable-personas returns DeliverablePersonaPayload JSON"
  verify unit "missing deliverable ID returns error with suggestion"
}

behavior surface_feature_overlap "Surface: Feature Overlap" {
  category command
  types    [FeatureOverlapPayload, ProductSurfaceError]
  contract """
    The product:feature-overlap CLI command MUST return features shared
    across 2+ deliverables, one page per the shared offset/limit
    contract. No positional arguments. Delegates to
    pe_query_feature_overlap.
    Wasm export: cmd__product_feature_overlap.
  """
  ensures {
    delegates   "delegates to pe_query_feature_overlap"
    json_output "stdout is a valid FeatureOverlapPayload JSON"
    exit_code   "exit 0 on success, 1 on ENTITY_NOT_FOUND, 2 on INVALID_INPUT (surface_error_handling)"
  }
  verify unit "feature-overlap returns FeatureOverlapPayload JSON"
  verify unit "no overlapping features returns empty list"
}

behavior surface_channel_features "Surface: Channel Features" {
  category command
  types    [ChannelFeaturePayload, ProductSurfaceError]
  contract """
    The product:channel-features CLI command MUST accept a positional
    channel argument (the entity id) and return a ChannelFeaturePayload via multi-hop
    channel->journey->feature traversal. Delegates to pe_query_channel_features.
    Wasm export: cmd__product_channel_features.
  """
  ensures {
    delegates       "delegates to pe_query_channel_features"
    json_output     "stdout is a valid ChannelFeaturePayload JSON"
    not_found_error "missing channel returns ProductSurfaceError with suggestion and exit code 1"
    exit_code       "exit 0 on success, 1 on ENTITY_NOT_FOUND, 2 on INVALID_INPUT (surface_error_handling)"
  }
  verify unit "channel-features returns ChannelFeaturePayload JSON"
  verify unit "missing channel ID returns error with suggestion"
}

// ════════════════════════════════════════════════════════════════
// CLI Command — Unscheduled Features
// ════════════════════════════════════════════════════════════════

behavior surface_unscheduled_features "Surface: Unscheduled Features" {
  category command
  types    [UnscheduledFeaturesPayload, ProductSurfaceError]
  contract """
    The specforge product unscheduled-features command MUST return all
    features not scheduled in any milestone. Under --format human it shows
    each feature's ID and status in a table with a header row.
    Wasm export: cmd__product_unscheduled_features.
    MCP tool: specforge.product.unscheduled_features.
  """
  ensures {
    json_output  "json format returns full UnscheduledFeaturesPayload"
    human_output "human format shows feature ID and status"
    exit_code    "exit 0 on success"
  }
  verify unit "product:unscheduled-features returns UnscheduledFeaturesPayload"
  verify unit "no unscheduled features returns empty list"
}

// ════════════════════════════════════════════════════════════════
// CLI Command — Coverage Matrix
// ════════════════════════════════════════════════════════════════

behavior surface_coverage_matrix "Surface: Coverage Matrix" {
  category command
  types    [
    PersonaCoverageMatrixPayload,
    PersonaCoverageEntry,
    PaginationMetadata,
    ProductSurfaceError,
  ]
  contract """
    The specforge product coverage-matrix command MUST return the persona
    coverage matrix showing feature reachability per persona, one page of
    personas per the shared offset/limit contract (--limit default 100,
    clamped to [1, 1000]; --offset default 0; total, offset, limit and
    has_more in the payload). Under --format human it shows persona ID,
    reachable count, unreachable count and coverage ratio in a table with
    a header row, then the overall coverage.
    Wasm export: cmd__product_coverage_matrix.
    MCP tool: specforge.product.coverage_matrix.
  """
  ensures {
    json_output  "json format returns full PersonaCoverageMatrixPayload"
    human_output "human format shows per-persona coverage"
    paged        "personas are paged by offset and limit, with total and has_more"
    exit_code    "exit 0 on success"
  }
  verify unit "product:coverage-matrix returns PersonaCoverageMatrixPayload"
  verify unit "no personas returns empty matrix"
}

// ════════════════════════════════════════════════════════════════
// CLI Command — Critical Path
// ════════════════════════════════════════════════════════════════

behavior surface_critical_path "Surface: Critical Path" {
  category command
  types    [CriticalPathPayload, CriticalPathNode, ProductSurfaceError]
  contract """
    The specforge product critical-path command MUST compute and return
    the critical path through the milestone dependency graph. Under
    --format human it shows milestone IDs, target dates, status and slack
    in a table with a header row.
    Wasm export: cmd__product_critical_path.
    MCP tool: specforge.product.critical_path.
  """
  ensures {
    json_output  "json format returns full CriticalPathPayload"
    human_output "human format shows milestones with dates and slack"
    exit_code    "exit 0 on success"
    cycle_safe   "returns empty path with message if milestone cycles exist"
  }
  verify unit "product:critical-path returns CriticalPathPayload"
  verify unit "empty graph returns empty path"
  verify unit "cycles return empty path with diagnostic message"
}

// ---------------------------------------------------------------------------
// v1.1 CLI surfaces — release, ownership, effort
// ---------------------------------------------------------------------------

behavior surface_list_releases "List Releases CLI" {
  category command
  types    [ReleaseListResult, ReleaseListEntry, ProductListFilter]
  contract """
    The specforge product releases command MUST list all release entities
    with the shared list contract: pagination, filtering and sorting.
    Wasm export: cmd__product_releases.
    MCP tool: specforge.product.releases.
  """
  ensures {
    pagination "offset/limit/has_more computed correctly"
    filtering  "status and tags filters applied before pagination"
  }
  verify unit "list-releases returns all releases with default pagination"
  verify unit "list-releases --status=released filters correctly"
  verify unit "list-releases --format=json returns valid JSON"
}

behavior surface_release_completion "Release Completion CLI" {
  category command
  types    [ReleaseCompletionPayload, ProductSurfaceError]
  contract """
    The specforge product release-completion <release> command MUST return
    the aggregate completion status of a release.
    Wasm export: cmd__product_release_completion.
    MCP tool: specforge.product.release_completion.
  """
  verify unit "release-completion returns correct shipped/total ratio"
}

behavior surface_owner_workload "Owner Workload CLI" {
  category command
  types    [OwnerWorkloadPayload, OwnerWorkloadEntry, PaginationMetadata]
  contract """
    The specforge product owner-workload command MUST return aggregate
    ownership statistics across features, milestones, deliverables,
    and releases, grouped by owner, one page of owners per the shared
    offset/limit contract.
    Wasm export: cmd__product_owner_workload.
    MCP tool: specforge.product.owner_workload.
  """
  verify unit "owner-workload returns grouped ownership statistics"
  verify unit "owner-workload reports unowned entities"
}

behavior surface_weighted_milestone_completion "Weighted Milestone Completion CLI" {
  category command
  types    [WeightedMilestoneCompletionPayload, ProductSurfaceError]
  contract """
    The specforge product weighted-milestone-completion <milestone> command
    MUST return the effort-weighted completion for the specified milestone,
    with the fixed weights xs=1, s=2, m=3, l=5, xl=8.
    Wasm export: cmd__product_weighted_milestone_completion.
    MCP tool: specforge.product.weighted_milestone_completion.
  """
  verify unit "weighted-milestone-completion returns effort breakdown"
  verify unit "weighted-milestone-completion with unknown ID returns ENTITY_NOT_FOUND"
}

// ════════════════════════════════════════════════════════════════
// CLI Commands — Term Analytics
// ════════════════════════════════════════════════════════════════

behavior surface_term_clusters "Surface: Term Clusters" {
  category command
  types    [TermClusterPayload, TermCluster, ProductSurfaceError]
  contract """
    The specforge product term-clusters command MUST return connected
    components in the TermReferencesRelatedTerm subgraph. Under --format
    human it shows cluster ID, term count and term IDs in a table with a
    header row, then cluster_count and isolated_count.
    Wasm export: cmd__product_term_clusters.
    MCP tool: specforge.product.term_clusters.
  """
  ensures {
    delegates    "delegates to pe_query_term_clusters"
    json_output  "json format returns full TermClusterPayload"
    human_output "human format shows per-cluster term lists"
    exit_code    "exit 0 on success"
  }
  verify unit "product:term-clusters returns TermClusterPayload"
  verify unit "no terms returns zero clusters and zero isolated"
}

behavior surface_term_density "Surface: Term Density" {
  category command
  types    [TermDensityPayload, ProductSurfaceError]
  contract """
    The specforge product term-density command MUST return connectivity
    statistics for the TermReferencesRelatedTerm subgraph. Under --format
    human it shows total terms, edges, average connections, hub count and
    isolated count.
    Wasm export: cmd__product_term_density.
    MCP tool: specforge.product.term_density.
  """
  ensures {
    delegates    "delegates to pe_query_term_density"
    json_output  "json format returns full TermDensityPayload"
    human_output "human format shows connectivity statistics"
    exit_code    "exit 0 on success"
  }
  verify unit "product:term-density returns TermDensityPayload"
  verify unit "empty graph returns zero stats"
}

// ════════════════════════════════════════════════════════════════
// CLI Commands — Module Analytics
// ════════════════════════════════════════════════════════════════

behavior surface_module_dependency_depth "Surface: Module Dependency Depth" {
  category command
  types    [ModuleDependencyDepthPayload, ProductSurfaceError]
  contract """
    The specforge product module-depth <module> command MUST return the
    longest dependency chain from a module. Under --format human it shows
    the depth and the chain's modules.
    Wasm export: cmd__product_module_depth.
    MCP tool: specforge.product.module_depth.
  """
  ensures {
    delegates       "delegates to pe_query_module_dependency_depth"
    json_output     "json format returns full ModuleDependencyDepthPayload"
    human_output    "human format shows depth and chain"
    not_found_error "missing module returns ProductSurfaceError with suggestion and exit code 1"
    exit_code       "exit 0 on success, 1 on ENTITY_NOT_FOUND, 2 on INVALID_INPUT (surface_error_handling)"
  }
  verify unit "product:module-depth returns ModuleDependencyDepthPayload"
  verify unit "missing module returns error with suggestion"
}

behavior surface_module_coupling "Surface: Module Coupling" {
  category command
  types    [ModuleCouplingPayload, ModuleCouplingEntry, PaginationMetadata, ProductSurfaceError]
  contract """
    The specforge product module-coupling command MUST return fan-in/fan-out
    coupling metrics for all modules, sorted by coupling descending, one
    page of modules per the shared offset/limit contract. Under --format
    human it shows module ID, fan_in, fan_out and coupling in a table with
    a header row.
    Wasm export: cmd__product_module_coupling.
    MCP tool: specforge.product.module_coupling.
  """
  ensures {
    delegates    "delegates to pe_query_module_coupling"
    json_output  "json format returns full ModuleCouplingPayload"
    human_output "human format shows per-module coupling metrics"
    paged        "modules are paged by offset and limit, with total and has_more"
    exit_code    "exit 0 on success"
  }
  verify unit "product:module-coupling returns ModuleCouplingPayload"
  verify unit "empty graph returns empty modules array"
}

// ════════════════════════════════════════════════════════════════
// CLI Command — Channel Coverage Matrix
// ════════════════════════════════════════════════════════════════

behavior surface_channel_coverage_matrix "Surface: Channel Coverage Matrix" {
  category command
  types    [
    ChannelCoverageMatrixPayload,
    ChannelCoverageEntry,
    PaginationMetadata,
    ProductSurfaceError,
  ]
  contract """
    The specforge product channel-coverage-matrix command MUST return the
    channel coverage matrix showing feature reachability per channel, one
    page of channels per the shared offset/limit contract. Under --format
    human it shows channel ID, reachable count, unreachable count and
    coverage ratio in a table with a header row, then the overall
    coverage. Symmetric counterpart to coverage-matrix (persona).
    Wasm export: cmd__product_channel_coverage_matrix.
    MCP tool: specforge.product.channel_coverage_matrix.
  """
  ensures {
    delegates    "delegates to pe_query_channel_coverage_matrix"
    json_output  "json format returns full ChannelCoverageMatrixPayload"
    human_output "human format shows per-channel coverage"
    paged        "channels are paged by offset and limit, with total and has_more"
    exit_code    "exit 0 on success"
  }
  verify unit "product:channel-coverage-matrix returns ChannelCoverageMatrixPayload"
  verify unit "no channels returns empty matrix"
}

// ════════════════════════════════════════════════════════════════
// CLI Commands — Project-wide summaries (built)
// ════════════════════════════════════════════════════════════════

behavior surface_bulk_status "Surface: Bulk Status" {
  category command
  types    [BulkStatusPayload, ProductSurfaceError]
  contract """
    The specforge product bulk-status command MUST count, for each product
    kind with a lifecycle status that has entities in the graph (feature,
    milestone, deliverable, persona, channel, release, in that order), how
    many entities have each status, "(none)" for those without one,
    statuses sorted by name. It takes no args.
    Wasm export: cmd__product_bulk_status.
    MCP tool: specforge.product.bulk_status.
  """
  ensures {
    json_output "json format returns a BulkStatusPayload"
    totals      "each kind's total is the sum of its status counts"
    exit_code   "exit 0 on success"
  }
  verify unit "bulk-status counts each kind's entities by status"
}

behavior surface_health "Surface: Project Health" {
  category command
  types    [HealthPayload, ProductSurfaceError]
  contract """
    The specforge product health command MUST report a 0 to 100 project
    health score, the mean of coverage (product entities something
    references), connectivity (edge density) and completeness (features
    with a status, milestones with references), with the entity counts per
    product kind, the unreferenced counts per kind that has entities, and the
    completeness counts. It takes no args.
    Wasm export: cmd__product_health.
    MCP tool: specforge.product.health.
  """
  ensures {
    json_output   "json format returns a HealthPayload"
    score_bounded "every score is in [0, 100]; overall is the mean of the three"
    exit_code     "exit 0 on success"
  }
  verify unit "health reports the score, counts and unreferenced entities"
}
