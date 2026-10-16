//! A run over files: each target read, formatted and written or reported, a failure recorded per file
//! (ADR 0021 D4).

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_common::{Diagnostic, DiagnosticList, ProjectPath, ProjectRoot, codes};
use std::path::{Path, PathBuf};

use super::document::{Configs, FormattedDocument, format_text};
use super::targets::{Targets, targets};
use crate::{Containment, OpError, OpErrorKind, Refusal};

/// What the run does with a file whose formatting would change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Rewrite it.
    #[default]
    Write,
    /// Report it without writing; a file that would change fails the run
    /// (`--check`, MCP `check`).
    Check,
    /// Report it without writing; a change does not fail the run (`--diff`
    /// alone, MCP `diff`).
    Preview,
}

impl Mode {
    /// The one reading of the surfaces' flags: `write` decides when given,
    /// else a run writes unless `check` or `diff`; `check` fails on a
    /// change, `diff` alone does not.
    pub fn of_flags(check: bool, diff: bool, write: Option<bool>) -> Mode {
        if write.unwrap_or(!check && !diff) {
            Mode::Write
        } else if check {
            Mode::Check
        } else {
            Mode::Preview
        }
    }

    /// Whether the run writes the formatted text.
    pub fn writes(self) -> bool {
        self == Mode::Write
    }
}

pub struct Request<'a> {
    /// The project the run starts in (where `specforge.json` lives): its
    /// sources are the default targets, and paths are shown relative to it.
    /// It does not bound configuration discovery: each file is formatted
    /// with the configuration of its own project (D2).
    pub root: &'a ProjectRoot,
    /// Files or directories to format; empty means the project's sources
    /// (`ProjectConfig::sources`), which end at its nested projects. A named
    /// file is always formatted. A named directory that is a project
    /// (`ProjectRoot::is_root`) is that project's sources; any other named
    /// directory is walked (`specforge_common::walk_spec_files`), taking the
    /// sources of each project the walk reaches, not those of a project
    /// nested in one of them, without the files their project's `exclude`
    /// entries leave out (D3).
    pub paths: &'a [PathBuf],
    pub mode: Mode,
    /// What the run may reach (ADR 0104): a named path that leaves it is refused before anything
    /// is read ([`FormatField::Paths`]); a target (a walked one, a symlinked source) that lands
    /// outside it is neither read nor written but a failure of kind
    /// [`OpErrorKind::InvalidInput`], and the other targets are formatted (ADR 0092 D8). MCP
    /// passes its bound; the CLI's user is already at the file system and passes
    /// [`Containment::unbounded`].
    pub containment: &'a Containment,
}

/// The field of a format [`Request`] a refusal can be about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatField {
    /// `Request::paths`: the files and directories to format.
    Paths,
}

/// A file whose formatting differs from its content.
#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: PathBuf,
    pub before: String,
    pub after: String,
    /// The formatted text is on disk now (write mode, and the write worked).
    pub written: bool,
}

/// A file the run could not read or write; the others were still done. It
/// carries what kind of failure the OS reported ([`OpErrorKind::of_io`]):
/// a surface answers a locked file as permission denied and a missing one
/// as not found, never as a generic internal failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    Read {
        path: PathBuf,
        kind: OpErrorKind,
        error: String,
    },
    Write {
        path: PathBuf,
        kind: OpErrorKind,
        error: String,
    },
}

/// The code of a [`Failure::Read`] as an [`OpError`]: the file could not be
/// read.
pub const UNREADABLE: &str = crate::authoring::rename::UNREADABLE;
/// The code of a [`Failure::Write`] as an [`OpError`]: the file could not be
/// written.
pub const UNWRITABLE: &str = "file_unwritable";

impl Failure {
    /// A read of `path` failed with `error`.
    pub(super) fn read(path: PathBuf, error: &std::io::Error) -> Self {
        Failure::Read {
            path,
            kind: OpErrorKind::of_io(error),
            error: error.to_string(),
        }
    }

