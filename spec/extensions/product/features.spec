// @specforge/product features — capability groupings

// Note: behavior imports removed to break circular dependency.
// Behaviors are resolved globally — use imports not needed for reference resolution.

feature pe_core_entity_kinds "Product Entity Kind Registration" {
  status   done
  problem  """
    The @specforge/product extension must register 9 entity kinds with
    full metadata, 20 edge types, field definitions, and validation rules.
    Without this registration, the compiler has zero knowledge of product
    planning concepts including the domain-neutral feature entity, who
    uses the system (persona), through which medium (channel), or how
    deliverables ship together (release).
  """
  solution """
    A comprehensive manifest declaration provides all entity kinds with
    testability flags, LSP metadata (semantic tokens, icons), DOT shapes,
    typed field definitions with edge mappings, and declarative validation
    rules. All 20 edge types produce traversable graph edges — no
    reference field is left unwired. Registration follows the zero-entity
    core protocol defined in ExtensionDeclaration. Feature is a domain-neutral hub:
    any extension's entities can reference features via their own fields
    and peer dependencies. Persona and channel are first-class entity
    kinds referenced by journeys via JourneyTargetsPersona and JourneyUsesChannel
    edges. Release coordinates multi-deliverable shipping via
    ReleaseIncludesDeliverable, ReleaseCompletesMilestone and
    ReleaseDependsOn edges.
  """
}

feature pe_query_dependency_analysis "Product Dependency Analysis Queries" {
  status   done
  problem  """
    Product entities form dependency chains (FeatureDependsOn, ModuleDependsOn,
    MilestoneDependsOn) but there is no way to compute topological ordering,
    detect cycles, find reverse dependents, or measure dependency depth. Without
    these queries, agents cannot answer "what order should features be built?"
    or "what is the critical path through milestones?"
  """
  solution """
    Dependency-focused query behaviors: feature ordering (topological sort
    via FeatureDependsOn), feature dependents (reverse FeatureDependsOn),
    critical path (longest incomplete milestone chain with slack analysis),
    module dependency depth (longest ModuleDependsOn chain), and module
    coupling (shared feature count between modules). Entity-not-found errors
    include fuzzy-match suggestions.
  """
}

feature pe_query_traceability "Product Traceability Queries" {
  status   done
  problem  """
    Product entities are richly connected across kinds — features belong to
    milestones and deliverables, journeys reference personas and channels,
    modules contain features — but there is no way to trace from any entity
    to its related entities. Without traceability queries, agents cannot answer
    "which deliverables contain this feature?" or "what features does this
    persona need?"
  """
  solution """
    Traceability query behaviors that traverse the product graph to find
    related entities: deliverable traceability (transitive features via
    journeys and modules), feature deliverables (reverse traversal from
    feature to containing deliverables), feature milestones (reverse
    MilestoneDeliversFeature), persona journeys (reverse JourneyTargetsPersona), channel
    journeys (reverse JourneyUsesChannel), module deliverables (reverse
    DeliverableContainsModule), journey deliverables (reverse DeliverableSupportsJourney),
    deliverable personas (multi-hop deliverable->journey->persona traversal),
    milestone deliverables (reverse DeliverableTrackedByMilestone), module features
    (forward ModuleContainsFeature), persona channels (multi-hop persona->journey->
    channel traversal), persona features (multi-hop persona->journey->feature
    traversal), channel features (multi-hop channel->journey->feature
    traversal), term graph (N-hop TermReferencesRelatedTerm traversal), term clusters
    (connected components via TermReferencesRelatedTerm), and term density (terms per
    entity kind ratio). Entity-not-found errors include fuzzy-match
    suggestions.
  """
}

feature pe_query_coverage_analysis "Product Coverage Analysis Queries" {
  status   done
  problem  """
    Product entities should form a well-connected graph, but there is no way
    to measure how well entities are connected: which features lack milestone
    scheduling, which features appear in multiple deliverables, or how
    thoroughly personas and channels are covered by journeys. Without coverage
    queries, quality gaps in the product specification go undetected.
  """
  solution """
    Coverage-focused query behaviors: journey coverage (features with
    status=done per journey), unscheduled features (features with zero
    MilestoneDeliversFeature edges), feature overlap (features reachable from 2+
    deliverables), persona coverage matrix (per-persona reachability with
    coverage ratios), and channel coverage matrix (per-channel reachability
    with coverage ratios). Entity-not-found errors include fuzzy-match
    suggestions.
  """
}

