// @specforge/formal coverage tracking — coverage tracking items + info diagnostics
//
// Moved from @specforge/software formal-proofs.spec per 10-expert panel.
// Terminology: "Verification Obligations" -> "Coverage Tracking Items"
// All behavior IDs renamed se_ -> fa_
// CoverageDischargeStatus extended with test_written and test_failing
// W035 aggregated: single summary per compilation with breakdown by kind

use "extensions/formal/invariants"
use "extensions/formal/types"
use "types/zero-entity-core"

defaults behavior {
  features [fa_coverage_tracking]
}

behavior fa_coverage_tracking_pass "Coverage Tracking Compiler Pass" {
  category command
  types    [ConditionEntry, RefinementChain, CoverageTrackingItem, CoverageTrackingKind]
  produces [fa_coverage_items_generated]
  contract """
    Generate machine-readable coverage tracking items after all
    analysis passes complete.
  """
  requires {
    all_passes_done "condition_check, layering_verify, event_graph_analyze passes completed"
  }
  ensures {
    condition_items       "condition_coverage items generated (requires/ensures hold)"
    invariant_items       "invariant_coverage items generated (maintains hold)"
    layering_items        "layering_coverage items generated (concrete satisfies abstract)"
    axiom_excluded        "axiom entities are explicitly excluded — axioms are assumed true, no coverage tracking items generated"
    layering_entity_items "layering_coverage items generated for refinement entities (condition delta verified)"
    process_items         "process_coverage items generated for process entities (alphabet completeness, composition safety)"
    json_output           "each item emitted as structured JSON: entity ID, kind, description, discharge status"
  }
  verify unit "condition_coverage items generated"
  verify unit "invariant_coverage items generated"
  verify unit "layering_coverage items generated"
  verify unit "items emitted as structured JSON"
}

behavior fa_track_coverage_discharge "Track Coverage Item Discharge" {
  category query
  types    [CoverageTrackingItem, CoverageDischargeStatus]
  contract """
    Track which coverage items are discharged by existing tests
    or by static analysis heuristics. Auto-discharge is OFF by default;
    entities or conditions must opt in via @auto-discharge-eligible.
    Extended status model: pending -> test_written -> test_failing ->
    test_covered (or heuristic_ok for opted-in auto-discharge). An item
    is test_covered exactly when @specforge/testing's coverage rule holds
    it proven (every obligation named by a passing recorded test or
    discharged by an entailed claim, no failing test), so W035 and
    `specforge analyze coverage` never disagree; W035 never suggests the
    retired `tests [...]` field.
  """
  requires {
    items_generated "coverage tracking items have been generated"
  }
  ensures {
    test_written_tracked    "item with associated test file transitions to test_written"
    test_failing_tracked    "item with failing test transitions to test_failing"
    test_discharge          "item covered by passing verify/test transitions to test_covered"
    analysis_discharge      "item discharged by static analysis (when opted in) transitions to heuristic_ok"
    undischarged_summary    "undischarged items produce single W035 summary per compilation with breakdown by kind (condition/invariant/layering) and link to drill-down command (specforge analyze coverage)"
    opt_in_required         "auto-discharge only applies to entities or conditions annotated @auto-discharge-eligible"
    tautological_auto       "opted-in postcondition that is tautologically true auto-discharges to heuristic_ok"
    enforced_invariant_auto "opted-in invariant with enforces referencing matching maintains condition auto-discharges to heuristic_ok"
    layering_superset_auto  "opted-in concrete behavior whose ensures is a strict superset of abstract ensures auto-discharges layering_coverage to heuristic_ok"
    tautological_criteria   "tautological auto-discharge applies only when: (a) postcondition uses no identifiers from requires, (b) description matches trivial patterns, (c) condition has no side-effect verbs — this is a closed list of criteria"
  }
  verify unit "item with test file transitions to test_written"
  verify unit "item with failing test transitions to test_failing"
  verify unit "item discharged by test transitions to test_covered"
  verify unit "item discharged by analysis transitions to heuristic_ok"
  verify unit "undischarged items produce single W035 summary"
  verify unit "W035 summary includes breakdown by kind"
  verify unit "tautological postcondition auto-discharges (when opted in)"
  verify unit "enforced invariant with matching maintains auto-discharges (when opted in)"
  verify unit "concrete ensures superset of abstract ensures auto-discharges (when opted in)"
  verify unit "non-trivial item remains pending without test"
  verify unit "@auto-discharge-eligible annotation enables auto-discharge"
  verify unit "entity without @auto-discharge-eligible skips all auto-discharge heuristics"
}

