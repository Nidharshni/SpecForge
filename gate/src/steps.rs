//! What each check of the list runs. Every function gets the run's [`Ctx`] and the [`Selection`] the list made.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use crate::checks::{self, Selection};
use crate::judge;
use crate::repo::{OUTSIDE_WORKSPACE, Repo};
use crate::run::{CARGO_AUDIT, Ctx, Mode, TREE_SITTER};

/// Ok when nothing failed; else every failure, one per line.
fn finish(failures: Vec<String>) -> Result<(), String> {
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

pub fn pending_snapshots(ctx: &Ctx, _: &Selection) -> Result<(), String> {
    let pending: Vec<String> = if ctx.mode == Mode::Hook {
        ctx.changes
            .matching(&["**/*.snap.new"])
            .into_iter()
            .map(String::from)
            .collect()
    } else {
        let (_, listed) = ctx.capture(ctx.cmd("git").args(["ls-files", "*.snap.new"]))?;
        listed.lines().map(String::from).collect()
    };
    if pending.is_empty() {
        Ok(())
    } else {
        Err(format!("pending insta snapshots: {}", pending.join(" ")))
    }
}

pub fn rustfmt(ctx: &Ctx, _: &Selection) -> Result<(), String> {
    let mut failures = Vec::new();
    if let Err(e) = ctx.sh(ctx.cmd("cargo").args(["fmt", "--all", "--", "--check"])) {
        failures.push(format!("rustfmt would change the workspace ({e})"));
    }
    for manifest in outside_workspace(ctx.repo) {
        if let Err(e) =
            ctx.sh(ctx
                .cmd("cargo")
                .args(["fmt", "--manifest-path", &manifest, "--", "--check"]))
        {
            failures.push(format!("rustfmt would change {manifest} ({e})"));
        }
    }
    finish(failures)
}

pub fn spec_format(ctx: &Ctx, selection: &Selection) -> Result<(), String> {
    let specforge = ctx.specforge()?;
    let targets: Vec<String> = match selection {
        Selection::Only(files) => files.clone(),
        _ => ctx
            .repo
            .projects()?
            .into_iter()
            .map(|p| p.spec_dir)
            .collect(),
    };
    ctx.sh(ctx
        .cmd(specforge)
        .args(["format", "--check"])
        .args(&targets))
}

pub fn grammar(ctx: &Ctx, _: &Selection) -> Result<(), String> {
    ctx.tool(&TREE_SITTER)?;
    let crate_dir = ctx.repo.root.join("crates/tree-sitter-specforge");
    let scratch = ctx.repo.target_dir.join("gate/grammar");
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    ctx.sh(ctx
        .cmd("tree-sitter")
        .current_dir(&crate_dir)
        .arg("generate")
        .arg("-o")
        .arg(&scratch))?;
    let stale: Vec<&str> = ["parser.c", "grammar.json", "node-types.json"]
        .into_iter()
        .filter(|file| {
            std::fs::read(scratch.join(file)).ok()
                != std::fs::read(crate_dir.join("src").join(file)).ok()
        })
        .collect();
    if stale.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "grammar.js generates something else than {}",
            stale.join(", ")
        ))
    }
}

pub fn zero_entity(ctx: &Ctx, _: &Selection) -> Result<(), String> {
    ctx.sh(ctx.cmd("bash").arg("scripts/check-zero-entity-core.sh"))
}

pub fn links(ctx: &Ctx, _: &Selection) -> Result<(), String> {
    ctx.sh(ctx
        .cmd("cargo")
        .args(["run", "-q", "-p", "xtask", "--bin", "check-links"]))
}

pub fn builtins(ctx: &Ctx, selection: &Selection) -> Result<(), String> {
    if let Selection::Only(fed) = selection {
        ctx.log(&format!("# fed by this change: {}", fed.join(", ")));
    }
    ctx.sh(ctx.cmd("cargo").args([
        "run",
        "-q",
        "-p",
        "xtask",
        "--bin",
        "build-blobs",
        "--",
        "--check",
    ]))
}

