// specforge-test crate features

use "behaviors"

feature test_annotation "Test Annotation" {
  problem  """
    Rust developers need a way to link their tests to spec entity IDs
    without changing how tests run. The standard test harness has no
    plugin API, and writing both #[test] and a linkage attribute on every
    test is noise that invites the two to drift apart.
  """
  solution """
    A proc macro #[specforge_test(behavior = "id", verify = "...")] that
    registers the test itself and injects a Drop-based guard. The guard
    records pass/fail/skipped per entity without interfering with the test
    harness. Runner attributes such as #[tokio::test] and #[rstest] keep
    registering their tests; the macro defers to them.
    `specforge collect` (@specforge/cargo-test) runs the tests and reads
    the per-binary reports.
  """
}

feature result_collection "Result Collection" {
  problem  """
    Test results must be captured and written to disk so that
    `specforge collect rust` can transform them into specforge-report.json.
    Multiple test binaries in a workspace each need their own report file.
  """
  solution """
    An atexit handler writes target/specforge/<binary-name>.json containing
    all collected TestRecordEntries. Convention-based mapping (module names,
    double-underscore naming) provides zero-config fallback for projects
    that don't use the proc macro.
  """
}
