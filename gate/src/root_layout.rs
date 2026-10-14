//! `root-layout`: the repository's top level holds what ships and what builds, tests or documents it, and
//! nothing else (ADR 0082). Planning rounds, strategy, archived plans and experiments live in the planning
//! repository. CONTRIBUTING.md#where-things-are describes the same entries for readers.

use std::collections::BTreeSet;

/// Every top-level entry the repository tracks, with what it is for, sorted. A new entry is a reviewed edit
/// here, with its reason, and in CONTRIBUTING.md#where-things-are.
pub const ROOT: &[(&str, &str)] = &[
    (".cargo", "the `cargo gate` alias"),
    (".claude", "Claude Code agents, commands and project skills"),
    (".codex", "Codex's MCP server for this repository"),
    (".config", "nextest's profiles"),
    (
        ".githooks",
        "the pre-commit hook (`scripts/setup-hooks.sh`)",
    ),
    (".github", "the CI and release workflows"),
    (".gitignore", "what git ignores"),
    (".mcp.json", "the MCP server for this repository"),
    ("AGENTS.md", "conventions for AI agents"),
    ("ARCHITECTURE.md", "how the crates fit"),
    ("CONTEXT.md", "the domain glossary"),
    ("CONTRIBUTING.md", "how to build, test and change SpecForge"),
    ("Cargo.lock", "the workspace lockfile"),
    ("Cargo.toml", "the workspace manifest"),
    (
        "LICENSE-APACHE",
        "a licence, packaged in every release archive",
    ),
    (
        "LICENSE-MIT",
        "a licence, packaged in every release archive",
    ),
    (
        "README.md",
        "the front page, packaged in every release archive",
    ),
    ("assets", "the README's images"),
    ("clippy.toml", "clippy's configuration"),
    ("crates", "the Rust workspace"),
    (
        "docs",
        "guides, references, ADRs, the vision and agent conventions",
    ),
    (
        "evaluation",
        "the first-attempt accuracy eval behind the north star's claim",
    ),
    ("examples", "worked example projects the gate checks"),
    (
        "extensions",
        "the builtin extensions and their vendored blobs",
    ),
    ("fixtures", "extension fixtures the tests load"),
    ("gate", "`cargo gate`, the check list"),
    (
        "install.sh",
        "the installer the README's `curl` line fetches from main",
    ),
    (
        "integrations",
        "the Rust test-tracing crates and the VS Code extension",
    ),
    ("rust-toolchain.toml", "the pinned toolchain"),
    (
        "schema",
        "the JSON Schemas `specforge.json` and the test reports name",
    ),
    ("scripts", "the shell helpers the gate and the release run"),
    ("spec", "SpecForge's own specification"),
    ("specforge.json", "the project file of `spec/`"),
    ("xtask", "build-blobs, revendor and check-links"),
];

/// Where the tracked paths' top level and `root` disagree: an entry tracked but not listed, or listed but no
/// longer tracked. Empty when they agree.
pub fn disagreements(root: &[(&str, &str)], tracked: &[String]) -> Vec<String> {
    let top: BTreeSet<&str> = tracked
        .iter()
        .map(|p| p.split('/').next().unwrap_or(p.as_str()))
        .collect();
    let listed: BTreeSet<&str> = root.iter().map(|(name, _)| *name).collect();
    let mut found: Vec<String> = top
        .difference(&listed)
        .map(|name| format!("`{name}` is tracked at the top level but not on the root list"))
        .collect();
    found.extend(
        listed.difference(&top).map(|name| {
            format!("the root list names `{name}`, which the repository does not track")
        }),
    );
    found
}

#[cfg(test)]
mod tests;
