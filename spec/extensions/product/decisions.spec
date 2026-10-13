// @specforge/product Architecture Decision Records

use "extensions/product/invariants"

decision pe_nine_entity_kinds "Nine Entity Kinds" {
  status       accepted
  date         2026-03-12
  context      """
    The @specforge/product extension needs to model the full planning-to-delivery
    chain for any domain. RES-27 validated the entity set through a 10-expert panel.
    The chain must cover what to build (feature), who uses it (persona), through what
    medium (channel), how they experience it (journey), what structurally
    composes it (module), when it ships (milestone), what artifact delivers it
    (deliverable), shared vocabulary (term), and coordinated shipping (release).
    All 9 are domain-neutral — they work for healthcare, real estate, education,
    and any other domain.
  """
  decision     """
    Register 9 entity kinds: feature, journey, deliverable, milestone, module,
    term, persona, channel, release. All have testable=false — they are planning
    constructs tested indirectly through behavior chains from domain extensions.
    Connected by 20 edge types forming a complete traceability graph from user
    need to shipped artifact. Release was added in v1.1 to coordinate
    multi-deliverable shipping with ReleaseIncludesDeliverable,
    ReleaseCompletesMilestone and ReleaseDependsOn edges.
  """
  consequences [
    "Complete planning-to-delivery traceability chain",
    "Domain-neutral — works for any industry",
    "All 9 kinds are non-testable planning constructs",
    "20 edge types cover all structural relationships",
    "Validated by RES-27 10-expert panel consensus",
    "Release entity enables coordinated multi-deliverable shipping",
  ]
}

decision pe_persona_channel_as_entities "Persona and Channel as First-Class Entities" {
  status       accepted
  date         2026-03-10
  context      """
    Three design options were considered for persona and channel:
    - Option A: Bare identifiers (strings in journey fields)
    - Option B: Config enums (defined in specforge.json)
    - Option C: First-class entity kinds

    Option A provides no validation — typos in persona names go undetected.
    Option B provides validation but no graph addressability — cannot query
    "which journeys reference this persona?" without special-case code.
    Option C makes personas and channels full graph nodes with edges,
    enabling unreferenced detection, cross-reference validation, and standard
    graph queries.
  """
  decision     """
    Declare persona and channel as first-class entity kinds in
    @specforge/product. Journeys reference them via JourneyTargetsPersona and
    JourneyUsesChannel edges. Both are testable=false. Unreferenced detection
    via I046 (persona) and I047 (channel) at info level to support
    incremental adoption.
  """
  consequences [
    "Personas and channels are graph-addressable nodes",
    "Reusable across journeys without duplication",
    "Unreferenced detection catches unused personas/channels",
    "The core validates references at compile time (E003 unresolved, E022 wrong kind)",
    "Info-level unreferenced diagnostics support incremental adoption",
    "Slight overhead vs bare identifiers — justified by validation value",
  ]
}

decision pe_feature_domain_neutral "Feature as Domain-Neutral Product Concept" {
  status       accepted
  date         2026-03-10
  context      """
    Features could be scoped to a single domain (with fields like
    complexity, story_points, sprint) or kept domain-neutral (problem/solution
    only). The @specforge/product extension serves all domains — healthcare,
    real estate, education, nonprofits — not just one.
  """
  decision     """
    Feature entities use domain-neutral problem/solution framing only.
    No domain-specific fields in the base feature type. Domain extensions
    reference features via their own fields and peer dependencies, creating
    cross-extension edges. Product features work standalone.
  """
  consequences [
    "Features work for any domain without any peer extension",
    "Cross-extension linkage is opt-in via peer_dependency",
    "No domain-specific fields pollute the base feature type",
    "All domains get full product planning without irrelevant fields",
  ]
}

decision pe_all_non_testable "All Product Entities Non-Testable" {
  status       accepted
  date         2026-03-10
  context      """
    Testability in SpecForge means an entity can have verify statements and
    generate test obligations tracked in specforge-report.json. Product entities
    (feature, journey, deliverable, milestone, module, term, persona, channel)
    are planning constructs — they describe what to build and when, not how
    the system behaves.
  """
  decision     """
    All 9 product entity kinds declare testable=false in the manifest.
    Product entities are tested indirectly through the behavior chains
    from domain extensions that reference features. Direct testability
    would create false obligations on
    planning constructs that have no executable contract.
  """
  consequences [
    "No verify statements on product entities",
    "No test obligations generated for planning constructs",
    "Testing occurs via domain extensions that reference features",
    "W017 (testable kind lacks verify support) never fires for product kinds",
    "Simpler mental model — product entities plan, domain entities execute",
  ]
  invariants   [pe_feature_non_testable, pe_persona_non_testable, pe_channel_non_testable]
}

decision pe_term_see_also_term_only "TermReferencesRelatedTerm Edge Restricted to Term-to-Term" {
  status       accepted
  date         2026-03-10
  context      """
    The term entity's see_also field accepts EntityId[]. It could accept
    references to any entity kind, with graph edges only for term-to-term
    references, or accept terms only.
  """
  decision     """
    Only term-to-term references in see_also produce TermReferencesRelatedTerm edges.
    Cross-kind references are rejected (E022): see_also accepts only terms.
    Terms are navigation aids for
    vocabulary consistency, not dependency-tracking nodes. Cross-kind
    edges from terms would pollute dependency analysis, cycle detection,
    and graph traversal with non-structural relationships.
  """
  consequences [
    "TermReferencesRelatedTerm edges form a clean term-only navigation graph",
    "Cross-kind see_also references are E022 errors",
    "No pollution of dependency analysis with vocabulary links",
    "Graph queries on TermReferencesRelatedTerm return only term neighbors",
    "Agents can build term relationship maps without noise",
    "Cross-kind documentation links use the standard refs field instead of see_also",
  ]
}

decision pe_acceptance_free_form "Acceptance Criteria as Free-Form Prose" {
  status       accepted
  date         2026-03-10
  context      """
    Feature acceptance criteria could be structured (Given/When/Then),
    machine-parseable checklists, or free-form prose. Structured formats
    would enable automated verification but impose domain-specific
    assumptions. SpecForge is domain-neutral — acceptance criteria for
    a healthcare workflow differ fundamentally from those for a CLI tool.
  """
  decision     """
    The acceptance field on features is free-form string[] — each entry
    is a prose acceptance criterion with no enforced structure. Machine
    verification of feature completeness belongs to behavior verify
    statements, not feature acceptance criteria. This preserves domain
    neutrality: any domain can express acceptance in its own vocabulary.
  """
  consequences [
    "Acceptance criteria work for any domain without format constraints",
    "No machine-parseable structure for automated acceptance checking",
    "Verification occurs at the behavior level via verify statements",
    "No rule checks the criteria's content; I048 only reports a feature that has none",
    "Domain neutrality preserved — healthcare, finance, and education all use natural language",
  ]
}

decision pe_interaction_model_metadata "Interaction Model as Metadata" {
  status       accepted
  date         2026-03-10
  context      """
    The channel entity's interaction_model field classifies how users interact
    through that channel (request_response, event_driven, batch, streaming,
    bidirectional, manual). This could be used as a validation constraint
    (e.g., verifying that journey flow steps match the channel's interaction
    model) or as passive metadata for tooling and agents.
  """
  decision     """
    The interaction_model field is metadata for tooling and AI agents, not
    a flow-step validation constraint. interaction_model is an enum of
    InteractionModel (a value outside it is E061), and the compiler does
    NOT enforce that journey flow steps are compatible with the
    referenced channel's interaction model. Flow-step compatibility is
    domain-specific and belongs to domain extensions or agent reasoning.
  """
  consequences [
    "Interaction model is queryable metadata for agents and tooling",
    "No coupling between journey flow steps and channel interaction model",
    "Domain extensions can add flow-step validation if needed",
    "The value is one of InteractionModel; nothing else is checked about it",
    "Agents can use interaction_model to reason about UX patterns",
  ]
}

decision pe_surface_contributions_v1 "Surface Contributions in v1" {
  status       deprecated
  date         2026-03-10
  context      """
    The @specforge/product extension has query behaviors and validation but no
    declared CLI commands or MCP tools. Users and agents cannot invoke product
    queries without surface contributions.
  """
  decision     """
    Declare 14 CLI commands in the manifest surfaces section. Each command is
    auto-promoted to an MCP tool. Commands cover entity listing for all 9 kinds
    plus milestone-completion, journey-coverage, feature-ordering,
    milestone-timeline, milestone-deliverables, and module-features queries.
    Remaining query-port methods are accessible via the programmatic
    ProductQueryPort API.
  """
  consequences [
    "Users can invoke product queries from CLI",
    "Agents can invoke product queries via MCP tools",
    "27 commands in v1.1 — 9 list, 6 query, 5 planning insights, 6 v1.1 additions",
  ]
}

decision pe_completion_via_status "Milestone Completion via Feature Status" {
  status       accepted
  date         2026-03-10
  context      """
    Milestone completion could be computed by counting edges from peer
    extensions or by reading the feature status field. The status field
    is product-native and does not require any peer extension.
  """
  decision     """
    Milestone completion uses the FeatureStatus field: done_count / total_features.
    Features with status=done are complete. No cross-extension edges are needed.
  """
  consequences [
    "Product extension is fully standalone for completion queries",
    "No dependency on any peer extension",
    "Feature status field must be maintained by spec authors",
  ]
}

decision pe_no_migration_hook_v1 "No Migration Hook in v1" {
  status       accepted
  date         2026-03-10
  context      """
    Migration hooks allow extensions to transform spec files between versions.
    The @specforge/product entity model is new — there is no prior version.
  """
  decision     """
    Omit migration_hook from the v1 manifest. Add migration hooks when
    the entity model evolves in a breaking way.
  """
  consequences [
    "Simpler v1 manifest — no unused migration infrastructure",
    "Migration hooks added when needed, not speculatively",
  ]
}

decision pe_tags_on_all_kinds "Tags Field on All Entity Kinds" {
  status       accepted
  date         2026-03-10
  context      """
    Tags enable cross-cutting categorization (e.g., "mvp", "security", "deferred").
  """
  decision     """
    All 9 product entity kinds get the tags field (string[] @optional) as a
    shared field.
  """
  consequences [
    "Cross-cutting categorization works on every product entity",
    "Slight field overhead on every entity — justified by cross-cutting value",
  ]
  invariants   [pe_tags_per_entity_kind]
}

decision pe_reverse_query_symmetry "Reverse Query Symmetry" {
  status       superseded
  date         2026-03-10
  context      """
    Forward queries exist (milestone->features, journey->features) but reverse
    queries (feature->milestones, persona->journeys) are missing. Agents need
    both directions for complete graph navigation.
  """
  decision     """
    Superseded by ADR 0011: one-hop lookups are core's
    `specforge query <id> --depth 1 --kind <kind>` (and MCP
    specforge.query / specforge.find_references), not product commands.
    term_graph (N-hop over see_also) stays a product command.

    Add 5 reverse query behaviors: feature milestones, persona journeys,
    channel journeys, module deliverables, and term graph (N-hop). Each
    reverse query traverses the corresponding edge type in reverse direction.
  """
  consequences [
    "Symmetric query capability — forward and reverse for all major edges",
    "Agents can navigate the graph in any direction",
    "5 additional ProductQueryPort methods",
    "All reverse queries return results sorted alphabetically by entity ID for deterministic ordering",
  ]
}

