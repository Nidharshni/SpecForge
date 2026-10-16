//! What tests serve in process as candidates: `@sdk/greet` (the fixture's
//! own declarations, `fixtures/greet-extension/src/contributions.rs`) under
//! bytes a test chooses, and builtin look-alikes under a builtin's real
//! embedded bytes (`InProcessRuntime::binary` serves whatever name the
//! candidate is loaded under).
//!
//! It also holds `ScriptedSolver`, the SMT solver tests script (ADR 0079), and `ScriptedMachine`, the
//! machine collect runs on (ADR 0125).
//!
//! Behind the `testing` feature; no production code reads it.
#![allow(
    clippy::unwrap_used,
    reason = "test support behind the `testing` feature: a failed unwrap fails the calling test"
)]

use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use specforge_extension_sdk::{ContributionsBuilder, ExtensionMeta};
use specforge_protocol_types::PeerDependency;
use specforge_wasm::testing::InProcessRuntime;

use crate::analysis::collect::{
    CollectorCommand, CollectorMachine, CommandApproval, Ran, RunFailure, RunnerOutput,
};
use crate::asking::Asking;
use crate::solver::{Availability, SolveFailure, Solver, Z3};
use crate::view::ProjectView;
use crate::{Caller, Containment};

/// Every path, for the fixtures below (ADR 0092 D6: the CLI's reach).
static UNBOUNDED: Containment = Containment::unbounded();

/// The caller most tests lend: every path, nobody to ask (the CLI's reach without a terminal).
pub fn caller() -> Caller<'static> {
    Caller::new(&UNBOUNDED, Asking::Nobody)
}

/// A caller bounded by `containment`, nobody to ask (MCP's).
pub fn bounded(containment: &Containment) -> Caller<'_> {
    Caller::new(containment, Asking::Nobody)
}

#[path = "../../../../fixtures/greet-extension/src/contributions.rs"]
mod greet;

/// A test's assembled project, viewed (ADR 0108): [`ProjectView::of`] its compiled project, rooted or detached.
pub trait Viewed {
    /// The view of the rooted compiled project.
    fn view(&self) -> ProjectView<'_>;
    /// The view of the same project without a root.
    fn rootless_view(&self) -> ProjectView<'_>;
}

impl Viewed for specforge_project::testing::Assembled {
    fn view(&self) -> ProjectView<'_> {
        ProjectView::of(self.compiled())
    }

    fn rootless_view(&self) -> ProjectView<'_> {
        ProjectView::of(self.detached())
    }
}

/// The bytes the in-process runtime serves as `@sdk/greet` 0.1.0.
pub const GREET: &[u8] = b"\0asm in-process @sdk/greet 0.1.0";
/// Other bytes that also serve `@sdk/greet` 0.1.0: a reinstall, a changed
/// binary.
pub const GREET_VARIANT: &[u8] = b"\0asm in-process @sdk/greet 0.1.0 variant";
/// The bytes the in-process runtime serves as `@test/probe` 0.1.0.
pub const PROBE: &[u8] = b"\0asm in-process @test/probe 0.1.0";

/// What `@sdk/greet` declares: the fixture's builder.
pub fn greet() -> ContributionsBuilder {
    greet::build()
}

/// A local file declaring `@probe/needs-software` 1.0.0, requiring `@specforge/software` ^9.0.
pub const NEEDS_SOFTWARE_9: &[u8] = b"\0asm in-process @probe/needs-software 1.0.0 software ^9";
/// The same, requiring `@specforge/software` ^1.0.
pub const NEEDS_SOFTWARE_1: &[u8] = b"\0asm in-process @probe/needs-software 1.0.0 software ^1";
/// `@probe/base` 2.0.0, no peers.
pub const BASE_2: &[u8] = b"\0asm in-process @probe/base 2.0.0";
/// `@probe/base` 1.2.0, no peers.
pub const BASE_1: &[u8] = b"\0asm in-process @probe/base 1.2.0";
/// `@probe/dep` 1.0.0, requiring `@probe/base` ^1.0.
pub const DEP_1: &[u8] = b"\0asm in-process @probe/dep 1.0.0";

/// An extension named `name` at `version` whose peers are `peers` (name,
/// requirement, optional).
pub fn declaring(
    name: &'static str,
    version: &'static str,
    peers: &'static [(&'static str, &'static str, bool)],
) -> impl Fn() -> ContributionsBuilder + Send + Sync + 'static {
    move || {
        let mut meta = ExtensionMeta::new(name, version);
        meta.peer_dependencies = peers
            .iter()
            .map(|(peer, requirement, optional)| PeerDependency {
                name: (*peer).to_string(),
                version: (*requirement).to_string(),
                optional: *optional,
            })
            .collect();
        ContributionsBuilder::new(meta)
    }
}

