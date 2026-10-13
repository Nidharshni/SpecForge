// specforge-test crate behaviors: the attribute, the drop guard and the
// per-binary report. Which obligations the recorded tests prove is scored by
// `specforge collect` and the coverage rule, never by this crate (ADR 0068).

use "events"
use "invariants"
use "ports"
use "types"

behavior expand_test_attribute "Expand Test Attribute" {
  features   [test_annotation]
  category   command
  invariants [zero_compiler_dependency, should_panic_expectation, single_registration]
  types      [TestGuard]
  contract   """
    The #[specforge_test(...)] proc macro MUST inject a TestGuard at the
    start of the annotated function body and MUST register the function
    as a test itself, so no separate #[test] is written (ADR 0002). It
    MUST parse entity_kind and entity_id from the attribute arguments.
    Multiple specforge attributes on the same function MUST each produce a
    separate TestGuard, and only the last one registers the test. When a
    runner attribute below it registers the test (#[tokio::test],
    #[rstest], #[test_case]), the macro MUST defer to it. A plain #[test]
    below it MUST be a compile error naming the attribute to remove. A
    #[test] above it is expanded by the compiler first and is invisible to
    the macro; the guard MUST then fail the second registration at run
    time (single_registration). The hidden `__guard_only` flag records the
    guard without registering a test, for functions called directly.
  """
  requires {
    valid_attribute_args "attribute contains entity_kind (any identifier) and entity_id"
  }
  ensures {
    guard_injected      "function body begins with a TestGuard binding"
    registers_test      "the annotated function is a test without a separate #[test]"
    runner_attr_defers  "a runner attribute below the macro registers the test instead"
    plain_test_rejected "a plain #[test] below the macro is a compile error"
    multi_attr_works    "N attributes produce N guards and one test"
  }
  verify unit "single behavior attribute expands correctly"
  verify unit "verify description attribute includes slug in guard"
  verify unit "multiple attributes on same function produce multiple guards"
  verify unit "the attribute alone registers the test"
  verify unit "a plain #[test] below the attribute is a compile error"
  verify unit "attribute alongside #[tokio::test] defers registration"
}

behavior record_test_result_on_drop "Record Test Result on Drop" {
  features   [test_annotation]
  category   command
  invariants [drop_guard_correctness]
  types      [TestGuard, TestRecordEntry, TestStatus]
  ports      [TestRegistry]
  produces   [test_result_recorded]
  contract   """
    When a TestGuard is dropped, it MUST check std::thread::panicking()
    to determine the test outcome. It MUST push a TestRecordEntry to the
    thread-safe TestRegistry. Pass if not panicking, fail if panicking.
    The guard MUST NOT catch or suppress panics.
  """
  requires {
    guard_is_live "TestGuard was constructed and not yet dropped"
  }
  ensures {
    entry_recorded  "TestRegistry contains exactly one new entry for this guard"
    status_correct  "status is failed iff std::thread::panicking() was true"
    no_panic_change "guard does not alter panic propagation"
  }
  verify unit "passing test records pass"
  verify unit "panicking test records fail"
  verify unit "guard does not catch the panic"
  verify property "concurrent drops do not lose entries"
}

behavior emit_binary_report "Emit Binary Report" {
  features   [result_collection]
  category   command
  invariants [atexit_write_once, zero_compiler_dependency]
  types      [BinaryReport, TestRecordEntry]
  ports      [ProcessExit, FileSystem, TestRegistry]
  consumes   [test_result_recorded]
  produces   [binary_report_written]
  contract   """
    At process exit, the atexit handler MUST drain all entries from the
    TestRegistry, construct a BinaryReport, and write it to
    $SPECFORGE_REPORT when `specforge collect` sets it, else to
    specforge/ in the cargo target directory the binary was built into,
    never the working directory, named <package>--<target>.json. It MUST
    use std::sync::Once to guarantee single invocation. If the registry is
    empty (no specforge tests ran), it MUST NOT write a report file. The
    report is all it writes, and it prints nothing: which obligations the
    tests prove is scored by `specforge collect` and `specforge analyze
    coverage` (ADR 0068).
  """
  requires {
    atexit_registered "atexit handler was registered during first TestGuard creation"
  }
  ensures {
    report_written "target/specforge/<binary>.json exists with all collected entries"
    empty_skipped  "no file written when zero entries collected"
    valid_json     "report file is valid JSON matching BinaryReport schema"
    report_only    "the handler writes the report and nothing else, and prints nothing"
  }
  verify unit "report contains all recorded test entries"
  verify unit "empty registry produces no file"
  verify unit "report file is valid JSON"
  verify unit "binary name is correctly resolved"
  verify unit "the exit handler writes the report and prints nothing"
  verify integration "multi-threaded test suite produces complete report"
}

behavior resolve_convention_mapping "Resolve Convention Mapping" {
  features   [result_collection]
  category   query
  invariants [convention_separator_unambiguous]
  types      [EntityMappingEntry, MappingResolutionLevel]
  contract   """
    Given a test function name, the resolver MUST attempt to extract an
    entity ID using the double-underscore convention: {entity_id}__{slug}.
    If that yields no known entity, it MUST check whether an enclosing
    module name (the innermost first) matches a known entity ID. The
    `specforge collect` host resolves these names, from the tests the
    cargo-test collector reads in libtest's output. The slugification algorithm MUST be
    deterministic: lowercase, spaces to underscores, angle brackets to
    lt/gt/lte/gte, strip non-alphanumeric, collapse consecutive underscores.
  """
  requires {
    test_name_non_empty "test function name is a non-empty string"
  }
  ensures {
    mapping_resolved   "returns Some(EntityMappingEntry) if convention matches, None otherwise"
    slug_deterministic "same verify description always produces same slug"
  }
  verify unit "double underscore splits entity_id from slug"
  verify unit "module name fallback matches entity ID"
  verify unit "no double underscore and no module match returns None"
  verify unit "slugification handles spaces"
  verify unit "slugification handles angle brackets"
  verify unit "slugification handles special characters"
  verify unit "slugification collapses consecutive underscores"
  verify property "slugify is idempotent"
}

behavior register_atexit_handler "Register Atexit Handler" {
  features   [test_annotation]
  category   command
  invariants [atexit_write_once]
  ports      [ProcessExit]
  contract   """
    The first TestGuard construction in a process MUST register the atexit
    handler via std::sync::Once. Subsequent TestGuard constructions MUST
    NOT re-register. The handler MUST be registered using libc::atexit
    on POSIX systems.
  """
  requires {
    first_guard "at least one TestGuard is being constructed"
  }
  ensures {
    registered_once "atexit handler is registered exactly once"
  }
  verify unit "first guard triggers atexit registration"
  verify unit "second guard does not re-register"
}
