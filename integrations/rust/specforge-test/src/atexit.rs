use crate::ports::RealFs;
use crate::ports::ReportWriter;
use crate::{registry, report};
use std::path::PathBuf;
use std::sync::Once;

static INIT: Once = Once::new();

/// Ensures the atexit handler is registered exactly once.
/// Called by TestGuard::new on first construction.
pub fn ensure_registered() {
    INIT.call_once(|| {
        // Safety: atexit is POSIX-standard. The function pointer is valid
        // for the lifetime of the process.
        unsafe {
            libc::atexit(on_exit);
        }
    });
}

/// Append one JSONL record the moment a test finishes (C11-06): results
/// survive panic=abort, SIGKILL from a CI timeout, nextest kills, and
/// segfaults — none of which run atexit. The atexit handler only finalizes.
pub fn append_jsonl(entry: &registry::TestRecordEntry) {
    let binary_name = binary_name();
    let dir = report_dir();
    let fs = RealFs;
    if fs.create_dir_all(&dir).is_err() {
        return;
    }
    if let Ok(line) = serde_json::to_string(entry) {
        let path = dir.join(format!("{binary_name}.jsonl"));
        let _ = fs.append_line(&path, &line);
    }
}

fn binary_name() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "unknown".to_string())
}

/// The final report's name: stable across rebuilds, so each run of a test
/// target replaces its previous report instead of leaving one per build
/// hash for `specforge collect --no-run` to count again. Cargo sets
/// `CARGO_PKG_NAME` when it runs tests; the target name is the binary's
/// stem without its build hash (`tests-87edb3de886b35bf` → `tests`).
fn report_base() -> String {
    let binary = binary_name();
    match std::env::var("CARGO_PKG_NAME") {
        Ok(package) => format!("{package}--{}", strip_build_hash(&binary)),
        Err(_) => binary,
    }
}

/// The nextest run this process belongs to: nextest runs each test in its
/// own process and sets `NEXTEST_RUN_ID` and `NEXTEST_TEST_NAME`.
fn nextest_run() -> Option<(String, String)> {
    let run = std::env::var("NEXTEST_RUN_ID").ok()?;
    let test = std::env::var("NEXTEST_TEST_NAME").ok()?;
    Some((run, test))
}

/// The report's file stem. Under `cargo test` one process runs a whole
/// test target: `<base>`. Under nextest each test is its own process, so
/// each writes its own report, `<base>--<run>--<test>`: one shared name
/// would keep only the last test of every target. `run` is the first 8
/// characters of the run ID, so reports of other runs can be pruned by name.
pub fn report_name(base: &str, nextest: Option<(&str, &str)>) -> String {
    match nextest {
        None => base.to_string(),
        Some((run, test)) => {
            let run: String = run.chars().take(8).collect();
            let test: String = test
                .replace("::", ".")
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '.' || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            format!("{base}--{run}--{test}")
        }
    }
}

/// Remove this target's reports that the new one supersedes, so
/// `specforge collect --no-run` never reads a stale or duplicate result:
/// a `cargo test` report removes the target's nextest reports, and a
/// nextest report removes the `cargo test` report and other runs' reports.
pub fn prune_superseded(dir: &std::path::Path, base: &str, nextest_run: Option<&str>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let per_test = format!("{base}--");
    let this_run = nextest_run.map(|run| {
        let run: String = run.chars().take(8).collect();
        format!("{run}--")
    });
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name();
        let Some(stem) = name.to_str().and_then(|n| n.strip_suffix(".json")) else {
            continue;
        };
        let superseded = match &this_run {
            None => stem.starts_with(&per_test),
            Some(this_run) => {
                stem == base
                    || stem
                        .strip_prefix(&per_test)
                        .is_some_and(|rest| !rest.starts_with(this_run.as_str()))
            }
        };
        if superseded {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// `name-<16 hex digits>` → `name`; anything else unchanged.
pub fn strip_build_hash(binary: &str) -> &str {
    match binary.rsplit_once('-') {
        Some((target, hash)) if hash.len() == 16 && hash.chars().all(|c| c.is_ascii_hexdigit()) => {
            target
        }
        _ => binary,
    }
}

/// Write this process's binary report: the one thing the exit handler does.
/// It reads nothing and prints only a failed write. Which obligations the
/// tests prove is scored by `specforge collect` and the coverage rule
/// (ADR 0068).
extern "C" fn on_exit() {
    let entries = registry::drain();
    if entries.is_empty() {
        return;
    }

    let dir = report_dir();

    let base = report_base();
    let nextest = nextest_run();
    prune_superseded(&dir, &base, nextest.as_ref().map(|(run, _)| run.as_str()));
    let name = report_name(
        &base,
        nextest
            .as_ref()
            .map(|(run, test)| (run.as_str(), test.as_str())),
    );
    if let Err(e) = report::write_report(&dir, &name, &entries) {
        eprintln!("[specforge-test] failed to write report: {e}");
    }
}

fn report_dir() -> PathBuf {
    // `specforge collect` names the report directory when it runs the tests
    // (`@specforge/cargo-test`), wherever the target directory lives.
    if let Some(dir) = std::env::var_os("SPECFORGE_REPORT") {
        return PathBuf::from(dir);
    }
    // Walk up from the current exe to find target/, then use target/specforge/
    std::env::current_exe()
        .ok()
        .and_then(|exe| {
            let mut dir = exe.parent()?;
            // exe is in target/debug/deps/ — walk up to target/
            while dir.file_name().is_some() {
                if dir.file_name().is_some_and(|n| n == "target") {
                    return Some(dir.join("specforge"));
                }
                dir = dir.parent()?;
            }
            None
        })
        .unwrap_or_else(|| PathBuf::from("target/specforge"))
}
