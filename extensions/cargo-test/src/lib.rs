//! @specforge/cargo-test — collects `cargo test` results (ADR 0002).
//!
//! Tests link themselves to entities with the `#[specforge_test(...)]`
//! attribute from the `specforge-test` crate, which records every annotated
//! test and writes one JSON report per test binary. This extension declares
//! the command `specforge collect` runs (`cargo test --workspace --no-fail-fast`, so one failing test
//! binary doesn't stop the others from reporting) and maps
//! those per-binary reports to entity results. It runs nothing itself: the
//! host runs the command with the user's consent and passes the report
//! files to `collect__cargo_test`.
//!
//! Plain `#[test]` functions leave no report; they only appear in libtest's
//! output. The collector captures that output and returns every test the
//! attribute didn't record as *unlinked*, for the host to link by naming
//! convention (`add_item__rejects_a_duplicate_item`, `mod add_item`).

use specforge_extension_sdk::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

const COLLECTOR: &str = "cargo-test";

#[specforge_extension_sdk::extension(
    name = "@specforge/cargo-test",
    version = "1.0.0",
    description = "Collects cargo test results for tests linked to entities with #[specforge_test]"
)]
struct CargoTest;

impl Contributions for CargoTest {
    fn contribute(c: &mut ContributionsBuilder) {
        c.meta.peer_dependencies.push(PeerDependency {
            name: "@specforge/testing".to_string(),
            version: "^1.0".to_string(),
            optional: false,
        });
        c.collector(COLLECTOR, |k| {
            k.input_format("specforge-test-json")
                .input_format("libtest-stdout")
                .detect_files(&["Cargo.toml"])
                .run(&["cargo", "test", "--workspace", "--no-fail-fast"])
                .report("target/specforge")
                .capture_stdout()
                .collect(collect);
        });
    }
}

/// One binary's report, as `specforge-test` writes it.
#[derive(serde::Deserialize)]
struct BinaryReport {
    #[serde(default)]
    entries: Vec<Entry>,
}

#[derive(serde::Deserialize)]
struct Entry {
    entity_id: String,
    test_name: String,
    #[serde(default)]
    module_path: Option<String>,
    #[serde(default)]
    verify: Option<String>,
    #[serde(default)]
    duration_ms: Option<f64>,
    status: TestStatus,
}

/// The tests the attribute recorded, by the name libtest prints for them.
/// Entries from a `specforge-test` too old to record the module path are
/// known by their function name alone.
#[derive(Default)]
struct Recorded {
    paths: BTreeSet<String>,
    bare_names: BTreeSet<String>,
}

impl Recorded {
    fn add(&mut self, entry: &Entry) {
        match entry.module_path.as_deref() {
            // libtest drops the crate (the first segment of the module path).
            Some(module) => {
                let path = match module.split_once("::") {
                    Some((_, inner)) => format!("{inner}::{}", entry.test_name),
                    None => entry.test_name.clone(),
                };
                self.paths.insert(path);
            }
            None => {
                self.bare_names.insert(entry.test_name.clone());
            }
        }
    }

    fn contains(&self, name: &str) -> bool {
        let last = name.rsplit("::").next().unwrap_or(name);
        self.paths.contains(name) || self.bare_names.contains(last)
    }
}

/// Map `specforge-test` reports to entity results, and return the tests in
/// libtest's output (the captured stdout, or a report file that isn't JSON)
/// that no report recorded as unlinked. A JSON object with an `entries` key
/// is a binary report, and an entry that does not decode (a status outside the
/// protocol's `TestStatus`, say) is the collector's error naming the file.
/// Any other JSON document in the report directory yields nothing.
fn collect(input: &CollectInput) -> Result<CollectOutput, String> {
    let mut by_entity: BTreeMap<String, Vec<TestResult>> = BTreeMap::new();
    let mut recorded = Recorded::default();
    let mut outputs: Vec<&str> = input.stdout.as_deref().into_iter().collect();
    for file in &input.reports {
        let value = match serde_json::from_str::<serde_json::Value>(&file.content) {
            Ok(value @ serde_json::Value::Object(_)) => value,
            _ => {
                outputs.push(&file.content);
                continue;
            }
        };
        if value.get("entries").is_none() {
            continue;
        }
        let report: BinaryReport =
            serde_json::from_value(value).map_err(|e| format!("{}: {e}", file.path))?;
        for entry in report.entries {
            recorded.add(&entry);
            by_entity
                .entry(entry.entity_id)
                .or_default()
                .push(TestResult {
                    name: entry.test_name,
                    status: entry.status,
                    verify: entry.verify,
                    duration_ms: entry.duration_ms,
                });
        }
    }
    let unlinked = outputs
        .into_iter()
        .flat_map(parse_libtest)
        .filter(|(name, _)| !recorded.contains(name))
        .map(|(name, status)| CollectUnlinkedTest {
            path: name.split("::").map(str::to_string).collect(),
            name,
            status,
        })
        .collect();
    Ok(CollectOutput {
        entity_results: by_entity
            .into_iter()
            .map(|(entity_id, test_results)| CollectEntityResult {
                entity_id,
                test_results,
            })
            .collect(),
        unlinked,
    })
}

/// The test results in libtest's human-readable output: every
/// `test <name> ... <result>` line, as `(name, status)`. Doc tests
/// (`src/lib.rs - item (line 3)`) and benchmarks are skipped, as is the
/// `failures:` section, where a failing test's own output is replayed.
fn parse_libtest(output: &str) -> Vec<(String, TestStatus)> {
    let mut results = Vec::new();
    let mut in_failures = false;
    for line in output.lines() {
        let line = strip_ansi(line.trim_end());
        if line == "failures:" {
            in_failures = true;
            continue;
        }
        if line.starts_with("test result:") || line.starts_with("running ") {
            in_failures = false;
            continue;
        }
        if in_failures {
            continue;
        }
        let Some((name, result)) = line
            .strip_prefix("test ")
            .and_then(|rest| rest.split_once(" ... "))
        else {
            continue;
        };
        let name = name.strip_suffix(" - should panic").unwrap_or(name);
        if name.contains(' ') {
            continue;
        }
        let status = match result {
            "ok" => TestStatus::Passed,
            "FAILED" => TestStatus::Failed,
            r if r == "ignored" || r.starts_with("ignored, ") => TestStatus::Skipped,
            _ => continue,
        };
        results.push((name.to_string(), status));
    }
    results
}

/// `text` without ANSI escape sequences (`ESC [ … letter`), which libtest
/// emits when it's asked for colour.
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

specforge_extension_sdk::component_guest!(build = specforge_extension_build);

#[cfg(test)]
mod tests;
