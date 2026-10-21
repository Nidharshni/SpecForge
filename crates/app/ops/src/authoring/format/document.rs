//! One document formatted with the configuration and the field forms of the project it belongs to (ADR 0021 D2): the
//! LSP's formatting requests, `format --stdin`, and each file of a run.

use specforge_common::{Diagnostic, ProjectRoot, Sym, codes};
use specforge_formatter::{FormatConfig, config, format_range, format_source};
use specforge_graph::FieldForms;
use specforge_project::{Environment, RuntimeSource};
use specforge_registry::{Disk, PlainDisk};
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

/// Where a format finds the field forms each document is written in (ADR 0147 D6): the forms of the
/// document's own project, found from its place as its configuration is ([`Place`]); none outside a project.
/// A caller lends the project it already holds, so a document of that project is written in the forms the caller
/// has; any other project's extensions load through `extensions`, once per run or document. Beside the
/// [`Caller`](crate::Caller), not in it (CONTEXT "Caller": the runtime is passed beside it).
#[derive(Clone)]
pub struct Forms<'a> {
    /// The project the caller holds and its forms: lent only when its environment was loaded at a project root.
    held: Option<(&'a ProjectRoot, &'a FieldForms)>,
    /// Where another project's extensions load to read its forms; `Fixed(None)` writes its values as they are.
    extensions: RuntimeSource,
}

impl<'a> Forms<'a> {
    /// No forms: every value is written as it is (a typed edit's and init's layout, which read only the configuration).
    pub fn none() -> Self {
        Forms {
            held: None,
            extensions: RuntimeSource::Fixed(None),
        }
    }

    /// Each project's forms loaded through `extensions` (the CLI, which holds no project).
    pub fn loading(extensions: RuntimeSource) -> Self {
        Forms {
            held: None,
            extensions,
        }
    }

    /// `held`'s forms for the documents of its project, any other project's loaded through `extensions` (the LSP,
    /// MCP). An environment with no project root (empty, assembled in memory) lends nothing.
    pub fn lending(held: &'a Environment, extensions: RuntimeSource) -> Self {
        Forms {
            held: held
                .project_root()
                .map(|root| (root, held.registries().field_forms())),
            extensions,
        }
    }
}

/// Format `text`, which belongs at `place`: the whole document, or the
/// blocks `lines` touch. Inside a project its `.specforgefmt.toml` (else the
/// defaults) decides, never `editor`; outside one, `editor` (else the
/// defaults). Values are written in the forms of the document's own project,
/// found from `place` as its configuration is; `forms` says which project the
/// caller lends and where another project's extensions load (ADR 0147 D6).
pub fn document<'a>(
    place: Place<'_>,
    text: &'a str,
    lines: Option<Lines>,
    editor: Option<EditorOptions>,
    forms: &Forms<'_>,
) -> FormattedDocument<'a> {
    // The LSP and `format --stdin` are unbounded (ADR 0092 D6): they read the file system as the user sees it.
    let mut configs = Configs::new(forms.clone(), &PlainDisk);
    let (file, resolved) = configs.place(place, editor);
    format_text(resolved, text, lines, file, configs.forms(place))
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

/// The configurations of one run, typed-edit plan or init, read through one disk: one project lookup per
/// directory, one walk per directory and project, one read per configuration file. A file's W141 goes out the
/// first time it is used. The disk is the caller's bound (ADR 0169): a name it refuses is a configuration that
/// cannot be read.
pub(crate) struct Configs<'d> {
    disk: &'d dyn Disk,
    /// Where the field forms of each project come from.
    forms: Forms<'d>,
    /// The forms of the projects loaded so far, by canonical root.
    loaded: HashMap<PathBuf, FieldForms>,
    none: FieldForms,
    projects: HashMap<PathBuf, Option<ProjectRoot>>,
    found: HashMap<(PathBuf, PathBuf), Option<PathBuf>>,
    read: HashMap<PathBuf, FormatConfig>,
}

impl<'d> Configs<'d> {
    pub(crate) fn new(forms: Forms<'d>, disk: &'d dyn Disk) -> Self {
        Configs {
            disk,
            forms,
            loaded: HashMap::new(),
            none: FieldForms::default(),
            projects: HashMap::new(),
            found: HashMap::new(),
            read: HashMap::new(),
        }
    }

