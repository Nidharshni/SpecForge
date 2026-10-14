use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{OnceLock, mpsc};
use std::time::{Duration, Instant};

use crate::change_set::ChangeSet;
use crate::checks::{CHECKS, Check, Selection};
use crate::judge;
use crate::repo::Repo;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The staged files; checks reaching `Hook` (none compiles the workspace).
    Hook,
    /// The working tree against HEAD; checks reaching `Hook` or `Quick`, by trigger.
    Quick,
    /// Every check reaching `Hook`, `Quick` or `Full`, triggers ignored.
    Full,
    /// `Full`, with nextest's `ci` profile, GitHub log groups and error annotations, and the pinned tools installed
    /// when missing.
    Ci,
    /// Checks reaching `Bootstrap`: a cold clone rebuilds every blob.
    Bootstrap,
}

impl Mode {
    pub fn parse(name: &str) -> Option<Mode> {
        Some(match name {
            "hook" => Mode::Hook,
            "quick" => Mode::Quick,
            "full" => Mode::Full,
            "ci" => Mode::Ci,
            "bootstrap" => Mode::Bootstrap,
            _ => return None,
        })
    }

    /// The nextest profile the tests run under.
    pub fn nextest_profile(self) -> &'static str {
        if self == Mode::Ci { "ci" } else { "gate" }
    }
}

/// A tool a check needs, pinned by the list: in `ci` a missing one is installed, elsewhere the check fails naming the
/// command.
pub struct Tool {
    pub bin: &'static str,
    pub version: Option<&'static str>,
    pub install: &'static str,
}

pub const TREE_SITTER: Tool = Tool {
    bin: "tree-sitter",
    version: Some("0.27.0"),
    install: "cargo install tree-sitter-cli --locked --version 0.27.0",
};
pub const CARGO_AUDIT: Tool = Tool {
    bin: "cargo-audit",
    version: None,
    install: "cargo install cargo-audit --locked",
};

/// What a check's `run` gets: the repo, the mode, the change set, the check's log file and the binaries the run
/// builds once.
pub struct Ctx<'a> {
    pub repo: &'a Repo,
    pub mode: Mode,
    pub changes: &'a ChangeSet,
    log: File,
    specforge: &'a OnceLock<Result<PathBuf, String>>,
}

impl Ctx<'_> {
    /// A command that runs in the repository root.
    pub fn cmd(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut cmd = Command::new(program);
        cmd.current_dir(&self.repo.root);
        cmd
    }

    /// Append a line to this check's log.
    pub fn log(&self, text: &str) {
        let _ = writeln!(&self.log, "{text}");
    }

    /// `cmd` with stdout and stderr appended to this check's log, stdin closed; Err names the command and its status.
    pub fn sh(&self, cmd: &mut Command) -> Result<(), String> {
        let shown = describe(cmd);
        self.log(&format!("$ {shown}"));
        let (out, err) = (self.log.try_clone(), self.log.try_clone());
        let (Ok(out), Ok(err)) = (out, err) else {
            return Err("cannot open the log".into());
        };
        let status = cmd
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(err)
            .status()
            .map_err(|e| format!("`{shown}`: {e}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("`{shown}` {status}"))
        }
    }

    /// Like [`Ctx::sh`], but stdout comes back (stderr goes to the log): whether the command succeeded, and what it
    /// printed.
    pub fn capture(&self, cmd: &mut Command) -> Result<(bool, String), String> {
        let shown = describe(cmd);
        self.log(&format!("$ {shown}"));
        let err = self.log.try_clone().map_err(|e| e.to_string())?;
        let output = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(err)
            .output()
            .map_err(|e| format!("`{shown}`: {e}"))?;
        Ok((
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
        ))
    }

    /// The `specforge` binary cargo built, wherever the target directory is: built once per run, from
    /// `cargo build`'s own artifact message (`judge::executable_of`).
    pub fn specforge(&self) -> Result<&Path, String> {
        self.specforge
            .get_or_init(|| self.build_specforge())
            .as_ref()
            .map(PathBuf::as_path)
            .map_err(Clone::clone)
    }

    fn build_specforge(&self) -> Result<PathBuf, String> {
        let mut cmd = self.cmd("cargo");
        cmd.args(["build", "-q"]);
        match self.mode {
            // The workspace's own feature set: after the workspace build this finds everything fresh.
            Mode::Full | Mode::Ci => cmd.args(["--workspace", "--bin", "specforge"]),
            _ => cmd.args(["-p", "specforge-cli"]),
        };
        cmd.args(["--message-format", "json-render-diagnostics"]);
        let (built, messages) = self.capture(&mut cmd)?;
        if !built {
            return Err("building the specforge binary failed".into());
        }
        judge::executable_of(&messages, "specforge")
            .ok_or_else(|| "cargo reported no `specforge` executable".to_string())
    }

    /// `<target_dir>/specforge`: where every test of the run reports (`SPECFORGE_REPORT`) and the dogfood reads.
    pub fn reports(&self) -> PathBuf {
        self.repo.target_dir.join("specforge")
    }

    /// `<target_dir>/ext`: one target directory for every guest.
    pub fn ext_target(&self) -> PathBuf {
        self.repo.target_dir.join("ext")
    }

    /// The tool is installed (at its pinned version), else (in `ci`) installed now, else Err naming the command.
    pub fn tool(&self, tool: &Tool) -> Result<(), String> {
        let present = || {
            Command::new(tool.bin)
                .arg("--version")
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
                .is_some_and(|text| tool.version.is_none_or(|v| text.contains(v)))
        };
        if present() {
            return Ok(());
        }
        if self.mode != Mode::Ci {
            return Err(format!("{} is not installed: {}", tool.bin, tool.install));
        }
        let mut words = tool.install.split_whitespace();
        let mut cmd = self.cmd(words.next().unwrap_or("true"));
        cmd.args(words);
        self.sh(&mut cmd)?;
        if present() {
            Ok(())
        } else {
            Err(format!(
                "{} is still missing after: {}",
                tool.bin, tool.install
            ))
        }
    }
}