feature pe_query_lifecycle_metrics "Product Lifecycle Metrics Queries" {
  status   done
  problem  """
    Product managers need to track progress and health across milestones,
    deliverables, and the overall product graph: completion ratios, timeline
    status, velocity trends, and priority derivation. Without lifecycle
    queries, agents cannot answer "how complete is this milestone?" or
    "which milestones are overdue?"
  """
  solution """
    Lifecycle-focused query behaviors: milestone completion (ratio of features
    with status=done), deliverable completion (aggregate milestone completion
    across DeliverableTrackedByMilestone edges), milestone timeline (chronological
    sort with query-time overdue detection via as_of_date parameter —
    not emitted during specforge check), milestone velocity (feature counts
    by status with days elapsed/remaining and features per day, using
    explicit as_of_date for determinism), deliverable
    priority (derived from constituent milestones and journeys via
    max-priority algorithm), and deliverable dependents (reverse
    DeliverableDependsOn). Entity-not-found errors include fuzzy-match
    suggestions.
  """
}

feature pe_surface_contributions "Product Surface Contributions" {
  status   done
  problem  """
    The @specforge/product extension has query behaviors and validation rules
    but no declared CLI commands or MCP tools to expose them to users and agents.
    Without typed input/output schemas, consumers cannot discover or validate
    the surface contract at compile time.
  """
  solution """
    The manifest declares the 40 specforge product commands as surface
    contributions, run by the extension's own cmd__product_* exports over
    the graph the host passes (MCP tools specforge.product.<id> by
    auto-promotion): listing for all 9 kinds (features, journeys,
    deliverables, milestones, modules, terms, personas, channels,
    releases), the multi-hop and computed queries (completion, coverage,
    impact, dependents, traceability, ordering, critical path, timeline,
    velocity, coverage matrices, term and module analytics, owner
    workload), plus bulk-status and health. One-hop neighbours are core's
    `specforge query <id> --depth 1 --kind <kind>`; the extension declares
    no MCP resources (ADR 0011).

    Every surface has a typed schema:
    - CLI list commands accept ProductListFilter (--status, --priority, --tags
      as a comma-separated string, --limit, --offset, --sort-by, --sort-order)
      and return per-kind list result types (FeatureListResult,
      JourneyListResult, etc.) with total, offset, limit and has_more.
    - CLI query commands accept the entity id as a positional arg named
      after its kind and return the corresponding query payload type.
    - Every command has the host's --format human|json (human by default on
      the CLI; MCP always asks for json).
    - Consistent errors on stderr: ENTITY_NOT_FOUND (exit 1, with a
      same-kind suggestion) and INVALID_INPUT (exit 2).
  """
}

feature pe_validation_suite "Product Validation Suite" {
  status   in_progress
  problem  """
    Without domain-specific validation rules, the compiler cannot detect
    product-level quality issues: unreferenced journeys, dependency cycles,
    milestones without features, or unused modules.
  """
  solution """
    Declarative validation rules (E007, E015, E052, W041-W046, W049, W057,
    W092, W093, I010, I046-I048, I050,
    I053-I055, I057, I059-I062, I066-I070, I080-I083, I086, I087, I089)
    detect common product
    specification quality issues. The core's declarative engine runs
    them, and every diagnostic is reported whatever its severity.

    Rules by severity: module, milestone and deliverable dependency
    cycles (E007, E015, E052); features, journeys and modules nothing
    references (W041, W042, W044); deliverables without journeys or
    modules (W043, W046); feature and release dependency
    cycles (W045, W092); milestones without features (W049); completed
    milestones without exit criteria (W057); a value outside a field's
    vocabulary (the core's E061; validate_product_vocabularies); release
    versions that are not semver (W093); terms with no edges (I010);
    personas and channels no journey references (I046, I047); features
    without acceptance criteria (I048); journeys with an empty flow
    (I050); milestone target dates, release dates and milestone start
    dates not in YYYY-MM-DD shape (I053, I086, I087); journeys without a
    persona or channels (I054, I055); blocked milestones without
    depends_on (I057); deferred features without reason (I059); blocked
    milestones without blockers (I060); deliverable versions that are
    not semver (I061); module families outside the standard set (I062);
    modules without features (I067); tags that are not
    lowercase-hyphenated (I068); deprecated deliverables, personas and
    channels without reason (I066, I069, I070); features, milestones,
    deliverables and releases without an owner (I080); features without
    effort (I081); releases without deliverables or milestones (I082,
    I083); recalled releases without reason (I089).
    Each rule uses the declarative pattern engine.

    What spans entities is checked by the extension's passes. The
    lifecycle pass runs with every compile: a completed milestone that
    delivers a feature neither done nor deprecated (W154), a done
    feature that depends on one not done (I063), a milestone due before
    a milestone it depends on (I064), a shipped deliverable tracked by a
    milestone not completed (I065). Under specforge analyze, which reads
    the recorded test report, the delivery_evidence pass reports a done
    feature the recorded tests do not prove (I071, ADR 0039).
  """
}

