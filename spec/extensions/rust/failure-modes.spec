// @specforge/rust extension failure modes — FMEA risk analysis
//
// Failure modes specific to the Rust language integration:
// entity mapping accuracy and proc macro reliability.

use "extensions/rust/invariants"

failure_mode rust_entity_mapping_mismatch "Rust Entity Mapping Mismatch" {
  invariant  entity_mapping_precedence
  severity   high
  occurrence occasional
  detection  undetectable
  rpn        90
  cause      "Test function mapped to wrong entity due to naming collision in double-underscore convention — e.g., validate__input vs validate_input__"
  effect     "Coverage report attributes test results to wrong entity — misleading spec coverage metrics and false confidence"
  mitigation "Explicit #[specforge_test] attribute takes precedence over the naming convention; ambiguous mappings produce diagnostics; results naming undeclared entities are W115 at collect"
  post_mitigation {
    severity   high
    occurrence rare
    detection  likely
    rpn        12
  }
  verify unit "Rust Entity Mapping Mismatch failure mode is handled"
}

failure_mode rust_proc_macro_silent_drop "Rust Proc Macro Silent Drop" {
  invariant  entity_mapping_precedence
  severity   medium
  occurrence unlikely
  detection  undetectable
  rpn        50
  cause      "TestGuard Drop handler fails to record result — e.g., atexit handler not registered, file write fails, or process killed before Drop"
  effect     "Test result lost — entity shows as untested in coverage report despite passing test"
  mitigation "Atexit handler registered on first guard creation writes one report per test target; a target whose process dies before exit leaves no report, so its tests reach collect only through libtest's output; collect fails with E045 when a run leaves no report"
  post_mitigation {
    severity   medium
    occurrence rare
    detection  likely
    rpn        10
  }
  verify unit "Rust Proc Macro Silent Drop failure mode is handled"
}