/// An in-process runtime serving `GREET` and `GREET_VARIANT` as `@sdk/greet`
/// and `PROBE` as `@test/probe`.
pub fn candidates() -> InProcessRuntime {
    InProcessRuntime::new()
        .binary(GREET, greet)
        .binary(GREET_VARIANT, greet)
        .binary(PROBE, declaring("@test/probe", "0.1.0", &[]))
}

/// `runtime`, also serving the embedded bytes of the builtin `name` as
/// `build` declares it.
pub fn serving_builtin(
    runtime: InProcessRuntime,
    name: &str,
    build: impl Fn() -> ContributionsBuilder + Send + Sync + 'static,
) -> InProcessRuntime {
    runtime.binary(
        specforge_project::builtins()
            .get(name)
            .expect("a builtin extension"),
        build,
    )
}

/// [`candidates`], also serving the probes above and the builtins `@specforge/software` 1.0.0 and
/// `@specforge/testing` 1.0.0 (no peers) from their embedded bytes.
pub fn probes() -> InProcessRuntime {
    let runtime = candidates()
        .binary(
            NEEDS_SOFTWARE_9,
            declaring(
                "@probe/needs-software",
                "1.0.0",
                &[("@specforge/software", "^9.0", false)],
            ),
        )
        .binary(
            NEEDS_SOFTWARE_1,
            declaring(
                "@probe/needs-software",
                "1.0.0",
                &[("@specforge/software", "^1.0", false)],
            ),
        )
        .binary(BASE_2, declaring("@probe/base", "2.0.0", &[]))
        .binary(BASE_1, declaring("@probe/base", "1.2.0", &[]))
        .binary(
            DEP_1,
            declaring("@probe/dep", "1.0.0", &[("@probe/base", "^1.0", false)]),
        );
    let runtime = serving_builtin(
        runtime,
        "@specforge/software",
        declaring("@specforge/software", "1.0.0", &[]),
    );
    serving_builtin(
        runtime,
        "@specforge/testing",
        declaring("@specforge/testing", "1.0.0", &[]),
    )
}

/// A solver that answers what a test scripts: its availability, then each
/// solve's stdout (or failure) in order. It keeps every script it was asked.
pub struct ScriptedSolver {
    availability: Availability,
    answers: Mutex<VecDeque<Result<String, SolveFailure>>>,
    scripts: Mutex<Vec<String>>,
}

impl ScriptedSolver {
    fn with(availability: Availability, answers: VecDeque<Result<String, SolveFailure>>) -> Self {
        Self {
            availability,
            answers: Mutex::new(answers),
            scripts: Mutex::new(Vec::new()),
        }
    }

    /// z3 runs (`Z3 version 4.13.0 - scripted`) and is asked nothing.
    pub fn ready() -> Self {
        Self::answering([])
    }

    /// z3 runs and answers `answers`, one per solve, in order.
    pub fn answering(answers: impl IntoIterator<Item = Result<String, SolveFailure>>) -> Self {
        Self::with(
            Availability::Ready {
                version: "Z3 version 4.13.0 - scripted".into(),
            },
            answers.into_iter().collect(),
        )
    }

    /// z3 is not on PATH: the W098 the z3 adapter gives for it.
    pub fn not_found() -> Self {
        Self::with(
            Availability::Unavailable(Z3::not_on_path()),
            VecDeque::new(),
        )
    }

    /// The scripts it was asked, in order.
    pub fn scripts(&self) -> Vec<String> {
        self.scripts.lock().unwrap().clone()
    }
}

impl Solver for ScriptedSolver {
    fn availability(&self) -> Availability {
        self.availability.clone()
    }

    fn solve(&self, script: &str) -> Result<String, SolveFailure> {
        self.scripts.lock().unwrap().push(script.to_string());
        self.answers.lock().unwrap().pop_front().unwrap_or_else(|| {
            panic!("the scripted solver was asked more than it was scripted to answer:\n{script}")
        })
    }
}

/// What one scripted run does: the files it writes, what it prints, how it ends.
#[derive(Debug, Clone, Default)]
pub struct ScriptedRun {
    /// Files it writes, each a path relative to the report location (`""` is the report file itself),
    /// stamped one second after [`ScriptedMachine::started`].
    pub files: Vec<(PathBuf, String)>,
    /// What it prints to its standard output (written to the capture when there is one).
    pub stdout: Option<String>,
    /// Its exit code.
    pub exit_code: Option<i32>,
    /// It fails instead of ending.
    pub fails: Option<ScriptedFailure>,
}

