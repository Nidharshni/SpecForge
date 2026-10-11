//! CLI↔MCP parity harness (architecture plan 03, step O0).
//!
//! Each scenario runs one user-level operation twice on identical fixture
//! projects: once through the `specforge` binary, once through an in-process
//! MCP server. It then compares what the operation did, and whether both
//! surfaces printed one outcome document (ADR 0052):
//!
//! - `Outcome`: whether the operation succeeded;
//! - `Files`:   every file under the project after the operation, except
//!   `specforge.json`;
//! - `Config`:  `specforge.json` after the operation, parsed;
//! - `Check`:   what a fresh `specforge check` reports afterwards;
//! - `Verdict`: whether the run passed: the CLI's exit code against the
//!   `ok` MCP returns (in the payload, or `_meta["specforge/check"]` for
//!   validate). Skipped where MCP returns none (ADR 0029).
//! - `Document`: the outcome document each surface printed, without its own
//!   extras (MCP's `note`, the CLI's credentials): the payload, or a failed
//!   MCP call's `data`. Only `specforge format`, which prints no JSON, differs.
//!
//! Today's differences are listed in [`EXPECTED_DIVERGENCES`]. A scenario
//! fails when an aspect differs without a row, and also when a row's aspect
//! no longer differs: fixing a divergence means deleting its row. The
//! surfaces' payloads, and everything above, are pinned per surface as
//! insta snapshots (`tests/snapshots/tests__parity__*.snap`).

use serde_json::{Value, json};
use specforge_mcp::McpServer;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Aspect {
    Outcome,
    Files,
    Config,
    Check,
    Verdict,
    Document,
}

const ASPECTS: [Aspect; 6] = [
    Aspect::Outcome,
    Aspect::Files,
    Aspect::Config,
    Aspect::Check,
    Aspect::Verdict,
    Aspect::Document,
];

/// Where the CLI and MCP behave differently today, and why. Later steps of
/// plan 03 delete rows; only spec-sanctioned differences should remain.
const EXPECTED_DIVERGENCES: &[(&str, Aspect, &str)] = &[
    // export: both surfaces export through `specforge_ops::export` (O2), but
    // only the CLI keeps `.specforge/schema-cache.json` for its W053 check.
    // The MCP export tool is a read-only query with nowhere to show W053: if
    // it rewrote the cache, the next CLI export would miss the warning.
    (
        "export",
        Aspect::Files,
        "only the CLI writes the schema cache",
    ),
    // check: both surfaces run `specforge_ops::check` (plan 08, ADR 0018).
    // Finding errors fails `specforge check` (exit 1) but is a successful
    // MCP call, whose verdict is `_meta["specforge/check"].ok`.
    (
        "check_failing",
        Aspect::Outcome,
        "MCP validate finding errors is a successful call (ADR 0004 D4-a); the CLI exits 1",
    ),
    // format --check: a file that would change fails `specforge format
    // --check` (exit 1); over MCP it is a successful call (ADR 0004 D4-a).
    (
        "format_check",
        Aspect::Outcome,
        "a format check that finds a change is a successful MCP call (ADR 0004 D4-a); the CLI exits 1",
    ),
    // analyze --min: a gate below its minimum fails `specforge analyze`
    // (exit 1); over MCP it is a successful call whose verdict is `ok`
    // (false) and whose `gate` says where it landed (ADR 0004 D4-a, 0029 D3a).
    (
        "analyze_gate_below",
        Aspect::Outcome,
        "an analysis whose gate is below its minimum is a successful MCP call (ADR 0004 D4-a); the CLI exits 1",
    ),
    // The build cache is opt-in and the CLI's: validate never writes it.
    (
        "check_cache",
        Aspect::Files,
        "only `check --cache` writes the build cache (write_build_cache opt_in)",
    ),
    (
        "extensions",
        Aspect::Document,
        "two JSON shapes today (plan 07 converges it)",
    ),
    (
        "migrate",
        Aspect::Document,
        "two JSON shapes today (plan 07 converges it)",
    ),
    (
        "migrate_failing",
        Aspect::Document,
        "two JSON shapes today (plan 07 converges it)",
    ),
    (
        "migrate_preview",
        Aspect::Document,
        "two JSON shapes today (plan 07 converges it)",
    ),
    (
        "format",
        Aspect::Document,
        "specforge format prints no JSON document; specforge.format answers McpFormatResult",
    ),
    (
        "format_check",
        Aspect::Document,
        "specforge format prints no JSON document; specforge.format answers McpFormatResult",
    ),
    (
        "format_preview",
        Aspect::Document,
        "specforge format prints no JSON document; specforge.format answers McpFormatResult",
    ),
];

// ── fixtures ────────────────────────────────────────────────────────────────

/// The registry every fixture points at: nothing listens on port 9, so a
/// surface that goes to the network fails at once instead of timing out.
const CONFIG: &str = r#"{
  "name": "demo",
  "version": "0.1.0",
  "spec_root": "spec",
  "extensions": ["@specforge/software"],
  "registries": [
    { "alias": "offline", "url": "http://127.0.0.1:9/v1", "default_registry": true }
  ]
}
"#;

const MAIN_SPEC: &str =
    "behavior alpha \"Alpha\" {\n  category \"core\"\n  contract \"The system MUST work\"\n}\n";

/// A third-party extension (`@sdk/greet` 0.1.0, contributing `greeting`).
fn greet_blob() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/greet-extension/greet.wasm")
        .canonicalize()
        .expect("the greet fixture is vendored")
}

const GREET: &str = "@sdk/greet";

fn project(root: &Path) {
    std::fs::create_dir_all(root.join("spec")).unwrap();
    std::fs::write(root.join("specforge.json"), CONFIG).unwrap();
    std::fs::write(root.join("spec/main.spec"), MAIN_SPEC).unwrap();
}

fn empty(_root: &Path) {}

fn project_with_product_enabled(root: &Path) {
    project(root);
    let config = CONFIG.replace(
        r#"["@specforge/software"]"#,
        r#"["@specforge/software", "@specforge/product"]"#,
    );
    std::fs::write(root.join("specforge.json"), config).unwrap();
}

