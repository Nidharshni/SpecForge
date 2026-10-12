//! The project a path is in, on every CLI surface (`locate_project_root`).
//!
//! Each project-taking command runs twice on two fresh copies of one project:
//! from the project's root and from `spec/docs` inside it, the path left at
//! its default (`.`). Both runs must act on the same project: the same exit
//! code, the same output once the project's directory is written `<P>`, and
//! (for a command that writes) the same files under the project.
//!
//! Today's differences are listed in [`DIVERGES_TODAY`]. A row that differs
//! without being listed fails, and so does a listed row that no longer
//! differs: fixing a surface means deleting its row.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use specforge_test_macros::test as specforge_test;
use tempfile::TempDir;

use crate::child_guard::{ChildGuard, guarded_command};

/// The rows whose two runs differ today.
const DIVERGES_TODAY: &[&str] = &[
    // Compiled at the path as given.
    "check",
    "stats",
    "export",
    "schema",
    "model",
    "outline",
    "query",
    "trace",
    "explore",
    "infer-guide",
    "analyze",
    "doctor",
    "providers",
    "extensions",
    "remove",
    "watch",
    "product features",
    // `specforge.json` read at the path as given.
    "search",
    "add",
    "update",
    "publish",
    "login",
];

/// The project every row runs in: software and product enabled, a provider
/// configured, an offline registry (nothing listens on port 9), and a spec
/// whose diagnostics depend on the extensions (E014, E006, W006).
const CONFIG: &str = r#"{
  "name": "p",
  "version": "0.1.0",
  "spec_root": "spec",
  "extensions": ["@specforge/software", "@specforge/product"],
  "registries": [
    { "alias": "offline", "url": "http://127.0.0.1:9/v1", "default_registry": true }
  ],
  "providers": [
    { "scheme": "file", "alias": "junit", "extension": "@specforge/rust" }
  ]
}
"#;

const SPEC: &str = "feature alpha \"Alpha\" {\n  problem \"Users need alpha\"\n}\n\nbehavior a \"A\" {\n  features [alpha]\n}\n";

/// The subdirectory every row runs from the second time.
const SUBDIR: &str = "spec/docs";

/// A third-party extension (`@sdk/greet` 0.1.0).
fn greet_blob() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/greet-extension/greet.wasm")
        .canonicalize()
        .expect("the greet fixture is vendored")
}

fn specforge() -> Command {
    Command::new(env!("CARGO_BIN_EXE_specforge"))
}

/// A fresh copy of the project.
fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(SUBDIR)).unwrap();
    std::fs::write(dir.path().join("specforge.json"), CONFIG).unwrap();
    std::fs::write(dir.path().join("spec/a.spec"), SPEC).unwrap();
    dir
}

