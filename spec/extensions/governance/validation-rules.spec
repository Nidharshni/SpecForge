// @specforge/governance extension validation rules
//
// These behaviors describe validation rules declared as
// ValidationRulePattern entries in the @specforge/governance manifest.
// The core declarative validation engine executes these patterns.

use "extensions/governance/invariants"
use "invariants/core"
use "invariants/validation"
use "types/diagnostics"

behavior ge_declare_vocabularies "Declare Governance Vocabularies" {
  features [ge_validation_suite]
  category validation
  types    [
    GovernanceDecision,
    GovernanceConstraint,
    GovernanceFailureMode,
    DecisionStatus,
    ConstraintPriority,
    Diagnostic,
  ]
  contract """
    The @specforge/governance extension MUST declare decision status,
    constraint priority and every failure mode rating (severity,
    occurrence, detection, before and after mitigation) as enums of their
    vocabularies. The core refuses another value with E061, naming the
    closest one; the extension declares no value rule of its own.
  """
  ensures {
    off_vocabulary_e061 "a governance vocabulary field with a value outside its vocabulary produces E061"
  }
  verify unit "decision status, constraint priority and every failure mode rating are declared enums of their vocabularies"
}

behavior validate_rpn_arithmetic "Validate RPN Arithmetic" {
  features   [ge_validation_suite]
  invariants [rpn_arithmetic_integrity]
  category   validation
  types      [Diagnostic]
  contract   """
    The @specforge/governance extension MUST declare a field_value_constraint
    validation pattern (implemented via Wasm validate() for arithmetic) that
    verifies rpn == severity * occurrence * detection in failure_mode
    entities. Mismatches MUST produce an E005 diagnostic. The same check
    MUST apply to post_mitigation blocks.
  """
  verify unit "correct RPN passes"
  verify unit "incorrect RPN produces E005"
  verify unit "post_mitigation RPN also validated"
}

behavior detect_unmitigated_high_risk_invariants "Detect Unmitigated High-Risk Invariants" {
  features [ge_validation_suite]
  types    [Diagnostic]
  category validation
  contract """
    The @specforge/governance extension MUST declare a field_value_constraint
    validation pattern that detects invariants with risk: high that have
    no corresponding failure_mode. Unmitigated high-risk invariants MUST
    produce a W047 warning.
  """
  verify unit "high-risk invariant without failure_mode produces W047"
  verify unit "high-risk invariant with failure_mode suppresses W047"
  verify unit "medium-risk invariant without failure_mode does not produce W047"
}

behavior detect_constraints_with_no_protected_invariants "Detect Constraints with No Protected Invariants" {
  features [ge_validation_suite]
  types    [Diagnostic]
  category validation
  contract """
    The @specforge/governance extension MUST declare a field_value_constraint
    validation pattern that detects constraints whose invariants list is
    empty or references no existing invariants. Such constraints MUST
    produce a W048 warning.
  """
  verify unit "constraint with empty protects list produces W048"
  verify unit "constraint with valid protects suppresses W048"
}
