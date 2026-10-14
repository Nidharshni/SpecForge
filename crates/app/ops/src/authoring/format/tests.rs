use super::targets::targets;
use super::*;
use crate::OpErrorKind;
use specforge_common::{Diagnostic, ProjectRoot, codes};
use specforge_formatter::FormatConfig;
use std::ops::Range;
use std::path::{Path, PathBuf};

/// A run that is not refused: the tests that expect a refusal call
/// [`super::run`].
fn run(request: &Request) -> Outcome {
    super::run(request).unwrap()
}

const MESSY: &str = "behavior messy \"Messy\" {\ncontract \"The system MUST work\"\n}\n";

fn project() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join("specforge.json"), "{}").unwrap();
    std::fs::create_dir(dir.path().join("spec")).unwrap();
    std::fs::write(dir.path().join("spec/a.spec"), MESSY).unwrap();
    std::fs::write(dir.path().join("spec/b.spec"), MESSY.replace("messy", "b")).unwrap();
    dir
}

fn request(root: &ProjectRoot, mode: Mode) -> Request<'_> {
    Request {
        root,
        paths: &[],
        mode,
    }
}

/// Write `files` (path, text) under a fresh directory holding
/// `specforge.json` = `config`.
fn project_with(config: &str, files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join("specforge.json"), config).unwrap();
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

/// The changed files of a check run over `root`, relative to it.
fn changed(root: &Path) -> Vec<String> {
    run(&request(
        &specforge_common::ProjectRoot::of(root).unwrap(),
        Mode::Check,
    ))
    .changes
    .iter()
    .map(|c| relative(root, &c.path))
    .collect()
}

/// `path` relative to `root`, with `/` separators.
fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// A check run from `root` over `paths` (relative to it).
fn check_paths(root: &Path, paths: &[&str]) -> Outcome {
    let paths: Vec<PathBuf> = paths.iter().map(|p| root.join(p)).collect();
    run(&Request {
        root: &specforge_common::ProjectRoot::of(root).unwrap(),
        paths: &paths,
        mode: Mode::Check,
    })
}

/// The changed files of `outcome`, relative to `root`.
fn changed_in(root: &Path, outcome: &Outcome) -> Vec<String> {
    outcome
        .changes
        .iter()
        .map(|c| relative(root, &c.path))
        .collect()
}

// --- One document (lsp_format_document, lsp_format_range,
// lsp_respect_editor_config, format_from_stdin) ---

use specforge_test_macros::test as specforge_test;

/// A behavior whose contract is indented by 6.
const MISINDENTED: &str = "behavior foo \"Foo\" {\n      contract \"does stuff\"\n}\n";

/// A behavior indented by 4.
const FOUR: &str = "behavior login \"Login\" {\n    contract \"The system MUST log in\"\n}\n";

const EDITOR_4: EditorOptions = EditorOptions {
    tab_size: 4,
    insert_spaces: true,
};

/// A project (`specforge.json` = `{}`) with `files` written (path, text).
fn on_disk(files: &[(&str, &str)]) -> tempfile::TempDir {
    project_with("{}", files)
}

mod document;
mod run;
mod targets;