fn project_with_greet_installed(root: &Path) {
    project(root);
    let out = cli()
        .args(["add", greet_blob().to_str().unwrap(), "--path"])
        .arg(root)
        .args(["--format", "json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "setup install failed: {out:?}");
}

/// The project with a reference to an invariant nobody declares: `check`
/// reports E003.
fn project_with_unresolved_reference(root: &Path) {
    project(root);
    std::fs::write(
        root.join("spec/main.spec"),
        "behavior alpha \"Alpha\" {\n  category \"core\"\n  contract \"The system MUST work\"\n  invariants [missing]\n}\n",
    )
    .unwrap();
}

fn project_unformatted(root: &Path) {
    project(root);
    std::fs::write(
        root.join("spec/messy.spec"),
        "behavior messy \"Messy\" {\ncategory \"core\"\ncontract \"The system MUST work\"\n}\n",
    )
    .unwrap();
}

fn project_at_old_format(root: &Path) {
    project(root);
    std::fs::write(
        root.join("spec/old.spec"),
        "// specforge-format: 0.1\nbehavior old_one \"Old\" {\n  category \"core\"\n  contract \"The system MUST work\"\n}\n",
    )
    .unwrap();
}

/// A project with a file whose format version no migration reaches.
fn project_with_failing_migration(root: &Path) {
    project(root);
    std::fs::write(
        root.join("spec/bad.spec"),
        "// specforge-format: 99.0\nbehavior bad_one \"Bad\" {\n  category \"core\"\n  contract \"The system MUST work\"\n}\n",
    )
    .unwrap();
}

// ── scenarios ───────────────────────────────────────────────────────────────

struct Scenario {
    name: &'static str,
    setup: fn(&Path),
    /// CLI arguments (after `specforge`); run with the project as cwd.
    cli: fn(&Path) -> Vec<String>,
    /// MCP tool name and arguments.
    mcp: fn(&Path) -> (&'static str, Value),
    /// Whether the MCP server is started on the project. `init` targets a
    /// directory that isn't a project yet.
    mcp_rooted: bool,
}

fn s(path: &Path) -> String {
    path.to_str().unwrap().to_string()
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|a| a.to_string()).collect()
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "init",
        setup: empty,
        cli: |_| args(&["init", "--name", "demo", "--format", "json"]),
        mcp: |root| ("specforge.init", json!({"path": s(root), "name": "demo"})),
        mcp_rooted: false,
    },
    Scenario {
        name: "add_builtin",
        setup: project,
        cli: |root| {
            args(&[
                "add",
                "@specforge/product",
                "--path",
                &s(root),
                "--format",
                "json",
            ])
        },
        mcp: |_| {
            (
                "specforge.add_extension",
                json!({"specifier": "@specforge/product"}),
            )
        },
        mcp_rooted: true,
    },
    Scenario {
        name: "add_local",
        setup: project,
        cli: |root| {
            args(&[
                "add",
                &s(&greet_blob()),
                "--path",
                &s(root),
                "--format",
                "json",
            ])
        },
        mcp: |_| {
            (
                "specforge.add_extension",
                json!({"specifier": s(&greet_blob())}),
            )
        },
        mcp_rooted: true,
    },
    Scenario {
        name: "add_local_again",
        setup: project_with_greet_installed,
        cli: |root| {
            args(&[
                "add",
                &s(&greet_blob()),
                "--path",
                &s(root),
                "--format",
                "json",
            ])
        },
        mcp: |_| {
            (
                "specforge.add_extension",
                json!({"specifier": s(&greet_blob())}),
            )
        },
        mcp_rooted: true,
    },
    Scenario {
        name: "remove_builtin",
        setup: project_with_product_enabled,
        cli: |root| {
            args(&[
                "remove",
                "@specforge/product",
                "--path",
                &s(root),
                "--format",
                "json",
            ])
        },
        mcp: |_| {
            (
                "specforge.remove_extension",
                json!({"name": "@specforge/product"}),
            )
        },
        mcp_rooted: true,
    },
    Scenario {
        name: "remove_installed",
        setup: project_with_greet_installed,
        cli: |root| args(&["remove", GREET, "--path", &s(root), "--format", "json"]),
        mcp: |_| ("specforge.remove_extension", json!({"name": GREET})),
        mcp_rooted: true,
    },
    Scenario {
        name: "extensions",
        setup: project_with_greet_installed,
        cli: |root| args(&["extensions", "--path", &s(root), "--format", "json"]),
        mcp: |_| ("specforge.extensions", json!({})),
        mcp_rooted: true,
    },
    Scenario {
        name: "providers",
        setup: project,
        cli: |root| args(&["providers", "--path", &s(root), "--format", "json"]),
        mcp: |_| ("specforge.providers", json!({})),
        mcp_rooted: true,
    },
    Scenario {
        name: "doctor",
        setup: project,
        cli: |root| args(&["doctor", "--path", &s(root), "--format", "json"]),
        mcp: |_| ("specforge.doctor", json!({})),
        mcp_rooted: true,
    },
    Scenario {
        name: "format",
        setup: project_unformatted,
        cli: |root| args(&["format", "--path", &s(root)]),
        mcp: |root| ("specforge.format", json!({"path": s(root)})),
        mcp_rooted: true,
    },
    Scenario {
        name: "format_preview",
        setup: project_unformatted,
        cli: |root| args(&["format", "--diff", "--path", &s(root)]),
        mcp: |root| ("specforge.format", json!({"path": s(root), "diff": true})),
        mcp_rooted: true,
    },
    Scenario {
        name: "format_check",
        setup: project_unformatted,
        cli: |root| args(&["format", "--check", "--path", &s(root)]),
        mcp: |root| ("specforge.format", json!({"path": s(root), "check": true})),
        mcp_rooted: true,
    },
    Scenario {
        name: "migrate",
        setup: project_at_old_format,
        cli: |root| args(&["migrate", "--path", &s(root), "--format", "json"]),
        mcp: |root| ("specforge.migrate", json!({"path": s(root)})),
        mcp_rooted: true,
    },
    Scenario {
        name: "migrate_preview",
        setup: project_at_old_format,
        cli: |root| {
            args(&[
                "migrate",
                "--dry-run",
                "--path",
                &s(root),
                "--format",
                "json",
            ])
        },
        mcp: |root| {
            (
                "specforge.migrate",
                json!({"path": s(root), "dry_run": true}),
            )
        },
        mcp_rooted: true,
    },
    Scenario {
        name: "migrate_failing",
        setup: project_with_failing_migration,
        cli: |root| args(&["migrate", "--path", &s(root), "--format", "json"]),
        mcp: |root| ("specforge.migrate", json!({"path": s(root)})),
        mcp_rooted: true,
    },
    Scenario {
        name: "export",
        setup: project,
        cli: |root| args(&["export", &s(root), "--format", "graph"]),
        mcp: |_| ("specforge.export", json!({"format": "graph"})),
        mcp_rooted: true,
    },
    Scenario {
        name: "check",
        setup: project,
        cli: |root| args(&["check", &s(root), "--format", "json"]),
        mcp: |_| ("specforge.validate", json!({})),
        mcp_rooted: true,
    },
    Scenario {
        name: "check_failing",
        setup: project_with_unresolved_reference,
        cli: |root| args(&["check", &s(root), "--format", "json"]),
        mcp: |_| ("specforge.validate", json!({})),
        mcp_rooted: true,
    },
    Scenario {
        name: "check_cache",
        setup: project,
        cli: |root| args(&["check", &s(root), "--format", "json", "--cache"]),
        mcp: |_| ("specforge.validate", json!({})),
        mcp_rooted: true,
    },
    Scenario {
        name: "analyze",
        setup: project,
        cli: |root| args(&["analyze", "--path", &s(root), "--json"]),
        mcp: |root| ("specforge.analyze", json!({"path": s(root)})),
        mcp_rooted: true,
    },
    Scenario {
        name: "analyze_gate_below",
        setup: project_with_a_coverage_gate,
        cli: |root| {
            args(&[
                "analyze",
                "coverage",
                "--min",
                "50",
                "--json",
                "--path",
                &s(root),
            ])
        },
        mcp: |root| {
            (
                "specforge.analyze",
                json!({"pass": "coverage", "min": 50, "path": s(root)}),
            )
        },
        mcp_rooted: true,
    },
];

// ── running a surface ───────────────────────────────────────────────────────

/// What one surface did to a project.
struct Observed {
    ok: bool,
    /// Whether the run passed, where the surface says (the CLI always: its
    /// exit code; MCP when it returns `ok`).
    verdict: Option<bool>,
    payload: Value,
    files: BTreeMap<String, String>,
    config: Value,
    check: Value,
    /// The outcome document: the payload (a failed call's `data`) without
    /// the surface's own extras.
    document: String,
}

/// The CLI's own keys beside an outcome document.
const CLI_EXTRAS: &[&str] = &["credentials", "credentials_failures"];
/// MCP's own keys beside an outcome document.
const MCP_EXTRAS: &[&str] = &["note"];

fn cli() -> Command {
    let mut cmd = Command::new(assert_cmd::cargo_bin!("specforge"));
    // No user credentials or caches leak into the snapshots.
    cmd.env("HOME", std::env::temp_dir().join("specforge-parity-home"));
    cmd
}

/// What a surface answered: whether the call succeeded, its payload and
/// its verdict, where it has one.
type Run = (bool, Value, Option<bool>);

fn run_cli(scenario: &Scenario, root: &Path) -> Run {
    let out = cli()
        .args((scenario.cli)(root))
        .current_dir(root)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let payload = serde_json::from_str(&stdout).unwrap_or(Value::String(stdout));
    (out.status.success(), payload, Some(out.status.success()))
}

fn run_mcp(scenario: &Scenario, root: &Path) -> Run {
    let mut server = if scenario.mcp_rooted {
        McpServer::with_project_root(root.to_path_buf())
    } else {
        McpServer::new()
    };
    let init = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}});
    server.handle_message(&init.to_string());
    let (tool, arguments) = (scenario.mcp)(root);
    let req = json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {"name": tool, "arguments": arguments}
    });
    let resp: Value = serde_json::from_str(&server.handle_message(&req.to_string()).unwrap())
        .expect("the server answers JSON");
    if let Some(error) = resp.get("error") {
        return (false, json!({"error": error}), None);
    }
    let result = &resp["result"];
    let text = result["content"][0]["text"].as_str().unwrap_or_default();
    let payload = serde_json::from_str(text).unwrap_or(Value::String(text.to_string()));
    // The verdict: `ok` in the payload (format, migrate, analyze), in a
    // failed call's data (a migration that failed), or validate's `_meta`.
    let verdict = payload["ok"]
        .as_bool()
        .or_else(|| payload["data"]["ok"].as_bool())
        .or_else(|| result["_meta"]["specforge/check"]["ok"].as_bool());
    (result["isError"] != true, payload, verdict)
}