feature pe_graph_rendering "Product Graph Rendering" {
  status   in_progress
  problem  """
    Product entities are registered in the KindRegistry and appear in the
    entity graph, but there is no specification for how they render in the
    Graph Protocol JSON output produced by specforge export. Without this,
    consumers cannot predict the shape of product entities in exported output.
  """
  solution """
    Product entities render as standard graph nodes following the core Graph
    Protocol schema. The core emitter handles all entity kinds uniformly —
    no product-specific renderer is needed (contributes.renderers = false).
    Each product entity appears with its kind, id, fields (as declared in
    FieldRegistry), and edges (as declared in EdgeTypeRegistry) in all three
    export formats:
    - context: full entity with all fields and resolved edges
    - graph: entity node with edge list (no field bodies)
    - brief: entity id and kind only
    Field ordering in JSON output follows FieldRegistry declaration order.
    Edge ordering follows source entity's field declaration order. Entities
    with validation errors still appear in output with an _diagnostics array.
  """
}

feature pe_cross_extension_cooperation "Cross-Extension Cooperation" {
  status   in_progress
  problem  """
    @specforge/product declares no peer_dependencies and operates standalone,
    but @specforge/software declares a peer_dependency on product and
    contributes the BehaviorImplementsFeature edge (behavior->feature) and an entity_enhancement
    adding a behaviors field to milestone. There is no specification for how cross-extension
    cooperation is validated end-to-end.
  """
  solution """
    Cross-extension integration is validated through: (1) product queries
    respect the 20-edge-type allowlist and never follow foreign edges,
    (2) entity_enhancements from peer extensions add fields to product
    entity kinds without modifying the product manifest, (3) the BehaviorImplementsFeature
    edge from @specforge/software creates a traversable link from behaviors
    to features without product extension awareness. Integration testing
    verifies the product+software combination produces correct traceability.
  """
}

feature pe_partial_graph_queries "Partial Graph Query Behavior" {
  status   done
  problem  """
    When the entity graph contains validation errors (e.g., unreferenced entities,
    broken references), the behavior of product queries is unspecified.
    Consumers cannot predict whether queries return partial results, fail
    entirely, or silently omit invalid entities.
  """
  solution """
    Product queries operate on the structural graph, not the validation
    result. Entities with validation errors are still traversable and
    appear in query results. Queries never filter entities based on
    diagnostic state. Consumers can cross-reference query results with
    validation diagnostics to identify which returned entities have
    quality issues.
  """
}

feature pe_migration_strategy "Extension Migration Strategy" {
  status   in_progress
  problem  """
    The v1 manifest declares migration_hook=null (no prior version), but
    there is no specification for what happens when the product extension
    version bumps. Schema evolution, field additions, and diagnostic code
    changes need a documented migration path.
  """
  solution """
    The product extension follows additive-only schema evolution for minor
    versions: new fields are always optional, new diagnostic codes use
    reserved ranges, new edge types require manifest version bump. Breaking
    changes (field removal, kind removal, edge type removal) require a major
    version bump with a migration hook that transforms persisted graph state.
    The migration hook receives the old manifest version and returns a list
    of graph transformations. Until v2, migration_hook remains null.
  """
}

feature pe_planning_insights "Advanced Planning Insights" {
  status   done
  problem  """
    Product managers and agents cannot answer key planning questions
    without manual graph traversal: which features are unscheduled,
    which features overlap across deliverables, what is each persona's
    feature coverage, and what is the critical path through milestones.
    These questions require multi-hop traversal that no existing query
    provides.
  """
  solution """
    Four new query methods covering the planning blind spots:
    (1) queryUnscheduledFeatures returns features with zero MilestoneDeliversFeature
    edges, (2) queryFeatureOverlap returns features reachable from 2+
    deliverables, (3) queryPersonaCoverageMatrix computes per-persona
    reachability with coverage ratios, (4) queryCriticalPath computes
    the longest incomplete milestone chain with slack analysis. Each is
    a specforge product command (unscheduled-features, feature-overlap,
    coverage-matrix, critical-path), auto-promoted to an MCP tool.
  """
}

