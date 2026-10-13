// @specforge/product extension types — entity kind shapes

use "types/core"

// ── Enum Types ─────────────────────────────────────────────

type Priority {
  values [critical, high, medium, low]
  verify property "Priority"
}

// Absent status is treated as proposed for all validation and query logic.
// This supports incremental adoption — features work without lifecycle fields.
type FeatureStatus {
  values [proposed, accepted, in_progress, done, deferred, deprecated]
  verify property "FeatureStatus"
}

type TechnicalLevel {
  values [expert, advanced, intermediate, beginner, non_technical]
  verify property "TechnicalLevel"
}

type InteractionModel {
  values [request_response, event_driven, batch, streaming, bidirectional, manual]
  verify property "InteractionModel"
}

type PersonaStatus {
  values [active, deprecated]
  verify property "PersonaStatus"
}

type ChannelStatus {
  values [active, deprecated]
  verify property "ChannelStatus"
}

type ArtifactType {
  values [cli, service, library, web_app, mobile_app, api, extension, documentation, package]
  verify property "ArtifactType"
}

// Module family is an open enum with a recommended standard set. A value
// outside it produces I062 (info).
type ModuleFamily {
  values [core, platform, extension, integration, advisory]
  verify property "ModuleFamily"
}

// Absent status is treated as draft for all validation and query logic.
// This supports incremental adoption — deliverables work without lifecycle fields.
type DeliverableStatus {
  values [draft, in_progress, shipped, deprecated]
  verify property "DeliverableStatus"
}

// Absent status is treated as planned for all validation and query logic.
// This supports incremental adoption — milestones work without lifecycle fields.
type MilestoneStatus {
  values [planned, in_progress, completed, blocked]
  verify property "MilestoneStatus"
}

// Effort uses t-shirt sizes whose weights are the scale's definition,
// Fibonacci-inspired and fixed (xs=1, s=2, m=3, l=5, xl=8); no project
// configuration changes them (ADR 0011).
type Effort "Effort T-Shirt Size" {
  values [xs, s, m, l, xl]
  verify property "Effort T-Shirt Size"
}

// Absent status is treated as planned for all validation and query logic.
// This supports incremental adoption — releases work without lifecycle fields.
type ReleaseStatus "Release Status" {
  values [planned, in_progress, released, recalled]
  verify property "Release Status"
}

// ── Entity Kind Shapes ───────────────────────────────────────

type ProductFeature {
  description  string        @optional
  problem      string
  solution     string        @optional
  priority     Priority      @optional
  // Absent status is treated as proposed (see FeatureStatus definition).
  status       FeatureStatus @optional // absent: the declared default (proposed)
  acceptance   string[]      @optional
  depends_on   EntityId[]    @optional
  features     EntityId[]    @optional
  reason       string        @optional
  owner        string        @optional
  contributors string[]      @optional
  effort       Effort        @optional
  tests        string[]      @optional
  tags         string[]      @optional
  verify property "ProductFeature"
}

type ProductJourney {
  persona     EntityId   @optional
  description string     @optional
  channels    EntityId[] @optional
  features    EntityId[] @optional
  flow        string[]
  priority    Priority   @optional
  tags        string[]   @optional
  verify property "ProductJourney"
}

type ProductDeliverable {
  description   string            @optional
  artifact_type ArtifactType
  // Absent status is treated as draft (see DeliverableStatus definition).
  status        DeliverableStatus @optional // absent: the declared default (draft)
  journeys      EntityId[]        @optional
  modules       EntityId[]        @optional
  // Semantic Versioning 2.0.0 (semver.org). Core format: MAJOR.MINOR.PATCH.
  // Pre-release tags (e.g., 1.0.0-alpha.1) and build metadata (e.g.,
  // 1.0.0+build.42) are valid. Non-conforming values produce I061.
  version       string            @optional
  milestones    EntityId[]        @optional
  depends_on    EntityId[]        @optional
  reason        string            @optional
  owner         string            @optional
  contributors  string[]          @optional
  tags          string[]          @optional
  verify property "ProductDeliverable"
}