/// Every file under `root` but `specforge.json`, by relative path. Binary
/// files are named by size.
fn files_under(root: &Path) -> BTreeMap<String, String> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(root, &path, out);
                continue;
            }
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .to_string();
            if rel == "specforge.json" {
                continue;
            }
            let bytes = std::fs::read(&path).unwrap();
            let content = match String::from_utf8(bytes) {
                Ok(text) => text,
                Err(e) => format!("<binary, {} bytes>", e.as_bytes().len()),
            };
            out.insert(rel, content);
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

fn read_config(root: &Path) -> Value {
    std::fs::read_to_string(root.join("specforge.json"))
        .map(|text| serde_json::from_str(&text).unwrap_or(Value::String(text)))
        .unwrap_or(Value::Null)
}

/// The codes a fresh `specforge check` reports, sorted, and its exit status.
fn check(root: &Path) -> Value {
    let out = cli()
        .args(["check", "--format", "json"])
        .arg(root)
        .current_dir(root)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut codes: Vec<String> = serde_json::from_str::<Value>(&stdout)
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .map(|d| {
            format!(
                "{} {}",
                d["severity"].as_str().unwrap_or("?"),
                d["code"].as_str().unwrap_or("?")
            )
        })
        .collect();
    codes.sort();
    json!({"ok": out.status.success(), "diagnostics": codes})
}

/// The outcome document of `payload`: a failed call's `data` when it has
/// one, without the top-level keys in `extras`.
fn document_of(payload: &Value, ok: bool, extras: &[&str]) -> Value {
    let mut document = match payload.get("data") {
        Some(data) if !ok && payload.is_object() => data.clone(),
        _ => payload.clone(),
    };
    if let Value::Object(map) = &mut document {
        for key in extras {
            map.remove(*key);
        }
    }
    document
}

fn observe(
    scenario: &Scenario,
    root: &Path,
    run: fn(&Scenario, &Path) -> Run,
    extras: &[&str],
) -> Observed {
    (scenario.setup)(root);
    let (ok, payload, verdict) = run(scenario, root);
    let files = files_under(root);
    let config = read_config(root);
    let check = check(root);
    let document = normalized(&document_of(&payload, ok, extras), root);
    Observed {
        ok,
        verdict,
        payload,
        files,
        config,
        check,
        document,
    }
}

/// `value` with the parts that churn for reasons outside this harness
/// redacted: the embedded Graph Protocol schema (any extension change moves
/// it), and whether z3 is installed on this machine.
fn redacted(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| {
                    let v = match k.as_str() {
                        "schema" if v.is_object() => json!("[SCHEMA]"),
                        "z3_available" => json!("[Z3]"),
                        _ => redacted(v),
                    };
                    (k.clone(), v)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(redacted).collect()),
        other => other.clone(),
    }
}

/// `text` with every 64-digit hex run (a SHA-256: blob hashes move whenever
/// an extension is rebuilt) replaced by `[SHA256]`.
fn without_hashes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut run = String::new();
    for c in text.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_hexdigit() {
            run.push(c);
            continue;
        }
        if run.len() == 64 {
            out.push_str("[SHA256]");
        } else {
            out.push_str(&run);
        }
        run.clear();
        out.push(c);
    }
    out.pop();
    out
}

/// `value` as pretty JSON with the machine-specific parts replaced.
pub(crate) fn normalized(value: &Value, root: &Path) -> String {
    let mut text = without_hashes(&serde_json::to_string_pretty(&redacted(value)).unwrap());
    let blob = s(&greet_blob());
    text = text.replace(&blob, "[BLOB]");
    let canonical = s(&root.canonicalize().unwrap());
    text = text.replace(&canonical, "[ROOT]");
    text = text.replace(&s(root), "[ROOT]");
    text
}

