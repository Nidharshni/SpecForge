use std::collections::BTreeSet;
use std::path::Path;

use crate::blob_inputs;
use crate::change_set::{ChangeSet, Pattern};
use crate::judge;
use crate::repo::Repo;
use crate::run::{Ctx, Mode};
use crate::steps;

/// The smallest run that reaches a check. Hook < Quick < Full; Bootstrap is the cold-clone job alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Reach {
    Hook,
    Quick,
    Full,
    Bootstrap,
}

/// Which changed paths make a hook or quick run run a check. Full, ci and bootstrap runs ignore triggers.
pub enum Trigger {
    /// Every hook or quick run the check is reached by.
    Always,
    /// A changed path matches one of these patterns.
    Paths(&'static [&'static str]),
    /// The check computes what the change set reaches (packages, guests, blobs or files); the text says what.
    Computed(
        &'static str,
        fn(&Repo, &ChangeSet) -> Result<Selection, String>,
    ),
}

/// What a run asks a check to judge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selection {
    Skip,
    Everything,
    /// The packages, guests, blobs or files the change set reaches (sorted, deduplicated).
    Only(Vec<String>),
}

pub struct Check {
    /// Unique, kebab-case; the log is `<target>/gate/<name>.log`.
    pub name: &'static str,
    /// One line: what passing proves.
    pub proves: &'static str,
    pub reach: Reach,
    pub trigger: Trigger,
    /// It compiles the workspace: at most one such check runs at a time.
    pub compiles: bool,
    /// Checks that must have passed first; when one failed or did not run, this one is reported `blocked`.
    pub after: &'static [&'static str],
    /// The command or edit that fixes a failure, printed under it.
    pub fix: &'static str,
    pub run: fn(&Ctx, &Selection) -> Result<(), String>,
}

impl Check {
    /// What `mode` asks of this check: `Skip` when the mode does not reach it or (hook, quick) nothing it
    /// watches changed.
    pub fn selection(
        &self,
        mode: Mode,
        repo: &Repo,
        changes: &ChangeSet,
    ) -> Result<Selection, String> {
        if !self.reached_by(mode) {
            return Ok(Selection::Skip);
        }
        if matches!(mode, Mode::Full | Mode::Ci | Mode::Bootstrap) {
            return Ok(Selection::Everything);
        }
        match &self.trigger {
            Trigger::Always => Ok(Selection::Everything),
            Trigger::Paths(patterns) if changes.matching(patterns).is_empty() => {
                Ok(Selection::Skip)
            }
            Trigger::Paths(_) => Ok(Selection::Everything),
            Trigger::Computed(_, select) => select(repo, changes),
        }
    }

    fn reached_by(&self, mode: Mode) -> bool {
        match mode {
            Mode::Hook => self.reach <= Reach::Hook,
            Mode::Quick => self.reach <= Reach::Quick,
            Mode::Full | Mode::Ci => self.reach <= Reach::Full,
            Mode::Bootstrap => self.reach == Reach::Bootstrap,
        }
    }
}