/// How a scripted run fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptedFailure {
    /// The command could not be started.
    NotRun,
    /// Its standard output could not be written to the capture.
    Capture,
}

/// A command a [`ScriptedMachine`] was asked to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RanCommand {
    pub argv: Vec<String>,
    pub dir: PathBuf,
    pub report: PathBuf,
    pub output: RunnerOutput,
}

/// A collector machine that does what a test scripts: it records each command line, writes the scripted
/// report files and stdout, answers a fixed start time, and keeps approvals in memory (ADR 0125).
pub struct ScriptedMachine {
    every_command: bool,
    unable_to_remember: Option<String>,
    approved: Mutex<Vec<CommandApproval>>,
    script: Mutex<VecDeque<ScriptedRun>>,
    ran: Mutex<Vec<RanCommand>>,
}

impl Default for ScriptedMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptedMachine {
    /// The start time every scripted run answers (2 000 000 s after the epoch). Files a run writes are
    /// stamped one second after it; a test stamps an earlier run's file one second before it.
    pub fn started() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(2_000_000)
    }

    /// Nothing approved, nothing scripted: a run panics, naming the command.
    pub fn new() -> Self {
        ScriptedMachine {
            every_command: false,
            unable_to_remember: None,
            approved: Mutex::new(Vec::new()),
            script: Mutex::new(VecDeque::new()),
            ran: Mutex::new(Vec::new()),
        }
    }

    /// Also approving `approval`, as if the user approved it before.
    pub fn approving(self, approval: CommandApproval) -> Self {
        self.approved.lock().unwrap().push(approval);
        self
    }

    /// Every command approved: what a test needs when it is not about consent.
    pub fn approving_every_command(mut self) -> Self {
        self.every_command = true;
        self
    }

    /// `approve` fails with `why`.
    pub fn unable_to_remember(mut self, why: &str) -> Self {
        self.unable_to_remember = Some(why.to_string());
        self
    }

    /// The next run does `run`.
    pub fn answering(self, run: ScriptedRun) -> Self {
        self.script.lock().unwrap().push_back(run);
        self
    }

    /// The commands it ran, in order.
    pub fn runs(&self) -> Vec<RanCommand> {
        self.ran.lock().unwrap().clone()
    }

    /// What it holds approved now (scripted plus remembered).
    pub fn approvals(&self) -> Vec<CommandApproval> {
        self.approved.lock().unwrap().clone()
    }
}

impl CollectorMachine for ScriptedMachine {
    fn run(
        &self,
        command: &CollectorCommand<'_>,
        capture: Option<&mut dyn Write>,
    ) -> Result<Ran, RunFailure> {
        self.ran.lock().unwrap().push(RanCommand {
            argv: command.argv.to_vec(),
            dir: command.dir.to_path_buf(),
            report: command.report.to_path_buf(),
            output: command.output,
        });
        let run = self.script.lock().unwrap().pop_front().unwrap_or_else(|| {
            panic!(
                "the scripted machine ran more than it was scripted to: {}",
                command.argv.join(" ")
            )
        });
        match run.fails {
            Some(ScriptedFailure::NotRun) => {
                return Err(RunFailure::NotRun(std::io::Error::other(
                    "not scripted to start",
                )));
            }
            Some(ScriptedFailure::Capture) => {
                return Err(RunFailure::Capture(std::io::Error::other(
                    "scripted capture failure",
                )));
            }
            None => {}
        }
        for (relative, contents) in &run.files {
            let at = if relative.as_os_str().is_empty() {
                command.report.to_path_buf()
            } else {
                command.report.join(relative)
            };
            if let Some(dir) = at.parent() {
                std::fs::create_dir_all(dir).unwrap();
            }
            let file = std::fs::File::create(&at).unwrap();
            (&file).write_all(contents.as_bytes()).unwrap();
            file.set_modified(Self::started() + Duration::from_secs(1))
                .unwrap();
        }
        if let (Some(capture), Some(stdout)) = (capture, &run.stdout) {
            capture
                .write_all(stdout.as_bytes())
                .map_err(RunFailure::Capture)?;
        }
        Ok(Ran {
            exit_code: run.exit_code,
            started: Self::started(),
        })
    }

    fn approved(&self, approval: &CommandApproval) -> bool {
        self.every_command || self.approved.lock().unwrap().contains(approval)
    }

    fn approve(&self, approval: &CommandApproval) -> Result<(), String> {
        if let Some(why) = &self.unable_to_remember {
            return Err(why.clone());
        }
        let mut approved = self.approved.lock().unwrap();
        approved.retain(|a| !approval.replaces(a));
        approved.push(approval.clone());
        Ok(())
    }
}