fn snapshot_of(o: &Observed, root: &Path) -> String {
    let files: BTreeMap<&String, Value> = o
        .files
        .iter()
        .map(|(path, content)| {
            // Binary files by kind only: blob sizes move on every rebuild.
            let shown = if content.starts_with("<binary,") {
                "<binary>".to_string()
            } else if path.ends_with("schema-cache.json") {
                "[SCHEMA]".to_string()
            } else {
                content.clone()
            };
            (path, Value::String(shown))
        })
        .collect();
    let doc = json!({
        "ok": o.ok,
        "payload": o.payload,
        "files": files,
        "config": o.config,
        "check": o.check,
    });
    normalized(&doc, root)
}

fn differs(aspect: Aspect, cli: &Observed, mcp: &Observed) -> bool {
    match aspect {
        Aspect::Outcome => cli.ok != mcp.ok,
        Aspect::Files => cli.files != mcp.files,
        Aspect::Config => cli.config != mcp.config,
        Aspect::Check => cli.check != mcp.check,
        // Only where both surfaces say.
        Aspect::Verdict => matches!((cli.verdict, mcp.verdict), (Some(c), Some(m)) if c != m),
        Aspect::Document => cli.document != mcp.document,
    }
}

fn parity(name: &str) {
    let scenario = SCENARIOS
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no scenario {name}"));
    let cli_dir = tempfile::tempdir().unwrap();
    let mcp_dir = tempfile::tempdir().unwrap();
    let cli_obs = observe(scenario, cli_dir.path(), run_cli, CLI_EXTRAS);
    let mcp_obs = observe(scenario, mcp_dir.path(), run_mcp, MCP_EXTRAS);

    let cli_snap = snapshot_of(&cli_obs, cli_dir.path());
    let mcp_snap = snapshot_of(&mcp_obs, mcp_dir.path());
    insta::assert_snapshot!(format!("{name}__cli"), cli_snap);
    insta::assert_snapshot!(format!("{name}__mcp"), mcp_snap);

    let mut unexpected = Vec::new();
    for aspect in ASPECTS {
        let expected = EXPECTED_DIVERGENCES
            .iter()
            .any(|(s, a, _)| *s == name && *a == aspect);
        let actual = differs(aspect, &cli_obs, &mcp_obs);
        if actual && !expected {
            unexpected.push(format!(
                "{aspect:?} differs but has no EXPECTED_DIVERGENCES row"
            ));
        }
        if !actual && expected {
            unexpected.push(format!(
                "{aspect:?} no longer differs: delete its EXPECTED_DIVERGENCES row"
            ));
        }
    }
    assert!(
        unexpected.is_empty(),
        "{name}:\n  {}\n--- cli ---\n{cli_snap}\n--- mcp ---\n{mcp_snap}",
        unexpected.join("\n  ")
    );
}

#[test]
fn every_expected_divergence_names_a_scenario() {
    for (name, aspect, _) in EXPECTED_DIVERGENCES {
        assert!(
            SCENARIOS.iter().any(|s| s.name == *name),
            "EXPECTED_DIVERGENCES row ({name}, {aspect:?}) names no scenario"
        );
    }
}

#[test]
fn parity_init() {
    parity("init");
}

#[test]
fn parity_add_builtin() {
    parity("add_builtin");
}

#[test]
fn parity_add_local() {
    parity("add_local");
}

#[test]
fn parity_add_local_again() {
    parity("add_local_again");
}

#[test]
fn parity_remove_builtin() {
    parity("remove_builtin");
}

#[test]
fn parity_remove_installed() {
    parity("remove_installed");
}

#[test]
fn parity_extensions() {
    parity("extensions");
}

#[test]
fn parity_providers() {
    parity("providers");
}

#[test]
fn parity_doctor() {
    parity("doctor");
}

#[test]
fn parity_format() {
    parity("format");
}

#[test]
fn parity_format_check() {
    parity("format_check");
}

#[test]
fn parity_format_preview() {
    parity("format_preview");
}

#[test]
fn parity_migrate_failing() {
    parity("migrate_failing");
}

/// The CLI's exit code and MCP's `ok` are one verdict for every operation
/// that judges: no `Verdict` divergence is expected, and each judging
/// scenario must actually have compared one.
#[specforge_test_macros::test(
    behavior = "report_command_outcome",
    verify = "the CLI's exit code and MCP's ok agree for check, analyze, format and migrate"
)]
fn verdicts_agree() {
    assert!(
        EXPECTED_DIVERGENCES
            .iter()
            .all(|(_, aspect, _)| *aspect != Aspect::Verdict),
        "a verdict divergence is a bug, not a difference to list"
    );
    for name in [
        "check",
        "check_failing",
        "analyze",
        "analyze_gate_below",
        "format",
        "format_check",
        "format_preview",
        "migrate",
        "migrate_preview",
        "migrate_failing",
    ] {
        let scenario = SCENARIOS.iter().find(|s| s.name == name).unwrap();
        let (cli_dir, mcp_dir) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let cli_obs = observe(scenario, cli_dir.path(), run_cli, CLI_EXTRAS);
        let mcp_obs = observe(scenario, mcp_dir.path(), run_mcp, MCP_EXTRAS);
        assert!(
            cli_obs.verdict.is_some() && mcp_obs.verdict.is_some(),
            "{name}: both surfaces say whether the run passed (cli {:?}, mcp {:?})",
            cli_obs.verdict,
            mcp_obs.verdict
        );
        assert_eq!(cli_obs.verdict, mcp_obs.verdict, "{name}");
    }
}

#[test]
fn parity_migrate() {
    parity("migrate");
}

#[test]
fn parity_migrate_preview() {
    parity("migrate_preview");
}

#[test]
fn parity_export() {
    parity("export");
}

#[test]
fn parity_analyze() {
    parity("analyze");
}

#[test]
fn parity_analyze_gate_below() {
    parity("analyze_gate_below");
}

#[test]
fn parity_check() {
    parity("check");
}

#[test]
fn parity_check_failing() {
    parity("check_failing");
}

#[test]
fn parity_check_cache() {
    parity("check_cache");
}

// ── export: one function, one schema policy (O2, ADR 0004 D3-a) ─────────────

/// An initialized MCP server on `root`.
fn mcp_on(root: &Path) -> McpServer {
    let mut server = McpServer::with_project_root(root.to_path_buf());
    let init = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}});
    server.handle_message(&init.to_string());
    server
}

/// The JSON document an MCP request answered with: a tool's text content,
/// or a resource's.
fn mcp_document(server: &mut McpServer, method: &str, params: Value) -> Value {
    let req = json!({"jsonrpc": "2.0", "id": 2, "method": method, "params": params});
    let resp: Value =
        serde_json::from_str(&server.handle_message(&req.to_string()).unwrap()).unwrap();
    let text = resp["result"]["content"][0]["text"]
        .as_str()
        .or_else(|| resp["result"]["contents"][0]["text"].as_str())
        .unwrap_or_else(|| panic!("no document in {resp}"));
    serde_json::from_str(text).unwrap()
}