/// Every check, in the order a run starts them.
pub static CHECKS: &[Check] = &[
    Check {
        name: "pending-snapshots",
        proves: "no insta `.snap.new` is staged (hook) or tracked",
        reach: Reach::Hook,
        trigger: Trigger::Always,
        compiles: false,
        after: &[],
        fix: "cargo insta review, then unstage the `.snap.new` files",
        run: steps::pending_snapshots,
    },
    Check {
        name: "rustfmt",
        proves: "the workspace, the nine guests and the fixtures are rustfmt-clean",
        reach: Reach::Hook,
        trigger: Trigger::Paths(&["**/*.rs"]),
        compiles: false,
        after: &[],
        fix: "cargo fmt --all; a guest or fixture: cargo fmt --manifest-path <its Cargo.toml>",
        run: steps::rustfmt,
    },
    Check {
        name: "spec-format",
        proves: "the .spec files of every project are formatted",
        reach: Reach::Hook,
        trigger: Trigger::Computed(
            "a .spec file in a project's spec dir; the formatter, parser or grammar",
            spec_files,
        ),
        compiles: false,
        after: &[],
        fix: "cargo run -q -p specforge-cli -- format <the files it names>",
        run: steps::spec_format,
    },
    Check {
        name: "grammar",
        proves: "the committed tree-sitter artifacts are what grammar.js generates",
        reach: Reach::Hook,
        trigger: Trigger::Paths(&[
            "crates/tree-sitter-specforge/grammar.js",
            "crates/tree-sitter-specforge/src/**",
        ]),
        compiles: false,
        after: &[],
        fix: "cd crates/tree-sitter-specforge && tree-sitter generate, then commit src/parser.c, src/grammar.json, src/node-types.json",
        run: steps::grammar,
    },
    Check {
        name: "zero-entity",
        proves: "the core crates name no entity kind of an extension",
        reach: Reach::Hook,
        trigger: Trigger::Paths(&["crates/*/src/**", "scripts/check-zero-entity-core.sh"]),
        compiles: false,
        after: &[],
        fix: "name the kind through the registry, or mark the line `// zero-entity-core: exempt`",
        run: steps::zero_entity,
    },
    Check {
        name: "links",
        proves: "every test's verify text and entity is declared by the spec (A016, static)",
        reach: Reach::Hook,
        trigger: Trigger::Computed(
            "a .spec file; a test file or one with a `verify =` attribute",
            link_sources,
        ),
        compiles: false,
        after: &[],
        fix: "edit the test's verify text to match the spec entity (or the spec), then re-stage",
        run: steps::links,
    },
    Check {
        name: "builtins",
        proves: "every builtin blob was built from the sources it recorded",
        reach: Reach::Quick,
        trigger: Trigger::Computed("a file some blob's inputs.json records", blobs_fed),
        compiles: false,
        after: &[],
        fix: "cargo run -p xtask --bin revendor (docs/agents/revendor.md)",
        run: steps::builtins,
    },
    Check {
        name: "guests",
        proves: "each guest's tests pass and it is clippy-clean for wasm and native",
        reach: Reach::Quick,
        trigger: Trigger::Paths(&[
            "extensions/**",
            "crates/**",
            "integrations/rust/**",
            "Cargo.toml",
        ]),
        compiles: false,
        after: &[],
        fix: "read <target>/gate/guests.log",
        run: steps::guests,
    },
    Check {
        name: "audit",
        proves: "no lockfile holds an unsound or yanked dependency",
        reach: Reach::Quick,
        trigger: Trigger::Paths(&["**/Cargo.lock"]),
        compiles: false,
        after: &[],
        fix: "cargo update -p <crate> --manifest-path <the lockfile's manifest>, or bump the dependency",
        run: steps::audit,
    },
    Check {
        name: "build",
        proves: "every workspace test binary and the CLI build",
        reach: Reach::Quick,
        trigger: Trigger::Computed(
            "a workspace member; the root manifest, lockfile or nextest config",
            packages,
        ),
        compiles: true,
        after: &[],
        fix: "read <target>/gate/build.log",
        run: steps::build,
    },
    Check {
        name: "tests",
        proves: "the workspace tests pass (quick: the changed crates and their dependents)",
        reach: Reach::Quick,
        trigger: Trigger::Computed(
            "a workspace member; the root manifest, lockfile or nextest config",
            packages,
        ),
        compiles: false,
        after: &["build"],
        fix: "read <target>/gate/tests.log",
        run: steps::tests,
    },
    Check {
        name: "clippy",
        proves: "the workspace is clippy-clean with warnings denied",
        reach: Reach::Quick,
        trigger: Trigger::Paths(&[
            "crates/**",
            "integrations/rust/**",
            "xtask/**",
            "gate/**",
            "Cargo.toml",
            "Cargo.lock",
            "clippy.toml",
        ]),
        compiles: true,
        after: &[],
        fix: "read <target>/gate/clippy.log",
        run: steps::clippy,
    },
    Check {
        name: "doctests",
        proves: "the doctests of the crates that have one pass",
        reach: Reach::Quick,
        trigger: Trigger::Computed(
            "a workspace member that holds a runnable doctest",
            doctest_packages,
        ),
        compiles: false,
        after: &["build"],
        fix: "read <target>/gate/doctests.log",
        run: steps::doctests,
    },
    Check {
        name: "specs",
        proves: "every project checks with no error and no warning above the baseline",
        reach: Reach::Quick,
        trigger: Trigger::Paths(&[
            "**/*.spec",
            "**/specforge.json",
            "gate/baseline.json",
            "extensions/**",
            "crates/*/src/**",
            "crates/*/Cargo.toml",
        ]),
        compiles: false,
        after: &[],
        fix: "fix the new errors and warnings; a count that fell: cargo gate baseline, in the same commit",
        run: steps::specs,
    },
    Check {
        name: "dogfood",
        proves: "the tests' reports leave no unknown entity (W115), failing proof (A014) or undeclared obligation (A016)",
        reach: Reach::Full,
        trigger: Trigger::Always,
        compiles: false,
        after: &["tests", "guests"],
        fix: "fix the test or the spec entity the finding names",
        run: steps::dogfood,
    },
    Check {
        name: "builtins-verify",
        proves: "a cold clone rebuilds every builtin blob to the vendored bytes",
        reach: Reach::Bootstrap,
        trigger: Trigger::Always,
        compiles: false,
        after: &[],
        fix: "cargo run -p xtask --bin revendor with this rustc",
        run: steps::builtins_verify,
    },
];