decision pe_journey_description_field "Journey Description Field" {
  status       accepted
  date         2026-03-10
  context      """
    Journey entities have no description field. Persona and channel have
    descriptions, but journeys rely on the flow steps for narrative context.
    A free-form description provides a summary without reading flow steps.
  """
  decision     """
    Add an optional description field (string) to the ProductJourney type.
    This provides a summary of the journey's purpose without requiring
    agents to parse individual flow steps.
  """
  consequences [
    "Journeys have a human/agent-readable summary",
    "Description is optional — incremental adoption preserved",
    "Agents can use description for context without parsing flow",
  ]
}

decision pe_i058_query_time_only "I058 Overdue Detection as Query-Time Only" {
  status       superseded
  date         2026-03-13
  context      """
    The original decision (pe_i058_inline_diagnostic) had I058 produced inline
    by pe_query_milestone_timeline during the validation phase. This made
    specforge check depend on wall-clock time: the same spec file could produce
    different diagnostics depending on when compilation ran. This violated the
    manifesto's "deterministic infrastructure" commitment.
  """
  decision     """
    I058 overdue markers are produced ONLY at query time, NOT during
    specforge check. The milestone timeline query flags overdue milestones
    via is_overdue=true in the result payload and includes an overdue_count.
    No I058 diagnostic is emitted to the validation diagnostic set.

    This preserves deterministic compilation: specforge check produces
    identical diagnostics regardless of when it runs. Overdue detection
    is available via the specforge product milestone-timeline command
    (and its MCP tool), whose --as-of defaults to the date the host
    passes (CommandInput.today, ADR 0011).
  """
  consequences [
    "specforge check is fully deterministic — no time-dependent diagnostics",
    "I058 appears only in timeline query results, not in validation output",
    "as_of_date parameter enables reproducible overdue detection",
    "No side effects from query methods — pure data return",
    "pe_validation_complete no longer chains to pe_query_milestone_timeline",
  ]
}

decision pe_diagnostic_code_gaps "Intentional Diagnostic Code Gaps" {
  status       accepted
  date         2026-03-10
  context      """
    Product's warning codes are W041-W046, W049, W057, W092 and W093
    (W077-W080, W083-W085 and W095 are retired to E061); its info codes are I010, I046-I048,
    I050, I053-I055, I057, I059-I062, I066-I070, I080-I083, I086, I087 and
    I089. Of the warning codes between them, W050-W052 are retired,
    W053, W054 and W060-W062 are core's, and W063 is retired; W047, W048,
    W055, W056, W058, W064-W076, W081, W082 and W086-W091 are
    unassigned. These gaps
    exist because codes were assigned incrementally as validation rules
    were added, not in a pre-allocated block.
  """
  decision     """
    Diagnostic code gaps are intentional and reserved for future validation rules.
    No existing code will be reassigned. New validation rules receive the next
    available code in the appropriate range (E, W, or I).
  """
  consequences [
    "Future validators can fill gaps without renumbering",
    "External tools must not assume contiguous code ranges",
    "Code registry documents all assigned codes",
  ]
}

decision pe_validation_naming_convention "Validation Behavior Naming Convention" {
  status       accepted
  date         2026-03-10
  context      """
    Validation-rules.spec behaviors use two naming prefixes: detect_* for
    structural checks (unreferenced entities, cycles, gaps, singleton tags) and validate_*
    for field-level checks (enum values, date formats, references). This
    convention emerged organically during specification development.
  """
  decision     """
    Adopt the detect_/validate_ prefix convention as a stable naming pattern:
    - detect_* behaviors check structural graph properties (edge presence, cycles,
      field emptiness, cross-entity consistency)
    - validate_* behaviors check individual field values against known enums,
      formats, or constraints
    Both prefixes use category=validation and produce diagnostics.
  """
  consequences [
    "Consistent naming aids discoverability and tooling",
    "detect_* implies graph traversal; validate_* implies field inspection",
    "New validation rules follow this convention",
  ]
}

decision pe_feature_ordering_no_args "Feature Ordering Takes No Entity ID Argument" {
  status       accepted
  date         2026-03-10
  context      """
    Most ProductQueryPort methods take an entity ID parameter to scope the query.
    queryFeatureOrdering() takes no arguments because it operates on the global
    feature dependency graph — there is no meaningful way to scope a topological
    sort to a single feature without losing dependency context.
  """
  decision     """
    queryFeatureOrdering() is a global query that returns all features in
    topological order. Scoping would require a subgraph extraction step that
    adds complexity without clear user value. Agents can filter the result
    by milestone or module membership after receiving the full ordering.
  """
  consequences [
    "Feature ordering is always global — no entity ID parameter",
    "Result may be large for projects with many features",
    "Agents filter results locally for scoped views",
    "Consistent with topological sort semantics (global property)",
  ]
}

decision pe_standalone_product "Product Extension Fully Standalone" {
  status       accepted
  date         2026-03-10
  context      """
    The @specforge/product extension could depend on peer extensions for
    completion and coverage queries, or operate purely on its own entity
    kinds and fields. Depending on peer extensions creates fragile coupling
    and makes product unusable in domains without that peer.
  """
  decision     """
    All product queries operate purely on product entity kinds, edge types,
    and the FeatureStatus field. No query requires peer extensions. Domain
    extensions that want to link to product features do so via their own
    manifests and peer_dependency declarations.
  """
  consequences [
    "Product extension works for any domain — healthcare, education, finance",
    "No fragile coupling to any peer extension",
    "Feature coverage uses status field, not cross-extension edges",
    "Cross-extension interactions are documented in the linking extension",
  ]
}

decision pe_term_weak_connectivity "Term Weak Connectivity" {
  status       accepted
  date         2026-03-11
  context      """
    Terms connected only via TermReferencesRelatedTerm edges have weak graph connectivity
    compared to other entity kinds. Terms can exist as isolated vocabulary
    entries with no incoming or outgoing structural edges. This is intentional.
  """
  decision     """
    Terms are vocabulary aids, not dependency nodes. TermReferencesRelatedTerm edges are
    optional navigation links — a term is valid without any see_also
    references. I010 detects terms with no edges at all at info level for awareness,
    but term isolation is not a structural defect. Terms serve a fundamentally
    different role than deliverables, modules, or milestones: they define
    vocabulary, not dependencies.
  """
  consequences [
    "Terms are valid in isolation — no mandatory incoming edges",
    "I010 is info-level, not warning-level",
    "Term graph query (N-hop) works on the TermReferencesRelatedTerm subgraph only",
    "Agents use terms for vocabulary lookup, not dependency analysis",
  ]
}

decision pe_journey_flow_opaque "Journey Flow Steps are Opaque" {
  status       accepted
  date         2026-03-11
  context      """
    Journey flow steps are free-form string[] with no enforced structure.
    Structured flows (step types, conditions, branching) could enable
    validation but impose domain-specific assumptions about interaction
    patterns.
  """
  decision     """
    Journey flow steps are intentionally opaque prose strings. Structured
    flow semantics (step typing, conditional branching, loop detection)
    belong to domain extensions that understand the interaction patterns
    of their specific domain. The base product extension only requires
    flow to be present (core E006: flow is a required field) and leaves
    content interpretation to
    consumers. This preserves domain neutrality.
  """
  consequences [
    "Flow steps work for any domain without format constraints",
    "No machine-parseable structure for automated flow validation",
    "Domain extensions can add structured flow via entity_enhancements",
    "Only flow's presence is checked (E006), not its content",
    "Agents interpret flow steps as natural language sequences",
  ]
}

decision pe_deliverable_lifecycle "Deliverable Lifecycle via Status Field" {
  status       accepted
  date         2026-03-11
  context      """
    Features have FeatureStatus, milestones have MilestoneStatus, but
    deliverables had no lifecycle management. You could not answer "has
    this deliverable shipped?" or "is this deliverable deprecated?" without
    external tracking. This asymmetry meant deliverables were the only
    planning entity without observable lifecycle state.
  """
  decision     """
    Add DeliverableStatus (draft, in_progress, shipped, deprecated) and
    a reason field to ProductDeliverable. The core refuses another value (E061). I066 detects
    deprecated deliverables without a documented reason. Status defaults
    to absent (treated as draft) for incremental adoption.
  """
  consequences [
    "Deliverables have observable lifecycle state like features and milestones",
    "Deprecated deliverables require justification via reason field",
    "One new diagnostic code: I066 (W085, its first value check, is retired to E061)",
    "Absent status treated as draft — no breaking change for existing specs",
  ]
  invariants   [deliverable_lifecycle_consistency]
}

decision pe_term_isolation_boundary "Term Isolation as Design Boundary" {
  status       accepted
  date         2026-03-11
  context      """
    Terms are connected to the product graph only via TermReferencesRelatedTerm edges
    (term-to-term). No other entity kind can reference a term via a graph
    edge. This means terms are structurally isolated from the main product
    graph (features, journeys, deliverables, milestones, modules).
    Decision pe_term_weak_connectivity acknowledges this but does not
    provide a path forward for extensions that need term integration.
  """
  decision     """
    Term isolation is an intentional design boundary in v1. Terms are
    vocabulary aids consumed by humans and agents via text — they are NOT
    dependency nodes. Extensions that need term-to-entity graph connectivity
    (e.g., a glossary-powered documentation renderer) can add a `terms`
    reference field to any entity kind via entity_enhancements in their
    own manifest, creating extension-owned edges without polluting the
    base product model. The product extension intentionally does not
    pre-wire term references on all entity kinds because most domains
    do not need graph-level term connectivity.
  """
  consequences [
    "Terms remain structurally isolated in the base product extension",
    "Extensions can add term connectivity via entity_enhancements",
    "No unnecessary fields on entity kinds that don't need term links",
    "Term graph query (N-hop) remains clean — only TermReferencesRelatedTerm edges",
    "Future @specforge/documentation extension could add terms fields to all kinds",
    "Extension integration pattern: declare a 'terms' field (EntityId[] @optional) via entity_enhancements targeting desired entity kinds, with a custom edge type (e.g., EntityTerm) in the extension manifest",
  ]
}