type ProductMilestone {
  description   string          @optional
  // Absent status is treated as planned (see MilestoneStatus definition).
  status        MilestoneStatus @optional // absent: the declared default (planned)
  features      EntityId[]      @optional
  // Each entry is a free-text criterion. Entries referencing entity IDs
  // (e.g., "all features in [some_feature] done") are "anchored" and
  // enable automated verification.
  exit_criteria string[]        @optional
  // ISO 8601 date format: YYYY-MM-DD. A value not matching ^\d{4}-\d{2}-\d{2}$
  // produces I053. Absent target_date is valid (incremental adoption).
  target_date   string          @optional
  // ISO 8601 date format: YYYY-MM-DD, no later than target_date. A value
  // not in YYYY-MM-DD shape produces I087; no rule checks the order.
  start_date    string          @optional
  modules       EntityId[]      @optional
  depends_on    EntityId[]      @optional
  // Free-text descriptions of external impediments (regulatory, third-party,
  // hiring). Unlike depends_on which references milestones, blockers describe
  // external factors. I060 reports a blocked milestone without blockers.
  blockers      string[]        @optional
  priority      Priority        @optional
  reason        string          @optional
  owner         string          @optional
  contributors  string[]        @optional
  tags          string[]        @optional
  verify property "ProductMilestone"
}

type ProductModule {
  // Standard values: core, platform, extension, integration, advisory.
  // See ModuleFamily enum (other values produce I062).
  family      ModuleFamily @optional
  description string       @optional
  features    EntityId[]   @optional
  depends_on  EntityId[]   @optional
  reason      string       @optional
  tags        string[]     @optional
  verify property "ProductModule"
}

type ProductTerm {
  definition string
  context    string     @optional
  aliases    string[]   @optional
  see_also   EntityId[] @optional
  module     EntityId   @optional
  tags       string[]   @optional
  verify property "ProductTerm"
}

type ProductPersona {
  description     string
  technical_level TechnicalLevel @optional
  goals           string[]       @optional
  pain_points     string[]       @optional
  // Absent status is treated as active for all validation and query logic.
  // This supports incremental adoption — personas work without lifecycle fields.
  status          PersonaStatus  @optional // absent: the declared default (active)
  reason          string         @optional // required when status=deprecated (I069)
  key_features    EntityId[]     @optional
  tags            string[]       @optional
  verify property "ProductPersona"
}

type ProductChannel {
  description       string
  interaction_model InteractionModel @optional
  url               string           @optional
  // Absent status is treated as active for all validation and query logic.
  // This supports incremental adoption — channels work without lifecycle fields.
  status            ChannelStatus    @optional // absent: the declared default (active)
  reason            string           @optional // required when status=deprecated (I070)
  tags              string[]         @optional
  verify property "ProductChannel"
}

type ProductRelease {
  description  string        @optional
  // SemVer 2.0.0 (semver.org). Non-conforming values produce W093.
  version      string
  // Absent status is treated as planned (see ReleaseStatus definition).
  status       ReleaseStatus @optional // absent: the declared default (planned)
  deliverables EntityId[]    @optional
  milestones   EntityId[]    @optional
  target_date  string        @optional
  // ISO 8601 date format: YYYY-MM-DD. Other values produce I086.
  release_date string        @optional
  changelog    string        @optional
  depends_on   EntityId[]    @optional
  owner        string        @optional
  contributors string[]      @optional
  reason       string        @optional
  tags         string[]      @optional
  verify property "ProductRelease"
}

// ── Event Payload Types ────────────────────────────────────

type ProductValidationPayload {
  warning_count integer
  info_count    integer
  error_count   integer
  verify property "ProductValidationPayload"
}

type ProductTraceabilityPayload {
  journey_count         integer
  feature_count         integer
  reachable_features    integer
  unreferenced_features integer
  verify property "ProductTraceabilityPayload"
}

// ── Registration Event Payloads ──────────────────────────────
// Each registration phase has its own payload reflecting only what
// was registered in that phase, avoiding misleading zero-value fields.

type ProductEntityKindsRegisteredPayload {
  entity_kind_count integer
  kind_names        string[]
  verify property "ProductEntityKindsRegisteredPayload"
}

type ProductEdgeTypesRegisteredPayload {
  edge_type_count integer
  edge_type_names string[]
  verify property "ProductEdgeTypesRegisteredPayload"
}

type ProductFieldsRegisteredPayload {
  field_count       integer
  kinds_with_fields string[]
  verify property "ProductFieldsRegisteredPayload"
}

