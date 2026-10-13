//! Pure functions the checks call; each is tested on literal input.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::{Map, Value};

/// Where the warning baseline lives; named in the verdict's messages.
pub const BASELINE: &str = "gate/baseline.json";

pub struct SpecVerdict {
    pub lines: Vec<String>,
    pub failed: bool,
    /// With `write`: the baseline lowered to today's counts (never raised, never with an error left).
    pub new_baseline: Option<Value>,
}

/// The spec checks' verdict (was `gate.sh:94-137`): errors per project, and every warning code whose count differs
/// from `baseline`. `write` lowers the baseline and refuses when a count rose or an error remains.
pub fn spec_verdict(
    found: &BTreeMap<String, Vec<Value>>,
    baseline: &Value,
    write: bool,
) -> SpecVerdict {
    let mut lines = Vec::new();
    let mut failed = false;
    let mut counts: BTreeMap<&str, BTreeMap<String, u64>> = BTreeMap::new();
    for (project, diagnostics) in found {
        fn severity(d: &Value) -> Option<&str> {
            d["severity"].as_str()
        }
        let errors: Vec<&Value> = diagnostics
            .iter()
            .filter(|d| severity(d) == Some("Error"))
            .collect();
        let mut warnings: BTreeMap<String, u64> = BTreeMap::new();
        for d in diagnostics
            .iter()
            .filter(|d| severity(d) == Some("Warning"))
        {
            *warnings
                .entry(d["code"].as_str().unwrap_or("?").to_string())
                .or_default() += 1;
        }
        lines.push(format!(
            "{project}: {} errors, {} warnings",
            errors.len(),
            warnings.values().sum::<u64>()
        ));
        for d in &errors {
            let span = &d["span"];
            lines.push(format!(
                "  {} {}:{}: {}",
                d["code"].as_str().unwrap_or("?"),
                span["file"].as_str().unwrap_or("?"),
                span["start_line"],
                d["message"].as_str().unwrap_or("")
            ));
        }
        failed |= !errors.is_empty();
        let allowed = &baseline[project.as_str()];
        let was_of = |code: &str| allowed[code].as_u64().unwrap_or(0);
        let mut codes: Vec<&str> = warnings.keys().map(String::as_str).collect();
        codes.extend(
            allowed
                .as_object()
                .into_iter()
                .flat_map(|o| o.keys().map(String::as_str)),
        );
        codes.sort_unstable();
        codes.dedup();
        for code in codes {
            let (now, was) = (warnings.get(code).copied().unwrap_or(0), was_of(code));
            if now > was {
                lines.push(format!(
                    "  {code} rose {was} -> {now}: fix the new warnings"
                ));
                failed = true;
            } else if now < was && !write {
                lines.push(format!(
                    "  {code} fell {was} -> {now}: lower it in {BASELINE} in the same commit (cargo gate baseline)"
                ));
                failed = true;
            }
        }
        counts.insert(project, warnings);
    }
    let mut new_baseline = None;
    if write {
        if failed {
            lines.push(format!(
                "{BASELINE} not written: fix the errors and risen counts first"
            ));
        } else {
            let object: Map<String, Value> = counts
                .into_iter()
                .filter(|(_, codes)| !codes.is_empty())
                .map(|(project, codes)| {
                    let codes: Map<String, Value> = codes
                        .into_iter()
                        .map(|(c, n)| (c, Value::from(n)))
                        .collect();
                    (project.to_string(), Value::Object(codes))
                })
                .collect();
            new_baseline = Some(Value::Object(object));
        }
    }
    SpecVerdict {
        lines,
        failed,
        new_baseline,
    }
}

