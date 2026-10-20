//! The one reader and renderer of `specforge.json` for the operations that
//! run without a compile. It edits nothing: the entries a change leaves are
//! the lock plan's (ADR 0126), and the file always comes out in one style
//! ([`render`]: pretty-printed, trailing newline).
//!
//! `add` and `update` run before or instead of a compile (ADR 0015, M8),
//! so they read the config themselves, through `read_project_config`, the
//! function the compile reads it with, and refuse an unusable one with
//! [`refusal`], the refusal `remove` builds from the compile's problems:
//! before anything is written, whatever the operation.
//!
//! Who asks which gate:
//! - `add` and `update` read a fresh config through [`usable`] and
//!   [`required`] ([`ConfigProblem::blocks_edits`]);
//! - `remove` reads the compile's problems through [`refuse_edit`];
//! - `format` and `migrate` rewrite the project's sources and read a fresh
//!   config through [`rewritable`] ([`ConfigProblem::blocks_rewrites`]);
//! - `rename` and a typed edit rewrite them too, and read the compile's
//!   problems through [`refuse_rewrite`].

use crate::{OpError, OpErrorKind};
use serde_json::Value;
use specforge_common::{ConfigProblem, ConfigRead, read_project_config};
use std::path::Path;

/// The config file's name, at the project root.
pub const CONFIG_FILE: &str = "specforge.json";

/// The refusal for a `specforge.json` that cannot be edited, from the
/// reason the compile reports as E069 (`problem` is one that
/// [`blocks_edits`](ConfigProblem::blocks_edits)): code `config_invalid`,
/// the reason as its message. The one builder: `add`, `update` and
/// `remove` refuse with it, from the problems they read or the compile's.
pub fn refusal(problem: &ConfigProblem) -> OpError {
    OpError::new(
        OpErrorKind::SchemaMismatch,
        "config_invalid",
        problem.to_string(),
    )
}

/// What a project's `specforge.json` is, for an operation that runs
/// without a compile: [`read_project_config`], refusing with [`refusal`]
/// when it cannot be used. A missing file is not refused (it is the
/// default config; [`ConfigRead::found`] says so).
pub fn usable(root: &Path) -> Result<ConfigRead, OpError> {
    let read = read_project_config(root);
    refuse_edit(&read.problems)?;
    Ok(read)
}

/// What a project's `specforge.json` is, for a run that rewrites the
/// project's sources (format, migrate): [`read_project_config`], refusing
/// with [`refusal`] the first problem that
/// [`blocks_rewrites`](ConfigProblem::blocks_rewrites), before the run reads
/// any source. The problems it goes around stay in the read
/// ([`ConfigRead::diagnostics`] gives their E069s). A missing file is not
/// refused: the default config, the root's sources.
pub fn rewritable(root: &Path) -> Result<ConfigRead, OpError> {
    let read = read_project_config(root);
    refuse_rewrite(&read.problems)?;
    Ok(read)
}

/// Refused with [`refusal`] for the first of `problems` that leaves the project's sources unknown
/// ([`ConfigProblem::blocks_rewrites`]): what every run that rewrites `.spec` files asks first, over a
/// fresh read (`format`, `migrate`, through [`rewritable`]) or the compile's (`rename`, a typed edit).
pub(crate) fn refuse_rewrite(problems: &[ConfigProblem]) -> Result<(), OpError> {
    refuse_first(problems, ConfigProblem::blocks_rewrites)
}

/// Refused with [`refusal`] for the first of `problems` that leaves `specforge.json` uneditable as an
/// `extensions` list ([`ConfigProblem::blocks_edits`]): `add`, `update` ([`usable`], [`required`]) and
/// `remove` (the compile's problems).
pub(crate) fn refuse_edit(problems: &[ConfigProblem]) -> Result<(), OpError> {
    refuse_first(problems, ConfigProblem::blocks_edits)
}

fn refuse_first(
    problems: &[ConfigProblem],
    blocks: fn(&ConfigProblem) -> bool,
) -> Result<(), OpError> {
    problems
        .iter()
        .find(|p| blocks(p))
        .map_or(Ok(()), |p| Err(refusal(p)))
}

/// [`usable`], and the project must have a `specforge.json`:
/// `config_not_found`, with the hint to `init`, when it has none.
pub fn required(root: &Path) -> Result<ConfigRead, OpError> {
    let read = usable(root)?;
    if read.found {
        Ok(read)
    } else {
        Err(not_found(root))
    }
}

fn not_found(root: &Path) -> OpError {
    OpError::new(
        OpErrorKind::FileNotFound,
        "config_not_found",
        format!("no {CONFIG_FILE} in {}", root.display()),
    )
    .with_suggestion("run `specforge init` first")
}

