// Lifecycle validation rules — status transitions, deprecation, dependency
// consistency, chain validation, and v1.1 rules
//
// Split from validation-rules.spec. These behaviors describe lifecycle and
// state-machine checks declared as ValidationRulePattern entries in the
// @specforge/product manifest.

use "extensions/product/features"
use "product/features"
use "types/diagnostics"
use "types/graph"

defaults behavior {
  features [pe_validation_suite]
}

// ════════════════════════════════════════════════════════════════
// Status Transition Validation (W087-W091)
//
// These rules validate status transitions by comparing the current
// status against the previous status stored in the build cache.
// The build cache is an explicit, opt-in artifact — NOT implicit
// compiler state. It is produced by `specforge check --cache` and
// stored as a deterministic JSON file (specforge-cache.json) in the
// project root. When no cache file exists (first build or
// cache-disabled workflows), transition rules are suppressed.
//
// This design preserves deterministic compilation: the same inputs
// (spec files + cache file) always produce the same outputs. The
// cache file is a declared input, not hidden state. CI pipelines
// that want transition validation commit the cache file; pipelines
// that want stateless builds simply omit it.
//
// Without cache: only vocabulary validity is checked (E061).
// With cache: transition validity is additionally checked (W087-W091).
// ════════════════════════════════════════════════════════════════

behavior validate_feature_status_transition "Validate Feature Status Transition" {
  category validation
  types    [Diagnostic, FeatureStatusTransition, StatusTransitionViolation]
  contract """
    The @specforge/product extension MUST declare a custom validation
    rule that detects invalid feature status transitions. Valid transitions:
    proposed->accepted, proposed->deferred, accepted->in_progress,
    accepted->deferred, in_progress->done, in_progress->deferred,
    deferred->proposed, deferred->accepted, done->deprecated.
    The deprecated state is terminal.
    Invalid transitions MUST produce a W087 warning including the current
    status, attempted status, and list of valid targets.

    Transition detection requires an explicit build cache file
    (specforge-cache.json) as a declared compiler input. The cache records
    entity statuses from the previous build. When no cache exists,
    W087 is suppressed entirely — only vocabulary validity (E061) is checked.
  """
  ensures {
    fires_on_invalid    "feature transitioning to an unreachable status produces W087"
    allows_valid        "feature following a valid transition suppresses W087"
    done_to_deprecated  "feature with status=done transitioning to deprecated is valid"
    terminal_deprecated "feature with status=deprecated transitioning to any other status produces W087"
    cache_required      "W087 requires build cache; suppressed when no cache file exists"
    absent_exempt       "feature not present in cache (new entity) suppresses W087"
    includes_targets    "W087 message includes valid target statuses for the current state"
  }
  features [product_validation]
  verify unit "proposed->accepted is valid, suppresses W087"
  verify unit "proposed->done is invalid, produces W087"
  verify unit "done->deprecated is valid, suppresses W087"
  verify unit "deprecated->proposed is invalid (terminal), produces W087"
  verify unit "no cache file: W087 suppressed entirely"
  verify unit "new entity not in cache: W087 suppressed"
  verify unit "W087 message lists valid targets"
}

behavior validate_milestone_status_transition "Validate Milestone Status Transition" {
  category validation
  types    [Diagnostic, MilestoneStatusTransition, StatusTransitionViolation]
  contract """
    The @specforge/product extension MUST declare a custom validation
    rule that detects invalid milestone status transitions. Valid transitions:
    planned->in_progress, planned->blocked, in_progress->completed,
    in_progress->blocked, blocked->planned, blocked->in_progress.
    The completed state is terminal. Invalid transitions MUST produce
    a W088 warning. Requires build cache for comparison; suppressed
    when no cache file exists.
  """
  ensures {
    fires_on_invalid   "milestone transitioning to unreachable status produces W088"
    allows_valid       "milestone following valid transition suppresses W088"
    terminal_completed "completed milestone transitioning to any other status produces W088"
    cache_required     "W088 requires build cache; suppressed when no cache file exists"
    absent_exempt      "milestone not present in cache (new entity) suppresses W088"
  }
  verify unit "planned->in_progress is valid, suppresses W088"
  verify unit "planned->completed is invalid, produces W088"
  verify unit "completed->planned is invalid (terminal), produces W088"
  verify unit "no cache file: W088 suppressed entirely"
}

behavior validate_deliverable_status_transition "Validate Deliverable Status Transition" {
  category validation
  types    [Diagnostic, DeliverableStatusTransition, StatusTransitionViolation]
  contract """
    The @specforge/product extension MUST declare a custom validation
    rule that detects invalid deliverable status transitions. Valid
    transitions: draft->in_progress, in_progress->shipped,
    in_progress->draft, shipped->deprecated. The deprecated state is
    terminal. Invalid transitions MUST produce a W089 warning. Requires
    build cache for comparison; suppressed when no cache file exists.
  """
  ensures {
    fires_on_invalid    "deliverable transitioning to unreachable status produces W089"
    allows_valid        "deliverable following valid transition suppresses W089"
    terminal_deprecated "deprecated deliverable transitioning to any other status produces W089"
    cache_required      "W089 requires build cache; suppressed when no cache file exists"
    absent_exempt       "deliverable not present in cache (new entity) suppresses W089"
  }
  verify unit "draft->in_progress is valid, suppresses W089"
  verify unit "draft->shipped is invalid, produces W089"
  verify unit "deprecated->draft is invalid (terminal), produces W089"
}

