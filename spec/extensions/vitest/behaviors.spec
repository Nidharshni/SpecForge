// @specforge/vitest behaviors

use "types/wasm"

behavior vt_declare_vitest_collector "Declare the vitest Collector" {
  features [vt_vitest_collection, ts_test_collection]
  category query
  types    [CollectorDescriptor]
  contract """
    @specforge/vitest MUST declare one collector, `vitest`, selected by a
    `vitest.config.*` or `vitest.workspace.*` file at the project root
    (a project that configures vitest inside vite.config.* enables the
    extension itself: `init` does when package.json depends on vitest, and
    a single enabled collector needs no detection). Its command is
    `npx --no vitest run --reporter=default --reporter=json
    --outputFile.json={report}`: `--no` uses the project's own vitest and
    never downloads one, and the default reporter keeps the user's usual
    output. Its report is `.specforge/reports/vitest.json`. It requires
    @specforge/testing.
  """
  ensures {
    detected_by_config   "a vitest config or workspace file selects vitest"
    never_downloads      "the command runs the project's own vitest only"
    report_path_expanded "the JSON report is written to the path the host chose"
  }
  verify unit "vitest declares its collector"
  verify integration "collect runs vitest with the report path and maps linked tests"
}

behavior vt_map_vitest_report "Map vitest Reports to Entities" {
  features [vt_vitest_collection]
  category query
  types    [CollectInput, CollectOutput]
  contract """
    A test links itself to an entity through vitest's test metadata, which
    the JSON reporter carries into the report: `meta.specforge` is an
    object naming an entity by kind (`{ behavior: 'create_user', verify:
    '...' }`) or a list of such objects. It may be set in the test's
    options, at run time on `task.meta`, or on a `describe` block, whose
    tests inherit it. `collect__vitest` MUST map every linked test to a
    result of each entity it names, with its full name, duration and
    `verify` text: `passed` and `failed` keep their status; `skipped`,
    `pending`, `todo` and `disabled` are skipped; a linked test with any
    other status is the collector's error, naming the file, the test and
    the status. Tests without `specforge`
    metadata prove nothing and are left out. It runs nothing and reads no
    files: the host passes the report text in.
  """
  ensures {
    linked_tests_mapped "each linked test becomes a result of the entities it names"
    lists_supported     "a list of links maps one test to several entities"
    unlinked_ignored    "tests without specforge metadata are left out"
  }
  verify unit "linked tests map to their entities with status and verify"
  verify unit "a list links one test to several entities"
  verify unit "unlinked tests and unreadable reports prove nothing"
  verify unit "a linked test whose status vitest's JSON reporter does not write is the collector's error"
}
