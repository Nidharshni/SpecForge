// specforge-test crate milestones
// Delivery of the attribute, the drop guard and the per-binary report.

use "behaviors"
use "features"
use "invariants"

milestone phase_1_annotation "Phase 1: Annotation and Collection" {
  status        in_progress
  features      [test_annotation, result_collection]
  behaviors     [
    expand_test_attribute,
    record_test_result_on_drop,
    register_atexit_handler,
    emit_binary_report,
    resolve_convention_mapping,
  ]
  exit_criteria [
    "#[specforge::test(behavior = 'id')] compiles and injects guard",
    "TestGuard Drop records pass/fail correctly",
    "target/specforge/<binary>.json is written at process exit",
    "double-underscore and module name conventions match entity IDs",
    "workspace with multiple test binaries produces separate reports",
    "specforge-test and specforge-test-macros published to crates.io",
    "cargo tree shows no specforge compiler crates",
  ]
}
