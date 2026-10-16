//! The collector machine on the user's machine (ADR 0125): a child process, and the user-level consent
//! store. It is the only place collect reaches the process, the wall clock and the environment.

use crate::machine::{
    CollectorCommand, CollectorMachine, CommandApproval, REPORT_ENV, Ran, RunFailure, RunnerOutput,
};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use std::time::SystemTime;

/// Overrides the user-level consent store (tests, custom homes).
const CONSENT_FILE_ENV: &str = "SPECFORGE_CONSENT_FILE";

/// A child process for the command and a file for the approvals. The store lives outside the project so a
/// cloned repository can't pre-approve its own commands.
#[derive(Debug, Clone)]
pub struct ProcessMachine {
    store: PathBuf,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ConsentStore {
    #[serde(default)]
    approved: Vec<CommandApproval>,
}

impl ProcessMachine {
    /// The user's machine: the consent store at `$SPECFORGE_CONSENT_FILE`, else
    /// `$HOME/.specforge/collector-consent.json` (`USERPROFILE`, then the temp dir). Read once, here.
    pub fn user() -> Self {
        if let Some(path) = std::env::var_os(CONSENT_FILE_ENV) {
            return Self::with_store(PathBuf::from(path));
        }
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        Self::with_store(home.join(".specforge").join("collector-consent.json"))
    }

    /// The same machine with its consent store at `store`.
    pub fn with_store(store: PathBuf) -> Self {
        ProcessMachine { store }
    }

    fn load(&self) -> ConsentStore {
        std::fs::read_to_string(&self.store)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }
}

impl CollectorMachine for ProcessMachine {
    fn run(
        &self,
        command: &CollectorCommand<'_>,
        capture: Option<&mut dyn Write>,
    ) -> Result<Ran, RunFailure> {
        let Some((program, args)) = command.argv.split_first() else {
            return Err(RunFailure::NotRun(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "no command",
            )));
        };
        let mut cmd = std::process::Command::new(program);
        cmd.args(args)
            .current_dir(command.dir)
            .env(REPORT_ENV, command.report)
            .stdin(std::process::Stdio::null());
        match command.output {
            RunnerOutput::Inherit => {}
            RunnerOutput::Stderr => {
                cmd.stdout(std::io::stderr());
            }
            RunnerOutput::Discard => {
                cmd.stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null());
            }
        }
        // File times come from the kernel's coarse clock, which can lag `now` by a few ticks: a report
        // written at once would look older than the start. A grace of 100 ms keeps it.
        let started = SystemTime::now()
            .checked_sub(std::time::Duration::from_millis(100))
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let Some(capture) = capture else {
            let status = cmd.status().map_err(RunFailure::NotRun)?;
            return Ok(Ran {
                exit_code: status.code(),
                started,
            });
        };
        let mut child = cmd
            .stdout(std::process::Stdio::piped())
            .spawn()
            .map_err(RunFailure::NotRun)?;
        let pipe = child.stdout.take().expect("stdout is piped");
        let tee = tee(pipe, capture, command.output);
        let status = child.wait().map_err(RunFailure::NotRun)?;
        tee.map_err(RunFailure::Capture)?;
        Ok(Ran {
            exit_code: status.code(),
            started,
        })
    }

    fn approved(&self, approval: &CommandApproval) -> bool {
        self.load().approved.contains(approval)
    }

    fn approve(&self, approval: &CommandApproval) -> Result<(), String> {
        let mut consents = self.load();
        consents.approved.retain(|c| !approval.replaces(c));
        consents.approved.push(approval.clone());
        if let Some(dir) = self.store.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let json =
            serde_json::to_string_pretty(&consents).expect("consent serialization cannot fail");
        std::fs::write(&self.store, json).map_err(|e| format!("{}: {e}", self.store.display()))
    }
}

/// Copy the child's standard output to `capture` and to where `output` sends the runner's output, as it
/// arrives.
fn tee(
    mut pipe: impl std::io::Read,
    capture: &mut dyn Write,
    output: RunnerOutput,
) -> std::io::Result<()> {
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
        capture.write_all(&buf[..n])?;
        // The terminal going away mustn't lose the capture.
        let _ = shown.write_all(&buf[..n]).and_then(|()| shown.flush());
    }
}

#[cfg(test)]
mod tests;
