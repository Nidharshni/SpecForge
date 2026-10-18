// Structural validation rules — unreferenced detection, cycle detection, and gap analysis
//
// Split from validation-rules.spec. These behaviors describe structural graph
// checks declared as ValidationRulePattern entries in the @specforge/product
// manifest. The core declarative validation engine executes these patterns.
//
// Naming convention: detect_* — structural graph checks (unreferenced entities, cycles, gaps, tags)

use "types/diagnostics"
use "types/graph"

defaults behavior {
  types    [Diagnostic]
  features [pe_validation_suite]
}

behavior detect_unreferenced_journeys "Detect Unreferenced Journeys" {
  category validation
  contract """
    The @specforge/product extension MUST declare a no_incoming_edges
    validation pattern that detects journeys not referenced by any
    deliverable. Unreferenced journeys MUST produce a W042 warning.
  """
  ensures {
    fires_when_unreferenced "journey with zero incoming DeliverableSupportsJourney edges produces W042"
    suppresses_deliverable  "journey in at least one deliverable suppresses W042"
  }
  verify unit "journey not in any deliverable produces W042"
  verify unit "journey in a deliverable suppresses W042"
}

behavior detect_unreferenced_modules "Detect Unreferenced Modules" {
  category validation
  contract """
    The @specforge/product extension MUST declare a no_incoming_edges
    validation pattern on modules. A module with no incoming edge of any
    kind (no deliverable or milestone lists it, no module depends on it, no
    term belongs to it) MUST produce a W044 warning.
  """
  ensures {
    fires_when_unreferenced "module with zero incoming edges produces W044"
    suppresses_referenced   "module referenced by a deliverable, milestone, module or term suppresses W044"
  }
  verify unit "module nothing references produces W044"
  verify unit "module in a deliverable suppresses W044"
  verify unit "module referenced only by a milestone suppresses W044"
}

behavior detect_unreferenced_terms "Detect Unreferenced Terms" {
  category validation
  contract """
    The @specforge/product extension MUST declare a no_edges validation
    pattern on terms. A term with no incoming and no outgoing edge (no
    see_also or module of its own, and no term's see_also naming it) MUST
    produce an I010 info diagnostic.
  """
  ensures {
    fires_when_isolated  "term with no incoming and no outgoing edges produces I010"
    suppresses_connected "term with any incoming or outgoing edge suppresses I010"
  }
  verify unit "term referenced by see_also suppresses I010"
  verify unit "term with its own see_also suppresses I010"
  verify unit "term with no edges produces I010"
}

behavior detect_unreferenced_personas "Detect Unreferenced Personas" {
  category validation
  contract """
    The @specforge/product extension MUST declare a no_incoming_edges
    validation pattern that detects persona entities not referenced by
    any journey's persona field. Unreferenced personas MUST produce an
    I046 info diagnostic. Info-level respects incremental adoption —
    personas may be declared before journeys reference them.
  """
  ensures {
    fires_when_unreferenced "persona with zero incoming JourneyTargetsPersona edges produces I046"
    suppresses_referenced   "persona referenced by at least one journey suppresses I046"
  }
  verify unit "persona referenced by a journey suppresses I046"
  verify unit "persona not referenced by any journey produces I046"
}

behavior detect_unreferenced_channels "Detect Unreferenced Channels" {
  category validation
  contract """
    The @specforge/product extension MUST declare a no_incoming_edges
    validation pattern that detects channel entities not referenced by
    any journey's channels field. Unreferenced channels MUST produce an
    I047 info diagnostic. Info-level respects incremental adoption —
    channels may be declared before journeys reference them.
  """
  ensures {
    fires_when_unreferenced "channel with zero incoming JourneyUsesChannel edges produces I047"
    suppresses_referenced   "channel referenced by at least one journey suppresses I047"
  }
  verify unit "channel referenced by a journey suppresses I047"
  verify unit "channel not referenced by any journey produces I047"
}

