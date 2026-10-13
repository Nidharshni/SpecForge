//! `specforge check --cache`: the build cache (`specforge-cache.json`) is
//! written only on request, only by a check that passes, and always the
//! same bytes for the same sources.

use crate::support::{Cli, TestProject};
use specforge_test_macros::test as specforge_test;
use std::fs;

const CACHE: &str = "specforge-cache.json";

fn project(files: &[(&str, &str)]) -> TestProject {
    TestProject::new().without_config().files(files)
}

fn check(dir: &TestProject, args: &[&str]) -> assert_cmd::assert::Assert {
    let mut argv = vec!["check"];
    argv.extend(args);
    Cli::on(dir).command(&argv).assert()
}

/// Product declares `status` the lifecycle field of feature and milestone;
/// software's behavior has a `status` too, but no lifecycle field.
const SOURCES: &[(&str, &str)] = &[
    (
        "specforge.json",
        r#"{"extensions": ["@specforge/software", "@specforge/product"]}"#,
    ),
    (
        "b.spec",
        "feature zeta \"Z\" {\n  problem \"p\"\n  status in_progress\n}\n\nbehavior beta \"B\" {\n  contract \"a status, but no lifecycle\"\n  status draft\n}\n",
    ),
    (
        "a.spec",
        "feature alpha \"A\" {\n  problem \"p\"\n  status done\n}\n\nmilestone mid \"M\" {\n  status \"planned\"\n}\n",
    ),
];

#[specforge_test(
    behavior = "write_build_cache",
    verify = "check --cache records each entity's kind and lifecycle state"
)]
fn check_cache_records_kinds_and_statuses() {
    let dir = project(SOURCES);

    check(&dir, &["--cache"]).success();

    let written = fs::read_to_string(dir.root().join(CACHE)).unwrap();
    assert_eq!(
        written,
        r#"{
  "format": 1,
  "statuses": {
    "alpha": {
      "kind": "feature",
      "status": "done"
    },
    "mid": {
      "kind": "milestone",
      "status": "planned"
    },
    "zeta": {
      "kind": "feature",
      "status": "in_progress"
    }
  }
}
"#
    );
}

#[specforge_test(
    behavior = "write_build_cache",
    verify = "the cache file is deterministic"
)]
fn the_cache_file_is_deterministic() {
    let dir = project(SOURCES);

    check(&dir, &["--cache"]).success();
    let first = fs::read(dir.root().join(CACHE)).unwrap();
    for _ in 0..3 {
        check(&dir, &["--cache"]).success();
        assert_eq!(fs::read(dir.root().join(CACHE)).unwrap(), first);
    }
    // No staging file is left beside it.
    let names: Vec<String> = fs::read_dir(dir.root())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(names.iter().all(|n| !n.ends_with(".tmp")), "{names:?}");
}

#[specforge_test(
    behavior = "write_build_cache",
    verify = "check without --cache never writes the cache"
)]
fn check_without_cache_never_writes_it() {
    let dir = project(SOURCES);

    check(&dir, &[]).success();
    assert!(!dir.root().join(CACHE).exists());

    // An existing cache is read, never rewritten, by a plain check.
    fs::write(dir.root().join(CACHE), "sentinel").unwrap();
    check(&dir, &[]).success();
    assert_eq!(
        fs::read_to_string(dir.root().join(CACHE)).unwrap(),
        "sentinel"
    );
}

#[specforge_test(
    behavior = "write_build_cache",
    verify = "check --cache with errors leaves the cache untouched"
)]
fn check_cache_with_errors_leaves_the_cache_untouched() {
    let dir = project(&[(
        "a.spec",
        "feature alpha \"A\" {\n  status done\n  behaviors [missing]\n}\n",
    )]);
    let previous = "{\"format\": 1, \"statuses\": {}}\n";
    fs::write(dir.root().join(CACHE), previous).unwrap();

    check(&dir, &["--cache"])
        .code(1)
        .stderr(predicates::str::contains(
            "specforge-cache.json not written",
        ));
    assert_eq!(
        fs::read_to_string(dir.root().join(CACHE)).unwrap(),
        previous
    );

    // Nor is a first cache written by a failing check.
    fs::remove_file(dir.root().join(CACHE)).unwrap();
    check(&dir, &["--cache"]).code(1);
    assert!(!dir.root().join(CACHE).exists());
}

#[specforge_test(
    behavior = "write_build_cache",
    verify = "MCP validate never writes the build cache"
)]
fn mcp_validate_never_writes_the_build_cache() {
    let dir = project(SOURCES);
    let mut server = specforge_mcp::McpServer::with_project_root(dir.root().to_path_buf());
    let mut call = |method: &str, params: serde_json::Value| -> serde_json::Value {
        let req =
            serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
        serde_json::from_str(&server.handle_message(&req.to_string()).unwrap()).unwrap()
    };
    call("initialize", serde_json::json!({}));
    // Without strict the project passes the check, which is when `check
    // --cache` writes; under strict its warnings fail it. validate takes no
    // cache argument, and an unknown one is refused (ADR 0033).
    let runs = [serde_json::json!({}), serde_json::json!({"strict": true})];
    let validate = |call: &mut dyn FnMut(&str, serde_json::Value) -> serde_json::Value,
                    arguments: &serde_json::Value| {
        let resp = call(
            "tools/call",
            serde_json::json!({"name": "specforge.validate", "arguments": arguments}),
        );
        assert_eq!(resp["result"]["isError"], false, "{resp}");
        let passes = arguments["strict"] != true;
        assert_eq!(
            resp["result"]["_meta"]["specforge/check"]["ok"], passes,
            "{arguments}: {resp}"
        );
    };
    for arguments in &runs {
        validate(&mut call, arguments);
        assert!(!dir.root().join(CACHE).exists(), "{arguments}");
    }
    let refused = call(
        "tools/call",
        serde_json::json!({"name": "specforge.validate", "arguments": {"cache": true}}),
    );
    assert_eq!(refused["result"]["isError"], true, "{refused}");
    assert_eq!(refused["result"]["structuredContent"]["argument"], "cache");
    assert!(!dir.root().join(CACHE).exists());

    // An existing cache is read, never rewritten.
    let previous = "{\"format\": 1, \"statuses\": {}}\n";
    fs::write(dir.root().join(CACHE), previous).unwrap();
    for arguments in &runs {
        validate(&mut call, arguments);
        assert_eq!(
            fs::read_to_string(dir.root().join(CACHE)).unwrap(),
            previous,
            "{arguments}"
        );
    }
}
