// @specforge/rust extension behaviors — linking Rust tests to spec entities
//
// Running the tests and reading their reports belongs to
// @specforge/cargo-test (spec/extensions/cargo-test); these behaviors
// cover how a Rust test names the entity it proves.

use "extensions/rust/decisions"
use "extensions/rust/invariants"
use "extensions/rust/types"
use "invariants/core"
use "types/core"

behavior resolve_entity_mapping "Resolve Entity Mapping" {
  features   [rust_test_collection]
  types      [EntityMappingEntry, MappingResolutionLevel]
  category   query
  invariants [entity_mapping_precedence]
  contract   """
    The system MUST resolve test-to-entity mappings using two-level
    precedence: (1) the #[specforge_test] proc macro attribute
    (explicit), (2) module name / double-underscore convention
    (implicit). The explicit level MUST override the implicit one.
    Ambiguous mappings MUST be reported. Spec files carry no test paths:
    the `tests` field is retired (ADR 0002).
  """
  verify unit "proc macro attribute overrides naming convention"
  verify unit "double-underscore convention extracts entity ID"
  verify unit "ambiguous mapping produces diagnostic"
}

behavior record_test_via_drop_guard "Record Test via Drop Guard" {
  features   [rust_proc_macro_annotation]
  invariants [entity_mapping_precedence]
  category   command
  types      [TestGuard, TestRegistry, RustFramework, RustFrameworkSupport, RustSupportLevel]
  contract   """
    The #[specforge_test] proc macro MUST register the annotated function
    as a test (no separate #[test]; it defers to a runner attribute such
    as #[tokio::test] below it) and inject a TestGuard that records
    pass/fail on Drop. The guard MUST check std::thread::panicking() to
    determine status, inverted under #[should_panic]. An #[ignore]d test
    stays ignored and records nothing; run with `--ignored` it records its
    real result. Results MUST be written via an atexit handler
    to $SPECFORGE_REPORT when `specforge collect` sets it, else to
    target/specforge/, one report per test target named
    <package>--<target>.json so a rebuild replaces its predecessor.
    Each entry carries the test's module path, so a collector can match
    it to the test's line in libtest's output. The exit handler prints
    nothing: which obligations the tests prove is the coverage rule's,
    after `specforge collect` (ADR 0068).
  """
  verify unit "successful test records pass via Drop"
  verify unit "panicking test records fail via Drop"
  verify unit "results written to target/specforge/ on process exit"
  verify unit "the attribute registers the test without #[test]"
  verify unit "an ignored test runs and is recorded only when libtest is asked to run it"
  verify unit "under nextest each test writes its own report and reports of other runs are pruned"
  verify unit "the recorded entry carries the test's module path"
  verify unit "the exit handler prints no coverage, even beside a graph export"
  verify unit "a recorded entry conforms to the published binary report schema"
  verify unit "the exit handler writes the report and nothing else"
  verify unit "without SPECFORGE_REPORT the report goes to the binary's cargo target directory, never the working directory"
  verify unit "a binary outside any cargo target directory writes no report and says so"
}