behavior detect_module_cycles "Detect Module Cycles" {
  category   validation
  invariants [module_dag]
  produces   [pe_module_cycle_detected]
  contract   """
    The @specforge/product extension MUST declare a cycle_detection
    validation pattern for the ModuleDependsOn edge type among module
    entities. Cycles MUST produce an E007 diagnostic naming the
    modules in the cycle.
  """
  ensures {
    cycle_detected "module dependency cycle produces E007 naming all cycle members"
    acyclic_passes "acyclic module graph produces no E007"
  }
  verify unit "module cycle produces E007"
  verify unit "acyclic module graph passes"
}

behavior detect_milestone_cycles "Detect Milestone Cycles" {
  category   validation
  invariants [milestone_dag]
  produces   [pe_milestone_cycle_detected]
  contract   """
    The @specforge/product extension MUST declare a cycle_detection
    validation pattern for the MilestoneDependsOn edge type among
    milestone entities. Cycles MUST produce an E015 diagnostic naming
    the milestones in the cycle.
  """
  ensures {
    cycle_detected "milestone dependency cycle produces E015 naming all cycle members"
    acyclic_passes "acyclic milestone graph produces no E015"
  }
  verify unit "milestone cycle produces E015"
  verify unit "acyclic milestone graph passes"
}

behavior detect_feature_dependency_cycles "Detect Feature Dependency Cycles" {
  category   validation
  invariants [feature_dag]
  types      [Diagnostic, ProductCycleDetectedPayload]
  produces   [pe_feature_cycle_detected]
  contract   """
    The @specforge/product extension MUST declare a cycle_detection
    validation pattern for the FeatureDependsOn edge type among feature
    entities. Cycles MUST produce a W045 warning naming the features
    in the cycle.
  """
  ensures {
    cycle_detected "feature dependency cycle produces W045 naming all cycle members"
    acyclic_passes "acyclic feature dependency graph produces no W045"
  }
  verify unit "feature dependency cycle produces W045"
  verify unit "acyclic feature dependency graph passes"
}

behavior detect_deliverable_cycles "Detect Deliverable Cycles" {
  category   validation
  invariants [deliverable_dag]
  types      [Diagnostic, ProductDeliverableCycleDetectedPayload]
  produces   [pe_deliverable_cycle_detected]
  contract   """
    The @specforge/product extension MUST declare a cycle_detection
    validation pattern for the DeliverableDependsOn edge type among
    deliverable entities. Cycles MUST produce an E052 diagnostic naming
    the deliverables in the cycle.
  """
  ensures {
    cycle_detected "deliverable dependency cycle produces E052 naming all cycle members"
    acyclic_passes "acyclic deliverable graph produces no E052"
  }
  verify unit "deliverable cycle produces E052"
  verify unit "acyclic deliverable graph passes"
}

behavior detect_deliverables_with_no_journeys "Detect Deliverables with No Journeys" {
  category validation
  contract """
    The @specforge/product extension MUST declare a no_outgoing_edges
    validation rule on deliverables scoped to the DeliverableSupportsJourney
    edge. A deliverable with no edge to a journey (no journeys field, an
    empty list, or only references that don't resolve) MUST produce a W043
    warning: "deliverable '{id}' supports no journeys".
  """
  ensures {
    fires_when_empty     "deliverable with no journeys field or an empty journeys list produces W043"
    suppresses_non_empty "deliverable with at least one journey suppresses W043"
  }
  verify unit "deliverable with no journeys produces W043"
  verify unit "deliverable with an empty journeys list produces W043"
  verify unit "deliverable with journeys suppresses W043"
}

behavior detect_deliverables_with_no_modules "Detect Deliverables with No Modules" {
  category validation
  contract """
    The @specforge/product extension MUST declare a no_outgoing_edges
    validation rule on deliverables scoped to the DeliverableContainsModule
    edge. A deliverable with no edge to a module (no modules field, an
    empty list, or only references that don't resolve) MUST produce a W046
    warning: "deliverable '{id}' contains no modules". A deliverable
    without modules has no structural decomposition.
  """
  ensures {
    fires_when_empty     "deliverable with no modules field or an empty modules list produces W046"
    suppresses_non_empty "deliverable with at least one module suppresses W046"
  }
  verify unit "deliverable with no modules produces W046"
  verify unit "deliverable with an empty modules list produces W046"
  verify unit "deliverable with modules suppresses W046"
}

