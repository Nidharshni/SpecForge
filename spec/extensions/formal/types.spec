// @specforge/formal extension types — structured conditions, specification layering,
// event graph linting, and coverage tracking types
//
// Extracted from @specforge/software per 10-expert panel recommendation.
// Terminology renamed: DbC -> Structured Conditions, B-Method -> Specification
// Layering, CSP -> Event Graph Linting, Verification Obligations -> Coverage
// Tracking Items. See ADR formal_extension_split, formal_terminology_rename.

use "types/core"
use "types/zero-entity-core"

// ── Property Entity Kind ────────────────────────────────────
// Temporal/behavioral assertion (safety, liveness, fairness).
// Distinct from condition: conditions are point-in-time state,
// properties are assertions about behavior over TIME.

type PropertyType = safety | liveness | fairness

type FormalProperty {
  description   string
  property_type PropertyType
  references    EntityId[] @optional
  verify property "FormalProperty"
}

// ── Axiom Entity Kind ───────────────────────────────────────
// Assumed-true foundation. No proof required; no coverage
// tracking item generated. Conditions depend on axioms.

type FormalAxiom {
  description   string
  justification string     @optional
  references    EntityId[] @optional
  verify property "FormalAxiom"
}

// ── Protocol Entity Kind ────────────────────────────────────
// Shared synchronization contract across events. Replaces
// duplicated inline sync blocks with a reusable entity.

type FormalProtocol {
  description string
  ordering    string[]          @optional
  timeout     string            @optional
  delivery    DeliverySemantics @optional
  references  EntityId[]        @optional
  verify property "FormalProtocol"
}

// ── Refinement Entity Kind ──────────────────────────────────
// First-class entity kind for specification layering.
// Captures abstract->concrete behavior mapping as a graph node.

type RefinementStatus = proposed | verified | discharged

type FormalRefinement {
  description     string
  abstract_entity EntityId
  concrete_entity EntityId
  conditions      ConditionDelta   @optional
  status          RefinementStatus @optional
  references      EntityId[]       @optional
  verify property "FormalRefinement"
}

// ── Process Entity Kind ─────────────────────────────────────
// CSP-style communicating process with alphabet, states, composition.

type CompositionOperator = parallel | sequential | choice | interleaving

type FormalProcess {
  description string
  alphabet    EntityId[]          @optional
  states      ProcessState[]      @optional
  composition CompositionOperator @optional
  references  EntityId[]          @optional
  verify property "FormalProcess"
}

type ProcessState {
  name        string  @readonly
  description string  @optional
  initial     boolean @optional
  accepting   boolean @optional
  verify property "ProcessState"
}

// ── Structured Condition Types ──────────────────────────────

type ConditionEntry {
  name        string @readonly
  description string
  verify property "ConditionEntry"
}

// Inline conditions produce ConditionEntry nodes in the AST.
// Inline:    requires { name "description" }
// Note: RequiresBlock, EnsuresBlock, MaintainsBlock, SyncBlock are defined
// in extensions/software/types.spec — shared via peer_dependency.

// ── Event Graph Linting Types ───────────────────────────────

type DeliverySemantics = at_most_once | at_least_once | exactly_once

// ── Specification Layering Types ────────────────────────────

type RefinementStep {
  fromId     EntityId       @readonly
  toId       EntityId       @readonly
  conditions ConditionDelta @optional
  verify property "RefinementStep"
}

type ConditionDelta {
  added_ensures    string[] @optional
  removed_requires string[] @optional
  verify property "ConditionDelta"
}

// RefinementChain is defined in extensions/software/types.spec — shared via peer_dependency.

// ── Coverage Tracking Types ─────────────────────────────────
// Renamed from "Verification Obligation" per expert review:
// these are heuristic structural checks, not formal mathematical proofs.

type CoverageTrackingItem {
  entityId     EntityId @readonly
  kind         CoverageTrackingKind
  description  string
  status       CoverageDischargeStatus
  dischargedBy string   @optional
  verify property "CoverageTrackingItem"
}

type CoverageTrackingKind = condition_coverage
  | invariant_coverage
  | layering_coverage
  | process_coverage

// Extended from 3 to 5 statuses per expert recommendation:
// test_written: test exists but has not yet been executed
// test_failing: test exists but is currently failing

type CoverageDischargeStatus = pending | test_written | test_failing | test_covered | heuristic_ok

// ── Event Graph Analysis Report ─────────────────────────────
// Renamed from ConcurrencyAnalysisReport; livelock_risks -> retry_cycle_risks

type EventGraphAnalysisReport {
  deadlock_count     integer
  retry_cycle_risks  integer
  unmatched_count    integer
  channel_mismatches integer
  timed_out          boolean @optional
  verify property "EventGraphAnalysisReport"
}

// ── Graph Annotation Types ──────────────────────────────────
// Analysis results attached as node properties for agent consumption

type FormalAnalysisResult = ConditionAnalysisResult
  | LayeringAnalysisResult
  | CycleAnalysisResult
  | CoverageAnalysisResult

type FormalAnalysisAnnotation {
  entity_id EntityId @readonly
  pass_name string   @readonly
  result    FormalAnalysisResult
  verify property "FormalAnalysisAnnotation"
}

type ConditionAnalysisResult {
  satisfiable    boolean
  reachable      boolean
  contradictions string[] @optional
  warnings       string[] @optional
  verify property "ConditionAnalysisResult"
}

type LayeringAnalysisResult {
  complete            boolean
  chain_depth         integer
  missing_concretes   string[] @optional
  contract_violations string[] @optional
  verify property "LayeringAnalysisResult"
}

type CycleAnalysisResult {
  has_cycles  boolean
  cycle_count integer
  mitigated   boolean
  cycle_paths string[][] @optional
  verify property "CycleAnalysisResult"
}

type CoverageAnalysisResult {
  total_items  integer
  pending      integer
  test_written integer
  test_failing integer
  test_covered integer
  heuristic_ok integer
  verify property "CoverageAnalysisResult"
}

// ── Specification Depth Levels ──────────────────────────────
// Renamed from FormalityLevel per terminology rename. A behavior's level
// (I014), each holding the one below: prose (no edges), entity_graph
// (edges), conditions (requires or ensures), invariants (also maintains
// or invariants refs), proofs (also proven by the coverage rule).

type SpecificationDepthLevel = prose | entity_graph | conditions | invariants | proofs
