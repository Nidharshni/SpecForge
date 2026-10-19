//! The CLI-to-MCP parity scenarios, one row per user-level operation (`SCENARIOS`), and the
//! `parity_*` test that runs each row through `super::parity` (the harness is in `parity.rs`).

use super::*;

pub(super) struct Scenario {
    pub(super) name: &'static str,
    pub(super) setup: fn(&TestProject),
    /// CLI arguments (after `specforge`); run with the project as cwd.
    pub(super) cli: fn(&Path) -> Vec<String>,
    /// MCP tool name and arguments.
    pub(super) mcp: fn(&Path) -> (&'static str, Value),
    /// Whether the MCP server is started on the project. `init` targets a
    /// directory that isn't a project yet.
    pub(super) mcp_rooted: bool,
}

pub(super) const SCENARIOS: &[Scenario] = &[
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
                json!({"specifiers": ["@specforge/product"]}),
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
                &s(&pairing::copied_into(root, &greet_blob())),
                "--path",
                &s(root),
                "--format",
                "json",
            ])
        },
        mcp: |root| {
            (
                "specforge.add_extension",
                json!({"specifiers": [s(&pairing::copied_into(root, &greet_blob()))]}),
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
                &s(&pairing::copied_into(root, &greet_blob())),
                "--path",
                &s(root),
                "--format",
                "json",
            ])
        },
        mcp: |root| {
            (
                "specforge.add_extension",
                json!({"specifiers": [s(&pairing::copied_into(root, &greet_blob()))]}),
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
        name: "add_dry_run",
        setup: project,
        cli: |root| {
            args(&[
                "add",
                "@specforge/product",
                "--dry-run",
                "--path",
                &s(root),
                "--format",
                "json",
            ])
        },
        mcp: |_| {
            (
                "specforge.add_extension",
                json!({"specifiers": ["@specforge/product"], "dry_run": true}),
            )
        },
        mcp_rooted: true,
    },
    Scenario {
        name: "add_dry_run_present",
        setup: project,
        cli: |root| {
            args(&[
                "add",
                "@specforge/software",
                "--dry-run",
                "--path",
                &s(root),
                "--format",
                "json",
            ])
        },
        mcp: |_| {
            (
                "specforge.add_extension",
                json!({"specifiers": ["@specforge/software"], "dry_run": true}),
            )
        },
        mcp_rooted: true,
    },
    Scenario {
        name: "remove_dry_run",
        setup: project_with_product_enabled,
        cli: |root| {
            args(&[
                "remove",
                "@specforge/product",
                "--dry-run",
                "--path",
                &s(root),
                "--format",
                "json",
            ])
        },
        mcp: |_| {
            (
                "specforge.remove_extension",
                json!({"name": "@specforge/product", "dry_run": true}),
            )
        },
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
        cli: |root| args(&["format", "--path", &s(root), "--format", "json"]),
        mcp: |root| ("specforge.format", json!({"path": s(root)})),
        mcp_rooted: true,
    },
    Scenario {
        name: "format_preview",
        setup: project_unformatted,
        cli: |root| args(&["format", "--diff", "--path", &s(root), "--format", "json"]),
        mcp: |root| ("specforge.format", json!({"path": s(root), "diff": true})),
        mcp_rooted: true,
    },
    Scenario {
        name: "format_check",
        setup: project_unformatted,
        cli: |root| args(&["format", "--check", "--path", &s(root), "--format", "json"]),
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
        name: "format_config_problem",
        setup: project_unformatted_with_a_config_problem,
        cli: |root| args(&["format", "--path", &s(root), "--format", "json"]),
        mcp: |root| ("specforge.format", json!({"path": s(root)})),
        mcp_rooted: true,
    },
    Scenario {
        name: "migrate_config_problem",
        setup: project_at_old_format_with_a_config_problem,
        cli: |root| args(&["migrate", "--path", &s(root), "--format", "json"]),
        mcp: |root| ("specforge.migrate", json!({"path": s(root)})),
        mcp_rooted: true,
    },
    Scenario {
        name: "export",
        setup: project,
        cli: |root| args(&["export", "--path", &s(root), "--format", "graph"]),
        mcp: |_| ("specforge.export", json!({"format": "graph"})),
        mcp_rooted: true,
    },
    Scenario {
        name: "check",
        setup: project,
        cli: |root| args(&["check", "--path", &s(root), "--format", "json"]),
        mcp: |_| ("specforge.validate", json!({})),
        mcp_rooted: true,
    },
    Scenario {
        name: "check_failing",
        setup: project_with_unresolved_reference,
        cli: |root| args(&["check", "--path", &s(root), "--format", "json"]),
        mcp: |_| ("specforge.validate", json!({})),
        mcp_rooted: true,
    },
    Scenario {
        name: "check_cache",
        setup: project,
        cli: |root| args(&["check", "--path", &s(root), "--format", "json", "--cache"]),
        mcp: |_| ("specforge.validate", json!({})),
        mcp_rooted: true,
    },
    Scenario {
        name: "analyze",
        setup: project,
        cli: |root| args(&["analyze", "--path", &s(root), "--format", "json"]),
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
                "--format",
                "json",
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
    Scenario {
        name: "explain",
        setup: project,
        cli: |_| args(&["explain", "w018", "--format", "json"]),
        mcp: |_| ("specforge.explain", json!({"code": "w018"})),
        mcp_rooted: true,
    },
    Scenario {
        name: "explain_retired",
        setup: project,
        cli: |_| args(&["explain", "E047", "--format", "json"]),
        mcp: |_| ("specforge.explain", json!({"code": "E047"})),
        mcp_rooted: true,
    },
    Scenario {
        name: "explain_retired_gone",
        setup: project,
        cli: |_| args(&["explain", "W024", "--format", "json"]),
        mcp: |_| ("specforge.explain", json!({"code": "W024"})),
        mcp_rooted: true,
    },
];

/// Where the CLI and MCP behave differently today, and why. Later steps of
/// plan 03 delete rows; only spec-sanctioned differences should remain.
pub(super) const EXPECTED_DIVERGENCES: &[(&str, Aspect, &str)] = &[
    // export: both surfaces export through `specforge_ops::read::export` (O2) and
    // report its notices, but only the CLI keeps `.specforge/schema-cache.json`.
    // The MCP export tool is a read-only query: if it rewrote the cache, the
    // next CLI export would miss its W053.
    (
        "export",
        Aspect::Files,
        "only the CLI writes the schema cache",
    ),
    // check: both surfaces run `specforge_ops::analysis::check` (plan 08, ADR 0018).
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
    // A run that goes around a problem in specforge.json fails its verdict (exit 1, ok false) but is no failed run:
    // a successful MCP call (ADR 0004 D4-a, ADR 0170).
    (
        "format_config_problem",
        Aspect::Outcome,
        "a run that goes around a specforge.json problem is a successful MCP call whose ok is false; the CLI exits 1",
    ),
    (
        "migrate_config_problem",
        Aspect::Outcome,
        "a run that goes around a specforge.json problem is a successful MCP call whose ok is false; the CLI exits 1",
    ),
    // The build cache is opt-in and the CLI's: validate never writes it.
    (
        "check_cache",
        Aspect::Files,
        "only `check --cache` writes the build cache (write_build_cache opt_in)",
    ),
];

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
fn parity_add_dry_run() {
    parity("add_dry_run");
}

#[test]
fn parity_add_dry_run_present() {
    parity("add_dry_run_present");
}

#[test]
fn parity_remove_dry_run() {
    parity("remove_dry_run");
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

#[test]
fn parity_explain() {
    parity("explain");
}

#[test]
fn parity_explain_retired() {
    parity("explain_retired");
}

#[test]
fn parity_explain_retired_gone() {
    parity("explain_retired_gone");
}

/// `CONFIG` with an exclude entry that can match no source: an E069 every run goes around.
fn config_with_a_problem(proj: &TestProject) {
    let config = CONFIG.replace(
        r#""spec_root": "spec","#,
        r#""spec_root": "spec", "exclude": ["["],"#,
    );
    std::fs::write(proj.root().join("specforge.json"), config).unwrap();
}

fn project_unformatted_with_a_config_problem(proj: &TestProject) {
    project_unformatted(proj);
    config_with_a_problem(proj);
}

fn project_at_old_format_with_a_config_problem(proj: &TestProject) {
    project_at_old_format(proj);
    config_with_a_problem(proj);
}

#[test]
fn parity_format_config_problem() {
    parity("format_config_problem");
}

#[test]
fn parity_migrate_config_problem() {
    parity("migrate_config_problem");
}