decision pe_scalability_tiers "Scalability Tiers" {
  status       accepted
  date         2026-03-11
  context      """
    All original product constraints targeted "up to 500 product entities"
    without addressing larger projects. Real-world projects in regulated
    industries (healthcare, finance, aerospace) can have 5000+ entities.
    Without scalability tiers, the extension provides no performance
    guarantees beyond medium-sized projects.
  """
  decision     """
    Define three scalability tiers with corresponding performance targets:
    - Tier 1 (≤500 entities): validation <50ms, queries <100ms (MUST)
    - Tier 2 (≤5000 entities): validation <500ms, queries <1s (SHOULD)
    - Tier 3 (>5000 entities): validation <2s, queries <2s or timeout (SHOULD)
    Cycle detection and unreferenced detection MUST remain O(V+E) at all tiers.
    Feature ordering (global topological sort) is the most expensive query
    and gets a dedicated 2-second budget at Tier 2.
  """
  consequences [
    "Three tiers provide graduated performance expectations",
    "Tier 1 targets unchanged — no regression for existing specs",
    "Tier 2 and 3 are SHOULD-level — degradation is documented, not forbidden",
    "O(V+E) algorithmic guarantees prevent pathological scaling",
    "Large projects get explicit timeout behavior instead of silent hangs",
  ]
}

decision pe_no_persona_channel_edge "No Direct PersonaChannel Edge" {
  status       accepted
  date         2026-03-11
  context      """
    "Which channels does this persona use?" requires multi-hop traversal
    (persona <- JourneyTargetsPersona <- journey -> JourneyUsesChannel -> channel).
    A direct PersonaChannel edge would simplify this but create denormalization.
  """
  decision     """
    No PersonaChannel edge in v1. The journey is the authoritative source
    of persona-channel binding. A direct edge would duplicate information
    and create sync issues when journeys change. Extensions can add this
    via entity_enhancements if needed. A composite query
    (pe_query_persona_channels) provides the convenience of a direct edge
    without the denormalization risk.
  """
  consequences [
    "Multi-hop query required for persona-to-channel traversal",
    "No denormalization risk from duplicated persona-channel bindings",
    "Journey remains the single source of truth for persona-channel relationships",
    "pe_query_persona_channels provides composite traversal convenience",
    "Extensions can add PersonaChannel edge via entity_enhancements if needed",
  ]
}

decision pe_typed_surface_schemas "Typed Surface Schemas" {
  status       accepted
  date         2026-03-11
  context      """
    The original pe_declare_surface_contributions behavior listed 14 CLI commands
    and 16 MCP resources with Wasm export names and URI patterns, but did not
    specify input/output schemas. Agents consuming these surfaces had no way to
    discover the exact contract at compile time. Three approaches were considered:
    - Option A: Untyped JSON (current state) — flexible but undiscoverable
    - Option B: JSON Schema files alongside the manifest — typed but disconnected
    - Option C: Typed input/output in the spec DSL — typed and traceable
  """
  decision     """
    Define per-surface input and output types in the spec DSL (Option C).
    List commands use ProductListFilter input and per-kind list result types
    (FeatureListResult, JourneyListResult, etc.) with pagination. Query commands
    use per-command input types and return existing query payload types.
    All commands share consistent error handling via ProductSurfaceError with
    two error codes: ENTITY_NOT_FOUND (exit 1) and INVALID_INPUT (exit 2).
    GRAPH_NOT_READY and the MCP resource envelope (ProductSurfaceResponse)
    were dropped by ADR 0011: a command only runs over a built graph, and
    product declares no MCP resources.
  """
  consequences [
    "Every surface has a traceable input/output contract in the spec graph",
    "Agents can discover surface schemas via the graph protocol",
    "Per-kind list types add ~8 types but enable kind-specific key fields in listings",
    "Two error codes keep the error space small and predictable",
    "surfaces.spec file contains 30 surface behaviors with full contracts",
    "Schema changes require updating both types.spec and surfaces.spec",
  ]
  invariants   [pe_surface_error_consistency]
}

decision pe_list_pagination "List Command Pagination" {
  status       accepted
  date         2026-03-11
  context      """
    List commands could return all entities (unbounded) or support pagination.
    Projects with 5000+ features need bounded result sets for CLI output and
    agent context windows. Three approaches:
    - Option A: No pagination — always return all entities
    - Option B: Cursor-based pagination — efficient but stateful
    - Option C: Offset/limit pagination — stateless and simple
  """
  decision     """
    Use offset/limit pagination (Option C). Default limit is 100, max is 1000.
    total reflects the filtered (pre-pagination) count. has_more indicates
    whether more results exist beyond the current page. Pagination is stateless
    — clients can request any page without server-side cursors. The matrix
    queries page the same way (ADR 0011).
  """
  consequences [
    "Stateless pagination — no server-side cursor management",
    "Default 100 results balances utility and context-window efficiency",
    "Max 1000 prevents memory issues on large projects",
    "total enables progress indication for paginating agents",
    "Offset/limit is less efficient than cursors for deep pagination but simpler",
    "No consistency guarantee between pages during concurrent edits",
  ]
  invariants   [pe_list_pagination_correctness]
}

decision pe_three_output_formats "Three Output Formats" {
  status       superseded
  date         2026-03-11
  context      """
    CLI commands need human-readable and machine-readable output. Agents
    need minimal output to fit context windows. Three fixed formats cover
    all use cases without requiring custom format strings.
  """
  decision     """
    Superseded by ADR 0011: the host owns --format human|json on
    every extension command (human by default on the CLI; MCP always asks
    for json). table and brief are not formats.

    All CLI commands support --format with three values:
    - json (default): full JSON output for programmatic consumption
    - table: aligned columns for human terminal reading
    - brief: minimal output — one ID per line for list commands, single
      value for query commands (e.g., completion ratio as plain number)
    MCP resources always return JSON regardless of format parameter.
  """
  consequences [
    "Three formats cover programmatic, human, and minimal use cases",
    "json is default — CLI output is machine-parseable by default",
    "brief format is ideal for piping: specforge product:features --format=brief | xargs ...",
    "table format makes terminal usage pleasant without external formatters",
    "MCP ignores format — agents always get structured JSON",
  ]
}

decision pe_reason_content_opaque "Reason Field Content is Opaque" {
  status       accepted
  date         2026-03-11
  context      """
    I059, I066, I069 and I070 check for empty/missing reason fields but do not
    validate content quality. A single-character reason like "x" satisfies
    the check.
  """
  decision     """
    Reason content is intentionally opaque prose. Quality validation
    belongs to domain extensions or human review. The product extension
    validates presence, not content, consistent with pe_acceptance_free_form
    and pe_journey_flow_opaque.
  """
  consequences [
    "Reason fields are validated for presence only, not content quality",
    "Consistent with acceptance criteria and journey flow opacity decisions",
    "Domain extensions can add content quality checks via entity_enhancements",
    "Human review remains the authority for prose quality",
  ]
}

decision pe_adopt_cross_extension_query_isolation "Cross-Extension Query Isolation" {
  status       accepted
  date         2026-03-11
  context      """
    When @specforge/software is co-installed with @specforge/product, software
    creates BehaviorImplementsFeature edges (behavior->feature) via peer_dependency. This raises
    the question: should product queries (milestone completion, journey coverage,
    deliverable traceability) traverse BehaviorImplementsFeature edges to discover behaviors
    linked to features?
  """
  decision     """
    Product queries MUST NOT traverse edges owned by other extensions. Product
    queries operate exclusively on the 20 product-owned edge types. The BehaviorImplementsFeature
    edge is owned by @specforge/software and is invisible to product queries.
    This ensures:
    1. Product extension is fully standalone — works without any peer extension
    2. Query results are deterministic regardless of which other extensions are installed
    3. No coupling between product query logic and other extensions' edge schemas
    4. Extensions that need cross-extension traversal implement it in their own queries

    Agents that want to combine product and software graph data should issue
    separate queries to each extension's surface and merge results client-side.
  """
  consequences [
    "Product queries are isolated from other extensions' edges",
    "Milestone completion counts features by status, not by linked behaviors",
    "Journey coverage uses FeatureStatus, not behavior test results",
    "Agents must merge cross-extension data client-side",
    "No false dependencies between product queries and peer extensions",
    "Deterministic query results regardless of installed extensions",
  ]
  invariants   [pe_manifest_nine_entity_kinds]
}

decision pe_cross_extension_enhancement_receiving "Cross-Extension Enhancement Receiving" {
  status       accepted
  date         2026-03-11
  context      """
    The product extension declares no entity_enhancements (it does not add
    fields to other extensions' entities). However, OTHER extensions may
    enhance product entity kinds. For example, @specforge/software could add
    a behaviors field to milestone via entity_enhancements. What constraints
    govern this?
  """
  decision     """
    Product entity kinds MAY receive entity_enhancements from peer extensions.
    Enhanced fields are visible in the entity graph and queryable via the
    graph protocol, but product-owned queries and validation rules do NOT
    inspect enhancement fields. Enhancement fields are the responsibility of
    the enhancing extension. Product validation rules only check fields
    declared in the product manifest's field definitions.
  """
  consequences [
    "Product entity kinds are open to enhancement by peer extensions",
    "Product validation rules ignore enhancement fields",
    "Enhancement field validation is the enhancing extension's responsibility",
    "No product diagnostic codes fire on enhancement field values",
    "Entity graph contains enhancement fields in export output",
    "Product list commands include enhancement fields in json output but not in human output",
  ]
}

decision pe_tag_semantics "Tag Naming Conventions" {
  status       accepted
  date         2026-03-11
  context      """
    Tags (string[] @optional on all 9 entity kinds) have no enforced naming
    conventions. Tags could contain spaces, special characters, mixed case,
    or empty strings. Without conventions, singleton tag detection (I052)
    produces false positives for case variants and typo suggestions for
    intentionally distinct short tags.
  """
  decision     """
    Tags are case-sensitive, lowercase, hyphen-separated strings matching
    the pattern [a-z0-9][a-z0-9-]*[a-z0-9] (min 2 chars, max 50 chars).
    Tags that violate this pattern produce an I068 info diagnostic with
    a suggested normalized form. Empty strings in the tags array are silently
    ignored. Tags are NOT namespaced — there are no reserved prefixes.
    This convention is enforced at info level to support incremental adoption
    without blocking compilation.
  """
  consequences [
    "Lowercase hyphen-separated convention matches common tagging patterns",
    "I068 is info-level — violations inform but do not block",
    "Case-sensitive comparison prevents 'MVP' and 'mvp' coexisting undetected",
    "Min 2 chars prevents single-letter tags that pollute I052 suggestions",
    "Max 50 chars prevents tags that are actually descriptions",
    "No namespacing — simplicity over hierarchy for v1",
    "Empty strings silently dropped — no diagnostic noise for trailing commas",
  ]
  invariants   [pe_tags_per_entity_kind]
}