fn mcp_export(root: &Path, arguments: Value) -> Value {
    mcp_document(
        &mut mcp_on(root),
        "tools/call",
        json!({"name": "specforge.export", "arguments": arguments}),
    )
}

fn cli_export(root: &Path, flags: &[&str]) -> Value {
    let out = cli()
        .arg("export")
        .arg(root)
        .args(flags)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    serde_json::from_slice(&out.stdout).unwrap()
}

#[specforge_test_macros::test(
    behavior = "provide_mcp_export_tool",
    verify = "the graph export is the document specforge export --format graph writes, Graph Protocol 2.0 with the schema embedded"
)]
fn mcp_graph_export_is_the_cli_export() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path());

    let mcp = mcp_export(dir.path(), json!({"format": "graph"}));
    let cli = cli_export(dir.path(), &["--format", "graph"]);

    assert_eq!(mcp["format_version"], "2.0", "{mcp}");
    assert!(mcp["schema"].is_object(), "{mcp}");
    assert_eq!(mcp, cli);
}

#[specforge_test_macros::test(
    behavior = "provide_mcp_export_tool",
    verify = "the embedded schema carries the version specforge export computes against the schema cache, which the tool leaves as it is"
)]
fn mcp_export_carries_the_schema_version_the_cli_computes() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path());
    // A CLI export caches its schema. Make the cache an older 1.2.3 that
    // also had a kind the project no longer has: a breaking change since.
    cli_export(dir.path(), &["--format", "graph"]);
    let cache_path = dir.path().join(".specforge/schema-cache.json");
    let mut cache: Value =
        serde_json::from_str(&std::fs::read_to_string(&cache_path).unwrap()).unwrap();
    cache["schema"]["schema_version"] = json!({"major": 1, "minor": 2, "patch": 3});
    cache["schema"]["entity_kinds"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name": "legacy", "source_extension": "x", "testable": false, "fields": []}));
    std::fs::write(&cache_path, cache.to_string()).unwrap();

    let mcp = mcp_export(dir.path(), json!({"format": "graph"}));
    assert_eq!(
        mcp["schema"]["schema_version"],
        json!({"major": 2, "minor": 0, "patch": 0}),
        "{}",
        mcp["schema"]["schema_version"]
    );
    assert_eq!(mcp["schema_version"], "2.0.0", "the envelope names it too");
    assert_eq!(
        std::fs::read_to_string(&cache_path).unwrap(),
        cache.to_string(),
        "the MCP export only reads the cache"
    );
    // The CLI export, which does write the cache, embeds the same version.
    let cli = cli_export(dir.path(), &["--format", "graph"]);
    assert_eq!(
        cli["schema"]["schema_version"],
        mcp["schema"]["schema_version"]
    );
}

#[specforge_test_macros::test(
    behavior = "provide_mcp_export_tool",
    verify = "with_schema embeds the schema in a context, brief or budgeted export, and no_schema leaves it out of a graph export"
)]
fn mcp_export_schema_flags_are_the_cli_flags() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path());

    for format in ["context", "brief"] {
        let plain = mcp_export(dir.path(), json!({"format": format}));
        assert!(plain.get("schema").is_none(), "{format}: {plain}");
        let with = mcp_export(dir.path(), json!({"format": format, "with_schema": true}));
        assert!(with["schema"].is_object(), "{format}: {with}");
        assert_eq!(
            with,
            cli_export(dir.path(), &["--format", format, "--with-schema"])
        );
    }

    let budgeted = mcp_export(
        dir.path(),
        json!({"format": "graph", "max_tokens": 100000, "with_schema": true}),
    );
    assert!(budgeted["schema"].is_object(), "{budgeted}");

    let without = mcp_export(dir.path(), json!({"format": "graph", "no_schema": true}));
    assert_eq!(without["format_version"], "1.0", "{without}");
    assert!(without.get("schema").is_none(), "{without}");
    assert_eq!(
        without,
        cli_export(dir.path(), &["--format", "graph", "--no-schema"])
    );
}

#[specforge_test_macros::test(
    behavior = "serve_graph_resource",
    verify = "specforge://graph under max_tokens stays within the budget, as the budgeted export does"
)]
fn graph_resource_under_a_budget_is_the_budgeted_export() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path());
    let budget = 300;

    let resource = mcp_document(
        &mut mcp_on(dir.path()),
        "resources/read",
        json!({"uri": format!("specforge://graph?max_tokens={budget}")}),
    );

    // The schema alone is thousands of tokens: a budgeted graph leaves it out.
    assert!(resource.get("schema").is_none(), "{resource}");
    let estimate = specforge_emitter::estimate_tokens(&resource.to_string());
    assert!(
        estimate <= budget,
        "{estimate} tokens > {budget}: {resource}"
    );
    assert_eq!(
        resource,
        cli_export(dir.path(), &["--format", "graph", "--max-tokens", "300"])
    );
}

// ── listings: one list per surface pair (O5) ────────────────────────────────

/// The CLI's `<command> --format json` and the MCP `tool`'s answer on the
/// same project.
fn both_listings(root: &Path, command: &str, tool: &str) -> (Value, Value) {
    let out = cli()
        .args([command, "--path"])
        .arg(root)
        .args(["--format", "json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let cli: Value = serde_json::from_slice(&out.stdout).unwrap();
    let mcp = mcp_document(
        &mut mcp_on(root),
        "tools/call",
        json!({"name": tool, "arguments": {}}),
    );
    (cli, mcp)
}

#[specforge_test_macros::test(
    behavior = "list_installed_extensions",
    verify = "the CLI and the MCP extensions tool list the same entries"
)]
fn extensions_listings_match() {
    let dir = tempfile::tempdir().unwrap();
    project_with_greet_installed(dir.path());
    std::fs::write(
        dir.path().join("spec/hello.spec"),
        "greeting hello \"Hello\" {\n  style warm\n}\n",
    )
    .unwrap();

    let (cli, mcp) = both_listings(dir.path(), "extensions", "specforge.extensions");

    assert_eq!(cli["extensions"], mcp["extensions"]);
    let greet = &cli["extensions"][0];
    assert_eq!(greet["name"], GREET, "{cli}");
    assert_eq!(greet["status"], "loaded", "{cli}");
    assert_eq!(greet["entity_kinds"], json!(["greeting"]), "{cli}");
    assert_eq!(greet["entity_count"], 1, "{cli}");
}

#[specforge_test_macros::test(
    behavior = "list_configured_providers",
    verify = "the CLI and the MCP providers tool list the same entries"
)]
fn providers_listings_match() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path());
    let mut config: Value = serde_json::from_str(CONFIG).unwrap();
    config["providers"] = json!([
        {"scheme": "gh", "alias": "work", "extension": "@acme/github"},
        {"scheme": "file", "alias": "local", "extension": "@specforge/software"},
    ]);
    std::fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();

    let (cli, mcp) = both_listings(dir.path(), "providers", "specforge.providers");

    assert_eq!(cli, mcp);
    assert_eq!(
        cli["providers"],
        json!([
            {"scheme": "gh", "alias": "work", "extension": "@acme/github", "status": "extension_not_loaded"},
            {"scheme": "file", "alias": "local", "extension": "@specforge/software", "status": "not_a_provider"},
        ])
    );
}