behavior detect_empty_milestones "Detect Empty Milestones" {
  category validation
  contract """
    The @specforge/product extension MUST declare a
    missing_field_when_flag_set validation pattern on the milestone features
    field. A milestone with no features field MUST produce a W049 warning,
    "milestone '{id}' has no features — it may be empty", whether or not it
    lists modules.
  """
  ensures {
    fires_without_features "milestone with no features produces W049"
    suppresses_features    "milestone with at least one feature suppresses W049"
  }
  verify unit "milestone with no features and no modules produces W049"
  verify unit "milestone with features suppresses W049"
  verify unit "milestone with modules but no features produces W049"
}

behavior detect_milestone_feature_module_gap "Detect Milestone Feature-Module Gap" {
  category   validation
  invariants [milestone_feature_module_consistency]
  contract   """
    For each milestone, the @specforge/product extension SHOULD check
    that features scheduled in the milestone are reachable from the
    milestone's modules via ModuleContainsFeature edges. A scheduled feature
    not covered by any module indicates a structural gap between
    planning (features) and implementation (modules). Gaps SHOULD
    produce an I051 info diagnostic per uncovered feature.
    Each I051 MUST identify both milestone ID and uncovered feature ID.
  """
  ensures {
    fires_per_uncovered "each uncovered feature in the milestone produces one I051"
    identifies_both     "I051 diagnostic identifies both milestone ID and uncovered feature ID"
    suppresses_covered  "milestone where all features are covered by modules produces no I051"
    no_modules_no_fire  "milestone with no modules produces no I051"
  }
  verify unit "milestone with matching feature and module assignments produces no I051"
  verify unit "milestone with feature not in any module produces I051"
  verify unit "milestone with no modules produces no I051"
  verify unit "I051 diagnostic identifies both milestone ID and uncovered feature ID"
}

behavior detect_deliverable_journey_module_gap "Detect Deliverable Journey-Module Feature Gap" {
  category   validation
  invariants [deliverable_journey_module_consistency]
  contract   """
    For each deliverable, the @specforge/product extension SHOULD check
    that features referenced by the deliverable's journeys are a subset
    of features assigned to the deliverable's modules. A journey feature
    not covered by any module indicates a traceability gap between what
    is promised (journey) and what is structurally built (module).
    Gaps SHOULD produce an I049 info diagnostic per uncovered feature.
    Each I049 MUST identify both deliverable ID and uncovered feature ID.
  """
  ensures {
    fires_per_uncovered "each uncovered journey feature in the deliverable produces one I049"
    identifies_both     "I049 diagnostic identifies both deliverable ID and uncovered feature ID"
    suppresses_covered  "deliverable where all journey features are in modules produces no I049"
    no_journeys_no_fire "deliverable with no journeys produces no I049"
  }
  verify unit "deliverable with matching journey and module features produces no I049"
  verify unit "deliverable with journey feature not in any module produces I049"
  verify unit "deliverable with no journeys produces no I049"
  verify unit "I049 diagnostic identifies both deliverable ID and uncovered feature ID"
}

behavior detect_modules_with_no_features "Detect Modules With No Features" {
  category validation
  types    [Diagnostic, ProductModule]
  contract """
    The @specforge/product extension MUST declare a no_outgoing_edges
    validation rule on modules scoped to the ModuleContainsFeature edge. A
    module with no edge to a feature (no features field, an empty list, or
    only references that don't resolve) MUST produce an I067 info
    diagnostic: "module '{id}' contains no features". A module that exists
    structurally but implements no features is likely incomplete.
    Info-level respects incremental adoption — modules may be declared
    before features are assigned.
  """
  ensures {
    fires_when_empty     "module with no features field or an empty features list produces I067"
    suppresses_non_empty "module with at least one feature suppresses I067"
  }
  verify unit "module with features suppresses I067"
  verify unit "module with empty features produces I067"
  verify unit "module with no features field produces I067"
}