behavior validate_persona_status_transition "Validate Persona Status Transition" {
  category validation
  types    [Diagnostic, PersonaStatusTransition, StatusTransitionViolation]
  contract """
    The @specforge/product extension MUST declare a custom validation
    rule that detects invalid persona status transitions. Valid
    transitions: active->deprecated. The deprecated state is terminal.
    Invalid transitions MUST produce a W090 warning. Requires build
    cache for comparison; suppressed when no cache file exists.
  """
  ensures {
    fires_on_invalid    "persona transitioning to unreachable status produces W090"
    allows_valid        "persona active->deprecated suppresses W090"
    terminal_deprecated "deprecated persona reverting to active produces W090"
    cache_required      "W090 requires build cache; suppressed when no cache file exists"
  }
  verify unit "active->deprecated is valid, suppresses W090"
  verify unit "deprecated->active is invalid (terminal), produces W090"
}

behavior validate_channel_status_transition "Validate Channel Status Transition" {
  category validation
  types    [Diagnostic, ChannelStatusTransition, StatusTransitionViolation]
  contract """
    The @specforge/product extension MUST declare a custom validation
    rule that detects invalid channel status transitions. Valid
    transitions: active->deprecated. The deprecated state is terminal.
    Invalid transitions MUST produce a W091 warning. Requires build
    cache for comparison; suppressed when no cache file exists.
  """
  ensures {
    fires_on_invalid    "channel transitioning to unreachable status produces W091"
    allows_valid        "channel active->deprecated suppresses W091"
    terminal_deprecated "deprecated channel reverting to active produces W091"
    cache_required      "W091 requires build cache; suppressed when no cache file exists"
  }
  verify unit "active->deprecated is valid, suppresses W091"
  verify unit "deprecated->active is invalid (terminal), produces W091"
}

// ════════════════════════════════════════════════════════════════
// First-Build Initial Status Validation (I092-I093)
//
// W087-W091 are suppressed when no build cache exists. These rules
// catch invalid initial states that would otherwise pass silently,
// closing the first-build validation gap. They fire regardless of
// cache presence since they check absolute state, not transitions.
// I092-I093 are info codes.
// ════════════════════════════════════════════════════════════════

behavior detect_terminal_initial_status "Detect Terminal Status on First Build" {
  category validation
  types    [
    Diagnostic,
    FeatureStatus,
    MilestoneStatus,
    DeliverableStatus,
    PersonaStatus,
    ChannelStatus,
    ReleaseStatus,
  ]
  contract """
    The @specforge/product extension SHOULD detect entities whose initial
    status is a terminal state (feature=done, milestone=completed,
    deliverable=deprecated, persona=deprecated, channel=deprecated,
    release=recalled) but that lack evidence of having passed through
    the lifecycle. On first build (no prior build state), transition rules
    (W087-W091) are suppressed, so a feature could appear with status=done
    without ever having been proposed, accepted, or in_progress. This rule
    produces I092 as an informational diagnostic for each such entity,
    alerting the user that the entity started in a terminal state.

    Terminal states per kind:
    - feature: done
    - milestone: completed
    - deliverable: deprecated
    - persona: deprecated
    - channel: deprecated
    - release: recalled

    The rule fires ONLY on first build (no prior build state) or for entities
    not present in any prior build. If a prior build exists and the entity
    was in the prior build, W087-W091 handle transition validation instead.
  """
  ensures {
    fires_terminal_first_build  "entity starting in terminal state on first build produces I092"
    suppresses_non_terminal     "entity starting in non-terminal state suppresses I092"
    suppresses_with_prior_build "entity present in prior build suppresses I092 (W087-W091 apply)"
    per_kind_terminal           "terminal state is kind-specific: done for feature, completed for milestone, etc."
  }
  verify unit "feature with status=done on first build produces I092"
  verify unit "feature with status=proposed on first build suppresses I092"
  verify unit "milestone with status=completed on first build produces I092"
  verify unit "deliverable with status=deprecated on first build produces I092"
  verify unit "entity in prior build does not trigger I092 even if terminal"
}

behavior detect_inconsistent_initial_done "Detect Done Feature Without Acceptance on First Build" {
  category validation
  types    [Diagnostic, ProductFeature, FeatureStatus]
  contract """
    The @specforge/product extension SHOULD detect features with status=done
    but empty acceptance criteria on first build. This is a stronger check
    than I048 (which fires regardless of status) — I093 specifically flags
    the combination of terminal status + missing acceptance as evidence of
    an incomplete bootstrap. A "done" feature SHOULD have acceptance criteria
    documenting what "done" means.

    Fires only on first build (no prior build state) or for new entities.
    Subsequent builds rely on transition validation (W087) instead.
  """
  ensures {
    fires_done_empty_acceptance "feature with status=done and empty acceptance on first build produces I093"
    suppresses_non_done         "feature with status!=done suppresses I093 regardless of acceptance"
    suppresses_with_acceptance  "feature with status=done and non-empty acceptance suppresses I093"
    suppresses_with_prior_build "feature in prior build suppresses I093"
  }
  verify unit "done feature without acceptance on first build produces I093"
  verify unit "done feature with acceptance on first build suppresses I093"
  verify unit "proposed feature without acceptance suppresses I093 (I048 handles this)"
  verify unit "done feature in prior build suppresses I093"
}