// ── Info Diagnostics ─────────────────────────────────────────

behavior fa_emit_coverage_item_covered_info "I008: Coverage Item Covered by Test" {
  category query
  types    [CoverageTrackingItem]
  contract """
    When the coverage_tracking pass (specforge analyze) finds a coverage
    item (an invariant or a testable entity) whose every obligation a
    passing recorded test names, under @specforge/testing's coverage
    rule, emit an I008 info diagnostic naming the tests that prove it.
    An item proven only by an entailed formal claim is not reported,
    and testing's coverage pass reports only what is unproven (A015),
    so the two never say the same thing.
  """
  ensures {
    info_emitted      "test_covered item emits I008 with test name"
    claim_only_silent "an item proven only by an entailed claim emits no I008"
  }
  verify unit "coverage item covered by test produces I008"
}

behavior fa_emit_no_structural_cycles_info "I009: No Structural Cycles Detected" {
  category query
  contract """
    When the event_graph_analyze pass finds no unmitigated cycle in an
    event graph that has flow edges (behaviors producing or consuming
    events): no E034 cycle through two or more behaviors and no W032
    retry cycle, emit one I009 info diagnostic. Note: this is
    structural analysis only — runtime deadlocks from dynamic conditions
    are not detected.
  """
  ensures {
    info_emitted "cycle-free event graph emits I009"
    cycle_silent "an event graph with an unmitigated cycle emits no I009"
  }
  verify unit "cycle-free event graph produces I009"
}

behavior fa_emit_formal_analysis_available "I015: Formal Analysis Available" {
  category query
  contract """
    When behaviors declare requires or ensures blocks, the
    @specforge/formal check-phase pass analysis_available emits one I015
    info per compile (specforge check, watch, the LSP, MCP) suggesting
    specforge analyze, which runs the formal passes over them. A project
    whose behaviors declare neither gets no I015.
  """
  ensures {
    info_emitted     "presence of condition blocks emits I015 suggesting specforge analyze"
    once_per_compile "a compile emits at most one I015"
    absent_silent    "behaviors without requires or ensures emit no I015"
  }
  verify unit "behaviors with requires/ensures trigger I015 info"
}

// ── Specification Depth Detection ────────────────────────────

behavior fa_detect_specification_depth "I014: Specification Depth Level" {
  category query
  types    [SpecificationDepthLevel, CoverageTrackingItem]
  contract """
    Compute each behavior's specification depth level (the
    SpecificationDepthLevel enum) in the coverage_tracking pass
    (specforge analyze). The levels are a ladder, each holding the one
    below it: prose (no edges in or out), entity_graph (edges, but no
    requires or ensures), conditions (level 2: requires or ensures),
    invariants (level 3: also names invariants in maintains or
    invariants), proofs (level 4: also proven under @specforge/testing's
    coverage rule, by recorded tests or entailed claims). Emit I014 info
    for each behavior at level 2 or deeper, with the step to the next
    level.
  """
  requires {
    all_passes_done "condition_check, layering_verify, event_graph_analyze, coverage_tracking passes completed"
  }
  ensures {
    level_computed    "each behavior's depth level computed: prose (no edges), entity_graph (has edges), conditions (has requires/ensures), invariants (also maintains or invariants refs), proofs (also proven by the coverage rule)"
    info_emitted      "behavior at Level 2+ emits I014 with current level and next-level suggestion"
    level_zero_silent "behavior at Level 0 (prose) emits no depth diagnostic"
    level_one_silent  "behavior at Level 1 (entity_graph) emits no depth diagnostic"
    adoption_nudge    "when >5 behaviors are at Level 0-1, one more I014 suggests adopting requires/ensures on critical behaviors"
  }
  verify unit "entity with requires/ensures computes as Level 2 (conditions)"
  verify unit "entity with maintains and invariant refs computes as Level 3 (invariants)"
  verify unit "entity with all obligations proven computes as Level 4 (proofs)"
  verify unit "entity at Level 2+ emits I014"
  verify unit "entity at Level 0 emits no depth diagnostic"
  verify unit ">5 prose-only behaviors triggers adoption nudge in I014"
}
