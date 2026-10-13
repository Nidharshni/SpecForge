//! `specforge` as a user runs it on one project. This is the one place the suite spells the binary, how a command
//! names its project (`--path <root>`, ADR 0053), how it is asked for its JSON document (`--format json`,
//! ADR 0052), and the environment it runs in.
//!
//! Every run:
//! - runs in the project's root;
//! - has stdin closed (empty) unless [`Cli::stdin`] says otherwise;
//! - runs under the project's own HOME ([`TestProject::home`], also `USERPROFILE`), with `SPECFORGE_WASMTIME_CACHE`
//!   pointing at the shared compile cache (`specforge_component::user_compile_cache_dir()` as the test process
//!   resolves it; `off` when there is none);
//! - runs without `NO_COLOR`, `CLICOLOR`, `CLICOLOR_FORCE`, `SPECFORGE_REGISTRY_TOKEN`, `SPECFORGE_CONSENT_FILE`,
//!   `SPECFORGE_REPORT` and `SPECFORGE_WATCH_DEBUG`.
//!
//! A test adds what its case needs with [`Cli::env`] and nothing else. ADR 0029 D9: these are the process-level
//! tests (clap, the exit table, the streams, the environment).

use std::ffi::OsString;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use specforge_project::testing::TestProject;

use super::child::{ChildGuard, guarded_command};

/// What a run does not inherit from the developer's environment.
pub(super) const REMOVED: [&str; 7] = [
    "NO_COLOR",
    "CLICOLOR",
    "CLICOLOR_FORCE",
    "SPECFORGE_REGISTRY_TOKEN",
    "SPECFORGE_CONSENT_FILE",
    "SPECFORGE_REPORT",
    "SPECFORGE_WATCH_DEBUG",
];

/// The compile cache every run shares, whatever its HOME: the test process's own (`off` when it has none).
pub(super) fn compile_cache() -> OsString {
    specforge_component::user_compile_cache_dir()
        .map(OsString::from)
        .unwrap_or_else(|| "off".into())
}

/// The binary under test, run on one project.
pub struct Cli<'p> {
    project: &'p TestProject,
    cwd: PathBuf,
    env: Vec<(OsString, OsString)>,
    stdin: Vec<u8>,
}

impl<'p> Cli<'p> {
    /// `specforge` on `project`, run from its root.
    pub fn on(project: &'p TestProject) -> Self {
        Cli {
            project,
            cwd: project.root().to_path_buf(),
            env: Vec::new(),
            stdin: Vec::new(),
        }
    }