// ════════════════════════════════════════════════════════════════
// Dependency Consistency
// ════════════════════════════════════════════════════════════════

behavior detect_blocked_milestone_without_dependency "Detect Blocked Milestone Without Dependency" {
  category validation
  types    [Diagnostic, ProductMilestone]
  contract """
    The @specforge/product extension MUST declare a conditional_field_required
    rule on milestones: when status is blocked, depends_on is required. A
    blocked milestone with no depends_on field, or an empty one, MUST
    produce an I057 info diagnostic: "milestone '{id}' has status 'blocked'
    but no depends_on". A blocked milestone with no dependencies has
    nothing to wait for — the status may be stale.
  """
  ensures {
    fires_blocked_empty    "blocked milestone with no or empty depends_on produces I057"
    suppresses_with_deps   "blocked milestone with at least one depends_on suppresses I057"
    suppresses_non_blocked "non-blocked milestone without depends_on suppresses I057"
  }
  verify unit "blocked milestone with depends_on suppresses I057"
  verify unit "blocked milestone without depends_on produces I057"
  verify unit "in_progress milestone without depends_on suppresses I057"
}

behavior detect_deferred_feature_without_reason "Detect Deferred Feature Without Reason" {
  category validation
  types    [Diagnostic, ProductFeature, FeatureStatus]
  contract """
    The @specforge/product extension SHOULD detect features with
    status=deferred but empty or missing reason field. A deferred feature
    without a reason has no documented justification for deferral.
    Produces an I059 info diagnostic.
  """
  ensures {
    fires_deferred_empty    "deferred feature with empty reason produces I059"
    suppresses_with_reason  "deferred feature with non-empty reason suppresses I059"
    suppresses_non_deferred "non-deferred feature without reason suppresses I059"
  }
  verify unit "deferred feature with reason suppresses I059"
  verify unit "deferred feature without reason produces I059"
  verify unit "accepted feature without reason suppresses I059"
}

behavior detect_blocked_milestone_without_reason "Detect Blocked Milestone Without Reason" {
  category validation
  types    [Diagnostic, ProductMilestone, MilestoneStatus]
  contract """
    No rule checks a blocked milestone's reason field. The rule for blocked
    milestones is I060, a conditional_field_required rule on blockers (see
    detect_blocked_milestone_without_blockers): a blocked milestone with a
    reason but no blockers still produces I060, and one with blockers but
    no reason produces none.
  """
  ensures {
    reason_does_not_suppress "blocked milestone with a reason but no blockers produces I060"
    reason_not_required      "blocked milestone with blockers but no reason produces no I060"
  }
  verify unit "blocked milestone with reason but no blockers produces I060"
  verify unit "blocked milestone with blockers but no reason produces no I060"
}

behavior detect_done_feature_with_incomplete_deps "Detect Done Feature With Incomplete Dependencies" {
  category validation
  types    [Diagnostic, ProductFeature, FeatureStatus]
  contract """
    The @specforge/product extension's lifecycle pass (check phase)
    MUST detect features with status=done that depend on features NOT
    having status=done via FeatureDependsOn edges (a feature without a
    status is proposed). A done feature with incomplete dependencies
    indicates a status inconsistency. Produces an I063 info diagnostic
    per incomplete dependency.
  """
  ensures {
    fires_per_incomplete "each non-done dependency of a done feature produces one I063"
    identifies_both      "I063 identifies both the done feature and its non-done dependency"
    suppresses_all_done  "done feature with all dependencies also done suppresses I063"
    suppresses_non_done  "non-done feature with incomplete dependencies suppresses I063"
    no_deps_no_fire      "done feature with no dependencies produces no I063"
  }
  verify unit "done feature with all done deps suppresses I063"
  verify unit "done feature with non-done dep produces I063"
  verify unit "in_progress feature with non-done dep suppresses I063"
  verify unit "done feature with no deps produces no I063"
}

behavior detect_milestone_temporal_inconsistency "Detect Milestone Temporal Inconsistency" {
  category validation
  types    [Diagnostic, ProductMilestone, MilestoneStatus]
  contract """
    The @specforge/product extension's lifecycle pass (check phase)
    MUST detect milestones that depend on other milestones (via
    MilestoneDependsOn) where the dependent milestone has an earlier
    target_date than its dependency; a date not in YYYY-MM-DD shape is
    not compared. This suggests
    a scheduling inconsistency — a milestone cannot realistically complete
    before its prerequisites. Produces an I064 info diagnostic per
    inconsistent dependency pair.
  """
  ensures {
    fires_per_inconsistency "milestone with earlier target_date than its dependency produces I064"
    identifies_pair         "I064 identifies both the milestone and the dependency with their dates"
    suppresses_consistent   "milestone with later target_date than dependency suppresses I064"
    absent_dates_no_fire    "milestone or dependency without target_date produces no I064"
  }
  verify unit "milestone before its dependency produces I064"
  verify unit "milestone after its dependency suppresses I064"
  verify unit "milestone or dependency without target_date produces no I064"
}

