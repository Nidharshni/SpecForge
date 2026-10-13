use crate::{registry, report};
use std::path::{Path, PathBuf};
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

    let Some(dir) = report_dir() else {
        eprintln!(
            "[specforge-test] no report written: this test binary is not inside a cargo target \
             directory; set SPECFORGE_REPORT to the directory to write it to"
        );
        return;
    };
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

/// Files cargo writes only at a target directory's root: the cache-directory
/// tag and the rustc-info cache. Two, because a target directory created
/// before cargo first ran in it (`mkdir -p target/specforge`) gets
/// `.rustc_info.json` but may lack the tag.
const TARGET_MARKERS: [&str; 2] = ["CACHEDIR.TAG", ".rustc_info.json"];

/// The cargo target directory `binary` was built into: its nearest ancestor
/// holding a target marker. Test binaries live at
/// `<target>/[<triple>/]<profile>/deps/`, and cargo exposes the target
/// directory to them at run time only through that path. None for a binary
/// outside any target directory.
pub fn target_dir_of(binary: &Path) -> Option<PathBuf> {
    binary
        .ancestors()
        .skip(1)
        .find(|dir| TARGET_MARKERS.iter().any(|m| dir.join(m).is_file()))
        .map(Path::to_path_buf)
}

/// Where this process's report goes: `$SPECFORGE_REPORT` when `specforge
/// collect` set it, else `specforge/` in the binary's target directory. None
/// when there is neither; the report is then not written, and never into the
/// working directory.
fn report_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("SPECFORGE_REPORT") {
        return Some(PathBuf::from(dir));
    }
    let exe = std::env::current_exe().ok()?;
    target_dir_of(&exe).map(|target| target.join("specforge"))
}