// ── Observability Event Payloads ─────────────────────────────

type ProductEntityRegistrationPayload {
  entity_kind_count integer
  edge_type_count   integer
  field_count       integer
  verify property "ProductEntityRegistrationPayload"
}

type ProductCycleDetectedPayload {
  cycle_kind    string
  cycle_members string[]
  diagnostic    string
  verify property "ProductCycleDetectedPayload"
}

type ProductDeliverableCycleDetectedPayload {
  cycle_members string[]
  diagnostic    string
  verify property "ProductDeliverableCycleDetectedPayload"
}

// ── Query Result Payloads ────────────────────────────────────

type MilestoneCompletionPayload {
  milestone_id     EntityId
  total_features   integer
  done_count       integer
  completion_ratio float             @optional
  done_features    EntityId[]        @optional
  // Whether the command's input carried recorded evidence, and what it
  // proves of the milestone's features, beside the declared count (ADR 0039).
  evidence         EvidenceState     @optional
  proven_count     integer           @optional
  proven_ratio     float             @optional
  proven_features  EntityId[]        @optional
  feature_evidence FeatureEvidence[] @optional
  verify property "MilestoneCompletionPayload"
}

// What the recorded tests prove of one feature: the behaviors that name
// it in their features field, how many the coverage rule counts proven,
// their obligations and failing tests. Proven when at least one behavior
// implements it and every one is proven.
type FeatureEvidence {
  feature_id         EntityId
  behaviors          integer
  proven_behaviors   integer
  obligations        integer
  proven_obligations integer
  failing            integer
  proven             boolean
  verify unit "a feature is proven when at least one behavior implements it and every one is proven"
}

type DeliverableTraceabilityPayload {
  deliverable_id      EntityId
  transitive_features EntityId[]
  journey_path_count  integer
  module_path_count   integer
  verify property "DeliverableTraceabilityPayload"
}

type JourneyCoveragePayload {
  journey_id         EntityId
  total_features     integer
  covered_count      integer
  uncovered_features EntityId[]
  verify property "JourneyCoveragePayload"
}

type FeatureOrderingPayload {
  sorted_features EntityId[]
  has_cycles      boolean
  cycle_members   EntityId[]
  verify property "FeatureOrderingPayload"
}

type MilestoneTimelinePayload {
  milestones    MilestoneTimelineEntry[]
  overdue_count integer
  verify property "MilestoneTimelinePayload"
}

type MilestoneTimelineEntry {
  milestone_id EntityId
  target_date  string          @optional
  status       MilestoneStatus @optional
  is_overdue   boolean
  priority     Priority        @optional
  verify property "MilestoneTimelineEntry"
}

type FeatureDeliverablePayload {
  feature_id        EntityId
  deliverables      EntityId[]
  via_journey_count integer
  via_module_count  integer
  verify property "FeatureDeliverablePayload"
}

type TermGraphPayload {
  term_id       EntityId
  related_terms EntityId[]
  max_hops      integer
  verify property "TermGraphPayload"
}

type DeliverableCompletionPayload {
  deliverable_id    EntityId
  milestone_count   integer
  completed_count   integer
  completion_ratio  float                        @optional
  milestone_details MilestoneCompletionPayload[] @optional
  verify property "DeliverableCompletionPayload"
}

type PersonaChannelPayload {
  persona_id EntityId
  channels   EntityId[]
  count      integer
  verify property "PersonaChannelPayload"
}

type FeatureDependentPayload {
  feature_id EntityId
  dependents EntityId[]
  count      integer
  verify property "FeatureDependentPayload"
}

type DeliverableDependentPayload {
  deliverable_id EntityId
  dependents     EntityId[]
  count          integer
  verify property "DeliverableDependentPayload"
}

type DeliverablePriorityPayload {
  deliverable_id EntityId
  priority       Priority @optional
  source_count   integer
  verify property "DeliverablePriorityPayload"
}

type ProductTagConsistencyPayload {
  tag        string
  entity_id  EntityId
  suggestion string @optional
  verify property "ProductTagConsistencyPayload"
}

// ── Surface I/O Types ─────────────────────────────────────────
// Input and output schemas for CLI commands.
// Every CLI command is auto-promoted to an MCP tool with the
// same input schema (JSON Schema) and output schema (JSON body).