/// `config` as the project's `specforge.json` is written: pretty-printed, with a trailing newline.
pub fn render(config: &Value) -> Result<Vec<u8>, OpError> {
    serde_json::to_string_pretty(config)
        .map(|text| (text + "\n").into_bytes())
        .map_err(|e| OpError::new(OpErrorKind::SchemaMismatch, "config_invalid", e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_test_macros::test as specforge_test;

    fn project(config: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(CONFIG_FILE), config).unwrap();
        dir
    }

    #[specforge_test(
        behavior = "management_operations_over_the_project_view",
        verify = "add, update and remove refuse an unusable specforge.json with one refusal, before they write"
    )]
    fn one_refusal_for_every_unusable_config() {
        for config in testing::UNUSABLE {
            let dir = project(config);
            let read = specforge_common::read_project_config(dir.path());
            let [problem] = read.problems.as_slice() else {
                panic!("{config}: {:?}", read.problems);
            };

            let refused = refusal(problem);
            assert_eq!(refused.code, "config_invalid", "{config}");
            assert_eq!(refused.kind, OpErrorKind::SchemaMismatch, "{config}");
            assert_eq!(refused.message, problem.to_string(), "{config}");
            // Whoever reads the file themselves gets the same refusal.
            assert_eq!(usable(dir.path()).unwrap_err(), refused, "{config}");
            assert_eq!(required(dir.path()).unwrap_err(), refused, "{config}");
        }
    }

    // The reason E069 names and the reason a refusal carries are one text, so `remove` can refuse from the
    // compile's reason without reading the file again.
    #[test]
    fn a_refusal_is_the_reason_the_compile_reports() {
        for config in [
            r#"{ "extensions": ["@specforge/product",  }"#,
            "[1,2]",
            r#"{"name":"p","extensions":"@acme/foo"}"#,
        ] {
            let dir = project(config);
            let read = specforge_common::read_project_config(dir.path());
            let [problem] = read.problems.as_slice() else {
                panic!("{config}: {:?}", read.problems);
            };
            assert!(problem.blocks_edits(), "{config}");

            let err = required(dir.path()).unwrap_err();

            assert_eq!(err.code, "config_invalid", "{config}");
            assert_eq!(err.message, problem.to_string(), "{config}");
        }
    }

    #[test]
    fn a_project_without_a_config_is_told_to_init() {
        let dir = tempfile::tempdir().unwrap();
        let err = required(dir.path()).unwrap_err();
        assert_eq!(err.code, "config_not_found");
        assert!(
            err.suggestion
                .as_deref()
                .unwrap()
                .contains("specforge init"),
            "{err:?}"
        );
    }

    #[test]
    fn a_config_is_rendered_pretty_with_a_trailing_newline() {
        let config = serde_json::json!({"version":"0.1.0","name":"p","extensions":["@acme/foo"]});

        assert_eq!(
            String::from_utf8(render(&config).unwrap()).unwrap(),
            "{\n  \"extensions\": [\n    \"@acme/foo\"\n  ],\n  \"name\": \"p\",\n  \"version\": \"0.1.0\"\n}\n"
        );
    }

    #[specforge_test(
        behavior = "management_operations_over_the_project_view",
        verify = "a config problem that leaves the sources or the extensions unknown blocks a rewrite"
    )]
    fn rewritable_refuses_what_hides_the_sources_and_keeps_the_rest() {
        for config in testing::HIDES_SOURCES {
            let dir = project(config);
            let read = specforge_common::read_project_config(dir.path());
            let first = read
                .problems
                .iter()
                .find(|p| p.blocks_rewrites())
                .unwrap_or_else(|| panic!("{config}: {:?}", read.problems));
            assert_eq!(
                rewritable(dir.path()).unwrap_err(),
                refusal(first),
                "{config}"
            );
        }

        // A problem the run goes around: Ok, with its problem in the read.
        let dir = project(r#"{"name": 5}"#);
        let read = rewritable(dir.path()).unwrap();
        assert_eq!(read.problems.len(), 1);
        assert_eq!(read.diagnostics().len(), 1);

        // No file: the default config, the root's sources.
        let dir = tempfile::tempdir().unwrap();
        let read = rewritable(dir.path()).unwrap();
        assert!(!read.found && read.problems.is_empty());
    }

    #[test]
    fn a_missing_config_is_usable_but_not_required() {
        let dir = tempfile::tempdir().unwrap();
        let read = usable(dir.path()).unwrap();
        assert!(!read.found && read.problems.is_empty());
        assert_eq!(required(dir.path()).unwrap_err().code, "config_not_found");
    }

    #[test]
    fn a_problem_that_does_not_block_an_edit_is_usable() {
        let dir = project(r#"{"extensions": ["@acme/foo", 42], "spec_root": 5}"#);
        let read = required(dir.path()).unwrap();
        assert_eq!(read.config.extensions, ["@acme/foo"]);
        assert_eq!(read.problems.len(), 2);
    }
}

/// What the tests of the operations that refuse an unusable config share.
#[cfg(test)]
pub(crate) mod testing {
    /// `specforge.json` texts that are there and can't be used: not JSON,
    /// not an object, an `extensions` value that is not an array.
    pub const UNUSABLE: [&str; 3] = [
        r#"{ "extensions": ["@specforge/product",  }"#,
        "[1,2]",
        r#"{"extensions": "@specforge/product"}"#,
    ];

    /// `specforge.json` texts that do not say which files are the project's
    /// sources (or which extensions it loads): every one refuses a rewrite.
    pub const HIDES_SOURCES: [&str; 5] = [
        r#"{"spec_root": "spec", "extensions": ["#, // not JSON
        "[1,2]",                                    // not an object
        r#"{"exclude": "vendor"}"#,                 // exclude not an array
        r#"{"spec_root": ["spec"]}"#,               // spec_root not a string
        r#"{"spec_root": "spec", "extensions": "@specforge/product"}"#, // extensions not an array
    ];

    /// Every file under `root` with its bytes: what a refused operation
    /// must leave as it was.
    pub fn files_under(
        root: &std::path::Path,
    ) -> std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
        let mut files = std::collections::BTreeMap::new();
        let mut dirs = vec![root.to_path_buf()];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dirs.push(path);
                } else {
                    files.insert(path.clone(), std::fs::read(&path).unwrap());
                }
            }
        }
        files
    }
}