/// The dogfood verdict (was `gate.sh:142-158`): W115 in collect's output, A014 or A016 in analyze's; the number of
/// obligations proven, printed.
pub fn dogfood_verdict(collect: &Value, analyze: &Value) -> (Vec<String>, bool) {
    let mut lines = Vec::new();
    let mut failed = false;
    let mut unknown = Vec::new();
    find_code(collect, "W115", &mut unknown);
    for d in unknown {
        lines.push(format!(
            "W115 in collect: {}",
            d["message"].as_str().unwrap_or("")
        ));
        failed = true;
    }
    let passes = analyze["passes"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    for finding in passes
        .iter()
        .flat_map(|p| p["findings"].as_array().into_iter().flatten())
    {
        let code = finding["code"].as_str().unwrap_or("");
        if code == "A014" || code == "A016" {
            lines.push(format!(
                "{code} {}",
                finding["message"].as_str().unwrap_or("")
            ));
            failed = true;
        }
    }
    let proven = passes
        .iter()
        .find(|p| {
            p["pass"]
                .as_str()
                .is_some_and(|name| name.ends_with(":coverage"))
        })
        .map(|p| &p["summary"]["test_results"]["obligations_proven"]);
    lines.push(format!(
        "obligations proven: {}",
        proven.map_or("none".to_string(), Value::to_string)
    ));
    (lines, failed)
}

fn find_code<'a>(value: &'a Value, code: &str, out: &mut Vec<&'a Value>) {
    match value {
        Value::Object(map) => {
            if map.get("code").and_then(Value::as_str) == Some(code) {
                out.push(value);
            }
            map.values().for_each(|v| find_code(v, code, out));
        }
        Value::Array(items) => items.iter().for_each(|v| find_code(v, code, out)),
        _ => {}
    }
}

/// The `executable` of `bin`'s compiler-artifact message in cargo's JSON output (the last one, when it was built
/// more than once).
pub fn executable_of(messages: &str, bin: &str) -> Option<PathBuf> {
    messages
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|m| m["reason"] == "compiler-artifact" && m["target"]["name"] == bin)
        .filter(|m| {
            m["target"]["kind"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|k| k == "bin"))
        })
        .filter_map(|m| m["executable"].as_str().map(PathBuf::from))
        .next_back()
}

/// Whether a crate's sources hold a runnable doctest (was `gate.sh:54-66`'s awk): a `///` or `//!` fence opening with
/// no language, `rust`, `compile_fail`, `should_panic` or `no_run`. A closing fence never reopens.
pub fn has_runnable_doctest(sources: &[&str]) -> bool {
    sources.iter().any(|source| {
        let mut open = false;
        source.lines().any(|line| {
            let Some(after) = fence_language(line) else {
                return false;
            };
            if open {
                open = false;
                return false;
            }
            open = true;
            matches!(
                after.trim_end_matches([' ', '\t']),
                "" | "rust" | "compile_fail" | "should_panic" | "no_run"
            )
        })
    })
}