pub fn guests(ctx: &Ctx, _: &Selection) -> Result<(), String> {
    let (target, reports) = (ctx.ext_target(), ctx.reports());
    // (what, cargo arguments before the manifest, after it)
    let passes: [(&str, &[&str], &[&str]); 3] = [
        ("tests", &["test", "-q"], &[]),
        (
            "wasm clippy",
            &["clippy", "-q", "--target", "wasm32-wasip2"],
            &["--", "-D", "warnings"],
        ),
        (
            "native clippy",
            &["clippy", "-q", "--all-targets"],
            &["--", "-D", "warnings"],
        ),
    ];
    let mut failures = Vec::new();
    for manifest in guests_of(ctx.repo) {
        for (what, before, after) in passes {
            let mut cmd = ctx.cmd("cargo");
            cmd.env("CARGO_TARGET_DIR", &target)
                .env("SPECFORGE_REPORT", &reports);
            cmd.args(before)
                .args(["--manifest-path", &manifest])
                .args(after);
            if let Err(e) = ctx.sh(&mut cmd) {
                failures.push(format!("{manifest} {what}: {e}"));
            }
        }
    }
    finish(failures)
}

pub fn audit(ctx: &Ctx, _: &Selection) -> Result<(), String> {
    ctx.tool(&CARGO_AUDIT)?;
    let (_, listed) = ctx.capture(ctx.cmd("git").args(["ls-files", "*Cargo.lock"]))?;
    let mut failures = Vec::new();
    for lock in listed.lines() {
        if let Err(e) = ctx.sh(ctx
            .cmd("cargo")
            .args(["audit", "--deny", "unsound", "--deny", "yanked", "-f", lock]))
        {
            failures.push(format!("{lock}: {e}"));
        }
    }
    finish(failures)
}

pub fn build(ctx: &Ctx, _: &Selection) -> Result<(), String> {
    ctx.sh(ctx
        .cmd("cargo")
        .args(["nextest", "run", "--workspace", "--no-run", "--cargo-quiet"]))
}

pub fn tests(ctx: &Ctx, selection: &Selection) -> Result<(), String> {
    let mut cmd = ctx.cmd("cargo");
    cmd.args([
        "nextest",
        "run",
        "--workspace",
        "--profile",
        ctx.mode.nextest_profile(),
    ])
    .env("SPECFORGE_REPORT", ctx.reports());
    if let Selection::Only(packages) = selection {
        // Only the changed crates and their dependents run; the workspace is built whole (same features).
        let filter: Vec<String> = packages.iter().map(|p| format!("rdeps(={p})")).collect();
        cmd.args(["-E", &filter.join(" | ")]);
    }
    ctx.sh(&mut cmd)
}

/// The exports of this repository, its examples and five fixtures against their published schemas
/// (`e2e_schema.rs`): the one test file whose names carry the claim.
pub fn published_schemas(ctx: &Ctx, _: &Selection) -> Result<(), String> {
    ctx.sh(ctx.cmd("cargo").args([
        "nextest",
        "run",
        "-p",
        "specforge-cli",
        "--profile",
        ctx.mode.nextest_profile(),
        "-E",
        "test(/project_check_admits|repository_and_its_example/)",
    ]))
}

pub fn clippy(ctx: &Ctx, _: &Selection) -> Result<(), String> {
    ctx.sh(ctx.cmd("cargo").args([
        "clippy",
        "-q",
        "--workspace",
        "--all-targets",
        "--",
        "-D",
        "warnings",
    ]))
}

pub fn doctests(ctx: &Ctx, selection: &Selection) -> Result<(), String> {
    let packages = match selection {
        Selection::Only(names) => names.clone(),
        _ => checks::doctest_members(ctx.repo),
    };
    if packages.is_empty() {
        return Ok(());
    }
    let mut cmd = ctx.cmd("cargo");
    cmd.args(["test", "-q", "--doc"]);
    for package in &packages {
        cmd.args(["-p", package]);
    }
    ctx.sh(&mut cmd)
}

pub fn specs(ctx: &Ctx, _: &Selection) -> Result<(), String> {
    let found = project_diagnostics(ctx)?;
    let verdict = judge::spec_verdict(&found, &read_baseline(ctx.repo)?, false);
    for line in &verdict.lines {
        ctx.log(line);
    }
    if verdict.failed {
        Err(first_problem(&verdict.lines))
    } else {
        Ok(())
    }
}