// ── init: one scaffold (O7) ─────────────────────────────────────────────────

#[specforge_test_macros::test(
    behavior = "provide_mcp_init_tool",
    verify = "specforge.init writes the files and config specforge init writes for the same inputs"
)]
fn mcp_init_writes_what_cli_init_writes() {
    let cli_dir = tempfile::tempdir().unwrap();
    let mcp_dir = tempfile::tempdir().unwrap();
    let (cli_root, mcp_root) = (cli_dir.path().join("demo"), mcp_dir.path().join("demo"));
    std::fs::create_dir(&cli_root).unwrap();

    let out = cli()
        .args([
            "init",
            "--extensions",
            "@specforge/software,@specforge/product",
        ])
        .current_dir(&cli_root)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let mut server = McpServer::new();
    let init = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}});
    server.handle_message(&init.to_string());
    let reply = mcp_document(
        &mut server,
        "tools/call",
        json!({"name": "specforge.init", "arguments": {
            "path": s(&mcp_root),
            "extensions": ["@specforge/software", "@specforge/product"],
        }}),
    );
    assert_eq!(reply["starter_file"], "spec/hello.spec", "{reply}");

    let files = files_under(&cli_root);
    assert_eq!(
        files.keys().collect::<Vec<_>>(),
        [".gitignore", "spec/hello.spec"]
    );
    assert_eq!(files, files_under(&mcp_root));
    assert_eq!(read_config(&cli_root), read_config(&mcp_root));
    assert_eq!(check(&mcp_root)["ok"], true);
}

// ── analyze: one payload on both surfaces (wayfinder #41) ───────────────────

/// The analyze project with a recorded report that proves nothing: a
/// `coverage --min 50` gate lands below (0 of 1).
fn project_with_a_coverage_gate(root: &Path) {
    analyze_project(root);
    std::fs::write(
        root.join("specforge-report.json"),
        r#"{"runner":"r","results":{}}"#,
    )
    .unwrap();
}

/// A project with the testing extension and a `widget` the report can prove.
fn analyze_project(root: &Path) {
    std::fs::create_dir_all(root.join("spec")).unwrap();
    std::fs::write(
        root.join("specforge.json"),
        r#"{"name":"cov","spec_root":"spec","extensions":["@specforge/software","@specforge/testing"]}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("spec/a.spec"),
        "type widget \"Widget\" {\n  id string @unique\n  verify unit \"widget valid\"\n}\n\
         invariant soft \"Softly unverified\" {\n  guarantee \"x\"\n  risk low\n}\n",
    )
    .unwrap();
}

/// A test report proving `widget` and, when `orphan` is set, naming an
/// entity the project does not have.
fn analyze_report(root: &Path, orphan: Option<&str>) -> String {
    let mut results = json!({"widget": {"tests": [{"name": "w", "status": "pass"}]}});
    if let Some(id) = orphan {
        results[id] = json!({"tests": [{"name": "x", "status": "pass"}]});
    }
    let path = root.join("report.json");
    std::fs::write(
        &path,
        json!({"runner": "r", "results": results}).to_string(),
    )
    .unwrap();
    s(&path)
}

/// What `specforge analyze --json` printed, its exit code and its stderr.
fn cli_analyze(root: &Path, extra: &[&str]) -> (Option<i32>, Value, String) {
    let out = cli()
        .args(["analyze", "--path", &s(root), "--json"])
        .args(extra)
        .current_dir(root)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let payload = serde_json::from_str(&stdout).unwrap_or(Value::Null);
    (
        out.status.code(),
        payload,
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

/// What `specforge.analyze` answered: whether it refused, and the document
/// (the payload, or the error object).
fn mcp_analyze(root: &Path, arguments: Value) -> (bool, Value) {
    let mut server = mcp_on(root);
    let req = json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {"name": "specforge.analyze", "arguments": arguments}
    });
    let resp: Value =
        serde_json::from_str(&server.handle_message(&req.to_string()).unwrap()).unwrap();
    let result = &resp["result"];
    let text = result["content"][0]["text"].as_str().unwrap_or_default();
    (
        result["isError"] == true,
        serde_json::from_str(text).unwrap_or(Value::String(text.to_string())),
    )
}

/// Both surfaces answer with the same payload, and neither has a top-level
/// field or a pass field the other lacks (named, so the failure says which).
fn assert_same_analyze_payload(cli_payload: &Value, mcp_payload: &Value) {
    let keys = |v: &Value| -> Vec<String> { v.as_object().unwrap().keys().cloned().collect() };
    assert_eq!(
        keys(cli_payload),
        keys(mcp_payload),
        "top-level fields differ\ncli: {cli_payload}\nmcp: {mcp_payload}"
    );
    let cli_passes = cli_payload["passes"].as_array().unwrap();
    let mcp_passes = mcp_payload["passes"].as_array().unwrap();
    assert_eq!(cli_passes.len(), mcp_passes.len(), "pass count differs");
    for (c, m) in cli_passes.iter().zip(mcp_passes) {
        assert_eq!(keys(c), keys(m), "pass fields differ\ncli: {c}\nmcp: {m}");
    }
    assert_eq!(cli_payload, mcp_payload);
}

#[test]
fn analyze_payload_is_equal_on_both_surfaces_by_default() {
    let dir = tempfile::tempdir().unwrap();
    analyze_project(dir.path());
    let report = analyze_report(dir.path(), None);

    let (code, cli_payload, _) = cli_analyze(dir.path(), &["--test-results", &report]);
    let (refused, mcp_payload) = mcp_analyze(dir.path(), json!({"test_results": report}));

    assert_eq!(code, Some(0));
    assert!(!refused, "{mcp_payload}");
    assert!(cli_payload["passes"].as_array().unwrap().len() >= 2);
    assert!(cli_payload.get("stray_records").is_none(), "{cli_payload}");
    assert_same_analyze_payload(&cli_payload, &mcp_payload);
}

#[test]
fn analyze_payload_is_equal_on_both_surfaces_under_strict() {
    let dir = tempfile::tempdir().unwrap();
    analyze_project(dir.path());
    let report = analyze_report(dir.path(), None);

    let (code, cli_payload, _) = cli_analyze(dir.path(), &["--strict", "--test-results", &report]);
    let (_, mcp_payload) = mcp_analyze(dir.path(), json!({"strict": true, "test_results": report}));

    assert_eq!(code, Some(1), "strict must promote the warning");
    assert_eq!(cli_payload["ok"], false);
    assert_same_analyze_payload(&cli_payload, &mcp_payload);
}