/// The project with `@sdk/greet` installed and locked, as `update` needs.
fn with_greet_locked(root: &Path) {
    let out = specforge()
        .args(["add", greet_blob().to_str().unwrap(), "--path"])
        .arg(root)
        .args(["--format", "json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "setup install failed: {out:?}");
}

fn nothing(_: &Path) {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Compare the exit code, stdout and stderr.
    Read,
    /// Compare the exit code, stdout and every file under the project.
    Writes,
    /// Compare the first event (`ready`).
    Watch,
    /// An extension command: compare the exit code, stdout and stderr.
    Extension,
}

struct Row {
    name: &'static str,
    /// The arguments after `specforge`; `{BLOB}` is the greet blob.
    args: &'static [&'static str],
    kind: Kind,
    setup: fn(&Path),
}

const fn row(name: &'static str, args: &'static [&'static str], kind: Kind) -> Row {
    Row {
        name,
        args,
        kind,
        setup: nothing,
    }
}

const ROWS: &[Row] = &[
    row("check", &["check", "--format", "json"], Kind::Read),
    row("stats", &["stats", "--format", "json"], Kind::Read),
    // Export writes the schema cache: where it lands is compared too.
    row("export", &["export", "--format", "json"], Kind::Writes),
    row("schema", &["schema"], Kind::Read),
    row("model", &["model", "--format", "json"], Kind::Read),
    row("outline", &["outline", "--format", "json"], Kind::Read),
    row("query", &["query", "alpha", "--format", "json"], Kind::Read),
    row("trace", &["trace", "--format", "json"], Kind::Read),
    row("explore", &["explore", "--format", "json"], Kind::Read),
    row("review", &["review", "--format", "json"], Kind::Read),
    row(
        "infer-guide",
        &["infer-guide", "--format", "json"],
        Kind::Read,
    ),
    row(
        "infer-status",
        &["infer-status", "--format", "json"],
        Kind::Read,
    ),
    row("analyze", &["analyze", "--format", "json"], Kind::Read),
    row("doctor", &["doctor", "--format", "json"], Kind::Read),
    row("providers", &["providers", "--format", "json"], Kind::Read),
    row(
        "extensions",
        &["extensions", "--format", "json"],
        Kind::Read,
    ),
    row(
        "collect",
        &["collect", "--no-run", "--format", "json"],
        Kind::Read,
    ),
    row(
        "migrate",
        &["migrate", "--dry-run", "--format", "json"],
        Kind::Read,
    ),
    row("format", &["format", "--check"], Kind::Read),
    row("search", &["search", "foo", "--format", "json"], Kind::Read),
    row(
        "add",
        &["add", "@specforge/governance", "--format", "json"],
        Kind::Writes,
    ),
    row(
        "remove",
        &["remove", "@specforge/product", "--format", "json"],
        Kind::Writes,
    ),
    Row {
        name: "update",
        args: &["update", "--format", "json"],
        kind: Kind::Writes,
        setup: with_greet_locked,
    },
    // The registry error names the registry the project configures.
    row(
        "publish",
        &["publish", "{BLOB}", "--format", "json"],
        Kind::Read,
    ),
    row(
        "login",
        &[
            "login",
            "--registry",
            "offline",
            "--token",
            "x",
            "--format",
            "json",
        ],
        Kind::Read,
    ),
    row("watch", &["watch", "--format", "json"], Kind::Watch),
    row(
        "product features",
        &["product", "features", "--format", "json"],
        Kind::Extension,
    ),
];

/// `text` with the project's directory (canonical and as spelled) written
/// `<P>`, the greet blob `[BLOB]`, and every SHA-256 `[SHA256]`.
fn normalized(text: &str, root: &Path) -> String {
    let canonical = root.canonicalize().unwrap();
    let text = text
        .replace(greet_blob().to_str().unwrap(), "[BLOB]")
        .replace(canonical.to_str().unwrap(), "<P>")
        .replace(root.to_str().unwrap(), "<P>");
    let mut out = String::with_capacity(text.len());
    let mut run = String::new();
    for c in text.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_hexdigit() {
            run.push(c);
            continue;
        }
        out.push_str(if run.len() == 64 { "[SHA256]" } else { &run });
        run.clear();
        out.push(c);
    }
    out.pop();
    out
}

/// Every file under `root`, by relative path, normalized.
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
            let content = match String::from_utf8(std::fs::read(&path).unwrap()) {
                Ok(text) if rel.ends_with("schema-cache.json") => {
                    // Its content moves with every extension rebuild; where
                    // it is written is what matters.
                    let _ = text;
                    "[SCHEMA]".to_string()
                }
                Ok(text) => normalized(&text, root),
                Err(_) => "<binary>".to_string(),
            };
            out.insert(rel, content);
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

/// What one run of `row` did, from `cwd` inside the project at `root`.
#[derive(Debug, PartialEq)]
struct Observed {
    exit: Option<i32>,
    stdout: String,
    stderr: String,
    files: BTreeMap<String, String>,
}

fn args_of(row: &Row) -> Vec<String> {
    let blob = greet_blob();
    row.args
        .iter()
        .map(|a| a.replace("{BLOB}", blob.to_str().unwrap()))
        .collect()
}

fn observe(row: &Row, root: &Path, cwd: &Path) -> Observed {
    (row.setup)(root);
    if row.kind == Kind::Watch {
        return Observed {
            exit: None,
            stdout: normalized(&first_watch_event(&args_of(row), cwd), root),
            stderr: String::new(),
            files: BTreeMap::new(),
        };
    }
    let out = specforge()
        .args(args_of(row))
        .current_dir(cwd)
        .output()
        .unwrap();
    Observed {
        exit: out.status.code(),
        stdout: normalized(&String::from_utf8_lossy(&out.stdout), root),
        stderr: match row.kind {
            Kind::Writes => String::new(),
            _ => normalized(&String::from_utf8_lossy(&out.stderr), root),
        },
        files: match row.kind {
            Kind::Writes => files_under(root),
            _ => BTreeMap::new(),
        },
    }
}

/// The first line `specforge watch` prints from `cwd`, or what it printed
/// before exiting when it stops first.
fn first_watch_event(args: &[String], cwd: &Path) -> String {
    let mut watch = specforge();
    watch.args(args);
    let mut command = guarded_command(&watch);
    command
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = ChildGuard::spawn(&mut command).expect("spawn specforge watch");
    let stdout = child.take_stdout().expect("stdout piped");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut lines = BufReader::new(stdout).lines().map_while(Result::ok);
        let _ = tx.send(lines.next().unwrap_or_default());
    });
    rx.recv_timeout(Duration::from_secs(120))
        .expect("watch prints its first event")
}