decision pe_extension_versioning "Extension Versioning Strategy" {
  status       accepted
  date         2026-03-11
  context      """
    The @specforge/product manifest declares manifestVersion: 2 (the manifest
    schema version) but has no version field for the extension itself. Without
    extension versioning, consumers cannot detect breaking changes in entity
    kinds, edge types, or query schemas.
  """
  decision     """
    The extension version follows semver (major.minor.patch) starting at 1.0.0.
    Breaking changes (removing entity kinds, edge types, or fields; changing
    query return types; renaming diagnostic codes) require a major version bump.
    Additive changes (new entity kinds, edge types, fields, queries, diagnostics)
    require a minor version bump. Bug fixes and documentation changes require a
    patch version bump. The extension version is declared in the manifest and
    included in the graph protocol output. Peer dependencies reference extension
    versions using semver ranges (e.g., ^1.0).
  """
  consequences [
    "Extension starts at version 1.0.0",
    "Breaking changes require major bump — consumers can pin to ^1.0",
    "New diagnostics or queries are minor bumps — non-breaking",
    "Version in graph protocol output enables consumer compatibility checks",
    "Peer dependencies use semver ranges for forward compatibility",
    "No migration hook needed until major version 2.0.0",
  ]
}

decision pe_priority_algorithm_null_handling "Priority Algorithm Null Handling" {
  status       accepted
  date         2026-03-11
  context      """
    pe_query_deliverable_priority derives priority from constituent milestones
    and journeys. When some milestones/journeys have a priority field and others
    do not, the algorithm must decide how to handle null-priority entities.
  """
  decision     """
    Null-priority entities are excluded from the derivation. Only milestones
    and journeys with an explicit priority field contribute to the derived
    deliverable priority. If ALL constituent entities have null priority,
    the deliverable priority is null. source_count reflects only entities
    WITH explicit priority, not total entities.
  """
  consequences [
    "Null-priority entities do not affect derivation — they are invisible",
    "A deliverable with one critical milestone and five null-priority journeys derives critical",
    "source_count may be less than total milestones + journeys",
    "Agents can distinguish 'no priority data' (null) from 'lowest priority' (low)",
    "Consistent with priority being optional on features, journeys, and milestones",
  ]
}

decision pe_description_on_planning_entities "Description Field on Planning Entities" {
  status       accepted
  date         2026-03-11
  context      """
    Journeys have a description field (pe_journey_description_field) and modules
    have one, but features only have problem/solution (no general description),
    and milestones and deliverables have no description at all. For agent
    consumption, a free-text description provides a summary that improves context
    quality without requiring agents to parse structured fields.
  """
  decision     """
    Add an optional description field (string) to ProductFeature,
    ProductMilestone, and ProductDeliverable. For features, description
    complements problem/solution — it serves as a human/agent-readable
    summary when the problem/solution framing is too verbose. For milestones
    and deliverables, description fills a gap where no summary field existed.
    All three fields are optional to preserve incremental adoption.
  """
  consequences [
    "Features have a summary field in addition to problem/solution",
    "Milestones and deliverables gain agent-readable context",
    "Description is optional — no breaking change for existing specs",
    "Agents can use description for context windows without parsing structured fields",
    "Consistent with journey and module description fields",
  ]
}

decision pe_persona_channel_reason "Reason Field on Persona and Channel" {
  status       accepted
  date         2026-03-11
  context      """
    Features have a reason field for deferred status (I059), milestones
    list blockers when blocked (I060), and deliverables have a reason for
    deprecated status (I066).
    Personas and channels can have status=deprecated but lack a reason field,
    creating an asymmetry where deprecation justification is enforced for
    some entity kinds but not others.
  """
  decision     """
    Add an optional reason field (string) to ProductPersona and
    ProductChannel. I069 detects deprecated personas without a reason.
    I070 detects deprecated channels without a reason. Both are info-level
    to support incremental adoption. This completes the lifecycle consistency
    pattern across all entity kinds with deprecation states.
  """
  consequences [
    "Consistent lifecycle documentation across all entity kinds with deprecation",
    "I069 and I070 are info-level — inform but do not block",
    "Two new diagnostic codes consuming from the reserved I-code range",
    "Reason content is opaque — presence-only validation per pe_reason_content_opaque",
  ]
  invariants   [persona_lifecycle_consistency, channel_lifecycle_consistency]
}

decision pe_surface_observability "Surface Operation Observability Events" {
  status       superseded
  date         2026-03-11
  context      """
    Query observability events exist for all 20 ProductQueryPort methods,
    but CLI list commands and MCP resource reads have no events. When an
    agent calls resource_deliverable_traceability or a user runs
    product:features, there is no observability trail for external tooling
    to subscribe to.
  """
  decision     """
    Superseded by ADR 0011: commands emit no events (they have no
    event channel); MCP records each auto-promoted command as
    surface_command_dispatched with its duration.

    Add two surface operation events: pe_cli_command_executed and
    pe_mcp_resource_accessed. Both emit a ProductSurfaceOperationPayload
    with the surface_id, surface_type (cli|mcp), optional entity_id,
    duration_ms, and response status. Both are observability-only events
    (empty consumers) on dedicated channels. This enables dashboards,
    usage analytics, and performance monitoring without coupling to
    surface internals.
  """
  consequences [
    "Full observability trail for all surface operations",
    "External tooling can monitor CLI and MCP usage patterns",
    "Performance metrics (duration_ms) enable latency alerting",
    "Two new events with no consumers — pure observability",
    "ProductSurfaceOperationPayload adds one new type to types.spec",
  ]
}

decision pe_mcp_payload_size_limits "MCP Payload Size Limits" {
  status       superseded
  date         2026-03-11
  context      """
    MCP resources return JSON payloads that AI agents consume within
    context windows. Large projects (5000+ entities) can produce responses
    that exceed typical agent context budgets. Without payload size bounds,
    agents may receive oversized responses for term graph
    traversals, or feature ordering queries on large graphs.
  """
  decision     """
    Superseded by ADR 0011: product declares no MCP resources;
    payload size is bounded by the list limit and the matrix queries'
    offset/limit pagination.

    MCP resource responses SHOULD NOT exceed 64KB of serialized JSON.
    Queries that may produce large payloads bound their result arrays
    to a configurable maximum (default 500 items) and return a truncated
    flag with partial results. This ensures token-budget-aware agents
    can consume responses without context overflow. The 64KB limit is
    a SHOULD — it informs but does not block. The configurable maximum
    allows deployment-specific tuning.
  """
  consequences [
    "64KB soft limit protects agent context windows",
    "Truncated flag enables agents to request additional pages",
    "Configurable maximum allows deployment-specific tuning",
    "SHOULD-level — does not block responses for power users",
    "Consistent with pe_list_pagination limit of 1000 for CLI commands",
  ]
}

decision pe_tag_cross_kind_awareness "Tag Cross-Kind Awareness" {
  status       accepted
  date         2026-03-11
  context      """
    Tags are shared across all 9 entity kinds with no namespacing
    (pe_tag_semantics). The same tag "core" on a feature and a module
    may mean different things but appears identical in tag-based queries.
    Without awareness, users get no signal when a tag drifts in meaning
    across entity kinds.
  """
  decision     """
    Add I084 to detect tags used on entities of 3 or more different
    kinds. The threshold of 3 avoids noise for common cross-cutting tags
    used on 2 kinds (e.g., "mvp" on features and milestones). I084 is
    info-level — cross-kind tags are valid and often intentional for
    cross-cutting categorization. The diagnostic suggests kind-specific
    prefixes (e.g., "core-module", "core-feature") as an opt-in
    convention, not a requirement.
  """
  consequences [
    "Users get awareness of potentially ambiguous cross-kind tags",
    "Threshold of 3 avoids noise for common 2-kind cross-cutting tags",
    "I084 is info-level — informs but does not block",
    "Prefix suggestion is opt-in convention, not enforced",
    "No namespacing added — keeps tag model simple per pe_tag_semantics",
  ]
  invariants   [tag_cross_kind_awareness]
}

decision pe_priority_asymmetry "Priority Field Asymmetry" {
  status       accepted
  date         2026-03-11
  context      """
    The Priority field (critical, high, medium, low) is declared on feature,
    journey, and milestone but NOT on deliverable, module, term, persona,
    or channel. This asymmetry is intentional.
  """
  decision     """
    Priority is only meaningful on planning-active entities that compete for
    scheduling attention: features (what to build first), journeys (which
    user flows to prioritize), and milestones (which phases are critical).
    Deliverables inherit priority from their milestones and journeys. Modules
    are structural — they exist or don't, independent of priority. Terms,
    personas, and channels are reference entities that describe vocabulary,
    roles, and mediums — they are not prioritized against each other.
  """
  consequences [
    "Priority on 3 of 9 kinds — feature, journey, milestone",
    "Deliverable priority is derived from constituent milestones/journeys",
    "Module, term, persona, channel have no priority field",
    "Priority is an enum only on kinds that declare it",
    "No false validation warnings on kinds without priority",
  ]
}

decision pe_schema_evolution_strategy "Schema Evolution Strategy" {
  status       accepted
  date         2026-03-11
  context      """
    The @specforge/product extension declares 9 entity kinds with typed field
    schemas (ProductFeature, ProductJourney, etc.). When entity shapes evolve
    (new fields, renamed fields, removed fields), existing .spec files must
    continue to work or be migrated. The manifest declares migration_hook=null
    for v1 (no prior version to migrate from), but a forward-looking strategy
    is needed for v2+.
  """
  decision     """
    Adopt additive-only evolution for minor versions (1.x): new fields are
    always @optional, no fields are removed or renamed. Breaking changes
    (field removal, field rename, type change) require a major version bump
    (2.0) and a migration_hook that transforms existing entities. The
    migration_hook is a Wasm export (migrate__product) that receives the
    old entity JSON and returns the new entity JSON. Migration hooks run
    during specforge migrate and are tested with before/after fixture pairs.
    The extension_versioning decision (pe_extension_versioning) governs the
    version numbering; this decision governs the schema-level contract.
  """
  consequences [
    "Minor versions (1.x) are always backward compatible — new @optional fields only",
    "Major versions (2.0) may break schemas but must provide migration_hook",
    "migration_hook is a Wasm export: migrate__product(old_json) -> new_json",
    "specforge migrate invokes hooks in topological order of peer dependencies",
    "Before/after fixture pairs test migration correctness",
    "Absent fields treated as null — no migration needed for new @optional fields",
  ]
}

decision pe_priority_propagation_algorithm "Priority Propagation Algorithm" {
  status       accepted
  date         2026-03-11
  context      """
    pe_query_deliverable_priority derives a deliverable's priority from its
    constituent milestones and journeys. The algorithm was not explicitly
    specified, leading to ambiguity about how null priorities and mixed
    priority levels are resolved.
  """
  decision     """
    The priority propagation algorithm uses max-priority semantics:
    1. Collect priorities from all milestones (via DeliverableTrackedByMilestone)
       and journeys (via DeliverableSupportsJourney) referencing the deliverable.
    2. Filter out null priorities — only explicit priorities participate.
    3. If no explicit priorities exist, return null (priority unknown).
    4. Otherwise, return the highest priority: critical > high > medium > low.
    5. source_count is the number of entities with explicit (non-null) priority.
    This is max-priority, not average or majority-vote. The rationale is
    that a deliverable containing even one critical milestone is itself
    critical — the highest-urgency component dominates.
  """
  consequences [
    "Max-priority semantics: critical > high > medium > low",
    "Null priorities are filtered out — they don't dilute the result",
    "All-null inputs produce null priority (genuinely unknown)",
    "source_count counts only entities with explicit priority",
    "Deterministic: same graph always produces same derived priority",
    "No priority on module, term, persona, channel per pe_priority_asymmetry",
  ]
}

