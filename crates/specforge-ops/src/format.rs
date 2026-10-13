//! `specforge format`, MCP `specforge.format` and the LSP's formatting
//! requests: one document, or every source of a project, formatted with the
//! configuration `specforge format` uses for each file (ADR 0021).
//!
//! A file that can't be read or written is recorded and the run goes on, so
//! a failure on one file never leaves the others half-done. The surfaces
//! present what happened: the CLI prints it, MCP returns it, the LSP turns
//! its edits into positions.

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_common::{
    Diagnostic, DiagnosticList, ProjectConfig, ProjectRoot, Sym, codes, walk_spec_files,
};
use specforge_formatter::config::{find_config_path, read_config_file};
use specforge_formatter::{FormatConfig, format_range, format_source};
use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::{OpError, OpErrorKind};

mod edits;

/// One change to a document: the source's bytes `range` become `new_text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    /// Byte offsets into the source the document was formatted from. Both ends lie on char
    /// boundaries, and neither falls between a `\r` and the `\n` after it (no editor can name that
    /// position).
    pub range: Range<usize>,
    pub new_text: String,
}

/// Where a document's text belongs, which decides its format configuration.
#[derive(Debug, Clone, Copy)]
pub enum Place<'a> {
    /// A file on disk. Its project is the nearest ancestor holding
    /// `specforge.json`; `.specforgefmt.toml` discovery starts at the file's
    /// directory and stops at that root. Outside any project it is formatted
    /// as [`Place::Detached`].
    File(&'a Path),
    /// Text of a project that is no file (`format --stdin`): discovery
    /// starts at `dir` and stops at `root`.
    InProject { root: &'a Path, dir: &'a Path },
    /// Text outside any project (an unsaved editor buffer).
    Detached,
}

/// An editor's indentation settings (LSP `FormattingOptions`). They apply
/// only to a document outside any project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorOptions {
    pub tab_size: usize,
    pub insert_spaces: bool,
}

impl EditorOptions {
    /// The defaults, indented as the editor says.
    fn config(self) -> FormatConfig {
        FormatConfig {
            indent_width: self.tab_size,
            use_tabs: !self.insert_spaces,
            ..FormatConfig::default()
        }
    }
}

/// Lines to format, 0-based and inclusive. The engine widens them to the
/// whole blocks they touch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lines {
    pub first: usize,
    pub last: usize,
}

/// Which configuration formatted a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigSource {
    /// The `.specforgefmt.toml` nearest the document, within its project.
    File(PathBuf),
    /// A project without one: the defaults, as `specforge format` uses.
    Defaults,
    /// No project: the editor's settings (the defaults without any).
    Editor,
}

/// One formatted document.
#[derive(Debug, Clone)]
pub struct FormattedDocument<'a> {
    source: &'a str,
    /// The whole document, formatted (only the widened lines change for a range).
    pub formatted: String,
    pub config: FormatConfig,
    pub config_source: ConfigSource,
    /// W141 when the configuration file is invalid (named in the message),
    /// then one W142 per region kept verbatim, spanned at its document
    /// lines; the span's file is the document's path (empty without one).
    pub diagnostics: Vec<Diagnostic>,
}

impl FormattedDocument<'_> {
    /// Whether formatting changes the text.
    pub fn changed(&self) -> bool {
        self.formatted != self.source
    }

    /// The edits that turn the source into [`formatted`](Self::formatted), byte for byte: applied to the
    /// source as an editor applies a response's edits (every range read in the source, all at once),
    /// they give exactly `formatted`. They are in order, none overlaps or touches the next, none is a
    /// no-op, and there are none when nothing changed.
    ///
    /// A line diff of the source and `formatted` (Myers), each change trimmed to the bytes it alters.
    /// Line breaks are text like any other: a removed blank line, an added final newline and a `\r\n`
    /// made `\n` are edits. Text formatting keeps is never replaced, so an editor keeps its cursor and
    /// marks there. For a range, only the blocks the range touches change (`format_range`), so every
    /// edit lies inside them.
    pub fn edits(&self) -> Vec<Edit> {
        edits::between(self.source, &self.formatted)
    }

    /// Whether every region parsed (no W142): the text is in canonical form
    /// when this holds and nothing changed.
    pub fn complete(&self) -> bool {
        !self.diagnostics.iter().any(|d| d.is(codes::W142))
    }
}

/// Format `text`, which belongs at `place`: the whole document, or the
/// blocks `lines` touch. Inside a project its `.specforgefmt.toml` (else the
/// defaults) decides, never `editor`; outside one, `editor` (else the
/// defaults).
pub fn document<'a>(
    place: Place<'_>,
    text: &'a str,
    lines: Option<Lines>,
    editor: Option<EditorOptions>,
) -> FormattedDocument<'a> {
    let mut configs = Configs::default();
    let (file, resolved) = match place {
        Place::File(path) => (Some(path), configs.for_file(path, editor)),
        Place::InProject { root, dir } => (None, configs.resolve(dir, root)),
        Place::Detached => (None, Resolved::editor(editor)),
    };
    format_text(resolved, text, lines, file)
}

/// The directory `file` is in (`.` for a bare file name).
fn directory_of(file: &Path) -> PathBuf {
    match file.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// A document's configuration, where it came from, and what reading it
/// reported (only the first time a run reads that file).
struct Resolved {
    config: FormatConfig,
    source: ConfigSource,
    diagnostics: Vec<Diagnostic>,
}

impl Resolved {
    /// Outside any project: the editor's settings, else the defaults.
    fn editor(editor: Option<EditorOptions>) -> Resolved {
        Resolved {
            config: editor.map_or_else(FormatConfig::default, EditorOptions::config),
            source: ConfigSource::Editor,
            diagnostics: Vec::new(),
        }
    }
}

/// The configurations of one run: one project lookup per directory, one
/// walk per directory and project, one read per configuration file. A
/// file's W141 goes out the first time it is used.
#[derive(Default)]
struct Configs {
    projects: HashMap<PathBuf, Option<PathBuf>>,
    found: HashMap<(PathBuf, PathBuf), Option<PathBuf>>,
    read: HashMap<PathBuf, FormatConfig>,
}

impl Configs {
    /// The configuration of `file`: its own project's (discovery from its
    /// directory up to that project's root), else outside any project the
    /// editor's.
    fn for_file(&mut self, file: &Path, editor: Option<EditorOptions>) -> Resolved {
        let dir = directory_of(file);
        match self.project_of(&dir) {
            Some(root) => self.resolve(&dir, &root),
            None => Resolved::editor(editor),
        }
    }

    /// The root of the project `dir` is in.
    fn project_of(&mut self, dir: &Path) -> Option<PathBuf> {
        self.projects
            .entry(dir.to_path_buf())
            .or_insert_with(|| ProjectRoot::enclosing(dir).map(|r| r.dir().to_path_buf()))
            .clone()
    }

    /// The configuration of a document in `dir`, in the project at `root`:
    /// the nearest `.specforgefmt.toml` up to `root`, else the defaults.
    fn resolve(&mut self, dir: &Path, root: &Path) -> Resolved {
        let file = self
            .found
            .entry((dir.to_path_buf(), root.to_path_buf()))
            .or_insert_with(|| find_config_path(dir, root))
            .clone();
        let Some(file) = file else {
            return Resolved {
                config: FormatConfig::default(),
                source: ConfigSource::Defaults,
                diagnostics: Vec::new(),
            };
        };
        let (config, diagnostics) = match self.read.get(&file) {
            Some(config) => (config.clone(), Vec::new()),
            None => {
                let (config, diagnostics) = read_config_file(&file);
                self.read.insert(file.clone(), config.clone());
                (config, diagnostics)
            }
        };
        Resolved {
            config,
            source: ConfigSource::File(file),
            diagnostics,
        }
    }
}

/// Format `text` (the blocks `lines` touch, else all of it) with
/// `resolved`, spanning what the engine reports at `file`.
fn format_text<'a>(
    resolved: Resolved,
    text: &'a str,
    lines: Option<Lines>,
    file: Option<&Path>,
) -> FormattedDocument<'a> {
    let result = match lines {
        // An empty document has no lines to widen.
        Some(lines) if !text.is_empty() => {
            format_range(text, lines.first, lines.last, &resolved.config)
        }
        _ => format_source(text, &resolved.config),
    };
    let file = Sym::new(&file.map(|f| f.display().to_string()).unwrap_or_default());
    let mut diagnostics = resolved.diagnostics;
    diagnostics.extend(result.diagnostics.into_iter().map(|mut d| {
        if let Some(span) = &mut d.span {
            span.file = file;
        }
        d
    }));
    FormattedDocument {
        source: text,
        formatted: result.formatted,
        config: resolved.config,
        config_source: resolved.source,
        diagnostics,
    }
}

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
    /// (`ProjectConfig::spec_files`). A named file is always formatted. A
    /// named directory that is a project (`ProjectRoot::is_root`) is that
    /// project's sources; any other named directory is walked as discovery
    /// walks (`specforge_common::walk_spec_files`), taking a nested
    /// project's sources where the walk reaches its root,
    /// without the files their project's `exclude` entries leave out (D3).
    pub paths: &'a [PathBuf],
    pub mode: Mode,
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
pub const UNREADABLE: &str = crate::rename::UNREADABLE;
/// The code of a [`Failure::Write`] as an [`OpError`]: the file could not be
/// written.
pub const UNWRITABLE: &str = "file_unwritable";