behavior detect_shipped_deliverable_incomplete_milestones "Detect Shipped Deliverable with Incomplete Milestones" {
  category   validation
  invariants [deliverable_lifecycle_consistency]
  types      [Diagnostic, ProductDeliverable, DeliverableStatus, ProductMilestone, MilestoneStatus]
  contract   """
    The @specforge/product extension's lifecycle pass (check phase)
    MUST detect deliverables with status=shipped that contain milestones
    not having status=completed via DeliverableTrackedByMilestone edges. A shipped deliverable with incomplete
    milestones indicates a status inconsistency. Produces an I065 info
    diagnostic per incomplete milestone.
  """
  ensures {
    fires_per_incomplete     "each non-completed milestone in a shipped deliverable produces one I065"
    identifies_both          "I065 identifies both the shipped deliverable and its non-completed milestone"
    suppresses_all_completed "shipped deliverable with all milestones completed suppresses I065"
    suppresses_non_shipped   "non-shipped deliverable with incomplete milestones suppresses I065"
    no_milestones_no_fire    "shipped deliverable with no milestones produces no I065"
  }
  verify unit "shipped deliverable with all completed milestones suppresses I065"
  verify unit "shipped deliverable with non-completed milestone produces I065"
  verify unit "draft deliverable with non-completed milestone suppresses I065"
  verify unit "shipped deliverable with no milestones produces no I065"
}

behavior detect_journeys_without_persona "Detect Journeys without Persona" {
  category validation
  types    [Diagnostic, ProductJourney]
  contract """
    The @specforge/product extension MUST declare a missing_required_field
    rule on the journey persona field. A journey with no persona field MUST
    produce an I054 info diagnostic: "journey '{id}' has no persona". A
    journey without a persona has no defined user role, weakening
    traceability. Info-level respects incremental adoption — journeys may
    be authored before personas are defined.
  """
  ensures {
    fires_when_absent  "journey without persona reference produces I054"
    suppresses_present "journey with persona reference suppresses I054"
  }
  verify unit "journey with persona suppresses I054"
  verify unit "journey without persona produces I054"
}

behavior detect_journeys_without_channels "Detect Journeys without Channels" {
  category validation
  types    [Diagnostic, ProductJourney]
  contract """
    The @specforge/product extension MUST declare a no_outgoing_edges rule
    on journeys scoped to the JourneyUsesChannel edge. A journey with no
    edge to a channel (no channels field, an empty list, or only references
    that don't resolve) MUST produce an I055 info diagnostic: "journey
    '{id}' uses no channels". A journey without channels has no defined
    interaction medium, weakening traceability. Info-level respects
    incremental adoption — journeys may be authored before channels are
    defined.
  """
  ensures {
    fires_when_absent  "journey without channel references produces I055"
    suppresses_present "journey with at least one channel reference suppresses I055"
  }
  verify unit "journey with channels suppresses I055"
  verify unit "journey without channels produces I055"
  verify unit "journey with an empty channels list produces I055"
}

// ════════════════════════════════════════════════════════════════
// Deprecation
// ════════════════════════════════════════════════════════════════

behavior detect_deprecated_deliverable_without_reason "Detect Deprecated Deliverable Without Reason" {
  category validation
  types    [Diagnostic, ProductDeliverable, DeliverableStatus]
  contract """
    The @specforge/product extension SHOULD detect deliverables with
    status=deprecated but empty or missing reason field. A deprecated
    deliverable without a reason has no documented justification.
    Produces an I066 info diagnostic.
  """
  ensures {
    fires_deprecated_empty    "deprecated deliverable with empty reason produces I066"
    suppresses_with_reason    "deprecated deliverable with non-empty reason suppresses I066"
    suppresses_non_deprecated "non-deprecated deliverable without reason suppresses I066"
  }
  verify unit "deprecated deliverable with reason suppresses I066"
  verify unit "deprecated deliverable without reason produces I066"
  verify unit "draft deliverable without reason suppresses I066"
}

behavior detect_deprecated_persona_without_reason "Detect Deprecated Persona Without Reason" {
  category validation
  types    [Diagnostic, ProductPersona, PersonaStatus]
  contract """
    The @specforge/product extension SHOULD detect personas with
    status=deprecated but empty or missing reason field. A deprecated
    persona without a reason has no documented justification for
    deprecation. Produces an I069 info diagnostic.
  """
  ensures {
    fires_deprecated_empty    "deprecated persona with empty reason produces I069"
    suppresses_with_reason    "deprecated persona with non-empty reason suppresses I069"
    suppresses_non_deprecated "non-deprecated persona without reason suppresses I069"
    absent_treated_active     "persona without status field produces no I069"
  }
  verify unit "deprecated persona with reason suppresses I069"
  verify unit "deprecated persona without reason produces I069"
  verify unit "active persona without reason suppresses I069"
  verify unit "persona without status produces no I069"
}

behavior detect_deprecated_channel_without_reason "Detect Deprecated Channel Without Reason" {
  category validation
  types    [Diagnostic, ProductChannel, ChannelStatus]
  contract """
    The @specforge/product extension SHOULD detect channels with
    status=deprecated but empty or missing reason field. A deprecated
    channel without a reason has no documented justification for
    deprecation. Produces an I070 info diagnostic.
  """
  ensures {
    fires_deprecated_empty    "deprecated channel with empty reason produces I070"
    suppresses_with_reason    "deprecated channel with non-empty reason suppresses I070"
    suppresses_non_deprecated "non-deprecated channel without reason suppresses I070"
    absent_treated_active     "channel without status field produces no I070"
  }
  verify unit "deprecated channel with reason suppresses I070"
  verify unit "deprecated channel without reason produces I070"
  verify unit "active channel without reason suppresses I070"
  verify unit "channel without status produces no I070"
}