feature pe_chain_validation "End-to-End Chain Validation" {
  status   proposed
  problem  """
    Existing validation checks journey-module gaps (I049) and milestone-
    module gaps (I051) independently, but no check validates the full
    deliverable→milestone→feature→module chain. A deliverable can have
    milestones scheduling features that no deliverable module implements,
    with no diagnostic. Additionally, features scheduled in multiple
    milestones, priority mismatches between features and milestones,
    and implicit temporal ordering conflicts go undetected.
  """
  solution """
    Four new validation rules: I076 detects deliverable end-to-end chain
    gaps, I077 flags features in multiple milestones (informational),
    I078 detects priority escalation gaps (high-priority feature in
    low-priority milestone), I079 detects implicit milestone ordering
    conflicts (shared features with conflicting dates but no depends_on).
  """
}

// ---------------------------------------------------------------------------
// v1.1 features — ownership, effort, release, temporal, blockers, flow
// ---------------------------------------------------------------------------

feature pe_ownership_tracking "Ownership Tracking" {
  status   in_progress
  problem  """
    Product entities have no concept of who is responsible. Product managers
    cannot answer "what features does Alice own?" or "which milestones have
    no owner?" — basic PM queries that every planning tool supports.
  """
  solution """
    Add owner (string @optional) and contributors (string[] @optional) fields
    to feature, milestone, deliverable, and release entities. Provide an
    owner-workload query that aggregates ownership across all product entities.
  """
  tags     ["ownership", "planning", "v1-1"]
}

feature pe_effort_estimation "Effort Estimation" {
  status   done
  problem  """
    Milestone completion is feature-count-based: 3/5 done = 60%. But a
    trivial feature and a month-long epic both count as 1. This makes
    completion ratios misleading for capacity planning.
  """
  solution """
    Add effort field (t-shirt size: xs, s, m, l, xl) to features. The
    sizes weigh 1, 2, 3, 5 and 8 (a Fibonacci-inspired scale): the weights
    define the scale and are not configurable (ADR 0011). Provide a weighted milestone completion query. Features
    without effort default to m weight.
  """
  tags     ["estimation", "planning", "v1-1"]
}

feature pe_release_coordination "Release Coordination" {
  status   in_progress
  problem  """
    Deliverables have individual versions and statuses, but multiple
    deliverables often ship together. There is no way to answer "what
    ships together?" or track coordinated release readiness.
  """
  solution """
    Add release as the 9th product entity kind with fields: description,
    version (required), status, deliverables, milestones, target_date,
    release_date, changelog, depends_on, owner, contributors, reason, refs.
    Three new edge types: ReleaseIncludesDeliverable,
    ReleaseCompletesMilestone and ReleaseDependsOn.
  """
  tags     ["release", "coordination", "v1-1"]
}

feature pe_temporal_planning "Temporal Planning" {
  status   done
  problem  """
    Milestones have only target_date — no start date. Duration cannot be
    computed, and milestone dependency ordering cannot validate temporal
    consistency of start dates.
  """
  solution """
    Add start_date (string @optional, ISO 8601) to milestones. I087 reports
    a start_date not in YYYY-MM-DD shape; no rule checks its order relative
    to target_date.
  """
  tags     ["temporal", "planning", "v1-1"]
}

feature pe_external_blockers "External Blocker Tracking" {
  status   done
  problem  """
    Blocked milestones only reference internal dependencies (depends_on).
    External factors (regulatory, third-party APIs, hiring) that block
    progress cannot be documented structurally.
  """
  solution """
    Add blockers (string[] @optional) to milestones. I060 reports a
    blocked milestone that lists no blockers.
  """
  tags     ["blockers", "planning", "v1-1"]
}

feature pe_journey_flow_validation "Journey Flow Validation" {
  status   proposed
  problem  """
    Journey flow steps are free-text with no structural validation.
    Steps may reference features via [brackets] but there is no check
    that referenced features exist in the journey's features list.
  """
  solution """
    Validate bracketed references in flow steps against the journey's
    declared features. I090 warns when a flow step references an
    undeclared feature.
  """
  tags     ["validation", "journeys", "v1-1"]
}