fn describe(cmd: &Command) -> String {
    std::iter::once(cmd.get_program())
        .chain(cmd.get_args())
        .map(|word| word.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ")
}

pub enum Outcome {
    Passed,
    Failed(String),
    Skipped,
    /// A check it runs after did not pass (its name).
    Blocked(&'static str),
}

pub struct Report {
    pub outcomes: Vec<(&'static str, Outcome, Duration)>,
    pub log_dir: PathBuf,
}

impl Report {
    pub fn passed(&self) -> bool {
        self.outcomes
            .iter()
            .all(|(_, outcome, _)| matches!(outcome, Outcome::Passed | Outcome::Skipped))
    }

    fn log_of(&self, name: &str) -> String {
        std::fs::read_to_string(self.log_dir.join(format!("{name}.log"))).unwrap_or_default()
    }

    /// The STEP / TIME / RESULT table; in `Ci` each check's log in a `::group::` and a failure as
    /// `::error title=<name>::<fix>`; in `Hook` each failure as `pre-commit: <name>: <proves>` with the log's first
    /// 20 lines and `fix: <fix>`, and nothing when all passed.
    pub fn print(&self, mode: Mode, out: &mut dyn Write) -> io::Result<()> {
        let fix_of = |name: &str| CHECKS.iter().find(|c| c.name == name).map_or("", |c| c.fix);
        let proves_of = |name: &str| {
            CHECKS
                .iter()
                .find(|c| c.name == name)
                .map_or("", |c| c.proves)
        };
        if mode == Mode::Hook {
            for (name, outcome, _) in &self.outcomes {
                let Outcome::Failed(why) = outcome else {
                    continue;
                };
                writeln!(out, "pre-commit: {name}: {}", proves_of(name))?;
                writeln!(out, "    {why}")?;
                for line in self.log_of(name).lines().take(20) {
                    writeln!(out, "    {line}")?;
                }
                writeln!(out, "    fix: {}", fix_of(name))?;
            }
            return Ok(());
        }
        if mode == Mode::Ci {
            for (name, outcome, took) in &self.outcomes {
                if matches!(outcome, Outcome::Skipped) {
                    continue;
                }
                let result = if matches!(outcome, Outcome::Passed) {
                    "ok"
                } else {
                    "FAILED"
                };
                writeln!(out, "::group::{name} ({}s) {result}", took.as_secs())?;
                write!(out, "{}", self.log_of(name))?;
                writeln!(out, "::endgroup::")?;
                if !matches!(outcome, Outcome::Passed) {
                    writeln!(out, "::error title={name}::{}", fix_of(name))?;
                }
            }
        }
        writeln!(out, "\n{:<18} {:>6}  RESULT", "STEP", "TIME")?;
        let mut total = Duration::ZERO;
        for (name, outcome, took) in &self.outcomes {
            total += *took;
            let secs = took.as_secs();
            match outcome {
                Outcome::Skipped => {}
                Outcome::Passed => writeln!(out, "{name:<18} {secs:>5}s  ok")?,
                Outcome::Failed(why) => {
                    writeln!(
                        out,
                        "{name:<18} {secs:>5}s  FAILED ({}; see {}/{name}.log)",
                        brief(why),
                        self.log_dir.display()
                    )?;
                    writeln!(out, "{:<18}         fix: {}", "", fix_of(name))?;
                }
                Outcome::Blocked(by) => writeln!(out, "{name:<18} {secs:>5}s  blocked (by {by})")?,
            }
        }
        writeln!(
            out,
            "{:<18} {:>5}s  (sum of the checks; they overlap)",
            "total",
            total.as_secs()
        )
    }
}

/// The first of a check's failures, and how many more there are.
fn brief(why: &str) -> String {
    let mut failures = why.split("; ");
    let first = failures.next().unwrap_or_default();
    match failures.count() {
        0 => first.to_string(),
        more => format!("{first}; and {more} more"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum State {
    Pending,
    Running,
    Passed,
    Failed,
    Blocked,
}

/// One selected check as the scheduler sees it: `after` indexes the other selected checks.
pub(crate) struct Step {
    pub compiles: bool,
    pub after: Vec<usize>,
}

pub(crate) struct Wave {
    pub start: Vec<usize>,
    pub blocked: Vec<usize>,
}

/// What to do now (the ordering rule of a run, pure so tests drive it): a pending check whose `after` all passed
/// starts, a compiling one only while none compiles; one with an `after` that failed or was blocked is blocked.
pub(crate) fn schedule(steps: &[Step], states: &[State]) -> Wave {
    let mut compiling = steps
        .iter()
        .zip(states)
        .any(|(s, st)| s.compiles && *st == State::Running);
    let (mut start, mut blocked) = (Vec::new(), Vec::new());
    for (i, step) in steps.iter().enumerate() {
        if states[i] != State::Pending {
            continue;
        }
        if step
            .after
            .iter()
            .any(|&a| matches!(states[a], State::Failed | State::Blocked))
        {
            blocked.push(i);
        } else if step.after.iter().all(|&a| states[a] == State::Passed) {
            if step.compiles {
                if compiling {
                    continue;
                }
                compiling = true;
            }
            start.push(i);
        }
    }
    Wave { start, blocked }
}

/// Run the checks `mode` selects (only `only` when given), at most one compiling check at a time, each after the
/// checks it names. Logs go to `<target_dir>/gate/`.
pub fn run(mode: Mode, repo: &Repo, only: Option<&str>) -> Result<Report, String> {
    if let Some(name) = only
        && !CHECKS.iter().any(|c| c.name == name)
    {
        return Err(format!("no check named `{name}` (cargo gate list)"));
    }
    let changes = match mode {
        Mode::Hook => ChangeSet::staged(repo)?,
        Mode::Quick => ChangeSet::working_tree(repo)?,
        _ => ChangeSet::default(),
    };
    let mut selections = Vec::new();
    for check in CHECKS {
        selections.push(if only.is_some_and(|name| name != check.name) {
            Selection::Skip
        } else {
            check.selection(mode, repo, &changes)?
        });
    }
    let log_dir = repo.target_dir.join("gate");
    std::fs::create_dir_all(&log_dir).map_err(|e| format!("{}: {e}", log_dir.display()))?;
    // A full run's dogfood judges what this run's tests reported, not a cached run's.
    let reports = repo.target_dir.join("specforge");
    if matches!(mode, Mode::Full | Mode::Ci) && only.is_none() {
        let _ = std::fs::remove_dir_all(&reports);
    }
    std::fs::create_dir_all(&reports).map_err(|e| format!("{}: {e}", reports.display()))?;

    let selected: Vec<usize> = (0..CHECKS.len())
        .filter(|&i| selections[i] != Selection::Skip)
        .collect();
    let steps: Vec<Step> = selected
        .iter()
        .map(|&i| Step {
            compiles: CHECKS[i].compiles,
            after: CHECKS[i]
                .after
                .iter()
                .filter_map(|name| selected.iter().position(|&j| CHECKS[j].name == *name))
                .collect(),
        })
        .collect();

    let specforge = OnceLock::new();
    let mut states = vec![State::Pending; steps.len()];
    let mut results: Vec<Option<(Outcome, Duration)>> = (0..steps.len()).map(|_| None).collect();
    std::thread::scope(|scope| {
        let (done, finished) = mpsc::channel::<(usize, Result<(), String>, Duration)>();
        loop {
            let wave = schedule(&steps, &states);
            for &k in &wave.blocked {
                states[k] = State::Blocked;
                let by = steps[k]
                    .after
                    .iter()
                    .find(|&&a| matches!(states[a], State::Failed | State::Blocked))
                    .map_or("", |&a| CHECKS[selected[a]].name);
                results[k] = Some((Outcome::Blocked(by), Duration::ZERO));
            }
            for &k in &wave.start {
                states[k] = State::Running;
                let (check, selection) = (&CHECKS[selected[k]], &selections[selected[k]]);
                let (done, specforge, changes, log_dir) =
                    (done.clone(), &specforge, &changes, &log_dir);
                scope.spawn(move || {
                    let began = Instant::now();
                    let result = run_one(check, selection, mode, repo, changes, specforge, log_dir);
                    let _ = done.send((k, result, began.elapsed()));
                });
            }
            let running = states.contains(&State::Running);
            if !running && wave.start.is_empty() && wave.blocked.is_empty() {
                break;
            }
            if !running {
                continue;
            }
            let Ok((k, result, took)) = finished.recv() else {
                break;
            };
            states[k] = if result.is_ok() {
                State::Passed
            } else {
                State::Failed
            };
            results[k] = Some((
                result.map_or_else(Outcome::Failed, |()| Outcome::Passed),
                took,
            ));
        }
    });

    let mut outcomes = Vec::new();
    for (i, check) in CHECKS.iter().enumerate() {
        let outcome = match selected.iter().position(|&j| j == i) {
            Some(k) => results[k]
                .take()
                .unwrap_or((Outcome::Blocked("scheduler"), Duration::ZERO)),
            None => (Outcome::Skipped, Duration::ZERO),
        };
        outcomes.push((check.name, outcome.0, outcome.1));
    }
    Ok(Report { outcomes, log_dir })
}

/// `cargo gate baseline [--only <check>]`: lower `gate/baseline.json`'s sections (`only`: that check's alone) to what
/// the checks find today; never raises an entry. The `specs` section builds the CLI; the `file-size` section reads the
/// tracked files and git's renames since `since` (`HEAD`). Returns the lines to print.
pub fn baseline(
    repo: &Repo,
    only: Option<&str>,
    since: Option<&str>,
) -> Result<Vec<String>, String> {
    if let Some(name) = only
        && !crate::baseline::SECTIONS.contains(&name)
    {
        return Err(format!(
            "`{name}` keeps no baseline ({})",
            crate::baseline::SECTIONS.join(", ")
        ));
    }
    let log_dir = repo.target_dir.join("gate");
    std::fs::create_dir_all(&log_dir).map_err(|e| format!("{}: {e}", log_dir.display()))?;
    let log = File::create(log_dir.join("baseline.log"))
        .map_err(|e| format!("cannot create the log: {e}"))?;
    let (specforge, changes) = (OnceLock::new(), ChangeSet::default());
    let ctx = Ctx {
        repo,
        mode: Mode::Quick,
        changes: &changes,
        log,
        specforge: &specforge,
    };
    crate::steps::write_baseline(&ctx, only, since)
}

fn run_one(
    check: &Check,
    selection: &Selection,
    mode: Mode,
    repo: &Repo,
    changes: &ChangeSet,
    specforge: &OnceLock<Result<PathBuf, String>>,
    log_dir: &Path,
) -> Result<(), String> {
    let log = File::create(log_dir.join(format!("{}.log", check.name)))
        .map_err(|e| format!("cannot create the log: {e}"))?;
    let ctx = Ctx {
        repo,
        mode,
        changes,
        log,
        specforge,
    };
    ctx.log(&format!("# {}: {:?}", check.name, selection));
    let result = (check.run)(&ctx, selection);
    if let Err(why) = &result {
        ctx.log(&format!("FAILED: {why}"));
    }
    result
}

#[cfg(test)]
mod tests;