behavior detect_transitive_deprecated_persona "Detect Transitive Deprecated Persona Reference" {
  category validation
  types    [Diagnostic]
  contract """
    The @specforge/product extension SHOULD detect when a deliverable
    references a journey that uses a deprecated persona. Unlike W075
    (direct journey->deprecated persona), I073 fires on the deliverable
    level to surface transitive lifecycle issues. This helps product
    managers identify deliverables that ship user flows targeting
    deprecated user roles.
  """
  ensures {
    fires_transitive       "deliverable->journey->deprecated persona produces I073 on the deliverable"
    identifies_journey     "I073 names the intermediate journey and the deprecated persona"
    suppresses_active      "deliverable->journey->active persona suppresses I073"
    per_journey_diagnostic "I073 fires per affected journey within the deliverable"
  }
  verify unit "deliverable with journey referencing deprecated persona produces I073"
  verify unit "deliverable with journey referencing active persona suppresses I073"
  verify unit "deliverable with multiple journeys fires I073 per affected journey"
}

behavior detect_transitive_deprecated_channel "Detect Transitive Deprecated Channel Reference" {
  category validation
  types    [Diagnostic]
  contract """
    The @specforge/product extension SHOULD detect when a deliverable
    references a journey that uses a deprecated channel. Unlike W076
    (direct journey->deprecated channel), I074 fires on the deliverable
    level to surface transitive lifecycle issues. This helps product
    managers identify deliverables that ship user flows through
    deprecated interaction mediums.
  """
  ensures {
    fires_transitive       "deliverable->journey->deprecated channel produces I074 on the deliverable"
    identifies_journey     "I074 names the intermediate journey and the deprecated channel"
    suppresses_active      "deliverable->journey->active channel suppresses I074"
    per_journey_diagnostic "I074 fires per affected journey within the deliverable"
  }
  verify unit "deliverable with journey referencing deprecated channel produces I074"
  verify unit "deliverable with journey referencing active channel suppresses I074"
  verify unit "deliverable with multiple journeys fires I074 per affected journey"
}

// ════════════════════════════════════════════════════════════════
// End-to-End Chain Validation (I076-I079)
// ════════════════════════════════════════════════════════════════

behavior detect_deliverable_chain_gap "Detect Deliverable End-to-End Chain Gap" {
  features [pe_chain_validation]
  category validation
  types    [Diagnostic]
  contract """
    The @specforge/product extension MUST declare a custom validation
    rule that traces the full deliverable->milestone->feature->module
    chain. For each deliverable, if a milestone schedules features
    that are not covered by any of the deliverable's modules, an I076
    info diagnostic MUST be produced identifying the gap. This is a
    deeper check than I049 (journey-module gap) — it validates the
    milestone path as well.
  """
  ensures {
    fires_when_gap   "deliverable with milestone feature not in any deliverable module produces I076"
    suppresses_match "deliverable where all milestone features are in modules suppresses I076"
    per_feature      "I076 fires once per uncovered feature, not once per milestone"
  }
  verify unit "deliverable with matching milestone-module features passes"
  verify unit "deliverable with milestone feature not in any module produces I076"
  verify unit "I076 identifies the specific uncovered feature and milestone"
}

behavior detect_feature_multi_milestone "Detect Feature in Multiple Milestones" {
  features [pe_chain_validation]
  category validation
  types    [Diagnostic]
  contract """
    The @specforge/product extension MUST declare a custom validation
    rule that detects features scheduled in more than one milestone.
    An I077 info diagnostic MUST be produced per feature identifying
    all milestones that reference it. This is informational — multi-
    milestone scheduling may be intentional for phased delivery.
  """
  ensures {
    fires_multi       "feature referenced by 2+ milestones produces I077"
    suppresses_single "feature referenced by exactly 1 milestone suppresses I077"
    lists_milestones  "I077 message lists all milestone IDs referencing the feature"
  }
  verify unit "feature in two milestones produces I077"
  verify unit "feature in one milestone suppresses I077"
  verify unit "feature in zero milestones suppresses I077"
}

behavior detect_priority_escalation_gap "Detect Priority Escalation Gap" {
  features [pe_chain_validation]
  category validation
  types    [Diagnostic, Priority]
  contract """
    The @specforge/product extension MUST declare a custom validation
    rule that detects features with higher priority than their containing
    milestone. If a feature with priority=critical or priority=high is
    scheduled in a milestone with priority=low, an I078 info diagnostic
    MUST be produced. Priority ordering: critical > high > medium > low.
    Missing priority on either side suppresses the check.
  """
  ensures {
    fires_escalation "critical feature in low-priority milestone produces I078"
    suppresses_match "high feature in high-priority milestone suppresses I078"
    absent_exempt    "feature or milestone without priority suppresses I078"
  }
  verify unit "critical feature in low milestone produces I078"
  verify unit "high feature in medium milestone suppresses I078"
  verify unit "feature without priority in low milestone suppresses I078"
}

