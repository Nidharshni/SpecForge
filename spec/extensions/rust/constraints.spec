// @specforge/rust extension constraints
//
// Non-functional requirements specific to the Rust language integration:
// entity mapping accuracy and report correctness.

use "behaviors/wasm-extensions"
use "extensions/cargo-test/behaviors"
use "extensions/rust/behaviors"
use "extensions/rust/invariants"
use "extensions/testing/behaviors"
use "invariants/core"

constraint test_coverage_accuracy "Test Coverage Accuracy" {
  description "Ensures coverage percentages are accurate and merge operations produce correct deduplicated results."
  category    reliability
  priority    critical
  metric      """
    coverage percentage matches actual verified/total ratio;
    merge produces correct deduplicated results
  """
  constrains  [ingest_collector_report, te_coverage_pass, te_coverage_gate]
  protects    [testable_entity_classification, traceability_chain_integrity]
  verify unit "coverage percentage and merge are accurate"
}

constraint rust_collection_accuracy "Rust Collection Accuracy" {
  description "Ensures entity mapping has zero false positives and every recorded result names a declared entity."
  category    reliability
  priority    critical
  metric      """
    entity mapping has zero false positives; every recorded result names a
    declared entity, and results that don't are reported as W115
  """
  constrains  [resolve_entity_mapping, record_test_via_drop_guard, ct_map_binary_reports]
  protects    [entity_mapping_precedence]
  verify unit "entity mapping and report generation are accurate"
}
