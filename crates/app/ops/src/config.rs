//! The one writer of `specforge.json`, and the one reader for the
//! operations that run without a compile.
//!
//! Every surface that changes the project config goes through here, so the
//! file always comes out in one style (pretty-printed, trailing newline)
//! and an extension entry is matched by its exact name, never a prefix.
//!
//! `add` and `update` run before or instead of a compile (ADR 0015, M8),
//! so they read the config themselves, through `read_project_config`, the
//! function the compile reads it with, and refuse an unusable one with
//! [`refusal`], the refusal `remove` builds from the compile's problems:
//! before anything is written, whatever the operation.
//!
//! `format`, `migrate` and `rename` rewrite the project's sources: they
//! read the config through [`rewritable`], which refuses with [`refusal`]
//! when a problem [`blocks_rewrites`](ConfigProblem::blocks_rewrites), and
//! they report every other problem as its E069.

use crate::{Held, OpError, OpErrorKind};
use serde_json::Value;
use specforge_common::{ConfigProblem, ConfigRead, read_project_config};
use std::path::Path;

/// The config file's name, at the project root.
pub const CONFIG_FILE: &str = "specforge.json";

/// The extension an `extensions` entry names: `name` or `name@version`
/// (`@acme/foo@1.2.0` names `@acme/foo`). A path entry is returned whole.
pub fn entry_name(entry: &str) -> &str {
    specforge_common::extension_entry_name(entry)
}

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
    first_blocking(read_project_config(root), ConfigProblem::blocks_edits)
}

/// What a project's `specforge.json` is, for a run that rewrites the
/// project's sources (format, migrate): [`read_project_config`], refusing
/// with [`refusal`] the first problem that
/// [`blocks_rewrites`](ConfigProblem::blocks_rewrites), before the run reads
/// any source. The problems it goes around stay in the read
/// ([`ConfigRead::diagnostics`] gives their E069s). A missing file is not
/// refused: the default config, the root's sources.
pub fn rewritable(root: &Path) -> Result<ConfigRead, OpError> {
    first_blocking(read_project_config(root), ConfigProblem::blocks_rewrites)
}

/// `read`, or the refusal of its first problem that `blocks`.
fn first_blocking(
    read: ConfigRead,
    blocks: fn(&ConfigProblem) -> bool,
) -> Result<ConfigRead, OpError> {
    match read.problems.iter().find(|problem| blocks(problem)) {
        Some(problem) => Err(refusal(problem)),
        None => Ok(read),
    }
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

/// Write `config` as the project's `specforge.json`, which `file` holds: pretty-printed, with
/// a trailing newline.
pub fn write(file: &Held, config: &Value) -> Result<(), OpError> {
    let text = serde_json::to_string_pretty(config)
        .map_err(|e| OpError::new(OpErrorKind::SchemaMismatch, "config_invalid", e.to_string()))?;
    file.write(text + "\n").map_err(|e| {
        OpError::new(
            OpErrorKind::of_io(&e),
            "config_write_failed",
            format!("failed to write {}: {e}", file.spelled.display()),
        )
    })
}

/// Apply `edit` to the `extensions` array of the project's
/// `specforge.json` (created empty when absent). The file is rewritten
/// only when `edit` returns `true`; the result is that flag. `file` holds the
/// project's `specforge.json`: what is written is the file the change checked.
pub fn edit_extensions(
    root: &Path,
    file: &Held,
    edit: impl FnOnce(&mut Vec<Value>) -> bool,
) -> Result<bool, OpError> {
    // The file as the compile reads it: what blocks an edit is its E069.
    let mut config = required(root)?
        .config
        .raw
        .expect("a usable config that was found has its JSON");
    let object = config
        .as_object_mut()
        .expect("a usable config is a JSON object");
    let extensions = object
        .entry("extensions")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .expect("a usable config's extensions are an array");

    let changed = edit(extensions);
    if changed {
        write(file, &config)?;
    }
    Ok(changed)
}

/// Whether `extensions` has an entry naming exactly `name`.
pub fn has_extension(extensions: &[Value], name: &str) -> bool {
    extensions
        .iter()
        .any(|e| e.as_str().is_some_and(|s| entry_name(s) == name))
}

/// What enabling extensions changed in `specforge.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enabled {
    /// The names appended, in order: none an entry already named.
    pub appended: Vec<String>,
    /// How many extensions the file enables after the edit: its string
    /// entries, as `read_project_config` counts them.
    pub total: usize,
}