behavior detect_milestone_implicit_ordering "Detect Milestone Implicit Ordering Conflict" {
  features [pe_chain_validation]
  category validation
  types    [Diagnostic]
  contract """
    The @specforge/product extension MUST declare a custom validation
    rule that detects milestones sharing features where the earlier
    milestone (by target_date) has not declared a depends_on to the
    later milestone. When milestone A has target_date before milestone B,
    and both schedule the same feature, but A does not depend_on B
    (directly or transitively), an I079 info diagnostic MUST be produced.
    Milestones without target_date are excluded from this check.
  """
  ensures {
    fires_implicit "milestones sharing feature with conflicting dates and no dependency produce I079"
    suppresses_dep "milestones with explicit depends_on suppress I079"
    absent_exempt  "milestones without target_date suppress I079"
  }
  verify unit "two milestones sharing feature with conflicting dates produce I079"
  verify unit "milestones with depends_on between them suppress I079"
  verify unit "milestones without target_date suppress I079"
}

// ════════════════════════════════════════════════════════════════
// Version Coherence (I094)
// ════════════════════════════════════════════════════════════════

behavior detect_deliverable_version_release_mismatch "Detect Deliverable-Release Version Mismatch" {
  category validation
  contract """
    The @specforge/product extension SHOULD detect deliverables whose
    version field does not match the version of the release they ship in.
    When a deliverable has a version field AND is referenced by a release
    (via ReleaseIncludesDeliverable edge) that also has a version field, the
    deliverable version SHOULD be a prefix of or equal to the release
    version, OR the deliverable version SHOULD be independently consistent
    within the release.

    The rule fires I094 when:
    1. A deliverable has version=X
    2. The deliverable is in release with version=Y
    3. X and Y are both valid SemVer
    4. The major version of X differs from the major version of Y

    This catches clear mismatches (e.g., deliverable 0.1.0 in release 2.0.0)
    without being overly strict about minor/patch alignment, since
    deliverables within a release may have independent version streams.

    Deliverables without a version field or releases without a version
    field suppress the rule. Multiple releases referencing the same
    deliverable check each release independently.
  """
  ensures {
    fires_major_mismatch   "deliverable with major version != release major version produces I094"
    suppresses_major_match "deliverable with matching major version suppresses I094"
    suppresses_no_version  "deliverable without version field suppresses I094"
    suppresses_no_release  "deliverable not in any release suppresses I094"
    per_release_check      "deliverable in multiple releases is checked against each"
  }
  features [pe_chain_validation]
  verify unit "deliverable 0.1.0 in release 0.5.0 suppresses I094 (same major)"
  verify unit "deliverable 0.1.0 in release 2.0.0 produces I094 (different major)"
  verify unit "deliverable without version in any release suppresses I094"
  verify unit "deliverable in two releases checked independently"
  verify unit "release without version suppresses I094"
}

// ════════════════════════════════════════════════════════════════
// v1.1 rules — release, ownership, effort, temporal, flow
// ════════════════════════════════════════════════════════════════

behavior detect_release_dependency_cycles "Detect Release Dependency Cycles" {
  features    [pe_release_coordination]
  category    validation
  invariants  [release_dag]
  produces    [pe_release_cycle_detected]
  contract    """
    The compiler SHOULD detect circular dependencies between release entities
    via depends_on fields. Cycles produce a W092 warning.
  """
  diagnostic  W092
  severity    warning
  description "Circular release dependency detected"
  verify unit "two releases with mutual depends_on produce W092"
  verify unit "linear release chain produces no W092"
  verify unit "self-referencing release produces W092"
}

behavior detect_release_without_deliverables "Detect Release Without Deliverables" {
  category    validation
  features    [pe_release_coordination]
  contract    """
    The @specforge/product extension MUST declare a no_outgoing_edges rule
    on releases scoped to the ReleaseIncludesDeliverable edge. A release
    with no edge to a deliverable (no deliverables field, an empty list, or
    only references that don't resolve) SHOULD produce an I082 info
    diagnostic: "release '{id}' includes no deliverables".
  """
  diagnostic  I082
  severity    info
  description "Release has no deliverables"
  verify unit "release with deliverables produces no I082"
  verify unit "release with empty deliverables list produces I082"
  verify unit "release without a deliverables field produces I082"
}

behavior detect_release_without_milestones "Detect Release Without Milestones" {
  category    validation
  features    [pe_release_coordination]
  contract    """
    The @specforge/product extension MUST declare a no_outgoing_edges rule
    on releases scoped to the ReleaseCompletesMilestone edge. A release
    with no edge to a milestone (no milestones field, an empty list, or
    only references that don't resolve) SHOULD produce an I083 info
    diagnostic: "release '{id}' completes no milestones".
  """
  diagnostic  I083
  severity    info
  description "Release has no milestones"
  verify unit "release with milestones produces no I083"
  verify unit "release without milestones produces I083"
}

behavior detect_released_release_incomplete_deliverables "Detect Released Release With Incomplete Deliverables" {
  category    validation
  features    [pe_release_coordination]
  invariants  [pe_release_lifecycle_consistency]
  contract    """
    A release with status=released SHOULD have all referenced deliverables
    in status=shipped. Produces I088 listing incomplete deliverables.
  """
  diagnostic  I088
  severity    info
  description "Released release has incomplete deliverables"
  verify unit "released release with all shipped deliverables produces no I088"
  verify unit "released release with draft deliverable produces I088"
}

behavior detect_recalled_release_without_reason "Detect Recalled Release Without Reason" {
  category    validation
  features    [pe_release_coordination]
  contract    """
    The @specforge/product extension MUST declare a
    conditional_field_required rule on releases: when status is recalled,
    reason is required. A recalled release with no reason, or an empty
    one, SHOULD produce an I089 info diagnostic: "release '{id}' has
    status 'recalled' but no reason".
  """
  diagnostic  I089
  severity    info
  description "Recalled release without reason"
  verify unit "recalled release with reason produces no I089"
  verify unit "recalled release without reason produces I089"
  verify unit "released release without reason produces no I089"
}