// -- Common filter/pagination for list commands --

type ProductListSortOrder {
  values [asc, desc]
  verify property "ProductListSortOrder"
}

type ProductListFilter {
  status     string               @optional
  priority   string               @optional
  tags       string[]             @optional
  limit      integer              @optional
  offset     integer              @optional
  sort_by    string               @optional
  sort_order ProductListSortOrder @optional
  verify property "ProductListFilter"
}

type ProductEntitySummary {
  id       EntityId
  title    string
  kind     string
  status   string   @optional
  priority string   @optional
  tags     string[] @optional
  verify property "ProductEntitySummary"
}

type ProductListResult {
  entities ProductEntitySummary[]
  total    integer
  offset   integer
  limit    integer
  has_more boolean
  verify property "ProductListResult"
}

// -- Per-kind list output types --
// Extend the common summary with kind-specific key fields.

type FeatureListEntry {
  id       EntityId
  title    string
  status   FeatureStatus @optional
  priority Priority      @optional
  problem  string        @optional
  tags     string[]      @optional
  verify property "FeatureListEntry"
}

type JourneyListEntry {
  id            EntityId
  title         string
  persona       EntityId @optional
  channel_count integer
  feature_count integer
  priority      Priority @optional
  tags          string[] @optional
  verify property "JourneyListEntry"
}

type DeliverableListEntry {
  id            EntityId
  title         string
  artifact_type ArtifactType      @optional
  status        DeliverableStatus @optional
  journey_count integer
  module_count  integer
  tags          string[]          @optional
  verify property "DeliverableListEntry"
}

type MilestoneListEntry {
  id            EntityId
  title         string
  status        MilestoneStatus @optional
  target_date   string          @optional
  feature_count integer
  priority      Priority        @optional
  tags          string[]        @optional
  verify property "MilestoneListEntry"
}

type ModuleListEntry {
  id            EntityId
  title         string
  family        ModuleFamily @optional
  feature_count integer
  depends_on    EntityId[]   @optional
  tags          string[]     @optional
  verify property "ModuleListEntry"
}

type TermListEntry {
  id          EntityId
  title       string
  definition  string
  alias_count integer
  tags        string[] @optional
  verify property "TermListEntry"
}

type PersonaListEntry {
  id              EntityId
  title           string
  technical_level TechnicalLevel @optional
  status          PersonaStatus  @optional
  journey_count   integer
  tags            string[]       @optional
  verify property "PersonaListEntry"
}

type ChannelListEntry {
  id                EntityId
  title             string
  interaction_model InteractionModel @optional
  status            ChannelStatus    @optional
  journey_count     integer
  tags              string[]         @optional
  verify property "ChannelListEntry"
}

// -- Typed list results per kind --

type FeatureListResult {
  features FeatureListEntry[]
  total    integer
  offset   integer
  limit    integer
  has_more boolean
  verify property "FeatureListResult"
}

type JourneyListResult {
  journeys JourneyListEntry[]
  total    integer
  offset   integer
  limit    integer
  has_more boolean
  verify property "JourneyListResult"
}

type DeliverableListResult {
  deliverables DeliverableListEntry[]
  total        integer
  offset       integer
  limit        integer
  has_more     boolean
  verify property "DeliverableListResult"
}

type MilestoneListResult {
  milestones MilestoneListEntry[]
  total      integer
  offset     integer
  limit      integer
  has_more   boolean
  verify property "MilestoneListResult"
}

type ModuleListResult {
  modules  ModuleListEntry[]
  total    integer
  offset   integer
  limit    integer
  has_more boolean
  verify property "ModuleListResult"
}

type TermListResult {
  terms    TermListEntry[]
  total    integer
  offset   integer
  limit    integer
  has_more boolean
  verify property "TermListResult"
}

type PersonaListResult {
  personas PersonaListEntry[]
  total    integer
  offset   integer
  limit    integer
  has_more boolean
  verify property "PersonaListResult"
}

type ChannelListResult {
  channels ChannelListEntry[]
  total    integer
  offset   integer
  limit    integer
  has_more boolean
  verify property "ChannelListResult"
}

// -- Query command input types --
// CLI flags map to these fields. MCP tool inputs use the same schema.

