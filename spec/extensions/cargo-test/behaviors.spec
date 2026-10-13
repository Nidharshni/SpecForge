// @specforge/cargo-test behaviors

use "types/wasm"

behavior ct_declare_cargo_collector "Declare the cargo test Collector" {
  features [ct_cargo_test_collection]
  category query
  types    [CollectorDescriptor]
  contract """
    @specforge/cargo-test MUST declare one collector, `cargo-test`, selected
    by a `Cargo.toml` at the project root. Its command is
    `cargo test --workspace --no-fail-fast` (one failing test binary must
    not stop the others from reporting) and its report is the `target/specforge`
    directory, where the `specforge-test` attribute writes one JSON report
    per test binary (or `$SPECFORGE_REPORT`, which the host sets when it
    runs the command). It captures the command's standard output, where
    plain `#[test]` functions report. It requires @specforge/testing, which
    owns the `verify` vocabulary the results prove.
  """
  ensures {
    detected_by_cargo_toml "a project with Cargo.toml at its root selects cargo-test"
    declares_command       "the declared command is cargo test --workspace --no-fail-fast"
    captures_stdout        "the command's standard output is captured"
    requires_testing       "@specforge/testing is a required peer, enabled with it by specforge add"
  }
  verify unit "cargo-test declares its collector"
}

behavior ct_map_binary_reports "Map specforge-test Reports to Entities" {
  features [ct_cargo_test_collection]
  category query
  types    [CollectInput, CollectOutput]
  contract """
    `collect__cargo_test` MUST map every per-binary report entry
    (`entity_id`, `test_name`, `verify`, `duration_ms`, `status`) to a test
    result of that entity, in entity order: `pass` is passed, `fail` is
    failed and `skipped` is skipped. An entry whose status is none of
    those is the collector's error, naming the file. A file that is not
    JSON is read as libtest output; a JSON file without `entries` yields
    nothing. It runs nothing and reads no files: the host passes the
    report text in.
  """
  ensures {
    grouped_by_entity   "results are grouped by entity across binaries"
    statuses_typed      "an entry's status is pass, fail or skipped; any other is the collector's error"
    non_reports_ignored "files that aren't binary reports are skipped"
  }
  verify unit "cargo-test maps per-binary reports to entity results"
  verify unit "a report entry whose status is not passed, failed or skipped is the collector's error"
}

behavior ct_report_unlinked_tests "Report Plain Tests as Unlinked" {
  features [ct_cargo_test_collection]
  category query
  types    [CollectInput, CollectOutput]
  contract """
    Plain `#[test]` functions write no report: they only appear in
    libtest's output as `test <path> ... ok|FAILED|ignored`.
    `collect__cargo_test` MUST read those lines from the captured standard
    output (and from any report file that isn't a binary report, so
    `cargo test > out.txt` can be passed with `--report`) and return every
    test the attribute didn't record as unlinked, with its path split on
    `::`, for the host to link by naming convention. A test the attribute
    recorded is recognised by its module path without the crate, then its
    name (by its name alone for reports without a module path). `ok` is
    passed, `FAILED` failed and `ignored` skipped; doc tests, benchmarks
    and the `failures:` section, where a failing test's output is
    replayed, are ignored.
  """
  ensures {
    unlinked_reported  "every libtest result the attribute didn't record is returned as unlinked"
    attribute_excluded "a test the attribute recorded is never also unlinked"
    noise_ignored      "doc tests, benchmarks and replayed failure output yield nothing"
  }
  verify unit "tests the attribute did not record are reported as unlinked"
}