behavior detect_invalid_release_date "Detect Invalid Release Date" {
  category    validation
  features    [pe_release_coordination]
  contract    """
    The @specforge/product extension MUST declare a field_value_constraint
    matches rule on the release release_date field with the regex
    ^\d{4}-\d{2}-\d{2}$ (YYYY-MM-DD). A release whose release_date is
    present and doesn't match SHOULD produce an I086 info diagnostic:
    "release '{id}' has release_date '{value}' — expected YYYY-MM-DD".
    The regex checks the shape only, not that the date exists.
  """
  diagnostic  I086
  severity    info
  description "Invalid release_date format"
  verify unit "release_date '2026-06-01' produces no I086"
  verify unit "release_date 'June 2026' produces I086"
}

behavior detect_invalid_start_date "Detect Invalid Start Date" {
  features    [pe_temporal_planning]
  category    validation
  invariants  [pe_milestone_temporal_consistency]
  contract    """
    The @specforge/product extension MUST declare a field_value_constraint
    matches rule on the milestone start_date field with the regex
    ^\d{4}-\d{2}-\d{2}$ (YYYY-MM-DD). A milestone whose start_date is
    present and doesn't match SHOULD produce an I087 info diagnostic:
    "milestone '{id}' has start_date '{value}' — expected YYYY-MM-DD".
    The regex checks the shape only, not that the date exists.
  """
  diagnostic  I087
  severity    info
  description "Invalid milestone start_date format"
  verify unit "start_date '2026-01-15' produces no I087"
  verify unit "start_date 'Jan 15' produces I087"
}

behavior detect_journey_flow_feature_references "Detect Journey Flow Feature References" {
  category    validation
  features    [pe_journey_flow_validation]
  contract    """
    Journey flow steps MAY contain bracketed entity references (e.g.,
    "[user_auth]"). Each bracketed reference SHOULD resolve to a feature
    in the journey's features list.
  """
  diagnostic  I090
  severity    info
  description "Journey flow step references unknown feature"
  verify unit "flow step with [declared_feature] produces no I090"
  verify unit "flow step with [undeclared_feature] produces I090"
  verify unit "flow step without brackets produces no I090"
}

behavior detect_release_version_not_semver "Detect Release Version Not SemVer" {
  category    validation
  features    [pe_release_coordination]
  contract    """
    A release with a version field that does not conform to Semantic
    Versioning 2.0.0 format SHOULD produce a W093 warning.
  """
  diagnostic  W093
  severity    warning
  description "Release version is not valid SemVer"
  verify unit "version '1.0.0' produces no W093"
  verify unit "version 'v1' produces W093"
}

behavior validate_release_status_transition "Validate Release Status Transition" {
  features    [pe_release_coordination]
  category    validation
  invariants  [pe_release_status_transition]
  produces    [pe_release_status_transition_validated]
  types       [ReleaseStatusTransition]
  contract    """
    Release status transitions SHOULD follow the declared state machine:
    planned->in_progress, in_progress->released, released->recalled.
    Recalled is terminal. Requires build cache for comparison; suppressed
    when no cache file exists.
  """
  diagnostic  W094
  severity    warning
  description "Invalid release status transition"
  verify unit "planned->in_progress produces no W094"
  verify unit "planned->released produces W094"
  verify unit "recalled->planned produces W094"
  verify unit "no cache file: W094 suppressed entirely"
}

behavior detect_missing_owner "Detect Missing Owner" {
  features    [pe_ownership_tracking]
  category    validation
  invariants  [pe_ownership_field_awareness]
  contract    """
    The @specforge/product extension MUST declare a missing_required_field
    rule on the owner field for each of feature, milestone, deliverable and
    release. An entity of those kinds without an owner field SHOULD
    produce an I080 info diagnostic: "{kind} '{id}' has no owner". Other
    kinds have no owner field and are not checked.
  """
  diagnostic  I080
  severity    info
  description "Entity has no owner"
  verify unit "feature with owner produces no I080"
  verify unit "feature without owner produces I080"
  verify unit "milestone without owner produces I080"
  verify unit "deliverable without owner produces I080"
  verify unit "release without owner produces I080"
  verify unit "journey without owner produces no I080"
}

behavior detect_missing_effort "Detect Missing Effort" {
  category    validation
  features    [pe_effort_estimation]
  contract    """
    The @specforge/product extension MUST declare a missing_required_field
    rule on the feature effort field. A feature without an effort field
    SHOULD produce an I081 info diagnostic: "feature '{id}' has no effort
    estimate". Features without effort default to m=3 in weighted queries.
  """
  diagnostic  I081
  severity    info
  description "Feature has no effort estimate"
  verify unit "feature with effort produces no I081"
  verify unit "feature without effort produces I081"
}

behavior detect_blocked_milestone_without_blockers "Detect Blocked Milestone Without Blockers" {
  category    validation
  invariants  [pe_blocker_status_consistency]
  contract    """
    The @specforge/product extension MUST declare a conditional_field_required
    rule on milestones: when status is blocked, blockers is required. A
    blocked milestone with no blockers entry produces an I060 info
    diagnostic, whether or not it has depends_on entries.
  """
  diagnostic  I060
  severity    info
  description "Blocked milestone has no blockers"
  features    [pe_external_blockers]
  verify unit "blocked milestone with blockers produces no I060"
  verify unit "blocked milestone with depends_on but no blockers produces I060"
  verify unit "planned milestone without blockers produces no I060"
}