decision pe_module_reason_field "Module Reason Field" {
  status       accepted
  date         2026-03-11
  context      """
    Six of nine product entity kinds had a reason field for lifecycle
    documentation (feature, milestone, deliverable, persona, channel).
    Module was missing reason, creating asymmetry. A module may be
    deprecated or scheduled for removal, and the justification should
    be documented.
  """
  decision     """
    Add reason as string @optional to ProductModule. No new validation
    rule is needed in v1 — the field is documentation-only, consistent
    with pe_reason_content_opaque. Future versions may add I-level
    diagnostics for deprecated modules without a reason.
  """
  consequences [
    "ProductModule gains reason field (string @optional)",
    "Seven of nine kinds now have reason: feature, milestone, deliverable, persona, channel, module, release",
    "Term and journey remain without reason (terms are vocabulary, journeys are flows)",
    "No new diagnostic in v1 — reason is documentation-only for modules",
  ]
}

decision pe_module_term_no_status "Module and Term Lack Status Lifecycle" {
  status       accepted
  date         2026-03-11
  context      """
    Six of nine product entity kinds have a status field (feature,
    milestone, deliverable, persona, channel, release). Module, term and
    journey have none. The module and term gap was questioned as an
    asymmetry.
  """
  decision     """
    Module and term intentionally have no status lifecycle field.

    Module rationale: modules map to code components (crates, packages).
    Their lifecycle is observable from the codebase itself — a module either
    exists in the dependency graph or it doesn't. Adding draft/active/deprecated
    would duplicate what the build system already knows. Module has a reason
    field for documenting deprecation intent, but status is the build artifact's
    responsibility, not the spec's.

    Term rationale: terms are vocabulary definitions. They are either defined
    or they are not. A term has no meaningful lifecycle stages between those
    two states. Deprecated terms should be removed and replaced with updated
    definitions — the see_also field handles term evolution by pointing to
    successor terms. Adding status to terms would create validation noise
    without improving planning outcomes.

    Future versions may add status if a concrete use case emerges (e.g.,
    "proposed" terms in a review workflow), but the bar is high: the status
    must enable a query or validation rule that cannot be achieved otherwise.
  """
  consequences [
    "Module and term remain without status field — intentional, not an oversight",
    "Module deprecation is documented via the reason field (free-text, no enum)",
    "Term evolution is expressed via see_also references to successor terms",
    "Six of nine kinds have status; three do not (term, module, journey) — this asymmetry is accepted",
    "No new validation rules needed — existing unreferenced detection (W044, I010) covers unused modules/terms",
  ]
}

decision pe_channel_url_field "Channel URL Field" {
  status       accepted
  date         2026-03-11
  context      """
    ProductChannel describes an interaction medium (CLI, IDE, API, MCP)
    but had no field linking to the actual surface. Agents consuming
    the graph cannot discover how to interact with a channel without
    external knowledge.
  """
  decision     """
    Add url as string @optional to ProductChannel. This field links to
    the canonical documentation, API endpoint, or configuration URL for
    the channel. It is informational only — no validation of URL format
    in v1 to avoid false positives on internal URIs and custom schemes.
  """
  consequences [
    "ProductChannel gains url field (string @optional)",
    "Agents can discover channel endpoints from the graph",
    "No URL format validation in v1 — accepts any string",
    "Future versions may add I-level diagnostic for malformed URLs",
  ]
}

decision pe_entity_kind_addition_migration "Entity Kind Addition Migration" {
  status       accepted
  date         2026-03-11
  context      """
    pe_schema_evolution_strategy covers field-level evolution (additive-only
    for minor versions, migration_hook for breaking changes). But it does
    not address what happens when a new entity kind is added to the extension
    — e.g., adding a 9th entity kind in v1.1. New kinds are structurally
    different from new fields: they require KindRegistry registration,
    potentially new edge types, new validation rules, and new diagnostic codes.
  """
  decision     """
    New entity kinds are a minor version addition (1.x), NOT a breaking change.
    The protocol:
      1. New kind is added to entity_kinds[] in the manifest (e.g., 9 kinds)
      2. New edge types connecting the kind are added to edge_types[]
      3. New validation rules and diagnostic codes are allocated from reserved gaps
      4. The manifest version bumps to 1.x (minor)
      5. Existing .spec files are unaffected — no migration needed
      6. The invariant pe_manifest_nine_entity_kinds covers the
         pe_manifest_entity_kinds_match_manifest (count matches manifest declaration)

    Removal of an existing entity kind is a major version change (2.0) requiring
    a migration_hook. Renaming a kind is also a major change. Only addition
    is minor-compatible.

    Edge type additions follow the same pattern: new edges are minor, removal
    is major.
  """
  consequences [
    "New entity kinds are backward compatible — existing specs need no migration",
    "Kind count invariant is manifest-declared, not hardcoded to 8",
    "Reserved diagnostic code gaps support future rule additions",
    "Kind removal or rename requires major version + migration_hook",
    "Edge type additions are minor; removals are major",
  ]
}

decision pe_deliverable_persona_composite "Deliverable-Persona Composite Query" {
  status       accepted
  date         2026-03-11
  context      """
    "Which personas does this deliverable serve?" requires two-hop traversal:
    deliverable -> DeliverableSupportsJourney -> journey -> JourneyTargetsPersona -> persona.
    Unlike persona-channel (also multi-hop), deliverable-persona is a
    high-frequency product management question: product managers routinely
    need to know which user roles are served by each shippable artifact.
    No direct DeliverablePersona edge exists because it would denormalize
    the journey relationship.
  """
  decision     """
    Add pe_query_deliverable_personas as a composite query on ProductQueryPort.
    The query traverses deliverable->journey->persona edges, deduplicates
    personas, and includes via_journey_ids for traceability. This mirrors
    the pe_query_persona_channels pattern: composite queries provide
    convenience without denormalizing edges. No new edge type is needed.
  """
  consequences [
    "Two-hop traversal encapsulated in a single query method",
    "No denormalization — journey remains the authority for persona binding",
    "via_journey_ids provides traceability for the composition path",
    "Consistent with pe_no_persona_channel_edge design pattern",
    "One new query method on ProductQueryPort, one new command",
  ]
}

decision pe_validation_rule_observability "Validation Rule Observability" {
  status       deprecated
  date         2026-03-11
  context      """
    The product extension emits pe_validation_complete with aggregate counts
    (error/warning/info), but external tooling cannot observe WHICH specific
    rules fired or were suppressed. Debugging validation behavior requires
    parsing diagnostic output or reading compiler logs. CI pipelines and
    dashboards need structured per-rule data.
  """
  decision     """
    Add two observability events:
    1. pe_validation_rule_fired — emitted per rule that produces a diagnostic,
       with rule_id, diagnostic_code, severity, entity_id (if entity-scoped),
       and message. Emitted on product.validation_rule_fired channel.
    2. pe_validation_summary — emitted once after all rules complete, with
       rules_evaluated, rules_fired, rules_suppressed, and by_severity counts.
       Emitted on product.validation_summary channel.

    Both are observability events with empty consumers (no orchestration).
    External tooling (CI, dashboards, MCP notifications) subscribes at runtime.
    This follows the pattern of existing query observability events.
  """
  consequences [
    "Per-rule observability without parsing diagnostic output",
    "CI pipelines can track specific rule trends over time",
    "Summary event enables aggregate monitoring dashboards",
    "No runtime cost when no consumers subscribe (events are lazy)",
    "Consistent with existing observability event pattern",
  ]
}

decision pe_observability_fire_and_forget "Observability Events Are Fire-and-Forget" {
  status       accepted
  date         2026-03-11
  context      """
    The product extension emits observability events (pe_validation_summary,
    pe_module_cycle_detected, etc.) with consumers=[]. Query commands emit
    none (ADR 0011: a command has no event channel). These
    events have no declared consumers within the product extension itself. The question is
    whether this is a gap or an intentional design choice.
  """
  decision     """
    Observability events are intentionally fire-and-forget. They exist for external
    consumers: CI dashboards, monitoring systems, extension analytics, and debugging tools.
    The product extension does NOT consume its own observability events — this would create
    circular dependencies. Consumers subscribe at the channel level (product.*) and filter
    by event type. The events carry structured payloads (typed via ProductXxxPayload types)
    so consumers can deserialize without parsing. No runtime cost when no consumers
    subscribe — events are lazy (emitted only when a listener exists on the channel).
    Future extensions (e.g., @specforge/analytics) can declare consumers for these events.
  """
  consequences [
    "Observability events have zero runtime cost when unsubscribed",
    "External tools consume events via channel subscription (product.*)",
    "No circular dependencies within the product extension",
    "Payload types are stable API — breaking changes require major version bump",
    "Future @specforge/analytics extension can consume product.* events",
    "Event schema documented via ProductXxxPayload types in types.spec",
  ]
}

decision pe_persona_channel_intentional_depth "Persona and Channel Entities Are Intentionally Shallow" {
  status       accepted
  date         2026-03-11
  context      """
    Personas have goals and pain_points but no structured priority or weighting.
    Channels have interaction_model but no capacity or throughput constraints.
    Both are leaf entities with very few outgoing edges — primarily referenced BY
    journeys rather than contributing graph structure themselves. This could be
    seen as a gap in a product planning tool.
  """
  decision     """
    Personas and channels are intentionally shallow leaf entities. Their purpose is
    to provide identity and classification for journey references, not to carry
    complex graph structure. Adding priority/weighting to personas would duplicate
    journey priority (which already captures persona-specific importance). Adding
    throughput to channels would conflate specification (what the system does) with
    infrastructure (how it performs) — violating the domain-neutral principle.
    Depth is added through journeys: a persona's importance is measured by the
    number and priority of journeys referencing it (core's
    `specforge query <persona> --depth 1 --kind journey`). A channel's
    utilization is measured by journey count and coverage. If a project needs richer persona modeling
    (e.g., weighted scoring, persona segments), an extension can use
    entity_enhancements to add fields to the persona kind without modifying
    the product manifest.
  """
  consequences [
    "Personas remain simple identity+classification entities",
    "Channels remain simple medium+interaction-model entities",
    "Complexity lives in journeys, not leaf entities",
    "Extensions can add fields via entity_enhancements if needed",
    "No duplicate priority semantics between persona and journey",
    "Domain-neutral principle preserved — no infrastructure fields",
  ]
}

