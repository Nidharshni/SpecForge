// specforge-test crate invariants

use "types"

invariant zero_compiler_dependency "Zero Compiler Dependency" {
  guarantee """
    The specforge-test and specforge-test-macros crates MUST have zero
    dependency on any SpecForge compiler crate (specforge-common,
    specforge-parser, specforge-graph, specforge-resolver, specforge-cli).
    The only coupling MUST be the binary report schema
    (schema/specforge-binary-report.schema.json): a data contract, not
    Rust types.
  """
  risk      critical
  verify property "no compiler crate appears in dependency tree"
  verify unit "Cargo.toml has no path dependency on crates/*"
}

invariant drop_guard_correctness "Drop Guard Correctness" {
  guarantee """
    The TestGuard MUST record exactly one result per test function
    invocation. It MUST record fail if std::thread::panicking() is true
    during Drop, and pass otherwise. It MUST NOT interfere with the
    test harness panic handling.
  """
  risk      high
  verify unit "non-panicking test records pass"
  verify unit "panicking test records fail"
  verify unit "guard records exactly once per test"
  verify property "no double-recording under concurrent test execution"
}

invariant atexit_write_once "Atexit Write-Once Guarantee" {
  guarantee """
    The atexit handler MUST write the binary report file exactly once
    per process, even when multiple test threads complete concurrently.
    It MUST use std::sync::Once to guarantee single invocation.
  """
  risk      high
  verify unit "report written exactly once with multiple test threads"
  verify unit "report contains all recorded entries"
}

invariant convention_separator_unambiguous "Convention Separator Unambiguous" {
  guarantee """
    The double-underscore separator (__) in convention-based test names
    MUST unambiguously split entity_id from description_slug. Entity IDs
    MUST NOT contain double underscores. The slugification algorithm MUST
    be deterministic and produce the same output for the same input across
    all platforms.
  """
  risk      medium
  verify property "slugify is deterministic for all valid verify descriptions"
  verify unit "double underscore splits correctly"
  verify unit "entity IDs with single underscores are not misinterpreted"
}

invariant should_panic_expectation "Should Panic Expectation" {
  guarantee """
    A test annotated with both #[should_panic] and #[specforge_test] MUST
    record `pass` when it panics and `fail` when it doesn't: the guard reads
    the expectation from the attribute instead of treating every panic as a
    failure. An #[ignore]d test MUST stay ignored: libtest reports it as
    ignored and nothing is recorded, and when libtest is asked to run it
    (`--ignored`, `--include-ignored`) its body runs and its real result is
    recorded.
  """
  risk      medium
  verify unit "should_panic test that panics records pass"
  verify unit "an ignored test runs and is recorded only when libtest is asked to run it"
}

invariant single_registration "Single Registration" {
  guarantee """
    Every annotated test MUST be registered with the test harness exactly
    once, so each run records each result once. The macro registers the
    test unless another attribute does; a test that runs twice in one test
    binary (a leftover #[test] above the attribute) MUST fail with a
    message telling the user to remove the #[test].
  """
  risk      high
  verify unit "a test registered twice fails its second run"
}