behavior detect_inconsistent_owner_strings "Detect Inconsistent Owner Strings" {
  category    validation
  features    [pe_ownership_tracking]
  invariants  [pe_ownership_field_awareness, pe_owner_string_consistency]
  contract    """
    The @specforge/product extension SHOULD detect when different entities
    use different string forms for the same logical owner (e.g.,
    "specforge-team" vs "Specforge Team" vs "specforge_team"). When two
    or more owner strings are within Levenshtein distance 2 of each other,
    an I085 info diagnostic SHOULD be produced suggesting normalization to
    the most common form. This improves owner workload query accuracy.
  """
  diagnostic  I085
  severity    info
  description "Inconsistent owner string detected"
  verify unit "identical owner strings across entities produce no I085"
  verify unit "owner 'specforge-team' and 'Specforge Team' produce I085"
  verify unit "completely different owners produce no I085"
}

behavior detect_cache_absent_awareness "Detect Cache-Absent Transition Suppression" {
  category    validation
  contract    """
    The @specforge/product extension SHOULD detect when entities have status
    fields that would trigger transition validation (W087-W091, W094) but no
    build cache file is present. In this situation, transition rules are
    silently suppressed — CI systems that don't persist the cache file will
    never see transition warnings. Produces I097 once per build (not per
    entity) when: (1) at least one entity has a status field, AND (2) no
    specforge-cache.json is found. I097 is a build-level awareness
    diagnostic, not per-entity.
  """
  diagnostic  I097
  severity    info
  description "Status transition validation suppressed (no build cache)"
  verify unit "build with status fields and no cache produces I097"
  verify unit "build with status fields and cache produces no I097"
  verify unit "build with no status fields and no cache produces no I097"
  verify unit "I097 fires at most once per build"
}

behavior detect_duplicate_release_version "Detect Duplicate Release Version" {
  category    validation
  features    [pe_release_coordination]
  invariants  [pe_release_version_uniqueness]
  contract    """
    The @specforge/product extension SHOULD detect when two or more release
    entities declare the same version string. Duplicate release versions
    create ambiguity about which release maps to a given version. Produces
    an I091 info diagnostic identifying all releases with the duplicate
    version. Releases without a version field are excluded from this check.
  """
  diagnostic  I091
  severity    info
  description "Duplicate release version detected"
  verify unit "two releases with same version produce I091"
  verify unit "releases with different versions produce no I091"
  verify unit "releases without version field produce no I091"
}

behavior detect_completed_milestone_with_unfinished_features "Detect Completed Milestone With Unfinished Features" {
  category   validation
  invariants [deliverable_lifecycle_consistency]
  types      [Diagnostic, ProductMilestone, MilestoneStatus, ProductFeature, FeatureStatus]
  contract   """
    The @specforge/product extension's lifecycle pass (check phase, so
    every compile, watch, the LSP and MCP report it) MUST detect
    milestones with status=completed whose features field names a
    feature that is neither done nor deprecated (a feature without a
    status is proposed). A completed milestone claims what its features
    do not. Produces a W154 warning per such feature, on the milestone.
    A deprecated feature is no contradiction: it was delivered, then
    retired.
  """
  ensures {
    fires_per_unfinished    "each feature of a completed milestone that is neither done nor deprecated produces one W154 on the milestone"
    suppresses_done         "a completed milestone whose features are all done or deprecated produces no W154"
    suppresses_not_complete "a milestone that is not completed produces no W154 whatever its features' status"
    unset_is_proposed       "a feature without a status counts as proposed"
  }
  verify unit "completed milestone with an unfinished feature produces W154"
  verify unit "completed milestone whose features are done or deprecated suppresses W154"
  verify unit "in_progress milestone with unfinished features suppresses W154"
}

behavior detect_done_feature_without_evidence "Detect Done Feature Without Evidence" {
  category validation
  types    [Diagnostic, ProductFeature, FeatureStatus, FeatureEvidence]
  contract """
    Under specforge analyze, with a recorded test report, the
    @specforge/product extension's delivery_evidence pass MUST report
    each feature with status=done that the recorded tests do not prove:
    no behavior names it in its features field, or not every such
    behavior is proven by the coverage rule (an obligation no passing
    test names, or a failing test). A feature's status is a claim and
    its evidence is derived (ADR 0039). Produces an I071 info diagnostic
    per such feature. Without a recorded report it reports nothing, and
    its summary says no evidence was recorded. Its summary counts the
    features, the done ones, the proven ones, the done ones not proven
    and the proven ones not marked done.
  """
  ensures {
    fires_done_unproven    "a done feature with an unproven implementing behavior, or none, produces I071"
    suppresses_proven      "a done feature whose implementing behaviors are all proven produces no I071"
    suppresses_not_done    "a feature that is not done produces no I071"
    needs_recorded_results "without recorded test results the pass reports nothing"
    summary_compares       "the pass summary counts features, done, proven, done_unproven and proven_not_done"
  }
  verify unit "done feature with an unproven implementing behavior produces I071"
  verify unit "done feature whose implementing behaviors are all proven suppresses I071"
  verify unit "without recorded test results delivery_evidence reports nothing"
}
