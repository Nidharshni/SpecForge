// @specforge/governance extension types — entity kind shapes

use "types/core"

// ── Entity Kind Shapes ───────────────────────────────────────

type GovernanceDecision {
  status       DecisionStatus @optional
  date         string         @optional
  context      string
  decision     string
  consequences string[]       @optional
  invariants   EntityId[]     @optional
  verify property "GovernanceDecision"
}

type GovernanceConstraint {
  category   string     @optional
  priority   ConstraintPriority
  // proof_role bound: the prove pass assumes its comparisons (ADR 0009)
  metric     string
  constrains EntityId[] @optional
  protects   EntityId[] @optional
  verify property "GovernanceConstraint"
}

type GovernanceFailureMode {
  invariant       EntityId
  severity        FailureSeverity
  occurrence      FailureOccurrence @optional
  detection       FailureDetection  @optional
  rpn             integer
  cause           string
  effect          string
  mitigation      string
  post_mitigation PostMitigation    @optional
  post_severity   FailureSeverity   @optional
  post_occurrence FailureOccurrence @optional
  post_detection  FailureDetection  @optional
  verify property "GovernanceFailureMode"
}

type PostMitigation {
  severity   integer
  occurrence integer
  detection  integer
  rpn        integer
  verify property "PostMitigation"
}

// ── Enums ────────────────────────────────────────────────────

type ConstraintPriority = critical | high | medium | low

type FailureSeverity = critical | high | medium | low

type FailureOccurrence = certain | likely | occasional | unlikely | rare

type FailureDetection = certain | likely | moderate | unlikely | undetectable

type ConstraintCategory = performance
  | security
  | reliability
  | compatibility
  | usability
  | portability

type DecisionStatus = proposed | accepted | deprecated | superseded