impl Failure {
    /// A read of `path` failed with `error`.
    fn read(path: PathBuf, error: &std::io::Error) -> Self {
        Failure::Read {
            path,
            kind: OpErrorKind::of_io(error),
            error: error.to_string(),
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
        FormatDocument {
            changed_files: self.changes.iter().map(|c| shown(&c.path)).collect(),
            total_checked: self.checked,
            ok: self.ok(),
            all_clean: self.clean(),
            check_only: !self.mode.writes(),
            diagnostics: DiagnosticList(self.diagnostics.clone()),
            diffs: diffs.then(|| self.changes.iter().map(FileDiff::of).collect()),
            message: failed.then(|| {
                let reasons: Vec<String> = self.failures.iter().map(ToString::to_string).collect();
                reasons.join("; ")
            }),
            failed_files: failed.then(|| self.failures.iter().map(|f| shown(f.path())).collect()),
            failures: failed.then(|| self.failures.iter().map(FileFailure::of).collect()),
        }
    }
}

/// A path as the format document shows it.
fn shown(path: &Path) -> String {
    path.display().to_string()
}

/// What `specforge format --format json` and `specforge.format` print
/// (`McpFormatResult`): the run's files, its verdict and what formatting
/// reported. What the run did not do is absent, never `null`.
#[derive(Debug, Clone, Serialize, Shape)]
pub struct FormatDocument {
    /// Files whose formatting differs, in discovery order.
    pub changed_files: Vec<String>,
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
    pub failed_files: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failures: Option<Vec<FileFailure>>,
}

/// One changed file's before and after text (`FormatDiff`).
#[derive(Debug, Clone, Serialize, Shape)]
pub struct FileDiff {
    pub file_path: String,
    pub before: String,
    pub after: String,
    pub insertions: usize,
    pub deletions: usize,
}

impl FileDiff {
    fn of(change: &FileChange) -> Self {
        let file_path = shown(&change.path);
        let stats = specforge_formatter::unified_diff(&file_path, &change.before, &change.after);
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
    pub file: String,
    /// `read` or `write`.
    pub operation: String,
    /// The kind of the file's failure ([`OpErrorKind::as_str`], MCP's
    /// `McpErrorCode`).
    pub code: String,
    pub message: String,
}

impl FileFailure {
    fn of(failure: &Failure) -> Self {
        FileFailure {
            file: shown(failure.path()),
            operation: failure.verb().to_string(),
            code: failure.kind().as_str().to_string(),
            message: failure.to_string(),
        }
    }
}

/// The files a run from `root` over `paths` formats, de-duplicated, in
/// discovery order (ADR 0021 D3). A file reached several times is one target,
/// shown as first spelled:
///
/// - no paths: the project's sources ([`ProjectConfig::spec_files`]);
/// - a named `.spec` file: that file, even if excluded (the user named it);
///   any other named file is skipped;
/// - a named directory that is a project root (it holds `specforge.json`):
///   that project's sources;
/// - any other named directory: its `.spec` files as discovery walks them
///   (`specforge_common::walk_spec_files`), taking a nested project's
///   sources where the walk reaches its root, without the files their
///   project's `exclude` entries leave out.
fn targets(root: &Path, paths: &[PathBuf]) -> Result<Targets, OpError> {
    let mut projects = Projects::default();
    let found: Vec<PathBuf> = if paths.is_empty() {
        projects.config(root)?.spec_files(root)
    } else {
        let mut found = Vec::new();
        for path in paths {
            if path.is_dir() {
                found.extend(projects.walk(path)?);
            } else if path.extension().is_some_and(|ext| ext == "spec") {
                found.push(path.clone());
            }
        }
        found
    };
    // One target per file, however many arguments reach it: keyed by the
    // canonical path (a file that does not exist keys by its spelling),
    // shown as first spelled.
    let mut seen = std::collections::HashSet::new();
    let files = found
        .into_iter()
        .filter(|path| seen.insert(path.canonicalize().unwrap_or_else(|_| path.clone())))
        .collect();
    Ok(Targets {
        files,
        diagnostics: projects.diagnostics,
    })
}

/// The run's targets and the E069s of the configs that selected them.
struct Targets {
    files: Vec<PathBuf>,
    diagnostics: Vec<Diagnostic>,
}

/// The `specforge.json` of each project a run selects sources from, read
/// once through [`crate::config::rewritable`], and the E069s of the problems
/// those reads went around, in the order the configs were read.
#[derive(Default)]
struct Projects {
    configs: HashMap<PathBuf, ProjectConfig>,
    roots: HashMap<PathBuf, Option<PathBuf>>,
    diagnostics: Vec<Diagnostic>,
}

impl Projects {
    /// The configuration of the project at `root`; refused when it does not
    /// say which files are its sources.
    fn config(&mut self, root: &Path) -> Result<&ProjectConfig, OpError> {
        if !self.configs.contains_key(root) {
            let read = crate::config::rewritable(root)?;
            self.diagnostics.extend(read.diagnostics());
            self.configs.insert(root.to_path_buf(), read.config);
        }
        Ok(&self.configs[root])
    }

    /// The `.spec` files of a named directory (see [`targets`]), sorted.
    fn walk(&mut self, dir: &Path) -> Result<Vec<PathBuf>, OpError> {
        if ProjectRoot::is_root(dir) {
            return Ok(self.config(dir)?.spec_files(dir));
        }
        let walked = walk_spec_files(dir, ProjectRoot::is_root);
        let mut files = Vec::new();
        for file in walked.files {
            if !self.excluded(&file)? {
                files.push(file);
            }
        }
        for project in walked.stopped {
            files.extend(self.config(&project)?.spec_files(&project));
        }
        files.sort();
        Ok(files)
    }