type MilestoneCompletionInput {
  milestone EntityId
  verify property "MilestoneCompletionInput"
}

type JourneyCoverageInput {
  journey EntityId
  verify property "JourneyCoverageInput"
}

type MilestoneTimelineInput {
  // YYYY-MM-DD; defaults to the today the host passes (CommandInput.today).
  as_of string @optional
  verify property "MilestoneTimelineInput"
}

// What a command writes to stderr under --format json when it cannot
// answer: code ENTITY_NOT_FOUND (exit 1) or INVALID_INPUT (exit 2).
type ProductSurfaceError {
  code       string
  message    string
  entity_id  EntityId @optional
  suggestion string   @optional
  verify property "ProductSurfaceError"
}

// ── Error types for product extension ports ──────────────────

type ProductQueryError {
  _tag      "ProductQueryError"
  entity_id EntityId @optional
  message   string
  verify property "ProductQueryError"
}

type ProductValidationError {
  _tag    "ProductValidationError"
  message string
  verify property "ProductValidationError"
}

type RegistrationError {
  _tag      "RegistrationError"
  kind_name string @optional
  message   string
  verify property "RegistrationError"
}

// ── Persona-Feature Traversal ────────────────────────────────

type PersonaFeaturePayload {
  persona_id      EntityId
  features        EntityId[]
  via_journey_ids EntityId[]
  count           integer
  verify property "PersonaFeaturePayload"
}

// ── Feature Impact Analysis ──────────────────────────────────

type FeatureImpactPayload {
  feature_id              EntityId
  affected_journeys       EntityId[]
  affected_milestones     EntityId[]
  affected_deliverables   EntityId[]
  affected_modules        EntityId[]
  dependent_features      EntityId[]
  total_affected_entities integer
  verify property "FeatureImpactPayload"
}

// ── Milestone Velocity ───────────────────────────────────────

type MilestoneVelocityPayload {
  milestone_id         EntityId
  total_features       integer
  done_features        integer
  in_progress_features integer
  remaining_features   integer
  completion_ratio     float   @optional
  days_elapsed         integer @optional
  days_remaining       integer @optional
  features_per_day     float   @optional
  verify property "MilestoneVelocityPayload"
}

// ── Deliverable-Persona Composite Query ────────────────────

type DeliverablePersonaPayload {
  deliverable_id  EntityId
  personas        EntityId[]
  via_journey_ids EntityId[]
  count           integer
  verify property "DeliverablePersonaPayload"
}

// ── Validation Rule Observability ──────────────────────────

type ProductValidationRuleFiredPayload {
  rule_id         string
  diagnostic_code string
  severity        string
  entity_id       EntityId @optional
  message         string
  verify property "ProductValidationRuleFiredPayload"
}

type ProductValidationSummaryPayload {
  rules_evaluated  integer
  rules_fired      integer
  rules_suppressed integer
  by_severity      ProductDiagnosticCounts
  verify property "ProductValidationSummaryPayload"
}

type ProductDiagnosticCounts {
  error   integer
  warning integer
  info    integer
  verify property "ProductDiagnosticCounts"
}

// ── Rendering Types ────────────────────────────────────────

type ProductRenderPayload {
  format         string
  entity_count   integer
  edge_count     integer
  kinds_rendered string[]
  verify property "ProductRenderPayload"
}

type ProductEntityDiagnostic {
  code     string
  severity string
  message  string
  verify property "ProductEntityDiagnostic"
}

// ── Status Transition Validation ─────────────────────────────
// Valid status transitions per entity kind. No rule checks
// transitions. Absent status fields
// are treated as the default value and are always valid starting points.

type FeatureStatusTransition {
  // proposed -> accepted | deferred
  // accepted -> in_progress | deferred
  // in_progress -> done | deferred
  // deferred -> proposed | accepted
  // done -> deprecated
  // deprecated is terminal (no outbound transitions)
  from FeatureStatus
  to   FeatureStatus
  verify property "FeatureStatusTransition"
}

type MilestoneStatusTransition {
  // planned -> in_progress | blocked
  // in_progress -> completed | blocked
  // blocked -> planned | in_progress
  // completed is terminal
  from MilestoneStatus
  to   MilestoneStatus
  verify property "MilestoneStatusTransition"
}