/// The text after the backticks of a doc-comment fence line (`/// ```rust` gives `rust`).
fn fence_language(line: &str) -> Option<&str> {
    let rest = line.trim_start_matches([' ', '\t']).strip_prefix("//")?;
    let rest = rest.strip_prefix('/').or_else(|| rest.strip_prefix('!'))?;
    rest.trim_start_matches([' ', '\t']).strip_prefix("```")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_runnable_fence_marks_a_doctest_crate() {
        for lang in ["", "rust", "no_run", "compile_fail", "should_panic"] {
            let source = format!("/// Doc.\n/// ```{lang}\n/// let a = 1;\n/// ```\nfn f() {{}}\n");
            assert!(has_runnable_doctest(&[&source]), "```{lang}");
        }
        for lang in ["text", "ignore", "json", "rust,ignore"] {
            let source = format!("//! ```{lang}\n//! x\n//! ```\n");
            assert!(!has_runnable_doctest(&[&source]), "```{lang}");
        }
        // The closing fence of a text block does not open a runnable one.
        assert!(!has_runnable_doctest(&["/// ```text\n/// x\n/// ```\n"]));
        // A plain comment is not a doc comment.
        assert!(!has_runnable_doctest(&["// ```\n// x\n// ```\n"]));
        assert!(!has_runnable_doctest(&[]));
    }

    use serde_json::json;

    fn found(project: &str, diagnostics: Value) -> BTreeMap<String, Vec<Value>> {
        BTreeMap::from([(project.to_string(), diagnostics.as_array().unwrap().clone())])
    }

    fn warnings(code: &str, count: usize) -> Value {
        Value::Array(vec![
            json!({"severity": "Warning", "code": code, "message": "m"});
            count
        ])
    }

    #[test]
    fn a_warning_that_rose_fails_and_one_that_fell_asks_for_the_baseline() {
        // §3.3: W003 1 -> 2 failed in gate.sh and passed in CI.
        let baseline = json!({".": {"W003": 1}});
        let rose = spec_verdict(&found(".", warnings("W003", 2)), &baseline, false);
        assert!(rose.failed);
        assert!(
            rose.lines
                .contains(&"  W003 rose 1 -> 2: fix the new warnings".to_string()),
            "{:?}",
            rose.lines
        );

        let fell = spec_verdict(&found(".", warnings("W003", 0)), &baseline, false);
        assert!(fell.failed);
        assert!(
            fell.lines
                .iter()
                .any(|l| l.contains("W003 fell 1 -> 0: lower it in gate/baseline.json"))
        );

        let same = spec_verdict(&found(".", warnings("W003", 1)), &baseline, false);
        assert!(!same.failed && same.new_baseline.is_none());

        // `write` lowers a fallen count, drops a project with none, and refuses after a rise.
        let lowered = spec_verdict(&found(".", warnings("W003", 0)), &baseline, true);
        assert!(!lowered.failed);
        assert_eq!(lowered.new_baseline, Some(json!({})));
        let refused = spec_verdict(&found(".", warnings("W003", 2)), &baseline, true);
        assert!(refused.failed && refused.new_baseline.is_none());
        assert!(refused.lines.last().unwrap().contains("not written"));
    }

    #[test]
    fn an_error_fails_the_spec_checks_whatever_the_baseline() {
        let error = json!([{"severity": "Error", "code": "E001", "message": "boom",
                            "span": {"file": "a.spec", "start_line": 7}}]);
        let verdict = spec_verdict(&found("p", error), &json!({}), false);
        assert!(verdict.failed);
        assert!(
            verdict.lines.contains(&"  E001 a.spec:7: boom".to_string()),
            "{:?}",
            verdict.lines
        );
        let written = spec_verdict(
            &found("p", json!([{"severity": "Error", "code": "E001"}])),
            &json!({}),
            true,
        );
        assert!(written.failed && written.new_baseline.is_none());
    }

    #[test]
    fn w115_a014_and_a016_fail_the_dogfood() {
        let analyze = |code: &str| json!({"passes": [{"pass": "@x:coverage", "findings": [{"code": code, "message": "m"}]}]});
        let collect = json!({"diagnostics": []});
        assert!(!dogfood_verdict(&collect, &analyze("A001")).1);
        assert!(dogfood_verdict(&collect, &analyze("A014")).1);
        assert!(dogfood_verdict(&collect, &analyze("A016")).1);
        let unknown = json!({"diagnostics": [{"code": "W115", "message": "no such entity"}]});
        let (lines, failed) = dogfood_verdict(&unknown, &analyze("A001"));
        assert!(failed);
        assert!(lines[0].starts_with("W115 in collect"));
    }

    #[test]
    fn the_dogfood_prints_the_obligations_proven() {
        let analyze = json!({"passes": [
            {"pass": "@x:other", "findings": []},
            {"pass": "@specforge/testing:coverage", "findings": [],
             "summary": {"test_results": {"obligations_proven": 1234}}}]});
        let (lines, failed) = dogfood_verdict(&json!({}), &analyze);
        assert!(!failed);
        assert_eq!(lines, ["obligations proven: 1234"]);
    }

    #[test]
    fn the_specforge_binary_is_where_cargo_says() {
        // §3.4: gate.sh assumed target/debug/specforge; cargo builds into CARGO_TARGET_DIR.
        let messages = [
            r#"{"reason":"compiler-artifact","target":{"name":"specforge","kind":["bin"]},"executable":"/elsewhere/targets/p04/debug/specforge"}"#,
            r#"{"reason":"compiler-artifact","target":{"name":"specforge","kind":["lib"]},"executable":null}"#,
            r#"{"reason":"compiler-artifact","target":{"name":"other","kind":["bin"]},"executable":"/x/other"}"#,
            r#"{"reason":"build-finished","success":true}"#,
            "not json",
        ]
        .join("\n");
        assert_eq!(
            executable_of(&messages, "specforge"),
            Some(PathBuf::from("/elsewhere/targets/p04/debug/specforge"))
        );
        assert_eq!(executable_of(&messages, "missing"), None);
    }
}