#[test]
fn analyze_payload_is_equal_on_both_surfaces_with_stray_records() {
    let dir = tempfile::tempdir().unwrap();
    analyze_project(dir.path());
    let report = analyze_report(dir.path(), Some("wodget"));

    let (code, cli_payload, stderr) = cli_analyze(dir.path(), &["--test-results", &report]);
    let (_, mcp_payload) = mcp_analyze(dir.path(), json!({"test_results": report}));

    assert_eq!(code, Some(0), "stray records never fail the run: {stderr}");
    assert!(
        cli_payload["stray_records"]
            .as_array()
            .is_some_and(|o| o.len() == 1),
        "{cli_payload}"
    );
    assert_same_analyze_payload(&cli_payload, &mcp_payload);
}

#[test]
fn analyze_bad_pass_uses_each_surfaces_channel() {
    let dir = tempfile::tempdir().unwrap();
    analyze_project(dir.path());

    // CLI: the operation refuses it, exit 2; under --json the error document
    // is on stdout and nothing is on stderr.
    let (code, stdout, stderr) = cli_analyze(dir.path(), &["nonsense"]);
    assert_eq!(code, Some(2));
    assert_eq!(stdout["code"], "unknown_pass", "{stdout}");
    assert!(
        stdout["error"]
            .as_str()
            .is_some_and(|e| e.contains("'nonsense'")),
        "{stdout}"
    );
    assert_eq!(stderr, "", "nothing on stderr");

    // MCP: an invalid_input error naming the pass.
    let (refused, error) = mcp_analyze(dir.path(), json!({"pass": "nonsense"}));
    assert!(refused, "{error}");
    let text = error.to_string();
    assert!(text.contains("invalid_input"), "{text}");
    assert!(text.contains("Unknown analysis pass 'nonsense'"), "{text}");
}

// ── export: depth and kinds narrow the export the same way ─────────────────

/// A project where `alpha` is in a feature and an invariant: three kinds,
/// and entities one hop from `alpha`.
fn export_project(root: &Path) {
    project(root);
    std::fs::write(
        root.join("spec/main.spec"),
        "behavior alpha \"Alpha\" {\n  category \"core\"\n  contract \"The system MUST work\"\n  invariants [law]\n}\n\
         invariant law \"Law\" {\n  guarantee \"x\"\n}\n\
         feature wide \"Wide\" {\n  behaviors [alpha]\n}\n",
    )
    .unwrap();
}

/// `specforge export --format brief` with `extra`, and `specforge.export`
/// with `arguments` over the same project: the two payloads.
fn export_both_ways(extra: &[&str], arguments: Value) -> (String, String) {
    let dir = tempfile::tempdir().unwrap();
    export_project(dir.path());
    let out = cli()
        .args(["export", &s(dir.path()), "--format", "brief"])
        .args(extra)
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let cli_text = String::from_utf8_lossy(&out.stdout).trim_end().to_string();

    let mut server = mcp_on(dir.path());
    let mut arguments = arguments;
    arguments["format"] = json!("brief");
    let req = json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {"name": "specforge.export", "arguments": arguments}
    });
    let resp: Value =
        serde_json::from_str(&server.handle_message(&req.to_string()).unwrap()).unwrap();
    assert_ne!(resp["result"]["isError"], true, "{resp}");
    let mcp_text = resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .trim_end()
        .to_string();
    (cli_text, mcp_text)
}

#[specforge_test_macros::test(
    behavior = "export_agent_graph_format",
    verify = "depth and kinds narrow the export the same way on every surface"
)]
fn export_scoped_depth_kinds() {
    let (cli_text, mcp_text) = export_both_ways(
        &["--scope", "alpha", "--depth", "1", "--kinds", "invariant"],
        json!({"scope": "alpha", "depth": 1, "kinds": ["invariant"]}),
    );
    assert_eq!(cli_text, mcp_text);
    assert!(cli_text.contains("alpha"), "{cli_text}");
    assert!(cli_text.contains("law"), "{cli_text}");
    assert!(!cli_text.contains("wide"), "kinds narrowed it: {cli_text}");

    // Without the narrowing the feature is in.
    let (all, _) = export_both_ways(
        &["--scope", "alpha", "--depth", "1"],
        json!({
            "scope": "alpha", "depth": 1
        }),
    );
    assert!(all.contains("wide"), "{all}");

    // The graph resource reads the same options (ADR 0024 D4).
    let dir = tempfile::tempdir().unwrap();
    export_project(dir.path());
    let mut server = mcp_on(dir.path());
    let resource = mcp_document_text(
        &mut server,
        "specforge://brief?scope=alpha&depth=1&kinds=invariant",
    );
    assert_eq!(resource.trim_end(), cli_text);
}

/// The text a resource read answered with.
fn mcp_document_text(server: &mut McpServer, uri: &str) -> String {
    let req = json!({
        "jsonrpc": "2.0", "id": 3, "method": "resources/read", "params": {"uri": uri}
    });
    let resp: Value =
        serde_json::from_str(&server.handle_message(&req.to_string()).unwrap()).unwrap();
    resp["result"]["contents"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("{resp}"))
        .to_string()
}

// ── infer: one progress and gap document on both surfaces ───────────────────

/// A Rust project half-way through inference: `src/lib.rs` is indexed and
/// its `alpha` has an entity, `src/net/wire.rs` is not.
fn infer_project(root: &Path) {
    std::fs::create_dir_all(root.join("spec")).unwrap();
    std::fs::create_dir_all(root.join("src/net")).unwrap();
    std::fs::write(
        root.join("specforge.json"),
        r#"{"name":"inf","spec_root":"spec","extensions":["@specforge/software","@specforge/rust"]}"#,
    )
    .unwrap();
    std::fs::write(root.join("spec/a.spec"), MAIN_SPEC).unwrap();
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn alpha() {}\npub fn beta() {}\n",
    )
    .unwrap();
    std::fs::write(root.join("src/net/wire.rs"), "pub struct Wire;\n").unwrap();
    std::fs::write(
        root.join("specforge-infer.json"),
        json!({
            "version": 1,
            "source_roots": ["src"],
            "source_index": [{
                "path": "src/lib.rs", "content_hash": "stale",
                "entities_produced": ["alpha"], "analyzed_at": "2026-10-01T00:00:00Z"
            }],
            "sessions": [
                {"session_id": "s-1", "agent": "claude", "status": "completed",
                 "started_at": "2026-10-01T00:00:00Z", "ended_at": "2026-10-01T01:00:00Z"},
                {"session_id": "s-2", "agent": "codex", "status": "active",
                 "started_at": "2026-10-02T00:00:00Z"}
            ]
        })
        .to_string(),
    )
    .unwrap();
}

/// `specforge infer-status --format json` with `flags`.
fn cli_infer_status(root: &Path, flags: &[&str]) -> Value {
    let out = cli()
        .args(["infer-status", "--format", "json", "--path", &s(root)])
        .args(flags)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    serde_json::from_slice(&out.stdout).unwrap()
}