behavior detect_features_with_no_acceptance "Detect Features with No Acceptance Criteria" {
  category validation
  contract """
    The @specforge/product extension MUST declare two I048 validation rules
    on the feature acceptance field: a missing_required_field rule (the
    field is absent) and a field_value_constraint non_empty rule (the field
    is an empty list). Either case MUST produce an I048 info diagnostic:
    "feature '{id}' has no acceptance criteria". Info-level respects
    incremental adoption — acceptance criteria can be added progressively.
  """
  ensures {
    fires_when_missing "feature with empty or missing acceptance produces I048"
    suppresses_present "feature with non-empty acceptance suppresses I048"
  }
  verify unit "feature with no acceptance criteria produces I048"
  verify unit "feature with an empty acceptance list produces I048"
  verify unit "feature with acceptance criteria suppresses I048"
}

behavior detect_completed_milestone_without_criteria "Detect Completed Milestone Without Exit Criteria" {
  category validation
  types    [Diagnostic, ProductMilestone]
  contract """
    The @specforge/product extension SHOULD detect milestones with
    status=completed but empty or missing exit_criteria. A completed
    milestone without exit criteria means there is no verifiable
    definition of "done." Produces a W057 warning.
  """
  ensures {
    fires_completed_empty    "completed milestone with empty exit_criteria produces W057"
    suppresses_with_criteria "completed milestone with non-empty exit_criteria suppresses W057"
    suppresses_non_completed "non-completed milestone without exit_criteria suppresses W057"
  }
  verify unit "completed milestone with exit_criteria suppresses W057"
  verify unit "completed milestone without exit_criteria produces W057"
  verify unit "planned milestone without exit_criteria suppresses W057"
}

behavior detect_unanchored_exit_criteria "Detect Unanchored Exit Criteria" {
  category validation
  types    [Diagnostic, ProductMilestone]
  contract """
    The @specforge/product extension SHOULD detect milestone exit_criteria
    entries that cannot be anchored to measurable graph properties. An exit
    criterion is considered "anchored" if it references at least one entity
    ID (feature, module, deliverable) present in the graph. Exit criteria
    that are purely prose (no entity references) produce an I075 info
    diagnostic encouraging the user to anchor the criterion to a graph-
    verifiable property (e.g., "all features done" or a specific feature ID).
    This is informational only — prose exit criteria are valid but weaker
    for automated planning queries.
  """
  ensures {
    fires_prose_only    "exit criterion with no entity ID reference produces I075"
    suppresses_anchored "exit criterion referencing an existing entity ID suppresses I075"
    per_criterion       "I075 fires per unanchored criterion, not per milestone"
    ignores_empty       "empty exit_criteria array produces no I075 (covered by W057)"
    non_completed_ok    "non-completed milestones with unanchored criteria still produce I075"
  }
  verify unit "prose-only exit criterion produces I075"
  verify unit "exit criterion containing entity ID suppresses I075"
  verify unit "milestone with mixed criteria fires I075 only for unanchored ones"
  verify unit "empty exit_criteria produces no I075"
}

behavior detect_singleton_tags "Detect Singleton Tags" {
  category validation
  types    [Diagnostic, ProductTagConsistencyPayload]
  contract """
    The @specforge/product extension SHOULD detect tag values that appear
    on exactly one entity across the entire product graph. A singleton tag
    suggests a possible typo or inconsistency. Tags appearing on 2 or more
    entities are considered valid. Singleton tags SHOULD produce an I052
    info diagnostic with a suggestion field when a similar tag exists
    within Levenshtein distance ≤2 (using the strsim crate). The check
    spans all 9 product entity kinds (global scope). When multiple
    similar tags exist within distance ≤2, the closest by Levenshtein
    distance is chosen; ties are broken alphabetically. When two tags
    are mutual singletons (each similar only to the other), both get
    I052 diagnostics.
  """
  ensures {
    global_scope       "singleton detection spans all 9 product entity kinds"
    one_per_tag        "each singleton tag produces exactly one I052 diagnostic"
    closest_suggestion "suggestion is the closest tag by Levenshtein distance with alphabetical tiebreak"
    mutual_singletons  "when two tags are mutual singletons, both produce I052"
  }
  verify unit "tag appearing on one entity produces I052"
  verify unit "tag appearing on two or more entities suppresses I052"
  verify unit "singleton tag with similar tag includes suggestion in payload"
  verify unit "entity with no tags produces no I052"
  verify unit "mutual singleton tags both produce I052"
  verify unit "closest tag by Levenshtein chosen as suggestion"
  verify unit "alphabetical tiebreak when multiple tags at same distance"
}

