//! @specforge/vitest — collects vitest results (ADR 0002).
//!
//! A test links itself to the entity it proves through vitest's test
//! metadata, which the JSON reporter carries into its report:
//!
//! ```ts
//! test('rejects a duplicate email',
//!   { meta: { specforge: { behavior: 'create_user', verify: 'rejects a duplicate email' } } },
//!   () => { ... })
//! ```
//!
//! `specforge` may also be a list of such objects (a test proving several
//! entities), set at run time (`task.meta.specforge = ...`) or on a
//! `describe` block, whose tests inherit it. This extension declares the
//! command `specforge collect` runs and maps the JSON report to entity
//! results. It runs nothing itself: the host runs the command with the
//! user's consent and passes the report to `collect__vitest`.

use specforge_extension_sdk::prelude::*;
use std::collections::BTreeMap;

const COLLECTOR: &str = "vitest";
/// The `meta` key a test's linkage lives under.
const META_KEY: &str = "specforge";
/// The linkage key naming the obligation; every other key names an entity.
const VERIFY_KEY: &str = "verify";

#[specforge_extension_sdk::extension(
    name = "@specforge/vitest",
    version = "1.0.0",
    description = "Collects vitest results for tests linked to entities through their specforge metadata"
)]
struct Vitest;

impl Contributions for Vitest {
    fn contribute(c: &mut ContributionsBuilder) {
        c.peer("@specforge/testing", "^1.0");
        c.collector(COLLECTOR, |k| {
            k.input_format("vitest-json")
                .detect_files(&["vitest.config.*", "vitest.workspace.*"])
                // `--no`: use the project's own vitest, never download one.
                .run(&[
                    "npx",
                    "--no",
                    "vitest",
                    "run",
                    "--reporter=default",
                    "--reporter=json",
                    "--outputFile.json={report}",
                ])
                .collect(|input| Ok(collect(input)));
        });
    }
}

/// The parts of vitest's JSON report this collector reads.
#[derive(serde::Deserialize)]
struct Report {
    #[serde(default, rename = "testResults")]
    test_results: Vec<FileResult>,
}

#[derive(serde::Deserialize)]
struct FileResult {
    #[serde(default, rename = "assertionResults")]
    assertion_results: Vec<Assertion>,
}

#[derive(serde::Deserialize)]
struct Assertion {
    #[serde(default, rename = "fullName")]
    full_name: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    meta: serde_json::Map<String, serde_json::Value>,
}

/// The `(entity, verify)` pairs a test's metadata links it to.
fn links(meta: &serde_json::Map<String, serde_json::Value>) -> Vec<(String, Option<String>)> {
    let objects: Vec<&serde_json::Map<String, serde_json::Value>> = match meta.get(META_KEY) {
        Some(serde_json::Value::Object(one)) => vec![one],
        Some(serde_json::Value::Array(many)) => many.iter().filter_map(|v| v.as_object()).collect(),
        _ => Vec::new(),
    };
    let mut out = Vec::new();
    for link in objects {
        let verify = link
            .get(VERIFY_KEY)
            .and_then(|v| v.as_str())
            .map(str::to_string);
        for (key, value) in link {
            if key == VERIFY_KEY {
                continue;
            }
            if let Some(entity) = value.as_str() {
                out.push((entity.to_string(), verify.clone()));
            }
        }
    }
    out
}

/// Map vitest JSON reports to entity results. Tests without `specforge`
/// metadata prove nothing and are left out; unparseable files are skipped.
fn collect(input: &CollectInput) -> CollectOutput {
    let mut by_entity: BTreeMap<String, Vec<CollectTestResult>> = BTreeMap::new();
    for file in &input.reports {
        let Ok(report) = serde_json::from_str::<Report>(&file.content) else {
            continue;
        };
        for assertion in report
            .test_results
            .iter()
            .flat_map(|f| &f.assertion_results)
        {
            let status = match assertion.status.as_str() {
                "passed" => "passed",
                "failed" => "failed",
                _ => "skipped",
            };
            let name = if assertion.full_name.is_empty() {
                assertion.title.trim()
            } else {
                assertion.full_name.trim()
            };
            for (entity, verify) in links(&assertion.meta) {
                by_entity
                    .entry(entity)
                    .or_default()
                    .push(CollectTestResult {
                        name: name.to_string(),
                        status: status.to_string(),
                        verify,
                        duration_ms: assertion.duration,
                    });
            }
        }
    }
    CollectOutput {
        entity_results: by_entity
            .into_iter()
            .map(|(entity_id, test_results)| CollectEntityResult {
                entity_id,
                test_results,
            })
            .collect(),
        ..CollectOutput::default()
    }
}

specforge_extension_sdk::component_guest!(build = specforge_extension_build);

#[cfg(test)]
mod tests;