/// `cargo gate list`: every check, the run it joins, what triggers it and what passing proves.
pub fn render_list() -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{:<18}{:<11}{}\n",
        "CHECK", "REACH", "TRIGGER (hook, quick)"
    ));
    for check in CHECKS {
        let trigger = match &check.trigger {
            Trigger::Always => "always".to_string(),
            Trigger::Paths(patterns) => patterns.join(", "),
            Trigger::Computed(what, _) => format!("computed: {what}"),
        };
        let reach = format!("{:?}", check.reach).to_lowercase();
        let after = if check.after.is_empty() {
            String::new()
        } else {
            format!(" (after {})", check.after.join(", "))
        };
        let compiles = if check.compiles { " [compiles]" } else { "" };
        out.push_str(&format!(
            "{:<18}{:<11}{trigger}{after}{compiles}\n",
            check.name, reach
        ));
        out.push_str(&format!("{:<29}{}\n", "", check.proves));
    }
    out
}

/// The `.spec` files a hook or quick run formats: those inside a project's spec directory (the set CI checks), or
/// every project's when the formatter, the parser or the grammar changed.
fn spec_files(repo: &Repo, changes: &ChangeSet) -> Result<Selection, String> {
    // Sources only: a `.spec` fixture under a crate's `tests/` is no project's spec.
    const FORMATTING_CODE: &[&str] = &[
        "crates/specforge-formatter/src/**",
        "crates/specforge-parser/src/**",
        "crates/tree-sitter-specforge/src/**",
        "crates/tree-sitter-specforge/grammar.js",
    ];
    if !changes.matching(FORMATTING_CODE).is_empty() {
        return Ok(Selection::Everything);
    }
    let dirs: Vec<String> = repo.projects()?.into_iter().map(|p| p.spec_dir).collect();
    let inside = |path: &str| {
        dirs.iter()
            .any(|dir| Pattern(&format!("{dir}/**/*.spec")).matches(path))
    };
    let files: BTreeSet<String> = changes
        .paths()
        .filter(|p| inside(p))
        .map(String::from)
        .collect();
    Ok(only(files))
}

/// A `.spec` file, a Rust file under a `tests/` directory, or a Rust file that holds a `verify =` attribute: what can
/// change a test's link to its spec entity.
fn link_sources(repo: &Repo, changes: &ChangeSet) -> Result<Selection, String> {
    let links = changes.paths().any(|path| {
        path.ends_with(".spec")
            || (path.ends_with(".rs")
                && (path.starts_with("tests/")
                    || path.contains("/tests/")
                    || holds_verify(&repo.root.join(path))))
    });
    Ok(if links {
        Selection::Everything
    } else {
        Selection::Skip
    })
}

/// Whether the file's text has `verify`, optional spaces, then `=`.
fn holds_verify(file: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(file) else {
        return false;
    };
    text.match_indices("verify").any(|(at, word)| {
        text[at + word.len()..]
            .trim_start_matches(' ')
            .starts_with('=')
    })
}

/// The builtins whose recorded inputs hold a changed path (each blob recorded what it was built from).
fn blobs_fed(repo: &Repo, changes: &ChangeSet) -> Result<Selection, String> {
    Ok(only(
        blob_inputs::fed_by(&repo.root, changes)?
            .into_iter()
            .collect(),
    ))
}

/// The members holding a changed path; a change to the root manifest, the lockfile or nextest's config reaches every
/// member.
fn packages(repo: &Repo, changes: &ChangeSet) -> Result<Selection, String> {
    if !changes
        .matching(&["Cargo.toml", "Cargo.lock", ".config/nextest.toml"])
        .is_empty()
    {
        return Ok(Selection::Everything);
    }
    let names: BTreeSet<String> = changes
        .paths()
        .filter_map(|p| repo.member_of(p))
        .map(|m| m.name.clone())
        .collect();
    Ok(only(names))
}

/// `packages`, narrowed to the members whose sources hold a runnable doctest.
fn doctest_packages(repo: &Repo, changes: &ChangeSet) -> Result<Selection, String> {
    let reached = packages(repo, changes)?;
    let Selection::Only(names) = reached else {
        return Ok(reached);
    };
    let with_doctest: BTreeSet<String> = names
        .into_iter()
        .filter(|name| {
            repo.members
                .iter()
                .any(|m| &m.name == name && member_has_doctest(repo, &m.dir))
        })
        .collect();
    Ok(only(with_doctest))
}

/// Every member whose sources hold a runnable doctest.
pub(crate) fn doctest_members(repo: &Repo) -> Vec<String> {
    repo.members
        .iter()
        .filter(|m| member_has_doctest(repo, &m.dir))
        .map(|m| m.name.clone())
        .collect()
}

fn member_has_doctest(repo: &Repo, dir: &str) -> bool {
    let mut sources = Vec::new();
    collect_rust_sources(&repo.root.join(dir).join("src"), &mut sources);
    judge::has_runnable_doctest(&sources.iter().map(String::as_str).collect::<Vec<_>>())
}

fn collect_rust_sources(dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rust_sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.extend(std::fs::read_to_string(&path));
        }
    }
}

fn only(set: BTreeSet<String>) -> Selection {
    if set.is_empty() {
        Selection::Skip
    } else {
        Selection::Only(set.into_iter().collect())
    }
}

#[cfg(test)]
mod tests;