    /// The configuration [`document`] formats `place` with, its W141 left out (a typed edit's and init's layout).
    pub(crate) fn layout(&mut self, place: Place<'_>) -> FormatConfig {
        self.place(place, None).1.config
    }

    /// The file `place` names (if it is one) and its resolved configuration.
    fn place<'p>(
        &mut self,
        place: Place<'p>,
        editor: Option<EditorOptions>,
    ) -> (Option<&'p Path>, Resolved) {
        match place {
            Place::File(path) => (Some(path), self.for_file(path, editor)),
            Place::InProject { root, dir } => (None, self.resolve(dir, root)),
            Place::Detached => (None, Resolved::editor(editor)),
        }
    }

    /// The field forms of the project `place` is in, as its configuration is found: the caller's when it holds that
    /// project, else loaded once per run; none outside a project.
    pub(super) fn forms(&mut self, place: Place<'_>) -> &FieldForms {
        let root = match place {
            Place::File(path) => self.project_of(&directory_of(path)),
            Place::InProject { root, .. } => self.project_of(root),
            Place::Detached => None,
        };
        match root {
            Some(root) => self.forms_of(&root),
            None => &self.none,
        }
    }

    /// The field forms of the project at `root`.
    fn forms_of(&mut self, root: &ProjectRoot) -> &FieldForms {
        if let Some((held, forms)) = self.forms.held
            && held.canonical() == root.canonical()
        {
            return forms;
        }
        if matches!(self.forms.extensions, RuntimeSource::Fixed(None)) {
            return &self.none;
        }
        let extensions = &self.forms.extensions;
        self.loaded
            .entry(root.canonical().to_path_buf())
            .or_insert_with(|| {
                Environment::load_from(root, extensions)
                    .registries()
                    .field_forms()
                    .clone()
            })
    }

    /// The configuration of `file`: its own project's (discovery from its
    /// directory up to that project's root), else outside any project the
    /// editor's.
    pub(super) fn for_file(&mut self, file: &Path, editor: Option<EditorOptions>) -> Resolved {
        let dir = directory_of(file);
        match self.project_of(&dir) {
            Some(root) => self.resolve(&dir, root.dir()),
            None => Resolved::editor(editor),
        }
    }

    /// The root of the project `dir` is in.
    fn project_of(&mut self, dir: &Path) -> Option<ProjectRoot> {
        self.projects
            .entry(dir.to_path_buf())
            .or_insert_with(|| ProjectRoot::enclosing(dir))
            .clone()
    }

    /// The configuration of a document in `dir`, in the project at `root`: the nearest configuration up to
    /// `root`, else the defaults. Its W141 only on the lookup that read it.
    fn resolve(&mut self, dir: &Path, root: &Path) -> Resolved {
        let key = (dir.to_path_buf(), root.to_path_buf());
        let (file, diagnostics) = match self.found.get(&key) {
            Some(file) => (file.clone(), Vec::new()),
            None => {
                let (file, diagnostics) = self.nearest(dir, root);
                self.found.insert(key, file.clone());
                (file, diagnostics)
            }
        };
        let Some(file) = file else {
            return Resolved {
                config: FormatConfig::default(),
                source: ConfigSource::Defaults,
                diagnostics: Vec::new(),
            };
        };
        let config = self.read.get(&file).cloned().unwrap_or_default();
        Resolved {
            config,
            source: ConfigSource::File(file),
            diagnostics,
        }
    }

    /// Walk from `dir` up to `root` (both canonical where they exist; never past `root`), asking the disk for
    /// the configuration file in each directory. Only `NotFound` is absence: any other answer (the bytes, a
    /// refused landing, a second name, a permission) is the nearest configuration, parsed or reported once.
    fn nearest(&mut self, dir: &Path, root: &Path) -> (Option<PathBuf>, Vec<Diagnostic>) {
        let real = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let (root, mut dir) = (real(root), real(dir));
        if !dir.starts_with(&root) {
            return (None, Vec::new());
        }
        loop {
            let file = dir.join(config::FILE);
            if self.read.contains_key(&file) {
                return (Some(file), Vec::new());
            }
            match self.disk.read(&file) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                read => {
                    let (config, diagnostics) = config::parse(&file, read);
                    self.read.insert(file.clone(), config);
                    return (Some(file), diagnostics);
                }
            }
            if dir == root || !dir.pop() {
                return (None, Vec::new());
            }
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