/// What the tool `name` answered on a server rooted at `root`.
fn mcp_tool(root: &Path, name: &str) -> Value {
    let mut server = mcp_on(root);
    let req = json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {"name": name, "arguments": {}}
    });
    let resp: Value =
        serde_json::from_str(&server.handle_message(&req.to_string()).unwrap()).unwrap();
    assert_ne!(resp["result"]["isError"], true, "{resp}");
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    serde_json::from_str(text).unwrap()
}

#[test]
fn infer_progress_is_equal_on_both_surfaces() {
    let dir = tempfile::tempdir().unwrap();
    infer_project(dir.path());

    let cli_doc = cli_infer_status(dir.path(), &[]);
    let mcp_doc = mcp_tool(dir.path(), "specforge.infer_progress");

    assert_eq!(
        cli_doc["unanalyzed"],
        json!(["src/net/wire.rs"]),
        "{cli_doc}"
    );
    assert_eq!(cli_doc["stale"], json!(["src/lib.rs"]), "{cli_doc}");
    assert_eq!(
        cli_doc["sessions"],
        json!([
            {"session_id": "s-1", "agent": "claude", "status": "completed",
             "started_at": "2026-10-01T00:00:00Z", "ended_at": "2026-10-01T01:00:00Z"},
            {"session_id": "s-2", "agent": "codex", "status": "active",
             "started_at": "2026-10-02T00:00:00Z"}
        ]),
        "{cli_doc}"
    );
    assert_eq!(cli_doc, mcp_doc);
}

#[specforge_test_macros::test(
    behavior = "provide_infer_status_cli",
    verify = "--format json includes the gaps --gaps and --gaps-detail ask for"
)]
fn infer_status_json_carries_the_gaps_asked_for() {
    let dir = tempfile::tempdir().unwrap();
    infer_project(dir.path());

    let plain = cli_infer_status(dir.path(), &[]);
    assert!(plain.get("unanalyzed_by_directory").is_none(), "{plain}");
    assert!(plain.get("gap_analysis").is_none(), "{plain}");

    let doc = cli_infer_status(dir.path(), &["--gaps", "--gaps-detail"]);
    assert_eq!(
        doc["unanalyzed_by_directory"],
        json!([{"directory": "src/net", "count": 1, "files": ["src/net/wire.rs"]}])
    );
    // The gap report is the one specforge.infer_gaps answers with.
    let gaps = &doc["gap_analysis"];
    assert_eq!(gaps, &mcp_tool(dir.path(), "specforge.infer_gaps"));
    let names: Vec<&str> = gaps["by_directory"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|d| d["items"].as_array().unwrap())
        .map(|i| i["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"beta") && names.contains(&"Wire"), "{gaps}");
    assert!(!names.contains(&"alpha"), "alpha has an entity: {gaps}");
}

// ── doctor: one report on both surfaces ─────────────────────────────────────

#[test]
fn doctor_report_is_the_same_on_both_surfaces() {
    let dir = tempfile::tempdir().unwrap();
    project_with_greet_installed(dir.path());
    // A lock entry whose binary is gone: an issue both must report.
    let lock_path = dir.path().join("specforge.lock");
    let mut lock: Value =
        serde_json::from_str(&std::fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["entries"].as_array_mut().unwrap().push(json!({
        "name": "@acme/gone", "version": "1.0.0", "source": "registry", "wasm_hash": "00"
    }));
    std::fs::write(&lock_path, lock.to_string()).unwrap();

    let out = cli()
        .args(["doctor", "--format", "json", "--path", &s(dir.path())])
        .output()
        .unwrap();
    let cli_doc: Value = serde_json::from_slice(&out.stdout).unwrap();
    let mcp_doc = mcp_tool(dir.path(), "specforge.doctor");

    // One report on both surfaces: the CLI adds the user's credentials.
    for key in [
        "ok",
        "findings",
        "extensions",
        "enhancements",
        "conflicts",
        "cache_status",
        "installed_count",
        "extensions_ok",
        "z3_available",
    ] {
        assert_eq!(cli_doc[key], mcp_doc[key], "{key} differs");
    }
    assert!(
        cli_doc["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["about"] == "binary"),
        "{cli_doc}"
    );
    assert_eq!(mcp_doc["ok"], false);
    assert_eq!(mcp_doc["extensions_ok"], false);
    // Registry credentials are the user's, reported by the CLI only.
    assert!(cli_doc.get("credentials").is_some());
    assert!(mcp_doc.get("credentials").is_none());
}

// ── collect: one outcome on both surfaces ───────────────────────────────────

/// A cargo-test project with a report already on disk: one test proves
/// `alpha`, one names an entity the project lacks (W115).
fn collect_project(root: &Path) {
    std::fs::create_dir_all(root.join("spec")).unwrap();
    std::fs::create_dir_all(root.join("target/specforge")).unwrap();
    std::fs::write(
        root.join("specforge.json"),
        r#"{"name":"col","spec_root":"spec","extensions":["@specforge/software","@specforge/testing","@specforge/cargo-test"]}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("spec/a.spec"),
        "behavior alpha \"Alpha\" {\n  verify unit \"works\"\n}\n",
    )
    .unwrap();
    std::fs::write(root.join("Cargo.toml"), "").unwrap();
    std::fs::write(
        root.join("target/specforge/demo.json"),
        json!({"entries": [
            {"entity_id": "alpha", "test_name": "works", "verify": "works", "status": "pass"},
            {"entity_id": "omega", "test_name": "lost", "status": "pass"}
        ]})
        .to_string(),
    )
    .unwrap();
}

#[test]
fn collect_outcome_is_the_same_on_both_surfaces() {
    let cli_dir = tempfile::tempdir().unwrap();
    let mcp_dir = tempfile::tempdir().unwrap();
    collect_project(cli_dir.path());
    collect_project(mcp_dir.path());

    let out = cli()
        .args([
            "collect",
            "--no-run",
            "--format",
            "json",
            "--path",
            &s(cli_dir.path()),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let cli_doc: Value = serde_json::from_slice(&out.stdout).unwrap();
    let mcp_doc = mcp_tool(mcp_dir.path(), "specforge.collect");

    assert_eq!(cli_doc["diagnostics"][0]["code"], "W115", "{cli_doc}");
    let without_report = |doc: &Value, root: &Path| {
        let mut doc = doc.clone();
        let report = doc["report"].as_str().unwrap().to_string();
        assert!(
            Path::new(&report).ends_with("specforge-report.json"),
            "{report}"
        );
        doc["report"] = json!(normalized(&json!(report), root));
        doc
    };
    assert_eq!(
        without_report(&cli_doc, cli_dir.path()),
        without_report(&mcp_doc, mcp_dir.path())
    );
    assert_eq!(
        std::fs::read_to_string(cli_dir.path().join("specforge-report.json")).unwrap(),
        std::fs::read_to_string(mcp_dir.path().join("specforge-report.json")).unwrap(),
    );
}