    /// `path` resolves to `real`, outside `within`: neither read nor written.
    fn outside(path: PathBuf, real: &Path, within: &Path) -> Self {
        Failure::Read {
            path,
            kind: OpErrorKind::InvalidInput,
            error: format!(
                "resolves to {}, outside {}: a symlink that leaves the project is not followed",
                real.display(),
                within.display()
            ),
        }
    }

    /// A write of `path` failed with `error`.
    fn write(path: PathBuf, error: &std::io::Error) -> Self {
        Failure::Write {
            path,
            kind: OpErrorKind::of_io(error),
            error: error.to_string(),
        }
    }

    /// The file that failed.
    pub fn path(&self) -> &Path {
        match self {
            Failure::Read { path, .. } | Failure::Write { path, .. } => path,
        }
    }

    /// What kind of failure it is: [`OpErrorKind::PermissionDenied`] when
    /// the OS refused, [`OpErrorKind::FileNotFound`] for a missing file,
    /// else [`OpErrorKind::Internal`].
    pub fn kind(&self) -> OpErrorKind {
        match self {
            Failure::Read { kind, .. } | Failure::Write { kind, .. } => *kind,
        }
    }

    /// Why it failed, as the OS said it.
    pub fn error(&self) -> &str {
        match self {
            Failure::Read { error, .. } | Failure::Write { error, .. } => error,
        }
    }

    /// What failed: `read` or `write`.
    pub fn verb(&self) -> &'static str {
        match self {
            Failure::Read { .. } => "read",
            Failure::Write { .. } => "write",
        }
    }

    /// The failure as every operation reports one: its kind, its own code
    /// ([`UNREADABLE`], [`UNWRITABLE`]) and the message [`Display`] gives.
    ///
    /// [`Display`]: std::fmt::Display
    pub fn to_op_error(&self) -> OpError {
        let code = match self {
            Failure::Read { .. } => UNREADABLE,
            Failure::Write { .. } => UNWRITABLE,
        };
        OpError::new(self.kind(), code, self.to_string())
    }
}

impl Failure {
    /// [`Display`](std::fmt::Display) naming the file as `name` does: what a document says.
    pub fn naming(&self, name: &ProjectPath) -> String {
        format!("failed to {} {name}: {}", self.verb(), self.error())
    }
}

impl std::fmt::Display for Failure {
    /// `failed to read <path>: <error>` (or `write`).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "failed to {} {}: {}",
            self.verb(),
            self.path().display(),
            self.error()
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct Outcome {
    /// The directory of the project the run started in: every file the document names is named from it.
    pub root: PathBuf,
    /// The mode the run ran in.
    pub mode: Mode,
    /// Files read and formatted.
    pub checked: usize,
    /// Files whose formatting differs, in discovery order.
    pub changes: Vec<FileChange>,
    /// Files that could not be read or written, in discovery order.
    pub failures: Vec<Failure>,
    /// Everything formatting reported: W141 once per configuration file
    /// used, W142 per region kept verbatim (spanned at its file and lines).
    pub diagnostics: Vec<Diagnostic>,
}

impl Outcome {
    /// The files this run wrote (ADR 0022's `Written.files` reads them).
    pub fn written(&self) -> impl Iterator<Item = &Path> {
        self.changes
            .iter()
            .filter(|c| c.written)
            .map(|c| c.path.as_path())
    }

    /// The files this run wrote, as every writing operation reports them
    /// (ADR 0022): each change whose write worked, recorded at the write.
    pub fn writes(&self) -> crate::Writes {
        self.written().collect()
    }

    /// No file failed.
    pub fn succeeded(&self) -> bool {
        self.failures.is_empty()
    }

    /// What kind of failure the run is, when files failed: the kind they
    /// all share (every file locked is permission denied), else
    /// [`OpErrorKind::Internal`]. `None` when no file failed.
    pub fn failure_kind(&self) -> Option<OpErrorKind> {
        let (first, rest) = self.failures.split_first()?;
        Some(if rest.iter().all(|f| f.kind() == first.kind()) {
            first.kind()
        } else {
            OpErrorKind::Internal
        })
    }