decision pe_entity_versioning_deliverables_only "Only Deliverables Track Versions" {
  status       accepted
  date         2026-03-11
  context      """
    Deliverables have a version field (SemVer 2.0.0), but no other entity kind
    tracks versions. Milestones, modules, and journeys evolve over time but have
    no version field or changelog mechanism. The spec tracks status lifecycle
    but not when transitions happened.
  """
  decision     """
    Only deliverables track versions because they are shippable artifacts with
    external consumers who depend on version numbers. Milestones, modules, and
    journeys are internal planning constructs — their evolution is tracked by
    version control (git diff on .spec files). Adding version fields to
    internal entities would create maintenance burden without consumer value.
    Temporal tracking (when status changed) is deliberately excluded from v1 —
    it requires either a changelog field or event sourcing, both of which add
    significant complexity. If per-entity temporal tracking is needed, an
    extension can add a changelog field via entity_enhancements.
  """
  consequences [
    "Deliverables are the only versioned entity kind",
    "Internal entities rely on version control for temporal comparison",
    "No changelog or timestamp fields in v1 — kept simple",
    "Extensions can add temporal fields via entity_enhancements",
  ]
}

decision pe_migration_additive_only "Additive-Only Schema Evolution" {
  status       accepted
  date         2026-03-11
  context      """
    The v1 manifest declares migration_hook=null (no prior version). The
    extension needs a documented migration strategy for version bumps to
    ensure consumers can plan for schema changes.
  """
  decision     """
    Minor versions (1.x) follow additive-only schema evolution:
    - New optional fields can be added to any entity kind
    - New diagnostic codes are allocated outside the third-party range
      (900-998) and added to the diagnostic catalog
    - New edge types require manifest version field bump but not major version
    - New entity kinds require major version bump
    - Field removal, kind removal, or edge type removal require major version
      bump with a migration hook
    The migration hook (introduced in v2) receives the old manifest version
    and returns a list of graph transformations (add_field, remove_field,
    rename_field, add_edge_type, remove_edge_type). Until v2, migration_hook
    remains null. Spec files authored for v1.0 parse without error under v1.x.
  """
  consequences [
    "v1.x minor bumps are always backward compatible",
    "New diagnostic codes enable growth without breaking changes",
    "Major version bumps require explicit migration hooks",
    "v2 introduces migration_hook for v1->v2 graph transformation",
    "Consumers can safely upgrade minor versions without spec file changes",
  ]
}

decision pe_no_product_renderer "Product Uses Core Emitter, No Custom Renderer" {
  status       accepted
  date         2026-03-11
  context      """
    The product extension manifest declares contributes.renderers=false. Product
    entities appear in the Graph Protocol JSON output, but there is no
    product-specific renderer. The question is whether product entities need
    special rendering logic.
  """
  decision     """
    Product entities render through the core emitter's uniform node rendering.
    All entity kinds registered in KindRegistry are rendered identically: kind,
    id, fields (per FieldRegistry order), edges (per EdgeTypeRegistry order).
    No product-specific rendering is needed because:
    (1) The graph protocol is kind-agnostic — consumers use the kind field to
        interpret entities, not format-specific rendering.
    (2) Product entities have no structured body content (unlike behaviors with
        Design-by-Contract blocks) — all data is in typed fields.
    (3) The three export formats (context, graph, brief) apply uniformly to
        all entity kinds.
    Entities with validation errors include a _diagnostics array in context
    format. This is a core emitter feature, not product-specific.
  """
  consequences [
    "Product entities use the same rendering path as all other entities",
    "No product-specific Wasm rendering export needed",
    "Core emitter changes automatically apply to product entities",
    "_diagnostics array available for entities with validation errors",
    "Reduces extension binary size — no rendering code shipped",
  ]
}

decision pe_planning_queries "Advanced Planning Queries" {
  status       accepted
  date         2026-03-12
  context      """
    Existing queries cover milestone completion, journey coverage, feature
    ordering, and impact analysis, but key planning questions remain
    unanswerable without manual graph traversal: which features are
    unscheduled, which features overlap across deliverables, what is each
    persona's feature coverage, and what is the critical path through
    milestones.
  """
  decision     """
    Add four new query methods to ProductQueryPort:
    1. queryUnscheduledFeatures — features with zero MilestoneDeliversFeature edges
    2. queryFeatureOverlap — features reachable from 2+ deliverables
    3. queryPersonaCoverageMatrix — per-persona feature reachability matrix
    4. queryCriticalPath — longest incomplete milestone chain with slack

    All four are specforge product commands (unscheduled-features,
    feature-overlap, coverage-matrix, critical-path), auto-promoted to MCP
    tools; product declares no MCP resources (ADR 0011).
  """
  consequences [
    "Four new query methods on ProductQueryPort",
    "Four new CLI commands, each an MCP tool",
    "Coverage matrix enables persona-centric planning",
    "Critical path enables dependency-aware scheduling",
    "Feature overlap enables cross-deliverable coordination",
    "Unscheduled features enables release planning gap detection",
  ]
}

decision pe_chain_validation_rules "End-to-End Chain Validation Rules" {
  status       accepted
  date         2026-03-12
  context      """
    I049 validates journey-module feature coverage within deliverables.
    I051 validates milestone-module feature coverage. But no check validates
    the full deliverable→milestone→feature→module chain. Additionally,
    features scheduled in multiple milestones, priority mismatches, and
    implicit temporal ordering conflicts were undetected.
  """
  decision     """
    Add four new I-level diagnostics:
    - I076: Deliverable end-to-end chain gap (milestone features not in modules)
    - I077: Feature in multiple milestones (informational awareness)
    - I078: Priority escalation gap (critical feature in low milestone)
    - I079: Implicit milestone ordering (shared features, conflicting dates)

    All are info-level to support incremental adoption. I077 is intentionally
    informational — multi-milestone scheduling is valid for phased delivery.
    I078 requires explicit priority on both feature and milestone (absent
    priority suppresses the check). I079 requires target_date on both
    milestones.
  """
  consequences [
    "Full deliverable→milestone→feature→module chain validated",
    "Multi-milestone scheduling detected but not blocked",
    "Priority mismatches surfaced for planning awareness",
    "Implicit ordering conflicts detected between milestones",
    "Four new I-level codes (I076-I079) consuming reserved range",
    "All checks degrade gracefully for absent optional fields",
  ]
}

// ---------------------------------------------------------------------------
// v1.1 decisions — ownership, effort, release, temporal, blockers, flow
// ---------------------------------------------------------------------------

decision pe_ownership_as_fields "Ownership as String Fields" {
  status       accepted
  context      """
    Product managers need to know who owns features, milestones, deliverables,
    and releases. Options: (a) owner as entity reference to persona, (b) owner
    as free-form string, (c) a dedicated assignment entity.
  """
  decision     """
    Owner is a free-form string field (@optional) on feature, milestone,
    deliverable, and release. Contributors is a string list field. No entity
    references — supports any naming convention.
  """
  consequences [
    "Works with any naming convention (email, handle, team name)",
    "No coupling to persona model",
    "No validation that owner strings are consistent (typos possible)",
    "Queryable via owner-workload aggregate query",
  ]
  alternatives [
    "Entity reference to persona — rejected: breaks domain neutrality",
    "Dedicated assignment entity — rejected: over-engineering for a string field",
  ]
  tags         ["ownership", "v1-1"]
}

decision pe_effort_t_shirt_sizing "Effort as T-Shirt Sizes" {
  status       accepted
  context      """
    Feature-count milestone completion is misleading — a trivial feature and
    a month-long epic both count as 1. Options: numeric story points,
    Fibonacci sequence, t-shirt sizes, or hours estimation.
  """
  decision     """
    T-shirt sizes (xs, s, m, l, xl) mapped to Fibonacci weights (1, 2, 3, 5, 8).
    Applied to features only. Features without effort default to m=3 in queries.
  """
  consequences [
    "Five intuitive values, no calibration needed",
    "Fibonacci weights preserve relative sizing",
    "Coarser than story points — decompose large features instead",
    "Default to medium avoids blocking on estimation",
  ]
  alternatives [
    "Numeric story points — rejected: false precision, calibration overhead",
    "Hours/days — rejected: estimation anxiety, varies by person",
  ]
  tags         ["effort", "estimation", "v1-1"]
}

decision pe_release_ninth_entity "Release as Ninth Entity Kind" {
  status       accepted
  context      """
    Multiple deliverables ship together as coordinated releases. The 8-entity
    model had no way to answer 'what ships together?' Options: version-field
    grouping, tag-based grouping, or first-class release entity.
  """
  decision     """
    Add release as the 9th product entity kind. Releases group deliverables
    via ReleaseIncludesDeliverable edges, complete milestones via
    ReleaseCompletesMilestone edges, and order releases via ReleaseDependsOn.
    Lifecycle: planned->in_progress->released->recalled. Three new edge
    types (total 20). Previous 8-kinds invariant updated to 9.
  """
  consequences [
    "Explicit coordination semantics; release cycles are W092, bad versions W093",
    "Supports release changelogs and versioning",
    "One more entity kind to learn (optional adoption mitigates)",
    "Manifest counts change to 9 kinds / 20 edge types",
  ]
  alternatives [
    "Version-field grouping — rejected: implicit, no structural validation",
    "Tag-based grouping — rejected: no lifecycle, no dependency tracking",
    "Milestone as release proxy — rejected: milestones are planning, not shipping",
  ]
  tags         ["release", "entity-model", "v1-1"]
}

decision pe_temporal_start_date "Start Date on Milestones" {
  status       accepted
  context      """
    Milestones had only target_date. Without start dates, duration cannot be
    computed and critical path analysis is topology-only.
  """
  decision     """
    Add start_date (string @optional, ISO 8601) to milestones. I087 reports
    a start_date not in YYYY-MM-DD shape; no rule checks its order.
  """
  consequences [
    "Duration is computable (target_date - start_date)",
    "Critical path can be time-aware",
    "Two dates to maintain per milestone",
  ]
  alternatives [
    "Infer from predecessor — rejected: fragile, milestones can overlap",
    "Date range type — rejected: over-engineering for two dates",
  ]
  tags         ["temporal", "planning", "v1-1"]
}

decision pe_blockers_on_milestones "External Blockers on Milestones" {
  status       accepted
  context      """
    Blocked milestones only had depends_on for internal dependencies.
    External blockers could not be documented structurally.
  """
  decision     """
    Add blockers (string[] @optional) to milestones. Free-text descriptions
    of external impediments. I060 reports a blocked milestone with no
    blockers (depends_on doesn't count).
  """
  consequences [
    "External blockers are visible in the graph",
    "Every blocked milestone is expected to list its blockers",
    "Free-text — no structured tracking or resolution dates",
  ]
  alternatives [
    "Blocker entity — rejected: over-engineering for temporary notes",
    "depends_on with strings — rejected: breaks EntityId[] semantics",
  ]
  tags         ["blockers", "planning", "v1-1"]
}