type DeliverableStatusTransition {
  // draft -> in_progress
  // in_progress -> shipped | draft
  // shipped -> deprecated
  // deprecated is terminal
  from DeliverableStatus
  to   DeliverableStatus
  verify property "DeliverableStatusTransition"
}

type PersonaStatusTransition {
  // active -> deprecated
  // deprecated is terminal
  from PersonaStatus
  to   PersonaStatus
  verify property "PersonaStatusTransition"
}

type ChannelStatusTransition {
  // active -> deprecated
  // deprecated is terminal
  from ChannelStatus
  to   ChannelStatus
  verify property "ChannelStatusTransition"
}

type ReleaseStatusTransition {
  // planned -> in_progress
  // in_progress -> released
  // released -> recalled
  // recalled is terminal
  from ReleaseStatus
  to   ReleaseStatus
  verify property "ReleaseStatusTransition"
}

type StatusTransitionViolation {
  entity_id     EntityId
  entity_kind   string
  from_status   string @optional
  to_status     string
  valid_targets string[]
  verify property "StatusTransitionViolation"
}

// ── Unscheduled Features Query ──────────────────────────────

type UnscheduledFeaturesPayload {
  features        EntityId[]
  count           integer
  total_features  integer
  scheduled_count integer
  verify property "UnscheduledFeaturesPayload"
}

// ── Cross-Deliverable Feature Overlap ───────────────────────

type FeatureOverlapEntry {
  feature_id        EntityId
  deliverable_ids   EntityId[]
  deliverable_count integer
  verify property "FeatureOverlapEntry"
}

type FeatureOverlapPayload {
  overlapping_features FeatureOverlapEntry[]
  count                integer
  total_features       integer
  total                integer
  offset               integer
  limit                integer
  has_more             boolean
  verify property "FeatureOverlapPayload"
}

// ── Persona Coverage Matrix ─────────────────────────────────

type PersonaCoverageEntry {
  persona_id           EntityId
  reachable_features   EntityId[]
  unreachable_features EntityId[]
  coverage_ratio       float @optional
  journey_count        integer
  verify property "PersonaCoverageEntry"
}

type PersonaCoverageMatrixPayload {
  personas         PersonaCoverageEntry[]
  total_features   integer
  overall_coverage float @optional
  total            integer
  offset           integer
  limit            integer
  has_more         boolean
  verify property "PersonaCoverageMatrixPayload"
}

// ── Channel Coverage Matrix ─────────────────────────────────

type ChannelCoverageEntry {
  channel_id           EntityId
  reachable_features   EntityId[]
  unreachable_features EntityId[]
  coverage_ratio       float @optional
  journey_count        integer
  verify property "ChannelCoverageEntry"
}

type ChannelCoverageMatrixPayload {
  channels         ChannelCoverageEntry[]
  total_features   integer
  overall_coverage float @optional
  total            integer
  offset           integer
  limit            integer
  has_more         boolean
  verify property "ChannelCoverageMatrixPayload"
}

// ── Critical Path Analysis ──────────────────────────────────

type CriticalPathNode {
  entity_id   EntityId
  entity_kind string
  target_date string  @optional
  status      string  @optional
  slack_days  integer @optional
  verify property "CriticalPathNode"
}

type CriticalPathPayload {
  critical_path       CriticalPathNode[]
  path_length         integer
  earliest_completion string @optional
  latest_completion   string @optional
  bottleneck_ids      EntityId[]
  // Why the path is empty when milestones depend on each other in a cycle.
  message             string @optional
  verify property "CriticalPathPayload"
}

// ── v1.1 Payload Types — ownership, effort, release ──────────

type OwnerKindBreakdown {
  features     integer
  milestones   integer
  deliverables integer
  releases     integer
  verify property "OwnerKindBreakdown"
}

type OwnerWorkloadEntry {
  owner        string
  entity_ids   EntityId[]
  entity_count integer
  by_kind      OwnerKindBreakdown
  verify property "OwnerWorkloadEntry"
}

type OwnerWorkloadPayload {
  owners         OwnerWorkloadEntry[]
  unowned_count  integer
  total_entities integer
  total          integer
  offset         integer
  limit          integer
  has_more       boolean
  verify property "OwnerWorkloadPayload"
}

type EffortBreakdownEntry {
  effort_level Effort
  total        integer
  done         integer
  verify property "EffortBreakdownEntry"
}

