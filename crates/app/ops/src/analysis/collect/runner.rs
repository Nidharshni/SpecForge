//! Running a collector's command, its output captured or passed through.

use super::collector::{Collector, command_line};
use crate::Held;
use std::path::Path;
use std::time::SystemTime;

/// Environment variable the host sets to the absolute report path when it
/// runs a collector, so a runner integration can write there directly.
pub(super) const REPORT_ENV: &str = "SPECFORGE_REPORT";

// ── running ─────────────────────────────────────────────────────────────────

/// Where the runner's own output goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerOutput {
    /// The user's terminal.
    Inherit,
    /// Stderr, keeping stdout clean for machine-readable output.
    Stderr,
    /// Discarded (the MCP server owns stdio).
    Discard,
}

/// What running a collector's command produced.
#[derive(Debug, Clone)]
pub(super) struct Ran {
    /// The runner's exit code; non-zero is normal when tests fail.
    pub exit_code: Option<i32>,
    /// When the command started: a report directory is only read for
    /// files written since, so an old report can never pass for a new run.
    pub started: SystemTime,
    /// The file holding the command's standard output, when the collector
    /// captures it.
    pub stdout: Option<Held>,
}

/// Run the collector's declared command in the project root. A report
/// *file* left by an earlier run is removed first; a report *directory* is
/// left alone (other tools may keep files there) and filtered by
/// modification time when it's read. A captured standard output still
/// reaches the chosen output as it's produced, and is also written to
/// `capture`. Every file it removes, creates or writes is a held one.
pub(super) fn run(
    collector: &Collector,
    root: &Path,
    report: &Held,
    capture: Option<&Held>,
    output: RunnerOutput,
) -> Result<Ran, String> {
    let argv = command_line(collector, &report.spelled);
    let Some((program, args)) = argv.split_first() else {
        return Err(format!(
            "collector '{}' of {} declares no command; use --no-run with its report",
            collector.name, collector.extension
        ));
    };
    if report.real.is_file() {
        report
            .remove()
            .map_err(|e| format!("{}: {e}", report.spelled.display()))?;
    }
    for file in std::iter::once(report).chain(capture) {
        file.create_parent()
            .map_err(|e| format!("{}: {e}", file.spelled.display()))?;
    }
    let mut cmd = std::process::Command::new(program);
    cmd.args(args)
        .current_dir(root)
        .env(REPORT_ENV, &report.spelled)
        .stdin(std::process::Stdio::null());
    match output {
        RunnerOutput::Inherit => {}
        RunnerOutput::Stderr => {
            cmd.stdout(std::io::stderr());
        }
        RunnerOutput::Discard => {
            cmd.stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
        }
    }
    // File times come from the kernel's coarse clock, which can lag
    // `now` by a few ticks: a report written at once would look older
    // than the start. A grace of 100 ms keeps it.
    let started = SystemTime::now()
        .checked_sub(std::time::Duration::from_millis(100))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let failed = |e: std::io::Error| format!("failed to run `{}`: {e}", argv.join(" "));
    let Some(capture) = capture else {
        let status = cmd.status().map_err(failed)?;
        return Ok(Ran {
            exit_code: status.code(),
            started,
            stdout: None,
        });
    };
    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(failed)?;
    let pipe = child.stdout.take().expect("stdout is piped");
    let tee = tee(pipe, capture, output);
    let status = child.wait().map_err(failed)?;
    tee.map_err(|e| format!("{}: {e}", capture.spelled.display()))?;
    Ok(Ran {
        exit_code: status.code(),
        started,
        stdout: Some(capture.clone()),
    })
}

/// Copy the child's standard output to `file` and to where `output` sends
/// the runner's output, as it arrives.
fn tee(mut pipe: impl std::io::Read, file: &Held, output: RunnerOutput) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = file.create()?;
    let mut shown: Box<dyn Write> = match output {
        RunnerOutput::Inherit => Box::new(std::io::stdout()),
        RunnerOutput::Stderr => Box::new(std::io::stderr()),
        RunnerOutput::Discard => Box::new(std::io::sink()),
    };
    let mut buf = [0u8; 8192];
    loop {
        let n = match pipe.read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        file.write_all(&buf[..n])?;
        // The terminal going away mustn't lose the capture.
        let _ = shown.write_all(&buf[..n]).and_then(|()| shown.flush());
    }
}