pub fn dogfood(ctx: &Ctx, _: &Selection) -> Result<(), String> {
    let specforge = ctx.specforge()?;
    let reports = ctx.reports();
    let json = |ctx: &Ctx, args: &[&str], extra: &[&Path]| -> Result<Value, String> {
        let mut cmd = ctx.cmd(specforge);
        cmd.args(args).args(extra).args(["--format", "json"]);
        let (ok, out) = ctx.capture(&mut cmd)?;
        let value = serde_json::from_str(&out)
            .map_err(|e| format!("`specforge {}` printed no JSON ({e})", args.join(" ")))?;
        if ok {
            Ok(value)
        } else {
            Err(format!("`specforge {}` failed", args.join(" ")))
        }
    };
    let collect = json(ctx, &["collect", "--report"], &[&reports])?;
    let analyze = json(ctx, &["analyze", "coverage"], &[])?;
    let (lines, failed) = judge::dogfood_verdict(&collect, &analyze);
    for line in &lines {
        ctx.log(line);
    }
    if failed {
        Err(first_problem(&lines))
    } else {
        Ok(())
    }
}

pub fn builtins_verify(ctx: &Ctx, _: &Selection) -> Result<(), String> {
    ctx.sh(ctx.cmd("cargo").args([
        "run",
        "-q",
        "-p",
        "xtask",
        "--bin",
        "build-blobs",
        "--",
        "--verify",
    ]))
}

/// `cargo gate baseline`: lower `gate/baseline.json` to today's warning counts; never raises one.
pub fn write_baseline(ctx: &Ctx) -> Result<Vec<String>, String> {
    let found = project_diagnostics(ctx)?;
    let verdict = judge::spec_verdict(&found, &read_baseline(ctx.repo)?, true);
    let mut lines = verdict.lines;
    match verdict.new_baseline {
        Some(baseline) if !verdict.failed => {
            let mut text = serde_json::to_string_pretty(&baseline).map_err(|e| e.to_string())?;
            text.push('\n');
            let path = ctx.repo.root.join(judge::BASELINE);
            std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
            lines.push(format!("wrote {}", judge::BASELINE));
            Ok(lines)
        }
        _ => Err(lines.join("\n")),
    }
}

fn first_problem(lines: &[String]) -> String {
    lines
        .iter()
        .find(|l| l.starts_with("  "))
        .or(lines.first())
        .map_or_else(String::new, |l| l.trim().to_string())
}

fn read_baseline(repo: &Repo) -> Result<Value, String> {
    let path = repo.root.join(judge::BASELINE);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// `specforge check --path <project> --format json` for every project: each one's diagnostics.
fn project_diagnostics(ctx: &Ctx) -> Result<BTreeMap<String, Vec<Value>>, String> {
    let specforge = ctx.specforge()?;
    let mut found = BTreeMap::new();
    for project in ctx.repo.projects()? {
        let (_, out) = ctx.capture(ctx.cmd(specforge).args([
            "check",
            "--path",
            &project.root,
            "--format",
            "json",
        ]))?;
        let diagnostics: Value = serde_json::from_str(&out).map_err(|e| {
            format!(
                "{}: `specforge check` printed no JSON ({e}): {out}",
                project.root
            )
        })?;
        let diagnostics = diagnostics
            .as_array()
            .cloned()
            .ok_or_else(|| format!("{}: `specforge check` printed no list", project.root))?;
        found.insert(project.root, diagnostics);
    }
    Ok(found)
}

/// The manifests outside the workspace (`OUTSIDE_WORKSPACE`): the guests and the fixtures.
pub(crate) fn outside_workspace(repo: &Repo) -> Vec<String> {
    OUTSIDE_WORKSPACE
        .iter()
        .flat_map(|pattern| manifests(&repo.root, pattern))
        .collect()
}

/// The guests' manifests: `extensions/*/Cargo.toml`.
fn guests_of(repo: &Repo) -> Vec<String> {
    manifests(&repo.root, "extensions/*/Cargo.toml")
}

/// `dir/*/Cargo.toml` expanded, sorted.
fn manifests(root: &Path, pattern: &str) -> Vec<String> {
    let Some((parent, _)) = pattern.split_once("/*/") else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(root.join(parent)) else {
        return Vec::new();
    };
    let mut found: Vec<String> = entries
        .flatten()
        .map(|entry| {
            format!(
                "{parent}/{}/Cargo.toml",
                entry.file_name().to_string_lossy()
            )
        })
        .filter(|manifest| root.join(manifest).is_file())
        .collect();
    found.sort();
    found
}