    /// Whether the project `file` belongs to leaves it out (`exclude`, or
    /// a skipped directory under its spec root).
    fn excluded(&mut self, file: &Path) -> Result<bool, OpError> {
        let Ok(file) = file.canonicalize() else {
            return Ok(false);
        };
        let dir = directory_of(&file);
        let root = self
            .roots
            .entry(dir.clone())
            .or_insert_with(|| ProjectRoot::enclosing(&dir).map(|r| r.dir().to_path_buf()))
            .clone();
        match root {
            Some(root) => {
                let config = self.config(&root)?;
                Ok(config.excludes(&config.spec_root_in(&root), &file))
            }
            None => Ok(false),
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
pub fn run(request: &Request) -> Result<Outcome, OpError> {
    let Targets { files, diagnostics } = targets(request.root.dir(), request.paths)?;
    // One set of configurations for the run: each file gets the one
    // `document(Place::File(file), ..)` gives it (no editor: the CLI and
    // MCP have none), and each configuration file is read once.
    let mut configs = Configs::default();
    let mut outcome = Outcome {
        mode: request.mode,
        diagnostics,
        ..Outcome::default()
    };
    for target in files {
        let source = match std::fs::read_to_string(&target) {
            Ok(source) => source,
            Err(e) => {
                outcome.failures.push(Failure::read(target, &e));
                continue;
            }
        };
        outcome.checked += 1;
        let resolved = configs.for_file(&target, None);
        let FormattedDocument {
            formatted,
            diagnostics,
            ..
        } = format_text(resolved, &source, None, Some(&target));
        outcome.diagnostics.extend(diagnostics);
        if formatted == source {
            continue;
        }
        let written = match request.mode {
            Mode::Write => match std::fs::write(&target, &formatted) {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A run that is not refused: the tests that expect a refusal call
    /// [`super::run`].
    fn run(request: &Request) -> Outcome {
        super::run(request).unwrap()
    }

    const MESSY: &str = "behavior messy \"Messy\" {\ncontract \"The system MUST work\"\n}\n";

    fn project() -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("specforge.json"), "{}").unwrap();
        std::fs::create_dir(dir.path().join("spec")).unwrap();
        std::fs::write(dir.path().join("spec/a.spec"), MESSY).unwrap();
        std::fs::write(dir.path().join("spec/b.spec"), MESSY.replace("messy", "b")).unwrap();
        dir
    }

    fn request(root: &ProjectRoot, mode: Mode) -> Request<'_> {
        Request {
            root,
            paths: &[],
            mode,
        }
    }

    #[test]
    fn check_mode_reports_every_change_and_writes_nothing() {
        let dir = project();

        let outcome = run(&request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Mode::Check,
        ));

        assert_eq!(outcome.checked, 2);
        assert_eq!(outcome.changes.len(), 2);
        assert!(outcome.changes.iter().all(|c| !c.written));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("spec/a.spec")).unwrap(),
            MESSY
        );
    }

    #[test]
    fn write_mode_rewrites_and_a_second_run_is_clean() {
        let dir = project();

        let first = run(&request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Mode::Write,
        ));
        let second = run(&request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Mode::Write,
        ));

        assert!(first.changes.iter().all(|c| c.written));
        assert_eq!(first.written().count(), 2);
        assert_eq!(second.checked, 2);
        assert!(second.changes.is_empty());
        assert!(second.clean());
    }

    #[test]
    fn ok_fails_only_a_check_on_a_change() {
        let dir = project();
        let ok = |mode| {
            run(&request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                mode,
            ))
            .ok()
        };

        assert!(!ok(Mode::Check), "a check that finds a change fails");
        assert!(ok(Mode::Preview), "a preview of a change passes");
        assert!(ok(Mode::Write), "a write passes, whatever it changed");
        // Everything is canonical after the write: every mode passes.
        assert!(ok(Mode::Check) && ok(Mode::Preview));

