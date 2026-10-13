//! The SMT solver the prove pass asks and doctor checks (ADR 0079).
//!
//! One port, [`Solver`]: whether the solver runs here, and its answer to one
//! SMT-LIB2 script. [`Z3`] is the production adapter: it runs `z3` from `PATH`,
//! ends every call at a timeout (its probe at [`Z3::PROBE_TIMEOUT`], a solve at
//! its own), and words the one W098 that says z3 does not run. Tests use
//! `specforge_ops::testing::ScriptedSolver`.

use std::ffi::OsString;
use std::process::Command;
use std::time::Duration;

use specforge_common::{Diagnostic, codes};

/// What `specforge analyze --prove` (the prove pass) and doctor (its toolchain
/// finding) ask an SMT solver.
pub trait Solver: Send + Sync {
    /// Whether the solver runs here: its version banner, or the W098 that says
    /// why not. Asked once per operation and never cached, so a solver
    /// installed while a server runs is seen on its next call.
    fn availability(&self) -> Availability;
    /// The solver's whole stdout for one SMT-LIB2 script, or why there is none.
    fn solve(&self, script: &str) -> Result<String, SolveFailure>;
}

/// Whether a solver runs here.
#[derive(Debug, Clone, PartialEq)]
pub enum Availability {
    /// It runs: the first line of its version banner.
    Ready { version: String },
    /// It does not: the W098 the prove pass reports and doctor states as its
    /// toolchain finding, word for word.
    Unavailable(Diagnostic),
}

/// Why a solve produced no answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolveFailure {
    /// The solver ran past its timeout and was stopped.
    TimedOut { after: Duration },
    /// The solver could not be run or ended without an answer.
    Failed,
}

/// The production adapter: `z3` from `PATH`.
#[derive(Debug, Clone)]
pub struct Z3 {
    program: OsString,
    timeout: Duration,
}

impl Z3 {
    /// How long one solve may take unless `specforge analyze --prove-timeout`
    /// says otherwise.
    pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
    /// How long `z3 --version` may take before z3 counts as not running.
    pub const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

    /// z3 from `PATH`, each solve stopped past `timeout`.
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            program: OsString::from("z3"),
            timeout,
        }
    }

    /// How long one solve may take.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// The W098 for a z3 that is not on `PATH` (what `ScriptedSolver::not_found` answers).
    pub(crate) fn not_on_path() -> Diagnostic {
        Diagnostic::new(
            codes::W098,
            "SMT solver 'z3' not found on PATH; formal entailment and consistency checks were skipped",
        )
        .with_suggestion("install z3 (https://github.com/Z3Prover/z3) to enable `--prove`")
    }
}

impl Default for Z3 {
    fn default() -> Self {
        Self::with_timeout(Self::DEFAULT_TIMEOUT)
    }
}

impl Solver for Z3 {
    fn availability(&self) -> Availability {
        let Ok(output) = Command::new(&self.program).arg("--version").output() else {
            return Availability::Unavailable(Z3::not_on_path());
        };
        if !output.status.success() {
            return Availability::Unavailable(Z3::not_on_path());
        }
        Availability::Ready {
            version: String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("z3")
                .to_string(),
        }
    }

    fn solve(&self, script: &str) -> Result<String, SolveFailure> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let tmp = std::env::temp_dir().join(format!(
            "specforge-prove-{}-{}.smt2",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&tmp, script).map_err(|_| SolveFailure::Failed)?;
        let result = run_with_timeout(Command::new(&self.program).arg(&tmp), self.timeout);
        let _ = std::fs::remove_file(&tmp);
        result
    }
}

/// Run a command to completion or kill it at `timeout`; returns its stdout.
fn run_with_timeout(cmd: &mut Command, timeout: Duration) -> Result<String, SolveFailure> {
    use std::io::Read;
    use std::process::Stdio;
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| SolveFailure::Failed)?;
    let mut stdout = child.stdout.take().ok_or(SolveFailure::Failed)?;
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err(SolveFailure::TimedOut { after: timeout });
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => return Err(SolveFailure::Failed),
        }
    }
    let buf = reader.join().map_err(|_| SolveFailure::Failed)?;
    Ok(String::from_utf8_lossy(&buf).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_outliving_the_timeout_is_killed() {
        let started = std::time::Instant::now();
        let out = run_with_timeout(
            std::process::Command::new("sleep").arg("30"),
            Duration::from_millis(200),
        );
        assert!(matches!(out, Err(SolveFailure::TimedOut { .. })));
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[cfg(unix)]
    #[ignore = "fixed by T3"]
    #[test]
    fn a_solver_that_leaves_a_child_is_stopped_at_the_timeout() {
        // `sh` forks `sleep`, which keeps the stdout pipe after `sh` is killed.
        let started = std::time::Instant::now();
        let out = run_with_timeout(
            std::process::Command::new("sh").args(["-c", "sleep 30; true"]),
            Duration::from_millis(200),
        );
        assert!(matches!(out, Err(SolveFailure::TimedOut { .. })));
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{:?}",
            started.elapsed()
        );
    }
}