decision pe_acceptance_criteria_future "Structured Acceptance Criteria" {
  status       proposed
  reason       "Deferred to entity_enhancement from @specforge/software"
  context      """
    Feature acceptance criteria are free-form string[]. Structured acceptance
    would allow traceability from feature->acceptance->behavior->test.
  """
  decision     """
    Defer structured acceptance to a future entity_enhancement from
    @specforge/software. The software extension can add acceptance_behaviors
    (EntityId[]) to features, linking acceptance to behaviors. This keeps
    @specforge/product domain-neutral.
  """
  consequences [
    "Preserves product extension domain neutrality",
    "Acceptance-behavior linking is a software engineering concern",
    "Until implemented, acceptance criteria remain unstructured",
  ]
  alternatives ["Add acceptance_behaviors to product — rejected: couples product to software"]
  tags         ["acceptance", "cross-extension", "v1-1"]
}

decision pe_journey_flow_feature_linking "Journey Flow Feature Linking" {
  status       accepted
  context      """
    Journey flow steps are string[] with no structural validation. Users
    naturally write [feature_id] in flow steps but this is never validated.
  """
  decision     """
    Validate bracketed references in flow steps against the journey's declared
    features list. I090 for unresolvable references. Lightweight structure
    without requiring a flow DSL.
  """
  consequences [
    "Catches typos in feature references",
    "No syntax change — existing flow strings work",
    "Only validates bracket patterns, non-bracketed references unchecked",
  ]
  alternatives [
    "Structured flow DSL — rejected: too complex, breaks existing flows",
    "Keep purely free-text — rejected: misses easy validation opportunity",
  ]
  tags         ["journeys", "validation", "v1-1"]
}

decision pe_release_v1_1_migration "Release Entity v1.1 Migration Path" {
  status       accepted
  context      """
    The release entity kind was introduced in v1.1 as the 9th product entity.
    Projects authored under v1.0 do not use release entities. When a v1.0
    project upgrades to v1.1, the question is: does anything break, and what
    happens to existing spec files that contain no release blocks?
  """
  decision     """
    Release entity adoption is fully additive and requires zero migration:
    1. The release kind is registered during pe_register_entity_kinds alongside
       the other 8 kinds — no conditional registration path.
    2. Every release field except version is optional; a project with zero
       release blocks parses and validates identically to v1.0.
    3. ReleaseIncludesDeliverable, ReleaseCompletesMilestone and
       ReleaseDependsOn edge types are registered but
       produce zero edges when no release entities exist — no phantom edges.
    4. Validation rules targeting releases (W092, W093) fire only
       when release entities are present in the graph. Zero releases = zero
       release diagnostics.
    5. CLI commands (specforge product releases, release-completion, etc.)
       return empty lists when no releases exist — consistent with other
       entity listing behavior.
    6. pe_query_release_completion returns null when no releases exist.
    No migration_hook is needed. No spec file changes are required. The
    pe_migration_additive_only decision governs: v1.1 is a minor version
    bump with additive-only changes.
  """
  consequences [
    "v1.0 projects upgrade to v1.1 with zero spec file changes",
    "Release entities are opt-in — ignored until first release block is written",
    "No migration_hook needed (additive-only per pe_migration_additive_only)",
    "Validation rules are release-presence-gated — no false diagnostics",
    "Release validation rules only fire when release entities exist",
    "CLI commands and their MCP tools return empty results, not errors",
  ]
  alternatives [
    "Conditional kind registration — rejected: adds branching to registration chain",
    "Separate extension for releases — rejected: release is structurally coupled to deliverable and milestone",
  ]
  tags         ["migration", "release", "v1-1", "backward-compatibility"]
}

decision pe_cross_extension_query_depth "Cross-Extension Query Depth" {
  status       proposed
  reason       "Deferred until extension interop protocol is stable"
  context      """
    Product queries traverse only the 20 product-owned edge types. Cross-extension
    queries require following foreign edges.
  """
  decision     """
    Defer cross-extension reverse queries. The 20-edge isolation guarantees
    are valuable for predictability. Cross-extension queries should be provided
    by extensions via surface contributions.
  """
  consequences [
    "Maintains query isolation guarantee",
    "No coupling to other extensions",
    "Cannot answer cross-extension questions from product CLI",
  ]
  alternatives [
    "Opt-in cross-extension traversal — rejected: breaks isolation invariant",
    "Wrapper queries — rejected: creates hard dependency on software extension",
  ]
  tags         ["cross-extension", "isolation", "v1-1"]
}

decision pe_ownership_field_exclusion "Ownership Field Exclusion Rationale" {
  status       accepted
  date         2026-03-12
  context      """
    v1.1 adds owner and contributors fields to 4 entity kinds: feature,
    milestone, deliverable, and release. The remaining 5 kinds (module, term,
    journey, persona, channel) do not receive ownership fields. This decision
    documents why.
  """
  decision     """
    Ownership fields are intentionally limited to the 4 entity kinds that
    represent actionable work items with individual accountability:

    - feature: someone owns the capability being built
    - milestone: someone owns the phase deadline and exit criteria
    - deliverable: someone owns the shippable artifact
    - release: someone owns the coordinated shipping event

    The 5 excluded kinds are excluded for specific reasons:

    - module: structural grouping, not a work item. Module ownership is
      implicit — the owner of the features within the module owns the module.
      Adding owner to modules would create redundant data that drifts from
      feature-level ownership.
    - term: vocabulary definitions are shared knowledge, not owned work.
      Terms evolve through consensus, not individual accountability.
    - journey: cross-functional flows spanning personas and channels.
      Journey ownership is ambiguous — is it the UX designer, the PM,
      or the engineer implementing the flow? The persona and feature
      owners are the accountable parties.
    - persona: user role archetypes are research artifacts, not work items.
      Persona definitions are owned by the product team collectively.
    - channel: interaction mediums are infrastructure concerns, not work
      items. Channel ownership is an ops/platform concern outside the
      product planning scope.

    Extensions can add ownership fields to any kind via entity_enhancements
    if a specific domain requires it. The product extension does not
    pre-wire ownership on kinds where it creates noise.
  """
  consequences [
    "4 kinds have owner/contributors: feature, milestone, deliverable, release",
    "5 kinds excluded: module, term, journey, persona, channel",
    "Owner workload query aggregates only the 4 ownable kinds",
    "Extensions can add owner to excluded kinds via entity_enhancements",
    "No redundant ownership data that drifts from feature-level accountability",
  ]
  alternatives [
    "Owner on all 9 kinds — rejected: creates noise on non-work-item entities",
    "Owner on modules too — rejected: redundant with feature-level ownership",
    "Configurable ownership fields — rejected: over-engineering for v1.1",
  ]
  invariants   [pe_ownership_field_awareness]
  tags         ["ownership", "v1-1", "entity-model"]
}

// ════════════════════════════════════════════════════════════════
// Cursor-Based Pagination for Matrix Queries
// ════════════════════════════════════════════════════════════════

decision pe_queries_as_derived_layer "Product Queries as Derived Convenience Layer" {
  status       accepted
  date         2026-03-13
  context      """
    The product extension provides 38 specialized query methods (coverage
    matrices, critical path, module coupling, term density, velocity metrics)
    that go beyond raw graph structure. These derivations are not part of the
    Graph Protocol JSON export. If a third-party compiler produces the
    identical Graph Protocol JSON, it would need to re-implement all 38
    queries to provide compatible CLI/MCP surfaces — creating lock-in to
    SpecForge's specific query engine and violating the vision principle
    "the standard is the moat."
  """
  decision     """
    Product queries are explicitly a derived convenience layer, NOT part of
    the Graph Protocol standard. Each query behavior contract documents its
    exact traversal algorithm — the edge types followed, the traversal
    direction, the aggregation logic — enabling any Graph Protocol consumer
    to independently derive the same results from the exported graph.

    The Graph Protocol output contains all raw entity fields and edges
    required for independent derivation. No query result depends on data
    absent from the graph export. Alternative compilers that produce a
    compliant Graph Protocol JSON are NOT required to implement these
    queries — they are extension-provided conveniences, not standard
    obligations.

    Each query method's contract serves as a reference algorithm
    specification. A different compiler, an agent, or a downstream tool
    can implement any query by following the documented traversal pattern
    against the standard graph export.
  """
  consequences [
    "Queries are convenience wrappers, not Graph Protocol requirements",
    "Graph Protocol output is sufficient for independent derivation",
    "Each query algorithm is fully documented in behavior contracts",
    "Alternative compilers need not implement queries to be compliant",
    "No lock-in: any tool can derive the same analytics from the graph",
    "Query methods are extension-scoped, not standard-scoped",
  ]
}

decision pe_status_transition_via_cache "Status Transitions via Explicit Build Cache" {
  status       accepted
  date         2026-03-13
  context      """
    Status transition validation (W087-W091, W094) requires comparing the
    current entity status against its previous status. A pure compiler sees
    only the current spec file — it has no knowledge of previous state.
    Implicitly tracking state across builds would make the compiler
    stateful, violating the manifesto's "deterministic infrastructure"
    commitment where identical inputs always produce identical outputs.
  """
  decision     """
    Status transition validation uses an explicit, opt-in build cache file
    (specforge-cache.json) as a declared compiler input. The cache records
    entity statuses from the previous successful build: product declares
    `status` the lifecycle field of feature, milestone, deliverable,
    persona, channel and release, and the host records the lifecycle field
    of each kind that declares one, naming no field itself (ADR 0009).
    When present,
    W087-W091/W094 compare current status against cached status. When
    absent, transition rules are suppressed and only vocabulary validity
    (E061) is checked.

    The cache file is a deterministic, reproducible artifact: same spec
    files always produce the same cache contents. It is NOT hidden compiler
    state — it is a version-controllable file that teams explicitly choose
    to maintain. This preserves deterministic compilation: identical inputs
    (spec files + optional cache file) always produce identical outputs.

    CI pipelines that want transition checking commit the cache file.
    Pipelines that want stateless builds simply omit it. Both modes are
    first-class supported.
  """
  consequences [
    "Transition validation requires explicit build cache (opt-in)",
    "First build or cache-disabled workflows: only enum validity checked",
    "Cache file is a declared input, not hidden compiler state",
    "CI pipelines can commit cache for transition checking",
    "Stateless builds remain fully deterministic without cache",
    "Same spec files + same cache = same diagnostics (deterministic)",
  ]
}