type WeightedMilestoneCompletionPayload {
  milestone_id     EntityId
  total_effort     integer
  done_effort      integer
  completion_ratio float @optional
  effort_breakdown EffortBreakdownEntry[]
  verify property "WeightedMilestoneCompletionPayload"
}

type ReleaseCompletionPayload {
  release_id       EntityId
  total            integer
  shipped          integer
  completion_ratio float @optional
  verify property "ReleaseCompletionPayload"
}

type ReleaseListEntry {
  id                EntityId
  title             string        @optional
  version           string        @optional
  status            ReleaseStatus @optional
  deliverable_count integer
  release_date      string        @optional
  tags              string[]      @optional
  verify property "ReleaseListEntry"
}

type ReleaseListResult {
  releases ReleaseListEntry[]
  total    integer
  offset   integer
  limit    integer
  has_more boolean
  verify property "ReleaseListResult"
}

// ── Term Analytics ──────────────────────────────────────────

type TermCluster {
  cluster_id integer
  term_ids   EntityId[]
  term_count integer
  verify property "TermCluster"
}

type TermClusterPayload {
  clusters       TermCluster[]
  cluster_count  integer
  isolated_count integer
  total_terms    integer
  verify property "TermClusterPayload"
}

type TermDensityPayload {
  total_terms     integer
  total_see_also  integer
  avg_connections float @optional
  max_connections integer
  hub_terms       EntityId[]
  isolated_terms  EntityId[]
  verify property "TermDensityPayload"
}

// ── Module Analytics ───────────────────────────────────────

type ModuleDependencyDepthPayload {
  module_id     EntityId
  depth         integer
  longest_chain EntityId[]
  verify property "ModuleDependencyDepthPayload"
}

type ModuleCouplingEntry {
  module_id EntityId
  fan_in    integer
  fan_out   integer
  coupling  integer
  verify property "ModuleCouplingEntry"
}

type ModuleCouplingPayload {
  modules         ModuleCouplingEntry[]
  avg_fan_in      float    @optional
  avg_fan_out     float    @optional
  most_coupled_id EntityId @optional
  total_modules   integer
  total           integer
  offset          integer
  limit           integer
  has_more        boolean
  verify property "ModuleCouplingPayload"
}

// ── Channel-Feature Traversal ──────────────────────────────

type ChannelFeaturePayload {
  channel_id      EntityId
  features        EntityId[]
  via_journey_ids EntityId[]
  count           integer
  verify property "ChannelFeaturePayload"
}

// ── Offset Pagination ─────────────────────────────────────────
// The one pagination contract (ADR 0011): the list commands and the
// matrix queries take --offset (integer, default 0) and --limit
// (integer, default 100, clamped to [1, 1000]) and report where the page
// sits. There are no cursors.

type PaginationMetadata {
  // Entries after filters, before paging.
  total    integer
  offset   integer
  limit    integer
  // offset + entries on this page < total.
  has_more boolean
  verify property "PaginationMetadata"
}

// ── Project-wide summaries ───────────────────────────────────

type StatusCount {
  status string
  count  integer
  verify property "StatusCount"
}

type KindStatusCounts {
  kind      string
  total     integer
  by_status StatusCount[]
  verify property "KindStatusCounts"
}

// bulk-status: one entry per lifecycle kind with entities, in the order
// feature, milestone, deliverable, persona, channel, release.
type BulkStatusPayload {
  kinds KindStatusCounts[]
  verify property "BulkStatusPayload"
}

type HealthScore {
  // Each in [0, 100]; overall is the mean of the other three.
  overall      float
  coverage     float
  connectivity float
  completeness float
  verify property "HealthScore"
}

type KindCount {
  kind  string
  count integer
  verify property "KindCount"
}

type KindUnreferencedCount {
  kind         string
  unreferenced integer
  total        integer
  verify property "KindUnreferencedCount"
}

type HealthCompleteness {
  features_with_status     integer
  features_total           integer
  milestones_with_features integer
  milestones_total         integer
  verify property "HealthCompleteness"
}

type HealthPayload {
  score               HealthScore
  entity_counts       KindCount[]
  unreferenced_counts KindUnreferencedCount[]
  completeness        HealthCompleteness
  verify property "HealthPayload"
}
