// specforge-test crate milestones
// Delivery of the attribute, the drop guard and the per-binary report.

use "behaviors"
use "features"
use "invariants"

milestone phase_1_annotation "Phase 1: Annotation and Collection" {
  status    in_progress
  features  [test_annotation, result_collection]
  behaviors [
    expand_test_attribute,
    record_test_result_on_drop,
    register_atexit_handler,
    emit_binary_report,
    resolve_convention_mapping,
  ]
  exit_criteria {
    proc_macro_works    "#[specforge::test(behavior = 'id')] compiles and injects guard"
    drop_records        "TestGuard Drop records pass/fail correctly"
    atexit_writes       "target/specforge/<binary>.json is written at process exit"
    convention_resolves "double-underscore and module name conventions match entity IDs"
    multi_binary        "workspace with multiple test binaries produces separate reports"
    published           "specforge-test and specforge-test-macros published to crates.io"
    zero_compiler_dep   "cargo tree shows no specforge compiler crates"
  }
}