/// Append each of `names` that no entry names yet, in order, as one edit:
/// the file is written once, or not at all when every one is enabled.
pub fn enable(root: &Path, file: &Held, names: &[&str]) -> Result<Enabled, OpError> {
    let mut appended = Vec::new();
    let mut total = 0;
    edit_extensions(root, file, |extensions| {
        for name in names {
            if !has_extension(extensions, name) {
                extensions.push(Value::from(*name));
                appended.push((*name).to_string());
            }
        }
        total = extensions.iter().filter(|e| e.is_string()).count();
        !appended.is_empty()
    })?;
    Ok(Enabled { appended, total })
}

/// Drop every entry naming `name` from the project's extensions.
/// `Ok(false)` when none did.
pub fn remove_extension(root: &Path, file: &Held, name: &str) -> Result<bool, OpError> {
    edit_extensions(root, file, |extensions| {
        let before = extensions.len();
        extensions.retain(|e| e.as_str().is_none_or(|s| entry_name(s) != name));
        extensions.len() != before
    })
}

/// Drop the entries written exactly `entry` (surrounding whitespace
/// ignored) from the project's extensions. `Ok(false)` when none was.
pub fn remove_entry(root: &Path, file: &Held, entry: &str) -> Result<bool, OpError> {
    let entry = entry.trim();
    edit_extensions(root, file, |extensions| {
        let before = extensions.len();
        extensions.retain(|e| e.as_str().is_none_or(|s| s.trim() != entry));
        extensions.len() != before
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_test_macros::test as specforge_test;

    /// The config of `root`, held by no bound.
    fn held(root: &Path) -> Held {
        crate::Containment::unbounded()
            .resolve(&root.join(CONFIG_FILE))
            .unwrap()
    }

    fn project(config: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(CONFIG_FILE), config).unwrap();
        dir
    }

    fn extensions(root: &Path) -> Vec<String> {
        let text = std::fs::read_to_string(root.join(CONFIG_FILE)).unwrap();
        let config: Value = serde_json::from_str(&text).unwrap();
        config["extensions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e.as_str().unwrap().to_string())
            .collect()
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "add extension appends to extensions list"
    )]
    fn add_appends_an_extension_whose_name_prefixes_an_enabled_one() {
        let dir = project(r#"{"name":"p","extensions":["@acme/foobar@1.0.0"]}"#);

        assert_eq!(
            enable(dir.path(), &held(dir.path()), &["@acme/foo"]),
            Ok(Enabled {
                appended: vec!["@acme/foo".into()],
                total: 2
            })
        );
        assert_eq!(extensions(dir.path()), ["@acme/foobar@1.0.0", "@acme/foo"]);
    }

    #[test]
    fn enable_appends_what_is_missing_in_one_write() {
        let dir = project(r#"{"name":"p","extensions":["@acme/foo@1.0.0", 7]}"#);

        let enabled = enable(
            dir.path(),
            &held(dir.path()),
            &["@acme/foo", "@acme/bar", "@acme/baz"],
        )
        .unwrap();

        assert_eq!(enabled.appended, ["@acme/bar", "@acme/baz"]);
        assert_eq!(enabled.total, 3, "string entries only");
        let text = std::fs::read_to_string(dir.path().join(CONFIG_FILE)).unwrap();
        let config: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            config["extensions"],
            serde_json::json!(["@acme/foo@1.0.0", 7, "@acme/bar", "@acme/baz"])
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_config_edit_writes_the_file_it_was_handed() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("real.json");
        std::fs::write(&target, r#"{"name":"p","extensions":[]}"#).unwrap();
        let link = dir.path().join(CONFIG_FILE);
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let held = crate::Containment::within(dir.path())
            .resolve(&link)
            .unwrap();

        enable(dir.path(), &held, &["@acme/foo"]).unwrap();

        // The target is rewritten and the link is kept.
        assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
        assert!(
            std::fs::read_to_string(&target)
                .unwrap()
                .contains("@acme/foo")
        );
    }

    #[test]
    fn add_leaves_an_extension_already_enabled_alone() {
        let dir = project(r#"{"name":"p","extensions":["@acme/foo@1.0.0"]}"#);
        let before = std::fs::read(dir.path().join(CONFIG_FILE)).unwrap();

        assert_eq!(
            enable(dir.path(), &held(dir.path()), &["@acme/foo"]),
            Ok(Enabled {
                appended: Vec::new(),
                total: 1
            })
        );
        assert_eq!(std::fs::read(dir.path().join(CONFIG_FILE)).unwrap(), before);
    }

    #[test]
    fn remove_drops_only_the_exact_name() {
        let dir =
            project(r#"{"name":"p","extensions":["@acme/foo@1.0.0","@acme/foobar","@acme/foo"]}"#);

        assert_eq!(
            remove_extension(dir.path(), &held(dir.path()), "@acme/foo"),
            Ok(true)
        );
        assert_eq!(extensions(dir.path()), ["@acme/foobar"]);
        assert_eq!(
            remove_extension(dir.path(), &held(dir.path()), "@acme/foo"),
            Ok(false)
        );
    }

    #[test]
    fn remove_entry_drops_only_the_entries_written_so() {
        let dir = project(
            r#"{"name":"p","extensions":["./ext/a.wasm"," ./ext/a.wasm ","./ext/a.wasm@1","@acme/foo"]}"#,
        );

        assert_eq!(
            remove_entry(dir.path(), &held(dir.path()), "./ext/a.wasm"),
            Ok(true)
        );
        assert_eq!(extensions(dir.path()), ["./ext/a.wasm@1", "@acme/foo"]);
        let before = std::fs::read(dir.path().join(CONFIG_FILE)).unwrap();
        assert_eq!(
            remove_entry(dir.path(), &held(dir.path()), " ./ext/a.wasm "),
            Ok(false)
        );
        assert_eq!(
            std::fs::read(dir.path().join(CONFIG_FILE)).unwrap(),
            before,
            "no write when nothing matched"
        );
    }

    #[test]
    fn entry_name_strips_only_a_version_suffix() {
        assert_eq!(entry_name("@acme/foo"), "@acme/foo");
        assert_eq!(entry_name("@acme/foo@1.2.0"), "@acme/foo");
        assert_eq!(
            entry_name(" @specforge/formal@latest "),
            "@specforge/formal"
        );
        assert_eq!(entry_name("plain@0.0.0"), "plain");
        assert_eq!(entry_name("./ext/local.wasm"), "./ext/local.wasm");
        assert_eq!(entry_name("./ext@2/local.wasm"), "./ext@2/local.wasm");
    }

    #[test]
    fn edits_write_pretty_json_with_a_trailing_newline_and_keep_other_fields() {
        let dir = project(r#"{"name":"p","version":"0.1.0","spec_root":"spec"}"#);

        assert!(enable(dir.path(), &held(dir.path()), &["@acme/foo"]).is_ok());
        let text = std::fs::read_to_string(dir.path().join(CONFIG_FILE)).unwrap();
        assert_eq!(
            text,
            "{\n  \"extensions\": [\n    \"@acme/foo\"\n  ],\n  \"name\": \"p\",\n  \"spec_root\": \"spec\",\n  \"version\": \"0.1.0\"\n}\n"
        );
    }

    #[test]
    fn an_edit_without_a_project_says_to_init() {
        let dir = tempfile::tempdir().unwrap();
        let err = enable(dir.path(), &held(dir.path()), &["@acme/foo"]).unwrap_err();
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
    fn an_edit_refuses_a_config_whose_extensions_are_not_an_array() {
        let dir = project(r#"{"name":"p","extensions":"@acme/foo"}"#);
        let err = enable(dir.path(), &held(dir.path()), &["@acme/bar"]).unwrap_err();
        assert_eq!(err.code, "config_invalid");
    }

    // The reason E069 names and the reason an edit refuses with are one
    // text, so `remove` can refuse from the compile's reason without
    // reading the file again.
    #[test]
    fn an_edit_refuses_with_the_reason_the_compile_reports() {
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

            let err = remove_extension(dir.path(), &held(dir.path()), "@acme/foo").unwrap_err();

            assert_eq!(err.code, "config_invalid", "{config}");
            assert_eq!(err.message, problem.to_string(), "{config}");
        }

        // A problem that does not block an edit: the edit goes around it.
        let dir = project(r#"{"extensions": ["@acme/foo", 42], "spec_root": 5}"#);
        let read = specforge_common::read_project_config(dir.path());
        assert_eq!(read.problems.len(), 2, "{:?}", read.problems);
        assert!(read.problems.iter().all(|p| !p.blocks_edits()));
        assert!(remove_extension(dir.path(), &held(dir.path()), "@acme/foo").unwrap());
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
            assert_eq!(
                enable(dir.path(), &held(dir.path()), &["@acme/foo"]).unwrap_err(),
                refused,
                "{config}"
            );
        }
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