    /// Set `key` for this driver's runs (a proxy, `PATH`, `NO_COLOR`, `SPECFORGE_REGISTRY_TOKEN`, …).
    pub fn env(mut self, key: &str, value: impl Into<OsString>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// Keep the caller's cargo and rustup homes (`CARGO_HOME`, `RUSTUP_HOME`): the run gets a HOME of its own, and a
    /// build or a `cargo test` needs the caller's package cache and installed toolchains.
    pub fn with_toolchain(self) -> Self {
        let home = |var: &str, default: &str| {
            std::env::var_os(var).map(PathBuf::from).unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").expect("HOME is set")).join(default)
            })
        };
        let (cargo, rustup) = (home("CARGO_HOME", ".cargo"), home("RUSTUP_HOME", ".rustup"));
        self.env("CARGO_HOME", cargo).env("RUSTUP_HOME", rustup)
    }

    /// Feed `bytes` on stdin.
    pub fn stdin(mut self, bytes: impl Into<Vec<u8>>) -> Self {
        self.stdin = bytes.into();
        self
    }

    /// Run from `dir` instead of the root (a relative `--path`, `init` in a subdirectory).
    pub fn in_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cwd = dir.into();
        self
    }

    /// `specforge <args> --path <root>`.
    pub fn run(&self, args: &[&str]) -> Run {
        self.finish(self.process(args, true, &[]))
    }

    /// `specforge <args> --path <root> --format json`: the exit code and the document on stdout, parsed (panics,
    /// naming the run, when stdout is not JSON).
    pub fn document(&self, args: &[&str]) -> Document {
        let run = self.finish(self.process(args, true, &["--format", "json"]));
        let json = run.json();
        Document {
            code: run.code,
            json,
            stderr: run.stderr,
        }
    }

    /// `specforge <args>`, no `--path`: commands that take none (`init`, `explain`, `mcp`, `new`, `extension …`)
    /// and the tests of `--path` itself.
    pub fn bare(&self, args: &[&str]) -> Run {
        self.finish(self.process(args, false, &[]))
    }

    /// [`Self::run`]'s command, unrun, for `assert_cmd`'s predicates.
    pub fn command(&self, args: &[&str]) -> assert_cmd::Command {
        self.assert(self.process(args, true, &[]))
    }

    /// [`Self::bare`]'s command, unrun.
    pub fn bare_command(&self, args: &[&str]) -> assert_cmd::Command {
        self.assert(self.process(args, false, &[]))
    }

    /// [`Self::run`]'s command started as a long-running child (`watch`), its stdout and stderr piped, stopped
    /// on drop.
    pub fn spawn(&self, args: &[&str]) -> ChildGuard {
        self.guard(self.process(args, true, &[]))
    }

    fn guard(&self, process: Command) -> ChildGuard {
        let mut wrapper = guarded_command(&process);
        self.environment(&mut wrapper);
        wrapper
            .current_dir(&self.cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        ChildGuard::spawn(&mut wrapper).expect("spawn specforge")
    }

    /// [`Self::bare`]'s command started as a long-running child: `watch` from a subdirectory, with no `--path`.
    pub fn bare_spawn(&self, args: &[&str]) -> ChildGuard {
        self.guard(self.process(args, false, &[]))
    }

    /// [`Self::bare`]'s command started, its stdin, stdout and stderr piped and not waited for: a test that closes
    /// one end or reads the other on its own terms (`specforge mcp`'s stdio, a closed stdout).
    pub fn start(&self, args: &[&str]) -> std::process::Child {
        self.process(args, false, &[])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn specforge")
    }

    /// The command line: `args`, then `--path <root>` when `path` is set, then `after`.
    fn process(&self, args: &[&str], path: bool, after: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_specforge"));
        command.args(args);
        if path {
            command.arg("--path").arg(self.project.root());
        }
        command.args(after);
        self.environment(&mut command);
        command.current_dir(&self.cwd);
        command
    }

    /// The environment of every run: the project's HOME, the shared compile cache, and nothing of the developer's
    /// terminal or registry session.
    fn environment(&self, command: &mut Command) {
        for key in REMOVED {
            command.env_remove(key);
        }
        command
            .env("HOME", self.project.home())
            .env("USERPROFILE", self.project.home())
            .env("SPECFORGE_WASMTIME_CACHE", compile_cache());
        for (key, value) in &self.env {
            command.env(key, value);
        }
    }

    fn assert(&self, command: Command) -> assert_cmd::Command {
        let mut assert = assert_cmd::Command::from_std(command);
        if !self.stdin.is_empty() {
            assert.write_stdin(self.stdin.clone());
        }
        assert
    }

    fn finish(&self, mut command: Command) -> Run {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().expect("spawn specforge");
        let mut stdin = child.stdin.take().expect("piped stdin");
        let bytes = self.stdin.clone();
        // A command that does not read stdin may exit first: a broken pipe is not the test's failure.
        let writer = std::thread::spawn(move || {
            let _ = stdin.write_all(&bytes);
        });
        let output = child.wait_with_output().expect("wait for specforge");
        let _ = writer.join();
        Run {
            code: output
                .status
                .code()
                .expect("specforge was not killed by a signal"),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }
}

/// What one run did.
pub struct Run {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Run {
    /// Stdout parsed as JSON (panics with the streams when it is not).
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.stdout).unwrap_or_else(|e| {
            panic!(
                "stdout is not JSON ({e})\nstdout: {}\nstderr: {}",
                self.stdout, self.stderr
            )
        })
    }

    /// Panics with the streams unless the exit code is `code`; `self` for chaining.
    pub fn exits(self, code: i32) -> Self {
        assert_eq!(
            self.code, code,
            "exit code\nstdout: {}\nstderr: {}",
            self.stdout, self.stderr
        );
        self
    }
}

/// A command's JSON document and its exit code (ADR 0052, ADR 0029).
pub struct Document {
    pub code: i32,
    pub json: serde_json::Value,
    pub stderr: String,
}