behavior detect_tag_namespace_collision "Detect Tag Namespace Collision" {
  category validation
  contract """
    The @specforge/product extension SHOULD detect when the same tag
    string is used on entities of different kinds with potentially
    different semantic meaning. When a tag appears on 3 or more entity
    kinds, it SHOULD produce an I084 info diagnostic suggesting the
    tag may benefit from kind-specific prefixes (e.g., "core-module"
    vs "core-feature"). This is informational only — cross-kind tags
    are valid and often intentional for cross-cutting categorization.
  """
  ensures {
    fires_three_or_more_kinds     "tag on entities of 3+ different kinds produces I084"
    suppresses_two_or_fewer_kinds "tag on entities of 1-2 kinds suppresses I084"
    identifies_kinds              "I084 lists which entity kinds share the tag"
    suggestion_includes_prefixes  "I084 suggests kind-specific prefixes"
  }
  verify unit "tag on features and modules only suppresses I084"
  verify unit "tag on features, modules, and milestones produces I084"
  verify unit "I084 identifies the entity kinds sharing the tag"
}

behavior detect_term_asymmetric_see_also "Detect Term Asymmetric See-Also Links" {
  category validation
  types    [Diagnostic, ProductTerm]
  contract """
    The @specforge/product extension SHOULD inform when a term's see_also
    field references another term that does not reciprocate the link. For
    each term pair (A, B) where A.see_also contains B but B.see_also does
    not contain A, an I096 info diagnostic is emitted on term A suggesting
    adding a reciprocal see_also entry. Mutual see_also links produce no
    diagnostic. Terms without see_also or with see_also referencing
    non-term entities (handled by I056) are excluded from this check.
    I096 is informational — asymmetric links are valid but may indicate
    an oversight in vocabulary cross-referencing.
  """
  ensures {
    fires_asymmetric    "term A referencing term B via see_also without reciprocal B->A produces I096 on term A"
    suppresses_mutual   "term A and term B with mutual see_also references produce no I096"
    suppresses_isolated "term with empty see_also produces no I096"
    suppresses_non_term "see_also reference to a non-term entity is excluded from I096 (covered by I056)"
    per_pair_diagnostic "each asymmetric pair produces one I096 on the referencing term"
    identifies_both     "I096 includes both term IDs and suggests adding reciprocal see_also"
  }
  verify unit "flags A->B without B->A"
  verify unit "no flag when mutual see_also"
  verify unit "no flag for isolated terms with empty see_also"
  verify unit "no flag for see_also referencing non-term entity"
  verify unit "multiple asymmetric refs produce one I096 each"
}

behavior detect_flow_step_structure "Detect Flow Step Structure" {
  category validation
  contract """
    The @specforge/product extension SHOULD detect journey flow steps that
    lack a recognizable structure. When a flow step does not begin with a
    numbered prefix (e.g., "1.", "2."), it SHOULD produce an I072 info
    diagnostic suggesting a numbered prefix for step ordering. This is
    informational only — free-form flow steps are valid per
    pe_journey_flow_opaque, but numbered prefixes improve agent parsability.
  """
  ensures {
    fires_no_number_prefix "flow step not starting with a digit followed by period produces I072"
    suppresses_numbered    "flow step starting with '1. ...' suppresses I072"
    per_step_diagnostic    "I072 fires per non-numbered step, not per journey"
    identifies_step_index  "I072 includes the step index within the flow"
  }
  verify unit "flow step without number prefix produces I072"
  verify unit "flow step with '1. Developer runs...' suppresses I072"
  verify unit "journey with mixed numbered and unnumbered steps fires I072 per unnumbered step"
}