decision pe_analytics_not_graph_protocol "Analytics Namespace Separate from Graph Protocol" {
  status       accepted
  date         2026-03-13
  context      """
    Product analytics (coverage matrices, critical path, module coupling,
    term density, velocity metrics) are computed derivations of the graph.
    Including them in the Graph Protocol JSON would make every alternative
    compiler responsible for implementing the same analytics — creating
    coupling between the standard and one extension's query engine.
  """
  decision     """
    Computed analytics are NEVER included in the Graph Protocol standard
    output. The graph export contains raw entities and edges only. Analytics
    are available exclusively through extension query endpoints (CLI commands,
    auto-promoted MCP tools, ProductQueryPort). This keeps the Graph Protocol
    clean and portable while making analytics available to consumers who
    want them.
  """
  consequences [
    "Graph Protocol JSON contains only raw entities and edges",
    "Analytics are extension-scoped query results, not graph data",
    "Graph Protocol compliance does not require analytics implementation",
    "Agents can derive analytics from raw graph data independently",
    "Clean separation: standard (graph) vs convenience (queries)",
  ]
}

decision pe_effort_weights_configurable "Configurable Effort Weights" {
  status       superseded
  date         2026-03-13
  context      """
    The Effort enum (xs, s, m, l, xl) uses Fibonacci-inspired weights
    (1, 2, 3, 5, 8) for weighted milestone completion. This is an
    opinionated methodology choice from Agile planning poker. Teams using
    different estimation methodologies (linear hours, custom point scales)
    cannot override these weights. The vision emphasizes domain-neutrality:
    @specforge/product serves any domain, not just Agile software teams.
  """
  decision     """
    Superseded by ADR 0011: the weights define the effort scale
    (xs=1, s=2, m=3, l=5, xl=8, missing = m) and are not configurable; no
    project configuration reaches a command.

    Effort weights are configurable via the effort_weights map in the
    @specforge/product extension configuration within specforge.json.
    Default weights remain Fibonacci-inspired (xs=1, s=2, m=3, l=5, xl=8)
    for zero-config adoption. Teams MAY override any or all weights.
    The Effort enum values (xs, s, m, l, xl) remain fixed — only the
    numeric weights are configurable. This preserves cross-project
    compatibility of the enum while allowing methodology flexibility.
  """
  consequences [
    "Default weights work out-of-the-box for Agile teams",
    "Non-Agile teams can configure their own weight scale",
    "Effort enum values are stable across all configurations",
    "Weighted queries use configured weights, not hardcoded values",
    "specforge.json is the single configuration point",
  ]
}

decision pe_diagnostic_profiles "Diagnostic Profiles for Progressive Adoption" {
  status       deprecated
  date         2026-03-13
  context      """
    The product extension declares 68+ validation rules across three severity
    levels. A user writing their first spec file with a single feature entity
    would see multiple I-code diagnostics (I048 missing acceptance, I080 no
    owner, I081 no effort, W041 unreferenced feature). This punishing first
    experience violates the vision principle "structure is a spectrum" and
    "seconds to value" — one entity should be better than zero, not a wall
    of warnings.
  """
  decision     """
    Product validation operates in two diagnostic profiles:

    - default: Only E-codes (errors) and W-codes (warnings) are emitted.
      This is the out-of-the-box experience. A minimal spec file produces
      at most structural warnings (unreferenced entities, cycles) — never informational
      suggestions about missing optional fields.

    - pedantic: All E-codes, W-codes, AND I-codes are emitted. Enabled
      via --lint=pedantic flag or warning_level=pedantic in specforge.json.
      This is the comprehensive experience for mature projects that want
      full completeness checking.

    The profile affects ONLY I-code emission. E-codes and W-codes always
    fire regardless of profile. This preserves correctness (errors are
    always caught) while making the first experience welcoming.
  """
  consequences [
    "First spec file with one feature: zero or few diagnostics (default profile)",
    "Mature projects opt into pedantic for full completeness checking",
    "E-codes and W-codes always fire — correctness is never compromised",
    "I-codes are informational suggestions, not requirements",
    "Progressive adoption: start minimal, grow into pedantic over time",
    "specforge.json and CLI flag both control the profile",
  ]
}

decision pe_deterministic_time_queries "Deterministic Time-Dependent Queries" {
  status       accepted
  date         2026-03-13
  context      """
    Several product queries depend on the current wall-clock time:
    milestone velocity (days elapsed), milestone timeline (overdue
    detection via I058), and overdue milestone validation. Running
    specforge check on the same spec file at two different times could
    produce different diagnostics. This is non-deterministic, violating
    the manifesto's commitment to "deterministic infrastructure."
  """
  decision     """
    All time-dependent commands accept an explicit --as-of date that
    defaults to the date the host passes in CommandInput (today, UTC,
    YYYY-MM-DD; ADR 0011). The same spec files + the same --as-of always
    produce the same results.

    Overdue detection (I058) is moved from compile-time validation to
    query-time only. It fires when the milestone timeline query is
    executed (either via CLI command or MCP tool), NOT during
    specforge check. This means specforge check is fully deterministic —
    it never depends on wall-clock time. Overdue markers appear only in
    query results, not in the validation diagnostic set.

    The milestone velocity query likewise accepts as_of_date, making
    velocity calculations reproducible for testing and CI.
  """
  consequences [
    "specforge check is fully deterministic — no time dependency",
    "I058 fires in query results only, not during validation",
    "as_of_date parameter enables reproducible queries",
    "--as-of overrides the host date for reproducible queries in tests and CI",
    "CI pipelines get identical results regardless of execution time",
    "Same spec files = same validation diagnostics, always",
  ]
}

decision pe_cursor_pagination_decision "Cursor-Based Pagination for Matrix Queries" {
  status       superseded
  date         "2026-03-13"
  deciders     ["specforge-core"]
  context      """
    Matrix queries (persona-coverage-matrix, module-coupling, channel-coverage-matrix,
    owner-workload, feature-overlap) can produce result sets that grow quadratically
    with the number of entities. For a project with 2000+ entities, a coverage matrix
    can exceed 4MB of JSON output, which is problematic for MCP tool responses
    (typical context window budget is 100K tokens) and CLI stdout piping.

    Options considered:
    A. Offset-based pagination (--offset, --limit) like list commands.
    B. Cursor-based pagination (opaque cursor token, --page-size).
    C. Server-side filtering to reduce result size.
    D. Streaming response (NDJSON).

    Option A has the well-known problem of inconsistent results when the dataset
    changes between pages (entity added/deleted shifts offsets). Option C helps
    but doesn't solve the fundamental size issue. Option D requires streaming
    support in MCP (not available in v1). Option B provides stable pagination
    through opaque cursors that encode the position, making results consistent
    even if entities change between pages.
  """
  decision     """
    Superseded by ADR 0011: matrix queries page with the list
    commands' offset/limit (default 100, clamped to [1, 1000]) and report
    total and has_more. With no graph revision a cursor detects nothing an
    offset does not.

    Matrix queries MUST support cursor-based pagination via PaginatedQueryInput:
    - cursor: opaque string encoding the current position (null for first page)
    - page_size: maximum entries per page (default 50, max 500)

    Responses include PaginationMetadata:
    - next_cursor: opaque string for next page (null if no more pages)
    - total: total entry count at query time
    - has_more: boolean shorthand for next_cursor != null

    Cursor encoding: base64-encoded JSON containing the last entity ID and a
    monotonic sequence number. The sequence number detects graph rebuilds —
    if the sequence differs, the cursor is stale and the query restarts from
    the beginning with an informational diagnostic.

    Non-matrix queries (entity-scoped, project-wide scalars) do NOT use
    cursor pagination — their result sizes are bounded by entity count, not
    entity count squared.
  """
  consequences [
    "Matrix queries return bounded-size pages suitable for MCP tool responses",
    "Cursor-based pagination is stable across entity additions/deletions",
    "Stale cursors (after graph rebuild) restart from beginning — no silent inconsistency",
    "Cursor encoding is opaque — clients must not parse or construct cursors",
    "Non-matrix queries are unaffected — no pagination overhead for simple queries",
    "Default page_size of 50 fits comfortably within MCP context budgets",
    "Maximum page_size of 500 prevents accidental full-matrix dumps",
  ]
  tags         ["pagination", "queries", "scalability", "matrix-queries", "mcp"]
}

decision pe_enable_product_verify "Enable Verify Annotations on Product Entities" {
  status       proposed
  date         2026-03-16
  context      """
    Vision principle P5 (Traceability is a feedback loop, not a report) requires
    bidirectional linking between entities and their verification evidence.
    Product entities (feature, deliverable, milestone) had supportsVerify=false,
    meaning they could not declare verify annotations. Traceability was one-way
    only: behaviors referenced features, but features could not declare "I am
    done when these acceptance tests pass." This created a gap in the
    traceability feedback loop — product entities were invisible to the
    verify-based evidence chain.
  """
  decision     """
    Enable supportsVerify=true on feature, deliverable, and milestone entity
    kinds. Declare 'acceptance' as a custom verify kind in verify_kinds.
    Keep testable=false — product entities do NOT produce coverage obligations
    in specforge-report.json. The distinction:

    - testable=false: entity does not appear in the test coverage matrix
    - supportsVerify=true: entity CAN have verify annotations in .spec syntax

    This allows features to declare:
      verify acceptance "features/user_login.feature"
      verify acceptance "acceptance/onboarding_flow.feature"

    These verify annotations create traceability edges to external acceptance
    test files without making features testable in the coverage sense.
    The acceptance verify kind is intentionally broad — it can reference
    Gherkin .feature files, Playwright test files, manual test plans, or
    any external artifact that proves the product entity is "done."

    Journey, module, term, persona, channel, and release retain
    supportsVerify=false — they are structural/descriptive entities that
    do not have meaningful acceptance criteria.
  """
  consequences [
    "Features, deliverables, milestones can declare verify acceptance annotations",
    "Traceability feedback loop is closed: behavior->feature->acceptance test",
    "Product entities remain non-testable (no coverage obligations)",
    "verify_kinds declares ['acceptance'] — one verify kind for product",
    "Journey, module, term, persona, channel, release remain supportsVerify=false",
    "External acceptance test files are linked via file_reference mechanism",
    "ADR pe_nine_entity_kinds consequence 'All 9 kinds are non-testable' remains true",
  ]
  invariants   [pe_product_verify_support, pe_feature_non_testable]
  tags         ["traceability", "verify", "product-entities", "vision-alignment"]
}

decision pe_vocabularies_are_enum_fields "Product Vocabularies Are Enum Fields" {
  status       accepted
  date         2026-10-13
  context      """
    Each closed product vocabulary was copied by hand into the field
    description, a one_of warning rule and its message, the list filter,
    the filter's help and the lifecycle pass, and the copies drifted:
    technical_level, interaction_model and release status had closed list
    filters and no rule, so check admitted values the lists refused.
  """
  decision     """
    Each vocabulary is one constant (ADR 0071). Its field is declared an
    enum of it, with the value an absent status counts as as its default;
    a value outside it is the core's E061. The list filters, their help,
    the sorts, the lifecycle pass and the rollups read the same constant.
    W077-W080, W083-W085 and W095 are retired to E061.
  """
  consequences [
    "An off-vocabulary value is an error, as the list filters already treated it",
    "The editor completes the values and the schema lists them",
    "technical_level, interaction_model and release status are closed",
    "A status field's absent value is its declared default",
  ]
}
