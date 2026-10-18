//! One document formatted with the configuration of where it belongs (ADR 0021 D2): the LSP's
//! formatting requests, `format --stdin`, and each file of a run.

use specforge_common::{Diagnostic, ProjectRoot, Sym, codes};
use specforge_formatter::config::{find_config_path, read_config_file};
use specforge_formatter::{FormatConfig, format_range, format_source};
use specforge_graph::FieldForms;
use specforge_project::{Environment, SharedRuntime};
use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};

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
        super::edits::between(self.source, &self.formatted)
    }

    /// Whether every region parsed (no W142): the text is in canonical form
    /// when this holds and nothing changed.
    pub fn complete(&self) -> bool {
        !self.diagnostics.iter().any(|d| d.is(codes::W142))
    }
}

/// The field forms of the project at `root`: its extensions loaded into
/// `runtime` (none without one), their declarations read (ADR 0147).
/// A project whose extensions do not load has the forms of those that do.
pub fn forms(root: &ProjectRoot, runtime: Option<SharedRuntime>) -> FieldForms {
    Environment::load(root, runtime)
        .registries()
        .field_forms()
        .clone()
}

/// Format `text`, which belongs at `place`: the whole document, or the
/// blocks `lines` touch. Inside a project its `.specforgefmt.toml` (else the
/// defaults) decides, never `editor`; outside one, `editor` (else the
/// defaults). Values are written in the form `forms` gives their fields
/// (ADR 0147).
pub fn document<'a>(
    place: Place<'_>,
    text: &'a str,
    lines: Option<Lines>,
    editor: Option<EditorOptions>,
    forms: &FieldForms,
) -> FormattedDocument<'a> {
    let mut configs = Configs::new(None);
    let (file, resolved) = match place {
        Place::File(path) => (Some(path), configs.for_file(path, editor)),
        Place::InProject { root, dir } => (None, configs.resolve(dir, root)),
        Place::Detached => (None, Resolved::editor(editor)),
    };
    format_text(resolved, text, lines, file, forms)
}

/// The directory `file` is in (`.` for a bare file name).
pub(super) fn directory_of(file: &Path) -> PathBuf {
    match file.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// A document's configuration, where it came from, and what reading it
/// reported (only the first time a run reads that file).
pub(super) struct Resolved {
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
pub(super) struct Configs {
    /// Where each project's extensions load, to read its field forms; none
    /// formats layout only.
    runtime: Option<SharedRuntime>,
    forms: HashMap<PathBuf, FieldForms>,
    none: FieldForms,
    projects: HashMap<PathBuf, Option<PathBuf>>,
    found: HashMap<(PathBuf, PathBuf), Option<PathBuf>>,
    read: HashMap<PathBuf, FormatConfig>,
}

impl Configs {
    pub(super) fn new(runtime: Option<SharedRuntime>) -> Self {
        Configs {
            runtime,
            forms: HashMap::new(),
            none: FieldForms::default(),
            projects: HashMap::new(),
            found: HashMap::new(),
            read: HashMap::new(),
        }
    }

    /// The field forms of `file`'s project, loaded once per run; none
    /// outside a project or without a runtime.
    pub(super) fn forms_for_file(&mut self, file: &Path) -> &FieldForms {
        let dir = directory_of(file);
        let Some(runtime) = self.runtime.clone() else {
            return &self.none;
        };
        let Some(root) = self.project_of(&dir) else {
            return &self.none;
        };
        self.forms.entry(root.clone()).or_insert_with(|| {
            ProjectRoot::enclosing(&dir)
                .map_or_else(FieldForms::default, |root| forms(&root, Some(runtime)))
        })
    }

    /// The configuration of `file`: its own project's (discovery from its
    /// directory up to that project's root), else outside any project the
    /// editor's.
    pub(super) fn for_file(&mut self, file: &Path, editor: Option<EditorOptions>) -> Resolved {
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
pub(super) fn format_text<'a>(
    resolved: Resolved,
    text: &'a str,
    lines: Option<Lines>,
    file: Option<&Path>,
    forms: &FieldForms,
) -> FormattedDocument<'a> {
    let result = match lines {
        // An empty document has no lines to widen.
        Some(lines) if !text.is_empty() => {
            format_range(text, lines.first, lines.last, &resolved.config, forms)
        }
        _ => format_source(text, &resolved.config, forms),
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