/// Run `row` from the root of one fresh copy and from its subdirectory in
/// another: whether the two runs differ, and both observations.
fn compare(row: &Row) -> (bool, Observed, Observed) {
    let at_root = project();
    let at_subdir = project();
    let root = observe(row, at_root.path(), at_root.path());
    let subdir = observe(row, at_subdir.path(), &at_subdir.path().join(SUBDIR));
    (root != subdir, root, subdir)
}

/// Compare `rows`, in parallel, and check the differing set against
/// [`DIVERGES_TODAY`].
fn assert_rows(kinds: &[Kind]) {
    let rows: Vec<&Row> = ROWS.iter().filter(|r| kinds.contains(&r.kind)).collect();
    let results: Vec<(&str, bool, Observed, Observed)> = std::thread::scope(|scope| {
        let handles: Vec<_> = rows
            .iter()
            .map(|row| {
                scope.spawn(move || {
                    let (differs, root, subdir) = compare(row);
                    (row.name, differs, root, subdir)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut report = String::new();
    for (name, differs, root, subdir) in &results {
        let listed = DIVERGES_TODAY.contains(name);
        if *differs != listed {
            report.push_str(&format!(
                "row `{name}`: {}\n  from the root:   {root:?}\n  from {SUBDIR}: {subdir:?}\n",
                if *differs {
                    "differs but is not in DIVERGES_TODAY"
                } else {
                    "is in DIVERGES_TODAY but no longer differs: delete it"
                }
            ));
        }
    }
    assert!(report.is_empty(), "{report}");
}

#[specforge_test(
    behavior = "locate_project_root",
    verify = "every command run from a subdirectory of a project acts on that project"
)]
fn every_command_from_a_subdirectory_acts_on_its_project() {
    assert_rows(&[Kind::Read, Kind::Writes]);
}

#[specforge_test(
    behavior = "locate_project_root",
    verify = "an extension command run from a subdirectory is routed over the project it is in"
)]
fn an_extension_command_from_a_subdirectory_is_routed_over_its_project() {
    assert_rows(&[Kind::Extension]);
}

#[specforge_test(
    behavior = "locate_project_root",
    verify = "watch run from a subdirectory watches the project it is in"
)]
fn watch_from_a_subdirectory_watches_its_project() {
    assert_rows(&[Kind::Watch]);
}

#[test]
fn a_missing_path_is_read_as_an_empty_project_today() {
    let dir = TempDir::new().unwrap();
    let check = specforge()
        .args(["check", "nope"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&check.stderr);
    assert_eq!(check.status.code(), Some(0), "{stderr}");
    assert!(stderr.contains("info[I002]"), "{stderr}");

    let stats = specforge()
        .args(["stats", "nope", "--format", "json"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_eq!(stats.status.code(), Some(0), "{stats:?}");
}

#[test]
fn a_file_path_reads_specforge_json_under_the_file_today() {
    let p = project();
    let check = specforge()
        .arg("check")
        .arg(p.path().join("spec/a.spec"))
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&check.stderr);
    assert!(stderr.contains("E069"), "{stderr}");
    assert!(stderr.contains("a.spec/specforge.json"), "{stderr}");
}