    /// No region was left unformatted (no W142).
    pub fn complete(&self) -> bool {
        !self.diagnostics.iter().any(|d| d.is(codes::W142))
    }

    /// Every `specforge.json` the run read is used as written: no E069.
    pub fn configured(&self) -> bool {
        !self.diagnostics.iter().any(|d| d.is(codes::E069))
    }

    /// The run's verdict (ADR 0021 D4, ADR 0029): every target was read
    /// and written, no region was left unformatted (W142), every config the
    /// run read is used as written (no E069), and under [`Mode::Check`] no
    /// file would change. `specforge format` exits by it; MCP
    /// `specforge.format` returns it as `ok`.
    pub fn ok(&self) -> bool {
        self.succeeded()
            && self.complete()
            && self.configured()
            && !(self.mode == Mode::Check && !self.changes.is_empty())
    }

    /// Every target was read and is in canonical form: no change, no
    /// failure, no region left unformatted (W142).
    pub fn clean(&self) -> bool {
        self.succeeded() && self.complete() && self.changes.is_empty()
    }

    /// No target at all (nothing found, nothing failed).
    pub fn found_nothing(&self) -> bool {
        self.checked == 0 && self.failures.is_empty()
    }

    /// The document both surfaces print (ADR 0052): `specforge format
    /// --format json` and `specforge.format`'s reply, a failed call's `data`.
    /// `diffs` adds each changed file's before and after (`--diff`, MCP
    /// `diff`).
    pub fn document(&self, diffs: bool) -> FormatDocument {
        let failed = !self.failures.is_empty();
        let named = |path: &Path| ProjectPath::of(&self.root, path);
        FormatDocument {
            changed_files: self.changes.iter().map(|c| named(&c.path)).collect(),
            total_checked: self.checked,
            ok: self.ok(),
            all_clean: self.clean(),
            check_only: !self.mode.writes(),
            diagnostics: DiagnosticList(self.diagnostics.clone()),
            diffs: diffs.then(|| {
                let of = |change| FileDiff::of(change, named(&change.path));
                self.changes.iter().map(of).collect()
            }),
            message: failed.then(|| {
                let reasons: Vec<String> = (self.failures.iter())
                    .map(|f| f.naming(&named(f.path())))
                    .collect();
                reasons.join("; ")
            }),
            failed_files: failed.then(|| self.failures.iter().map(|f| named(f.path())).collect()),
            failures: failed.then(|| {
                let of = |failure| FileFailure::of(failure, named(failure.path()));
                self.failures.iter().map(of).collect()
            }),
        }
    }
}

/// What `specforge format --format json` and `specforge.format` print
/// (`McpFormatResult`): the run's files, its verdict and what formatting
/// reported. What the run did not do is absent, never `null`.
#[derive(Debug, Clone, Serialize, Shape)]
pub struct FormatDocument {
    /// Files whose formatting differs, in discovery order.
    pub changed_files: Vec<ProjectPath>,
    /// Files read and formatted.
    pub total_checked: usize,
    /// The run's verdict ([`Outcome::ok`]).
    pub ok: bool,
    /// Every file was read and is in canonical form ([`Outcome::clean`]).
    pub all_clean: bool,
    /// The run did not write (check or diff).
    pub check_only: bool,
    /// E069 per problem in a specforge.json the run went around, then W141
    /// per configuration file used, W142 per region kept verbatim.
    pub diagnostics: DiagnosticList,
    /// Each file that would change, before and after (asked for: `--diff`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diffs: Option<Vec<FileDiff>>,
    /// Why the run failed: each failed file's reason, `; `-joined.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// The files the run could not read or write.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_files: Option<Vec<ProjectPath>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failures: Option<Vec<FileFailure>>,
}

/// One changed file's before and after text (`FormatDiff`).
#[derive(Debug, Clone, Serialize, Shape)]
pub struct FileDiff {
    pub file_path: ProjectPath,
    pub before: String,
    pub after: String,
    pub insertions: usize,
    pub deletions: usize,
}

impl FileDiff {
    fn of(change: &FileChange, file_path: ProjectPath) -> Self {
        let stats =
            specforge_formatter::unified_diff(file_path.as_str(), &change.before, &change.after);
        FileDiff {
            file_path,
            before: change.before.clone(),
            after: change.after.clone(),
            insertions: stats.insertions,
            deletions: stats.deletions,
        }
    }
}

/// One file the run could not read or write (`McpFormatFailure`).
#[derive(Debug, Clone, Serialize, Shape)]
pub struct FileFailure {
    pub file: ProjectPath,
    /// `read` or `write`.
    pub operation: String,
    /// The kind of the file's failure ([`OpErrorKind::as_str`], MCP's
    /// `McpErrorCode`).
    pub code: String,
    pub message: String,
}

impl FileFailure {
    fn of(failure: &Failure, file: ProjectPath) -> Self {
        FileFailure {
            operation: failure.verb().to_string(),
            code: failure.kind().as_str().to_string(),
            message: failure.naming(&file),
            file,
        }
    }
}

/// Format every target of `request`. Refused (`config_invalid`, nothing
/// read or written) when the `specforge.json` of a project whose sources
/// the run selects does not say which files they are
/// ([`ConfigProblem::blocks_rewrites`](specforge_common::ConfigProblem::blocks_rewrites));
/// each other problem of those configs is one E069, first among the
/// outcome's diagnostics, and fails [`Outcome::ok`]. A named `.spec` file
/// selects no config: it is formatted all the same (ADR 0021 D3).
pub fn run(request: &Request) -> Result<Outcome, Refusal<FormatField>> {
    // Every named path is held to the containment before anything is read: relative ones are read
    // against its root (bounded) or the process's directory (unbounded).
    let named = request
        .paths
        .iter()
        .map(|path| {
            request
                .containment
                .resolve(path)
                .map(|held| held.spelled)
                .map_err(|outside| OpError::from(outside).about(FormatField::Paths))
        })
        .collect::<Result<Vec<PathBuf>, _>>()?;
    let Targets { files, diagnostics } =
        targets(request.root.dir(), &named).map_err(|error| Refusal {
            error,
            subject: None,
        })?;
    // One set of configurations for the run: each file gets the one
    // `document(Place::File(file), ..)` gives it (no editor: the CLI and
    // MCP have none), and each configuration file is read once.
    let mut configs = Configs::default();
    let mut outcome = Outcome {
        root: request.root.dir().to_path_buf(),
        mode: request.mode,
        diagnostics,
        ..Outcome::default()
    };
    for target in files {
        // What is read and written is what the containment resolved: the target, named as given.
        let held = match request.containment.resolve(&target) {
            Ok(held) => held,
            Err(outside) => {
                outcome
                    .failures
                    .push(Failure::outside(target, outside.landed(), outside.root()));
                continue;
            }
        };
        let source = match std::fs::read_to_string(&held.real) {
            Ok(source) => source,
            Err(e) => {
                outcome.failures.push(Failure::read(target, &e));
                continue;
            }
        };
        outcome.checked += 1;
        let resolved = configs.for_file(&target, None);
        // A span names the file as the document does.
        let name = ProjectPath::of(&outcome.root, &target);
        let FormattedDocument {
            formatted,
            diagnostics,
            ..
        } = format_text(resolved, &source, None, Some(Path::new(name.as_str())));
        outcome.diagnostics.extend(diagnostics);
        if formatted == source {
            continue;
        }
        let written = match request.mode {
            Mode::Write => match std::fs::write(&held.real, &formatted) {
                Ok(()) => true,
                Err(e) => {
                    outcome.failures.push(Failure::write(target.clone(), &e));
                    false
                }
            },
            Mode::Check | Mode::Preview => false,
        };
        outcome.changes.push(FileChange {
            path: target,
            before: source,
            after: formatted,
            written,
        });
    }
    Ok(outcome)
}