        // An unformatted region fails every mode.
        let region = project_with(
            "{}",
            &[("spec/a.spec", "behavior a \"A\" {\n  @@@ ]]\n}\n")],
        );
        for mode in [Mode::Write, Mode::Check, Mode::Preview] {
            assert!(
                !run(&request(
                    &specforge_common::ProjectRoot::of(region.path()).unwrap(),
                    mode
                ))
                .ok(),
                "{mode:?}"
            );
        }
    }

    #[test]
    fn clean_needs_every_file_read_and_canonical() {
        let canonical = "behavior messy \"Messy\" {\n  contract \"The system MUST work\"\n}\n";
        // Canonical: clean.
        let dir = project_with("{}", &[("spec/a.spec", canonical)]);
        let outcome = run(&request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Mode::Check,
        ));
        assert!(outcome.clean() && outcome.complete() && outcome.succeeded());

        // A change: not clean, though complete and succeeded.
        let dir = project_with("{}", &[("spec/a.spec", MESSY)]);
        let outcome = run(&request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Mode::Check,
        ));
        assert!(!outcome.clean() && outcome.complete() && outcome.succeeded());

        // A region left unformatted: not complete, so not clean.
        let broken = format!("{canonical}\n}}}}}}\n");
        let dir = project_with("{}", &[("spec/a.spec", &broken)]);
        let outcome = run(&request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Mode::Check,
        ));
        assert!(outcome.changes.is_empty(), "{:?}", outcome.changes);
        assert!(!outcome.complete() && !outcome.clean() && outcome.succeeded());

        // A file that cannot be read: a failure, so not clean.
        let dir = project_with("{}", &[("spec/a.spec", canonical)]);
        let missing = [dir.path().join("spec/missing.spec")];
        let outcome = run(&Request {
            root: &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            paths: &missing,
            mode: Mode::Check,
        });
        assert!(!outcome.succeeded() && !outcome.clean());
        assert!(!outcome.found_nothing());
        assert!(matches!(&outcome.failures[0], Failure::Read { path, .. } if path == &missing[0]));
        // A missing file is not found, and the run says so.
        assert_eq!(outcome.failures[0].kind(), OpErrorKind::FileNotFound);
        assert_eq!(outcome.failure_kind(), Some(OpErrorKind::FileNotFound));
        let error = outcome.failures[0].to_op_error();
        assert_eq!(
            (error.kind, &*error.code),
            (OpErrorKind::FileNotFound, UNREADABLE)
        );
        assert!(error.message.starts_with("failed to read "), "{error:?}");
    }

    #[cfg(unix)]
    #[test]
    fn written_lists_only_files_on_disk() {
        use std::os::unix::fs::PermissionsExt;
        let dir = project();
        let locked = dir.path().join("spec/a.spec");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o444)).unwrap();

        let outcome = run(&request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Mode::Write,
        ));
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).unwrap();

        let change = outcome.changes.iter().find(|c| c.path == locked).unwrap();
        assert!(!change.written);
        assert_eq!(outcome.failures.len(), 1, "{:?}", outcome.failures);
        assert!(matches!(&outcome.failures[0], Failure::Write { path, .. } if path == &locked));
        // A file the OS refused to write is permission denied.
        assert_eq!(outcome.failures[0].kind(), OpErrorKind::PermissionDenied);
        assert_eq!(outcome.failure_kind(), Some(OpErrorKind::PermissionDenied));
        assert_eq!(&*outcome.failures[0].to_op_error().code, UNWRITABLE);
        let written: Vec<&Path> = outcome.written().collect();
        assert_eq!(written, [dir.path().join("spec/b.spec").as_path()]);
    }

    #[cfg(unix)]
    #[test]
    fn a_locked_or_missing_file_fails_with_the_kind_the_os_gave() {
        use std::os::unix::fs::PermissionsExt;
        let dir = project();
        let locked = dir.path().join("spec/a.spec");
        let missing = dir.path().join("spec/missing.spec");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        let named = [locked.clone(), missing.clone()];
        let outcome = run(&Request {
            root: &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            paths: &named,
            mode: Mode::Check,
        });
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).unwrap();

        let kinds: Vec<(&Path, OpErrorKind)> = outcome
            .failures
            .iter()
            .map(|f| (f.path(), f.kind()))
            .collect();
        assert_eq!(
            kinds,
            [
                (locked.as_path(), OpErrorKind::PermissionDenied),
                (missing.as_path(), OpErrorKind::FileNotFound),
            ]
        );
        // Failures of different kinds are an internal failure of the run;
        // one kind is that kind.
        assert_eq!(outcome.failure_kind(), Some(OpErrorKind::Internal));
        let only_locked = Outcome {
            failures: outcome.failures[..1].to_vec(),
            ..Outcome::default()
        };
        assert_eq!(
            only_locked.failure_kind(),
            Some(OpErrorKind::PermissionDenied)
        );
        assert_eq!(Outcome::default().failure_kind(), None);
    }

    #[test]
    fn mode_reads_the_flags_once() {
        use Mode::{Check, Preview, Write};
        let flags = [
            // (check, diff, write) → mode
            ((false, false, None), Write),
            ((true, false, None), Check),
            ((false, true, None), Preview),
            ((true, true, None), Check),
            ((false, false, Some(false)), Preview),
            ((true, false, Some(true)), Write),
            ((false, true, Some(true)), Write),
            ((false, false, Some(true)), Write),
        ];
        for ((check, diff, write), mode) in flags {
            assert_eq!(
                Mode::of_flags(check, diff, write),
                mode,
                "{check} {diff} {write:?}"
            );
        }
    }

    /// Write `files` (path, text) under a fresh directory holding
    /// `specforge.json` = `config`.
    fn project_with(config: &str, files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("specforge.json"), config).unwrap();
        for (path, text) in files {
            let path = dir.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        dir
    }

    /// The changed files of a check run over `root`, relative to it.
    fn changed(root: &Path) -> Vec<String> {
        run(&request(
            &specforge_common::ProjectRoot::of(root).unwrap(),
            Mode::Check,
        ))
        .changes
        .iter()
        .map(|c| relative(root, &c.path))
        .collect()
    }

    /// `path` relative to `root`, with `/` separators.
    fn relative(root: &Path, path: &Path) -> String {
        path.strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }

    /// A check run from `root` over `paths` (relative to it).
    fn check_paths(root: &Path, paths: &[&str]) -> Outcome {
        let paths: Vec<PathBuf> = paths.iter().map(|p| root.join(p)).collect();
        run(&Request {
            root: &specforge_common::ProjectRoot::of(root).unwrap(),
            paths: &paths,
            mode: Mode::Check,
        })
    }

    /// The changed files of `outcome`, relative to `root`.
    fn changed_in(root: &Path, outcome: &Outcome) -> Vec<String> {
        outcome
            .changes
            .iter()
            .map(|c| relative(root, &c.path))
            .collect()
    }

    #[specforge_test(
        behavior = "discover_format_targets",
        verify = "no arguments formats all .spec files under spec_root"
    )]
    fn targets_are_the_files_under_spec_root() {
        let dir = project_with(
            r#"{"spec_root": "specs"}"#,
            &[
                ("specs/a.spec", MESSY),
                ("specs/sub/b.spec", MESSY),
                ("fixtures/fx.spec", MESSY),
            ],
        );
        assert_eq!(changed(dir.path()), ["specs/a.spec", "specs/sub/b.spec"]);

        // A spec/ directory beside the spec root is no source either.
        std::fs::create_dir(dir.path().join("spec")).unwrap();
        std::fs::write(dir.path().join("spec/old.spec"), MESSY).unwrap();
        assert_eq!(changed(dir.path()), ["specs/a.spec", "specs/sub/b.spec"]);

        // Without spec_root, every source under the root, as check reads them.
        std::fs::write(dir.path().join("specforge.json"), "{}").unwrap();
        assert_eq!(
            changed(dir.path()),
            [
                "fixtures/fx.spec",
                "spec/old.spec",
                "specs/a.spec",
                "specs/sub/b.spec"
            ]
        );
    }

    #[specforge_test(
        behavior = "discover_format_targets",
        verify = "files the project's exclude entries leave out are not formatted"
    )]
    fn excluded_files_are_not_formatted() {
        let dir = project_with(
            r#"{"spec_root": "spec", "exclude": ["drafts"]}"#,
            &[("spec/a.spec", MESSY), ("spec/drafts/d.spec", MESSY)],
        );

        assert_eq!(changed(dir.path()), ["spec/a.spec"]);
        // A file named explicitly is formatted all the same.
        let named = check_paths(dir.path(), &["spec/drafts/d.spec"]);
        assert_eq!(changed_in(dir.path(), &named), ["spec/drafts/d.spec"]);
    }

    #[specforge_test(
        behavior = "discover_format_targets",
        verify = "explicit file paths format only those files"
    )]
    fn explicit_files_format_only_those() {
        let dir = project();

        let outcome = check_paths(dir.path(), &["spec/a.spec"]);

        assert_eq!(outcome.checked, 1);
        assert_eq!(changed_in(dir.path(), &outcome), ["spec/a.spec"]);
    }

    #[specforge_test(
        behavior = "discover_format_targets",
        verify = "directory argument recursively discovers .spec files"
    )]
    fn a_directory_argument_is_walked() {
        let dir = project_with(
            r#"{"exclude": ["drafts"]}"#,
            &[
                ("spec/sub/a.spec", MESSY),
                ("spec/sub/deep/b.spec", MESSY),
                ("spec/sub/drafts/d.spec", MESSY),
                ("spec/sub/target/t.spec", MESSY),
                ("spec/other.spec", MESSY),
            ],
        );

        let outcome = check_paths(dir.path(), &["spec/sub"]);

        assert_eq!(
            changed_in(dir.path(), &outcome),
            ["spec/sub/a.spec", "spec/sub/deep/b.spec"]
        );
    }

    #[specforge_test(
        behavior = "discover_format_targets",
        verify = "non-.spec files are skipped with no error"
    )]
    fn non_spec_files_are_skipped() {
        let dir = project_with("{}", &[("notes.md", "# notes"), ("a.spec", MESSY)]);

        let outcome = check_paths(dir.path(), &["notes.md", "a.spec"]);

        assert_eq!(outcome.checked, 1);
        assert!(outcome.failures.is_empty());
    }

    #[specforge_test(
        behavior = "discover_format_targets",
        verify = "a named directory that is a project formats that project's sources"
    )]
    fn a_named_project_directory_formats_its_sources() {
        let dir = project_with(
            r#"{"spec_root": "specs"}"#,
            &[("specs/a.spec", MESSY), ("fixtures/fx.spec", MESSY)],
        );

        let named = check_paths(dir.path(), &[""]);

        assert_eq!(changed_in(dir.path(), &named), ["specs/a.spec"]);
        assert_eq!(changed(dir.path()), ["specs/a.spec"]);
    }

    #[specforge_test(
        behavior = "discover_format_targets",
        verify = "a named directory that is a project formats that project's sources"
    )]
    fn a_walk_takes_a_nested_projects_sources() {
        let dir = project_with(
            "{}",
            &[
                ("examples/p/specforge.json", r#"{"spec_root": "spec"}"#),
                ("examples/p/spec/a.spec", MESSY),
                ("examples/p/fixtures/f.spec", MESSY),
                ("examples/loose.spec", MESSY),
            ],
        );

        let outcome = check_paths(dir.path(), &["examples"]);

        assert_eq!(
            changed_in(dir.path(), &outcome),
            ["examples/loose.spec", "examples/p/spec/a.spec"]
        );
    }

    #[specforge_test(
        behavior = "discover_format_targets",
        verify = "Discover Format Targets: format target discovery holds — project_root_available, filesystem_accessible, all_spec_files_discovered, exclusions_applied, non_spec_skipped"
    )]
    fn discover_contract() {
        // project_root_available, filesystem_accessible: a project on disk.
        let dir = project_with(
            r#"{"spec_root": "spec", "exclude": ["vendor"]}"#,
            &[
                ("spec/a.spec", MESSY),
                ("spec/sub/b.spec", MESSY),
                ("spec/vendor/v.spec", MESSY),
                ("spec/c.txt", "not a spec"),
            ],
        );
        let root = dir.path();

        // all_spec_files_discovered, exclusions_applied.
        assert_eq!(
            targets(root, &[]).unwrap().files,
            [root.join("spec/a.spec"), root.join("spec/sub/b.spec")]
        );
        // non_spec_skipped: named, a non-.spec file is no target and no failure.
        let outcome = check_paths(root, &["spec/a.spec", "spec/c.txt"]);
        assert_eq!(outcome.checked, 1);
        assert!(outcome.failures.is_empty());
    }

    /// The paths the CI gate formats (`.github/workflows/ci.yml`:
    /// `specforge format --check spec integrations/rust/spec
    /// examples/todo-app examples/shop`), and the source directory each
    /// one's files must lie in.
    const CI_GATE: [(&str, &str); 4] = [
        ("spec", "spec"),
        ("integrations/rust/spec", "integrations/rust/spec"),
        ("examples/todo-app", "examples/todo-app/spec"),
        ("examples/shop", "examples/shop/spec"),
    ];

    #[test]
    fn the_ci_gate_paths_reach_only_their_sources() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let workspace = workspace.canonicalize().unwrap();
        let paths: Vec<PathBuf> = CI_GATE.iter().map(|(p, _)| workspace.join(p)).collect();

        let found = targets(&workspace, &paths).unwrap().files;

        let sources: Vec<PathBuf> = CI_GATE
            .iter()
            .flat_map(|(_, dir)| {
                specforge_common::discover_spec_files(
                    &workspace.join(dir),
                    &specforge_common::Exclude::default(),
                )
            })
            .collect();
        for target in &found {
            assert!(
                sources.contains(target),
                "{} is no source",
                target.display()
            );
        }
        assert_eq!(found.len(), sources.len());
        let outcome = run(&Request {
            root: &specforge_common::ProjectRoot::of(&workspace).unwrap(),
            paths: &paths,
            mode: Mode::Check,
        });
        assert_eq!(outcome.checked, sources.len());
    }

    #[specforge_test(
        behavior = "load_format_config",
        verify = "a file's configuration does not depend on where format runs"
    )]
    fn each_file_uses_its_nearest_config() {
        let two = "behavior login \"Login\" {\n  contract \"The system MUST log in\"\n}\n";
        let dir = project_with(
            "{}",
            &[
                ("spec/sub/.specforgefmt.toml", "indent_width = 4\n"),
                ("spec/sub/a.spec", FOUR),
                ("spec/b.spec", two),
            ],
        );

        // From the root, each file gets its nearest configuration: both are
        // canonical.
        assert_eq!(changed(dir.path()), Vec::<String>::new());
        // Named from the subdirectory, the same.
        let named = check_paths(dir.path(), &["spec/sub"]);
        assert_eq!((named.checked, named.changes.len()), (1, 0));
    }

    #[specforge_test(
        behavior = "load_format_config",
        verify = "invalid indent_width produces diagnostic and uses default"
    )]
    fn an_invalid_config_is_reported_once_per_run() {
        let dir = project_with(
            "{}",
            &[
                (".specforgefmt.toml", "indent_width = 99\n"),
                ("spec/a.spec", MESSY),
                ("spec/b.spec", MESSY),
            ],
        );

        let outcome = run(&request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Mode::Check,
        ));

        assert_eq!(outcome.checked, 2);
        let w141: Vec<_> = outcome
            .diagnostics
            .iter()
            .filter(|d| d.is(codes::W141))
            .collect();
        assert_eq!(w141.len(), 1, "{w141:?}");
    }

    #[specforge_test(
        behavior = "load_format_config",
        verify = "a file is formatted with the configuration of its own project"
    )]
    fn a_nested_projects_file_uses_its_own_projects_config() {
        let two = "behavior login \"Login\" {\n  contract \"The system MUST log in\"\n}\n";
        let dir = project_with(
            "{}",
            &[
                (".specforgefmt.toml", "indent_width = 4\n"),
                ("inner/specforge.json", "{}"),
                ("inner/spec/a.spec", two),
            ],
        );
        let file = dir.path().join("inner/spec/a.spec");

        // From the outer project, the inner file keeps its own project's
        // defaults; the outer configuration stops at the inner root.
        let outcome = check_paths(dir.path(), &["inner"]);
        assert_eq!((outcome.checked, outcome.changes.len()), (1, 0));
        // As the editor formats it.
        let doc = document(Place::File(&file), two, None, None);
        assert_eq!(doc.config_source, ConfigSource::Defaults);
        assert!(!doc.changed());
    }

    // --- One document (lsp_format_document, lsp_format_range,
    // lsp_respect_editor_config, format_from_stdin) ---

    use specforge_test_macros::test as specforge_test;

    /// A behavior whose contract is indented by 6.
    const MISINDENTED: &str = "behavior foo \"Foo\" {\n      contract \"does stuff\"\n}\n";
    /// Two blocks: `foo` (whose fields full formatting aligns) and `bar`,
    /// misindented on line 6.
    const TWO_BLOCKS: &str = "behavior foo \"Foo\" {\n  contract \"a\"\n  types [x]\n}\n\nbehavior bar \"Bar\" {\n      contract \"b\"\n}\n";
    /// A canonical block, then a stray `}}}` on line 5.
    const BROKEN: &str = "behavior login \"Login\" {\n  contract \"ok\"\n}\n\n}}}\n";
    /// A behavior indented by 4.
    const FOUR: &str = "behavior login \"Login\" {\n    contract \"The system MUST log in\"\n}\n";

    const EDITOR_4: EditorOptions = EditorOptions {
        tab_size: 4,
        insert_spaces: true,
    };

    /// Whether `edits` are in order and none overlaps or touches the next.
    fn in_order_without_overlap(edits: &[Edit]) -> bool {
        edits.windows(2).all(|w| w[0].range.end < w[1].range.start)
    }

    /// The W142 diagnostics of `doc`.
    fn kept_regions<'d>(doc: &'d FormattedDocument) -> Vec<&'d Diagnostic> {
        doc.diagnostics
            .iter()
            .filter(|d| d.is(codes::W142))
            .collect()
    }

    /// A project (`specforge.json` = `{}`) with `files` written (path, text).
    fn on_disk(files: &[(&str, &str)]) -> tempfile::TempDir {
        project_with("{}", files)
    }

    #[specforge_test(
        behavior = "lsp_format_document",
        verify = "formatting request returns TextEdit list"
    )]
    fn a_misformatted_document_gets_edits() {
        let doc = document(Place::Detached, MISINDENTED, None, None);

        assert!(doc.changed());
        // Only the four extra spaces of line 1, which starts at byte 21.
        assert_eq!(
            doc.edits(),
            [Edit {
                range: 23..27,
                new_text: String::new()
            }]
        );
    }

    #[specforge_test(
        behavior = "lsp_format_document",
        verify = "applying the formatting edits to the document gives the formatted text byte for byte"
    )]
    fn edits_rebuild_the_formatted_text() {
        let blank = |range: Range<usize>| Edit {
            range,
            new_text: String::new(),
        };
        let expected: [(&str, Vec<Edit>); 6] = [
            (
                "behavior a \"A\" {\n\n  contract \"x\"\n}\n",
                vec![blank(17..18)],
            ),
            (
                "\n\n\nbehavior a \"A\" {\n  contract \"x\"\n}\n",
                vec![blank(0..3)],
            ),
            (
                "behavior a \"A\" {\n  contract \"x\"\n}\n\n\n",
                vec![blank(34..36)],
            ),
            (
                "behavior a \"A\" {\n  contract \"x\"\n}",
                vec![Edit {
                    range: 33..33,
                    new_text: "\n".into(),
                }],
            ),
            (
                "behavior a \"A\" {\r\n  contract \"x\"\r\n}\r\n",
                vec![Edit {
                    range: 16..37,
                    new_text: "\n  contract \"x\"\n}\n".into(),
                }],
            ),
            (
                "behavior a \"A\" {\n      contract \"x\"\n}\n",
                vec![blank(19..23)],
            ),
        ];
        for (text, edits) in expected {
            let doc = document(Place::Detached, text, None, None);
            assert_eq!(doc.edits(), edits, "{text:?}");
            assert_eq!(
                edits::tests::apply(text, &doc.edits()),
                doc.formatted,
                "{text:?}"
            );
        }
        // Two blank lines between blocks: which of the two goes is the diff's choice; the text is not.
        let between = "behavior a \"A\" {\n  contract \"x\"\n}\n\n\nbehavior b \"B\" {\n  contract \"y\"\n}\n";
        let doc = document(Place::Detached, between, None, None);
        assert_eq!(edits::tests::apply(between, &doc.edits()), doc.formatted);
        // A multibyte line: offsets count bytes, on char boundaries.
        let text = "behavior foo \"Foo\" {\n      contract \"é\"\n}\n";
        assert_eq!(
            document(Place::Detached, text, None, None).edits(),
            [blank(23..27)]
        );
    }

    #[specforge_test(
        behavior = "lsp_format_document",
        verify = "TextEdit operations in a response do not overlap"
    )]
    fn edits_do_not_overlap() {
        let text = "behavior foo \"Foo\" {\n      contract \"a\"\n      types [x]\n}\n";
        let edits = document(Place::Detached, text, None, None).edits();

        assert!(!edits.is_empty());
        assert!(in_order_without_overlap(&edits), "{edits:?}");
    }

    #[specforge_test(
        behavior = "lsp_format_document",
        verify = "parse errors in document trigger format_with_parse_errors delegation"
    )]
    fn a_parse_error_is_reported_and_kept() {
        let dir = on_disk(&[("spec/x.spec", BROKEN)]);
        let file = dir.path().join("spec/x.spec");

        let doc = document(Place::File(&file), BROKEN, None, None);

        let kept = kept_regions(&doc);
        assert_eq!(kept.len(), 1, "{:?}", doc.diagnostics);
        let span = kept[0].span.as_ref().unwrap();
        assert_eq!(span.file.as_str(), file.display().to_string());
        assert_eq!(span.start_line, 5);
        assert_eq!(doc.formatted.lines().nth(4), Some("}}}"));
        assert!(!doc.complete());
    }

    #[specforge_test(
        behavior = "lsp_format_document",
        verify = "formats document within 50ms for files under 1000 lines"
    )]
    fn formats_a_document_within_50ms() {
        let mut source = String::from("use types/core\n\n");
        for i in 0..50 {
            source.push_str(&format!(
                "behavior b{i} \"Behavior {i}\" {{\n  invariants [a, b]\n  types [x]\n  contract \"thing {i}\"\n  verify unit \"test {i}\"\n}}\n\n"
            ));
        }

        let start = std::time::Instant::now();
        document(Place::Detached, &source, None, None).edits();
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 50,
            "formatting took {}ms",
            elapsed.as_millis()
        );

        // The worst case for the line diff: every line differs.
        let crlf = source.replace('\n', "\r\n");
        let start = std::time::Instant::now();
        document(Place::Detached, &crlf, None, None).edits();
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 50,
            "formatting a CRLF document took {}ms",
            elapsed.as_millis()
        );
    }

    #[specforge_test(
        behavior = "lsp_format_document",
        verify = "LSP Format Document: LSP document formatting holds — document_open, format_config_loaded, textedit_list_returned, cli_parity_enforced, format_complete_emitted"
    )]
    fn document_contract() {
        // document_open: the text is the open document's.
        let dir = on_disk(&[("spec/a.spec", MISINDENTED)]);
        let file = dir.path().join("spec/a.spec");

        let doc = document(Place::File(&file), MISINDENTED, None, Some(EDITOR_4));

        // format_config_loaded: the project's (no file: the defaults), not
        // the editor's.
        assert_eq!(doc.config_source, ConfigSource::Defaults);
        assert_eq!(doc.config, FormatConfig::default());
        // textedit_list_returned: non-overlapping edits.
        let edits = doc.edits();
        assert!(!edits.is_empty());
        assert!(in_order_without_overlap(&edits), "{edits:?}");
        // cli_parity_enforced: what `specforge format` writes for the file.
        let outcome = run(&request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Mode::Check,
        ));
        assert_eq!(outcome.changes[0].after, doc.formatted);
        assert_eq!(
            edits::tests::apply(MISINDENTED, &edits),
            outcome.changes[0].after
        );
        assert!(doc.complete());
    }

    #[specforge_test(
        behavior = "lsp_format_range",
        verify = "range is expanded to block boundaries"
    )]
    fn the_range_widens_to_whole_blocks() {
        let lines = Lines { first: 6, last: 6 };

        let edits = document(Place::Detached, TWO_BLOCKS, Some(lines), None).edits();

        let line = |n: usize| TWO_BLOCKS.match_indices('\n').nth(n - 1).unwrap().0 + 1;
        assert_eq!(
            edits,
            [Edit {
                range: line(6) + 2..line(6) + 6,
                new_text: String::new()
            }]
        );
        // The whole document would also realign `foo` (lines 0–4).
        let full = document(Place::Detached, TWO_BLOCKS, None, None).edits();
        assert!(full.iter().any(|e| e.range.start < line(5)), "{full:?}");
    }

    #[specforge_test(
        behavior = "lsp_format_range",
        verify = "range formatting matches full formatting for affected blocks"
    )]
    fn a_range_formats_its_blocks_as_the_whole_document_does() {
        let range = document(
            Place::Detached,
            TWO_BLOCKS,
            Some(Lines { first: 4, last: 6 }),
            None,
        );
        let full = document(Place::Detached, TWO_BLOCKS, None, None);

        let block = |text: &str| text.lines().skip(4).take(4).collect::<Vec<_>>().join("\n");
        assert_eq!(block(&range.formatted), block(&full.formatted));
    }

    #[specforge_test(
        behavior = "lsp_format_range",
        verify = "parse errors within range are left unchanged per format_with_parse_errors"
    )]
    fn a_parse_error_in_a_range_is_kept() {
        let text = "behavior a \"A\" {\n  contract \"a\"\n}\n\nbehavior b \"B\" {\n  contract \"b\"\n}\n\nbehavior c \"C\" {\n      contract \"c\"\n}\n\n}}}\n";

        let doc = document(
            Place::Detached,
            text,
            Some(Lines { first: 8, last: 12 }),
            None,
        );

        assert_eq!(doc.formatted.lines().nth(12), Some("}}}"));
        assert_eq!(doc.formatted.lines().nth(9), Some("  contract \"c\""));
        let kept = kept_regions(&doc);
        assert_eq!(kept.len(), 1, "{:?}", doc.diagnostics);
        assert_eq!(kept[0].span.as_ref().unwrap().start_line, 13);
    }

    #[specforge_test(
        behavior = "lsp_format_range",
        verify = "formats range within 20ms for ranges under 200 lines"
    )]
    fn formats_a_range_within_20ms() {
        let mut source = String::from("use types/core\n\n");
        for i in 0..20 {
            source.push_str(&format!(
                "behavior b{i} \"Behavior {i}\" {{\n  contract \"thing {i}\"\n}}\n\n"
            ));
        }

        let start = std::time::Instant::now();
        document(
            Place::Detached,
            &source,
            Some(Lines {
                first: 10,
                last: 20,
            }),
            None,
        );
        let elapsed = start.elapsed();

        assert!(
            elapsed.as_millis() < 20,
            "range formatting took {}ms",
            elapsed.as_millis()
        );
    }

    #[specforge_test(
        behavior = "lsp_format_range",
        verify = "LSP Format Range: LSP range formatting holds — document_open, format_config_loaded, range_expanded, textedit_list_returned, full_format_parity, format_complete_emitted"
    )]
    fn range_contract() {
        // document_open, format_config_loaded: a project file, with its
        // project's configuration.
        let dir = on_disk(&[
            (".specforgefmt.toml", "indent_width = 4\n"),
            ("spec/a.spec", TWO_BLOCKS),
        ]);
        let file = dir.path().join("spec/a.spec");
        let lines = Lines { first: 6, last: 6 };

        let range = document(Place::File(&file), TWO_BLOCKS, Some(lines), None);
        let full = document(Place::File(&file), TWO_BLOCKS, None, None);

        assert_eq!(range.config.indent_width, 4);
        // range_expanded, textedit_list_returned: one block's edits, in order.
        let edits = range.edits();
        let line = |n: usize| TWO_BLOCKS.match_indices('\n').nth(n - 1).unwrap().0 + 1;
        assert!(edits.iter().all(|e| e.range.start >= line(5)), "{edits:?}");
        assert!(in_order_without_overlap(&edits), "{edits:?}");
        // full_format_parity: the block reads as full formatting has it.
        let tail = |text: &str| text.lines().skip(5).collect::<Vec<_>>().join("\n");
        assert_eq!(tail(&range.formatted), tail(&full.formatted));
    }

    #[specforge_test(
        behavior = "lsp_respect_editor_config",
        verify = "editor settings are used for a document outside any project"
    )]
    fn editor_settings_apply_outside_a_project() {
        let doc = document(Place::Detached, FOUR, None, Some(EDITOR_4));
        assert_eq!(doc.config.indent_width, 4);
        assert_eq!(doc.config_source, ConfigSource::Editor);
        assert!(doc.edits().is_empty());

        // A file with no specforge.json above it.
        let dir = tempfile::TempDir::new().unwrap();
        let loose = dir.path().join("loose.spec");
        std::fs::write(&loose, FOUR).unwrap();
        let doc = document(Place::File(&loose), FOUR, None, Some(EDITOR_4));
        assert_eq!(doc.config_source, ConfigSource::Editor);
        assert_eq!(doc.config.indent_width, 4);
        assert!(doc.edits().is_empty());
    }

    #[specforge_test(
        behavior = "lsp_respect_editor_config",
        verify = "config file takes precedence over editor settings"
    )]
    fn config_file_takes_precedence_over_editor() {
        let dir = on_disk(&[
            (".specforgefmt.toml", "indent_width = 4\n"),
            ("spec/a.spec", FOUR),
        ]);
        let file = dir.path().join("spec/a.spec");
        let editor = EditorOptions {
            tab_size: 8,
            insert_spaces: true,
        };

        let doc = document(Place::File(&file), FOUR, None, Some(editor));

        let ConfigSource::File(config_file) = &doc.config_source else {
            panic!("{:?}", doc.config_source);
        };
        assert!(config_file.ends_with(".specforgefmt.toml"));
        assert_eq!(doc.config.indent_width, 4);
        assert!(doc.edits().is_empty());
    }

    #[specforge_test(
        behavior = "lsp_respect_editor_config",
        verify = "a project without a config file formats with the defaults, not the editor's settings"
    )]
    fn a_project_without_a_config_file_uses_the_defaults() {
        let dir = on_disk(&[("spec/a.spec", FOUR)]);
        let file = dir.path().join("spec/a.spec");

        let doc = document(Place::File(&file), FOUR, None, Some(EDITOR_4));

        assert_eq!(doc.config_source, ConfigSource::Defaults);
        assert_eq!(doc.config.indent_width, 2);
        assert!(doc.changed(), "reindented to the defaults");
    }

    #[specforge_test(
        behavior = "lsp_respect_editor_config",
        verify = "LSP Respect Editor Config: editor config respect holds — lsp_initialized_fired, config_precedence_enforced, editor_fallback_applied"
    )]
    fn editor_config_contract() {
        // lsp_initialized_fired: the editor's settings are given.
        let editor = Some(EDITOR_4);
        // editor_fallback_applied: outside any project.
        let detached = document(Place::Detached, FOUR, None, editor);
        assert_eq!(detached.config_source, ConfigSource::Editor);
        assert_eq!(detached.config.indent_width, 4);

        // config_precedence_enforced: inside a project, its config file …
        let dir = on_disk(&[
            (".specforgefmt.toml", "indent_width = 3\n"),
            ("spec/a.spec", FOUR),
            ("plain/specforge.json", "{}"),
            ("plain/spec/b.spec", FOUR),
        ]);
        let with_file = document(
            Place::File(&dir.path().join("spec/a.spec")),
            FOUR,
            None,
            editor,
        );
        assert_eq!(with_file.config.indent_width, 3);
        // … else the defaults, never the editor's settings.
        let without = document(
            Place::File(&dir.path().join("plain/spec/b.spec")),
            FOUR,
            None,
            editor,
        );
        assert_eq!(without.config_source, ConfigSource::Defaults);
        assert_eq!(without.config.indent_width, 2);
    }

    #[specforge_test(
        behavior = "format_from_stdin",
        verify = "stdin content is formatted and written to stdout"
    )]
    fn stdin_text_uses_the_config_of_its_directory() {
        let dir = on_disk(&[("spec/sub/.specforgefmt.toml", "indent_width = 4\n")]);
        let sub = dir.path().join("spec/sub");

        let doc = document(
            Place::InProject {
                root: dir.path(),
                dir: &sub,
            },
            MISINDENTED,
            None,
            None,
        );

        assert_eq!(doc.config.indent_width, 4);
        assert!(
            doc.formatted.contains("\n    contract"),
            "{}",
            doc.formatted
        );
    }

    #[specforge_test(
        behavior = "discover_format_targets",
        verify = "a directory named like a .spec file is no target, walked or discovered"
    )]
    fn a_directory_named_like_a_spec_file_is_no_target() {
        let dir = project_with("{}", &[("sub/a.spec", MESSY)]);
        std::fs::create_dir_all(dir.path().join("sub/archive.spec")).unwrap();

        let outcome = run(&request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Mode::Check,
        ));
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        assert_eq!(outcome.checked, 1);

        // The directory walk leaves it out too.
        let walked = check_paths(dir.path(), &["sub"]);
        assert!(walked.failures.is_empty());
        assert_eq!(walked.checked, 1);
    }

    #[specforge_test(
        behavior = "discover_format_targets",
        verify = "a file several arguments reach is formatted once"
    )]
    fn a_file_several_arguments_reach_is_formatted_once() {
        let dir = project_with("{}", &[("a.spec", MESSY)]);
        std::fs::create_dir(dir.path().join("x")).unwrap();

        let twice = check_paths(dir.path(), &["a.spec", "x/../a.spec"]);
        assert_eq!(twice.checked, 1);
        assert_eq!(changed_in(dir.path(), &twice), ["a.spec"]);

        // `Path` equality already folds a redundant component.
        let folded = check_paths(dir.path(), &["", "a.spec"]);
        assert_eq!(folded.checked, 1);
    }

    #[specforge_test(
        behavior = "discover_format_targets",
        verify = "a named directory that is a project formats that project's sources"
    )]
    fn a_walk_reads_a_nested_project_directory_as_a_project() {
        let dir = project_with(
            "{}",
            &[
                ("sub/N/specforge.json", "{}"),
                ("sub/N/n.spec", MESSY),
                ("sub/N/inner/specforge.json", r#"{"exclude": ["drafts"]}"#),
                ("sub/N/inner/drafts/d.spec", MESSY),
            ],
        );

        let outcome = check_paths(dir.path(), &["sub"]);

        // N is a project (its `specforge.json` marks it), so its sources are
        // N's own discovery, which does not stop at `inner`.
        assert_eq!(
            changed_in(dir.path(), &outcome),
            ["sub/N/inner/drafts/d.spec", "sub/N/n.spec"]
        );
    }

    #[ignore = "fixed by T2"]
    #[specforge_test(
        behavior = "discover_format_targets",
        verify = "a project's sources end at a nested project, which a run from the project does not format"
    )]
    fn a_run_from_a_project_leaves_a_nested_project_alone() {
        let dir = project_with(
            "{}",
            &[
                ("a.spec", MESSY),
                ("sub/specforge.json", r#"{"exclude": ["b.spec"]}"#),
                ("sub/b.spec", MESSY),
                ("sub/c.spec", MESSY),
            ],
        );

        assert_eq!(changed(dir.path()), ["a.spec"]);
        let named = check_paths(dir.path(), &[""]);
        assert_eq!(changed_in(dir.path(), &named), ["a.spec"]);
        let nested = check_paths(dir.path(), &["sub"]);
        assert_eq!(changed_in(dir.path(), &nested), ["sub/c.spec"]);
    }

    #[specforge_test(
        behavior = "format_spec_files",
        verify = "the format document names each changed file, each failed file with its kind, and the run's verdict"
    )]
    fn the_document_is_the_run_both_surfaces_print() {
        let dir = project();
        let mut outcome = run(&request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Mode::Check,
        ));

        let plain = serde_json::to_value(outcome.document(false)).unwrap();
        assert_eq!(plain["total_checked"], 2);
        assert_eq!(plain["changed_files"].as_array().unwrap().len(), 2);
        assert_eq!(plain["ok"], false, "a check that finds a change fails");
        assert_eq!(plain["all_clean"], false);
        assert_eq!(plain["check_only"], true);
        assert_eq!(plain["diagnostics"], serde_json::json!([]));
        // No null: what the run did not do is absent.
        for absent in ["diffs", "message", "failed_files", "failures"] {
            assert!(plain.get(absent).is_none(), "{absent}");
        }

        let diffs = serde_json::to_value(outcome.document(true)).unwrap();
        let first = &diffs["diffs"][0];
        assert_eq!(first["before"], MESSY);
        assert_eq!(first["file_path"], plain["changed_files"][0]);
        assert!(first["insertions"].as_u64().unwrap() > 0);

        let locked = dir.path().join("spec/locked.spec");
        outcome.failures.push(Failure::read(
            locked.clone(),
            &std::io::Error::from(std::io::ErrorKind::PermissionDenied),
        ));
        let failed = serde_json::to_value(outcome.document(false)).unwrap();
        let shown = locked.display().to_string();
        assert_eq!(failed["failed_files"], serde_json::json!([shown]));
        assert_eq!(failed["failures"][0]["file"], shown);
        assert_eq!(failed["failures"][0]["operation"], "read");
        assert_eq!(failed["failures"][0]["code"], "permission_denied");
        assert_eq!(failed["message"], failed["failures"][0]["message"]);
    }

    /// [`MESSY`] in canonical form.
    const CANONICAL: &str = "behavior messy \"Messy\" {\n  contract \"The system MUST work\"\n}\n";

    #[specforge_test(
        behavior = "discover_format_targets",
        verify = "a specforge.json that does not say which files are the project's sources refuses the run, and nothing is written"
    )]
    fn a_config_that_hides_the_sources_refuses_the_run() {
        for config in crate::config::testing::HIDES_SOURCES {
            for mode in [Mode::Write, Mode::Check, Mode::Preview] {
                let dir = project_with(config, &[("spec/a.spec", MESSY), ("vendor/v.spec", MESSY)]);
                let before = crate::config::testing::files_under(dir.path());

                let refused = super::run(&Request {
                    root: &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                    paths: &[],
                    mode,
                })
                .unwrap_err();

                let read = specforge_common::read_project_config(dir.path());
                let first = read.problems.iter().find(|p| p.blocks_rewrites()).unwrap();
                assert_eq!(refused.code, "config_invalid", "{config}");
                assert_eq!(refused.message, first.to_string(), "{config}");
                assert_eq!(
                    crate::config::testing::files_under(dir.path()),
                    before,
                    "{config} {mode:?}"
                );
            }
        }
    }

    #[specforge_test(
        behavior = "discover_format_targets",
        verify = "a specforge.json that does not say which files are the project's sources refuses the run, and nothing is written"
    )]
    fn a_nested_projects_config_refuses_the_walk_that_reaches_it() {
        let dir = project_with(
            "{}",
            &[
                ("spec/a.spec", MESSY),
                ("ws/b.spec", MESSY),
                ("ws/p/specforge.json", r#"{"spec_root": "#),
                ("ws/p/spec/p.spec", MESSY),
            ],
        );
        let before = crate::config::testing::files_under(dir.path());

        let refused = super::run(&Request {
            root: &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            paths: &[dir.path().join("ws")],
            mode: Mode::Write,
        })
        .unwrap_err();

        assert_eq!(refused.code, "config_invalid");
        assert!(
            refused.message.contains("ws/p/specforge.json"),
            "{}",
            refused.message
        );
        assert_eq!(crate::config::testing::files_under(dir.path()), before);
    }

    #[specforge_test(
        behavior = "check_formatting",
        verify = "a problem in specforge.json the run goes around is an E069 that fails the check"
    )]
    fn a_problem_the_run_goes_around_is_e069_and_fails_ok() {
        let config = r#"{"spec_root":"spec","exclude":["[", 5],"name":7}"#;
        let dir = project_with(
            config,
            &[("spec/a.spec", CANONICAL), ("vendor/v.spec", MESSY)],
        );
        let root = specforge_common::ProjectRoot::of(dir.path()).unwrap();

        let check = run(&Request {
            root: &root,
            paths: &[],
            mode: Mode::Check,
        });

        let e069: Vec<_> = check
            .diagnostics
            .iter()
            .take_while(|d| d.is(codes::E069))
            .collect();
        assert_eq!(e069.len(), 3, "{:?}", check.diagnostics);
        assert!(
            e069.iter().any(|d| d.message.contains("\"name\"")),
            "{e069:?}"
        );
        assert!(!check.ok());
        assert!(check.clean(), "formatting itself is clean");

        let write = run(&Request {
            root: &root,
            paths: &[],
            mode: Mode::Write,
        });
        assert!(!write.ok());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("vendor/v.spec")).unwrap(),
            MESSY,
            "outside spec_root"
        );
    }

    #[specforge_test(
        behavior = "discover_format_targets",
        verify = "a named file is formatted whatever specforge.json says"
    )]
    fn a_named_file_reads_no_config() {
        let dir = project_with(r#"{"spec_root": "#, &[("spec/a.spec", MESSY)]);
        let file = dir.path().join("spec/a.spec");

        let outcome = super::run(&Request {
            root: &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            paths: std::slice::from_ref(&file),
            mode: Mode::Write,
        })
        .unwrap();

        assert!(outcome.configured(), "{:?}", outcome.diagnostics);
        assert_eq!(outcome.changes.len(), 1);
        assert_ne!(std::fs::read_to_string(file).unwrap(), MESSY);
    }
}
