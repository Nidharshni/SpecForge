//! Turning a project's `extensions` entries into loaded extensions and
//! their declarations (ADR 0028): a builtin from its embedded bytes, an
//! installed extension from its pinned module, a `.wasm` file entry from its
//! file under the name it declares. What does not load is a typed
//! [`LoadFailure`] on its entry with one diagnostic; the runtime keeps no
//! load state. Each entry is loaded by one step: its bytes read once and
//! loaded under its key, its declaration read once, the name it declares
//! admitted or refused; a refused entry leaves nothing loaded.

use std::path::{Path, PathBuf};

use specforge_common::{Diagnostic, ExtensionEntry, codes};
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry::Disk;
use specforge_wasm::WasmRuntime;
use specforge_wasm::protocol::load_declaration;

use crate::Installed;
use crate::lock::LockFileEntry;
use crate::module::Module;
use crate::resolve::{Declared, Origin, Resolved};

/// The builtin extensions the host embeds: name and component bytes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Builtins<'a>(pub &'a [(&'a str, &'a [u8])]);

impl<'a> Builtins<'a> {
    /// No builtin extension (a host that embeds none).
    pub const fn none() -> Self {
        Builtins(&[])
    }

    /// The builtins' names, in embedded order.
    pub fn names(self) -> impl Iterator<Item = &'a str> + 'a {
        self.0.iter().map(|(name, _)| *name)
    }

    /// The embedded bytes of the builtin `name` (`@specforge/product`).
    pub fn get(&self, name: &str) -> Option<&'a [u8]> {
        self.0
            .iter()
            .find(|(builtin, _)| *builtin == name)
            .map(|(_, bytes)| *bytes)
    }

    /// Whether `name` is a builtin.
    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }
}

/// What loading a project's `extensions` entries did.
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    /// What each entry enabled, in `entries` order.
    pub enabled: Vec<EnabledExtension>,
    /// Each loaded extension's declaration, once, in the order the
    /// entries name them: the registry build's input, which it puts in
    /// load order (ADR 0041).
    pub declarations: Vec<ExtensionDeclaration>,
}

/// What one `specforge.json` `extensions` entry enables, as the load left it.
#[derive(Debug, Clone, PartialEq)]
pub struct EnabledExtension {
    /// The entry as specforge.json writes it (trimmed).
    pub entry: String,
    /// A named entry's name; a `.wasm` file entry's declared name once it
    /// loaded, else the name written before `=`, else the path.
    pub name: String,
    /// Where the load takes its extension from ([`Resolution::origin`](crate::Resolution::origin)):
    /// a repeated entry's is the one it repeats'. It does not depend on whether the entry loaded.
    pub origin: Origin,
    /// Whether it loaded its extension: the registry build holds its declaration under `name`.
    /// False when it failed, repeats another entry, or nothing was loaded.
    pub loaded: bool,
    /// Why it did not load. `None`: it loaded, it repeats another entry, or nothing was loaded
    /// ([`EnabledExtension::unloaded`]).
    pub failure: Option<LoadFailure>,
    /// What it reported though it loaded: W149 (its lock entry pins no
    /// hash), then W153 and W138 as its declaration was read. Empty when it
    /// did not load or nothing was loaded.
    pub warnings: Vec<Diagnostic>,
}

/// What one diagnostic of an extension load is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadSubject {
    /// `specforge.lock`, which cannot be read (its E033).
    Lock,
    /// The `extensions` entry at this index.
    Entry(usize),
}

impl EnabledExtension {
    /// The path a `.wasm` file entry names, as written: its origin's.
    pub fn file(&self) -> Option<&str> {
        match &self.origin {
            Origin::File { path } => Some(path),
            _ => None,
        }
    }

    /// Of the entries that enable one extension, in entry order, the one whose origin is the
    /// extension's: the entry that loaded it, else the first the load tried (a named entry before
    /// a `.wasm` file entry, `Resolution::in_load_order`'s order). A repeat never wins over its
    /// first: both have the same keys and the first comes earlier. `None` for no entry.
    pub fn loader<'e>(enabling: &[&'e EnabledExtension]) -> Option<&'e EnabledExtension> {
        enabling
            .iter()
            .copied()
            .min_by_key(|e| (!e.loaded, e.file().is_some()))
    }

    /// What `entry` names when nothing is loaded (no runtime), from `origin`.
    pub fn unloaded(entry: &str, origin: Origin) -> Self {
        match ExtensionEntry::parse(entry) {
            ExtensionEntry::Named(name) => EnabledExtension {
                entry: entry.trim().to_string(),
                name: name.to_string(),
                origin,
                loaded: false,
                failure: None,
                warnings: Vec::new(),
            },
            ExtensionEntry::File { name, path } => EnabledExtension {
                entry: entry.trim().to_string(),
                name: name.unwrap_or(path).to_string(),
                origin,
                loaded: false,
                failure: None,
                warnings: Vec::new(),
            },
        }
    }
}

/// Why an entry did not load: the problem, and the one diagnostic that
/// reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadFailure {
    pub problem: LoadProblem,
    pub diagnostic: Diagnostic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadProblem {
    /// E028: no lock entry.
    NotInstalled,
    /// E028: specforge.lock is there and can't be read.
    LockUnreadable,
    /// E072: the name is no package name, so no module can be installed
    /// under it.
    NotAPackageName { reason: String },
    /// E028: locked, no module.
    ModuleMissing { path: PathBuf },
    /// E028: the module can't be read.
    ModuleUnreadable { path: PathBuf, reason: String },
    /// E070: the module is not the one its lock entry pins.
    Changed { locked: String, actual: String },
    /// E070: the module declares another extension than its lock entry.
    NotItsLockEntry { declared: String },
    /// E028: the bytes are not a component the runtime loads.
    NotAComponent { reason: String },
    /// E028: its declaration can't be read (handshake, describe, protocol
    /// major).
    NoDeclaration { reason: String },
    /// E028, file entry: the file does not exist.
    FileMissing { path: PathBuf },
    /// E028, file entry: declares another name than the one written before
    /// `=`.
    NotTheNameWritten { declared: String, written: String },
    /// E028, file entry: declares the name of a builtin this specforge embeds; only the builtin
    /// loads under it.
    BuiltinsName { declared: String },
    /// E028, file entry: declares an extension another entry already loads.
    AlreadyLoaded { declared: String },
}

impl LoadProblem {
    /// A problem [`Installed::health`] reports for the lock entry too
    /// (`ModuleMissing`, `Changed`): doctor lists it once.
    pub fn is_module_health(&self) -> bool {
        matches!(
            self,
            LoadProblem::ModuleMissing { .. } | LoadProblem::Changed { .. }
        )
    }
}

/// What a problem is about, for its message.
enum Subject<'a> {
    /// An extension named by an entry.
    Named(&'a str),
    /// A `.wasm` file entry: the entry as written (trimmed), the file it
    /// resolves to, the path as written and the name written before `=`.
    File {
        key: &'a str,
        file: &'a Path,
        path: &'a str,
    },
}

impl Subject<'_> {
    /// An E028 about the entry for a problem the per-entry step finds, in
    /// the same words for every kind of entry: `what` says what failed.
    fn says(&self, what: String) -> Diagnostic {
        match self {
            Subject::Named(name) => {
                Diagnostic::new(codes::E028, format!("extension '{name}' {what}"))
            }
            Subject::File { key, file, .. } => Diagnostic::new(
                codes::E028,
                format!(
                    "extension entry '{key}' in specforge.json: {} {what}",
                    file.display()
                ),
            )
            .with_suggestion("build it with specforge-extension-sdk for wasm32-wasip2"),
        }
    }
}

impl LoadProblem {
    /// The one diagnostic that reports this problem for `subject`.
    fn diagnostic(&self, subject: &Subject, installed: &Installed) -> Diagnostic {
        match self {
            LoadProblem::NotAComponent { reason } => {
                return subject.says(format!("does not load as an extension component: {reason}"));
            }
            LoadProblem::NoDeclaration { reason } => {
                return subject.says(format!("has no declaration the host can read: {reason}"));
            }
            _ => {}
        }
        match subject {
            Subject::Named(name) => self.named(name, installed),
            Subject::File { key, file, path } => self.file(key, file, path),
        }
    }

    fn named(&self, name: &str, installed: &Installed) -> Diagnostic {
        match self {
            LoadProblem::NotInstalled => Diagnostic::new(
                codes::E028,
                format!(
                    "extension '{name}' is enabled in specforge.json but not installed (no specforge.lock entry)"
                ),
            )
            .with_suggestion(format!("install it with: specforge add {name}")),
            LoadProblem::LockUnreadable => Diagnostic::new(
                codes::E028,
                format!(
                    "extension '{name}' is enabled in specforge.json but not loaded: specforge.lock can't be read"
                ),
            )
            .with_suggestion(format!(
                "fix specforge.lock, or delete it and install the extension again: specforge add {name}"
            )),
            LoadProblem::NotAPackageName { reason } => {
                specforge_common::package::invalid(reason)
            }
            LoadProblem::ModuleMissing { path } => Diagnostic::new(
                codes::E028,
                format!(
                    "extension '{name}': .wasm binary not found at '{}'",
                    path.display()
                ),
            )
            .with_suggestion(format!(
                "install the extension with: {}",
                installed.reinstall(name)
            )),
            LoadProblem::ModuleUnreadable { path, reason } => Diagnostic::new(
                codes::E028,
                format!(
                    "extension '{name}': cannot read .wasm binary at '{}': {reason}",
                    path.display()
                ),
            ),
            LoadProblem::Changed { locked, actual } => Diagnostic::new(
                codes::E070,
                format!(
                    "integrity mismatch for '{name}': lockfile records hash {locked} but the installed binary is {actual}"
                ),
            )
            .with_suggestion(format!(
                "the installed binary changed after install — re-install it: {}",
                installed.reinstall(name)
            )),
            LoadProblem::NotItsLockEntry { declared } => Diagnostic::new(
                codes::E070,
                format!(
                    "extension '{name}': the installed binary declares '{declared}', not the extension its lock entry names"
                ),
            )
            .with_suggestion(format!(
                "re-install it: {}",
                installed.reinstall(name)
            )),
            LoadProblem::FileMissing { .. }
            | LoadProblem::NotTheNameWritten { .. }
            | LoadProblem::BuiltinsName { .. }
            | LoadProblem::AlreadyLoaded { .. }
            | LoadProblem::NotAComponent { .. }
            | LoadProblem::NoDeclaration { .. } => {
                unreachable!("a named entry names no file, or the step's problem")
            }
        }
    }

    fn file(&self, key: &str, file: &Path, path: &str) -> Diagnostic {
        let refused = |message: String, suggestion: String| {
            Diagnostic::new(
                codes::E028,
                format!("extension entry '{key}' in specforge.json: {message}"),
            )
            .with_suggestion(suggestion)
        };
        match self {
            LoadProblem::FileMissing { path: missing } => refused(
                format!("the file {} does not exist", missing.display()),
                "build the extension, or correct the path (a relative path is relative to the project root)"
                    .to_string(),
            ),
            LoadProblem::NotTheNameWritten { declared, written } => refused(
                format!("{} declares '{declared}', not '{written}'", file.display()),
                format!("write \"{declared}={path}\", or just \"{path}\""),
            ),
            LoadProblem::BuiltinsName { declared } => refused(
                format!(
                    "{} declares '{declared}', the name of a builtin extension: only the builtin loads under it",
                    file.display()
                ),
                format!(
                    "write \"{declared}\" in place of this entry to enable the builtin, or rebuild the extension under a name of its own"
                ),
            ),
            LoadProblem::AlreadyLoaded { declared } => refused(
                format!(
                    "{} declares '{declared}', and '{declared}' is already loaded by another entry",
                    file.display()
                ),
                "remove one of the two entries".to_string(),
            ),
            LoadProblem::ModuleUnreadable { reason, .. } => refused(
                format!("{} cannot be read: {reason}", file.display()),
                "make the file readable, or correct the path (a relative path is relative to the project root)"
                    .to_string(),
            ),
            LoadProblem::NotInstalled
            | LoadProblem::LockUnreadable
            | LoadProblem::NotAPackageName { .. }
            | LoadProblem::ModuleMissing { .. }
            | LoadProblem::Changed { .. }
            | LoadProblem::NotItsLockEntry { .. }
            | LoadProblem::NotAComponent { .. }
            | LoadProblem::NoDeclaration { .. } => {
                unreachable!("a file entry is not installed, or the step's problem")
            }
        }
    }
}

/// Who a resolved entry's diagnostics are about.
fn subject<'a>(resolved: &'a Resolved) -> Subject<'a> {
    match resolved {
        Resolved::Builtin { name, .. }
        | Resolved::Locked { name, .. }
        | Resolved::Unloadable { name, .. } => Subject::Named(name),
        Resolved::File(file) => Subject::File {
            key: file.key,
            file: &file.file,
            path: file.path,
        },
        Resolved::Again { .. } => unreachable!("left out of the load order"),
    }
}

/// What one entry that loaded brings.
struct Done {
    declaration: ExtensionDeclaration,
    /// What the entry itself reports although it loaded (W149).
    notices: Vec<Diagnostic>,
    /// W153 and W138, in the order they were read.
    warnings: Vec<Diagnostic>,
}

/// What an entry left in the load.
enum Slot {
    /// Not loaded again: another entry enables the same extension.
    Skipped,
    Loaded(Box<Done>),
    Failed,
}

impl Installed {
    /// What `entries` enable when nothing loads (no runtime): each entry
    /// [`EnabledExtension::unloaded`] with the origin [`Installed::load`] would record, no
    /// declaration, no diagnostic. Reads nothing.
    pub fn unloaded(&self, entries: &[String], builtins: &Builtins) -> Loaded {
        Loaded {
            enabled: self.resolve(entries, builtins).unloaded(entries),
            declarations: Vec::new(),
        }
    }

    /// Load what `entries` enable into `runtime` and read each extension's
    /// declaration once:
    /// - a builtin from its embedded bytes;
    /// - a named entry from its installed module, only when the module's
    ///   SHA-256 is the one its lock entry pins;
    /// - a `.wasm` file entry from its file, under the name it declares,
    ///   after every named entry (one that declares a name already loaded
    ///   is refused).
    ///
    /// The bytes compiled are the bytes hashed. Never fails: what does not
    /// load is a [`LoadFailure`] on its entry with its diagnostic, and what an
    /// entry reports though it loaded is on the entry too
    /// ([`Installed::load_diagnostics`] orders them).
    pub fn load(
        &self,
        entries: &[String],
        builtins: &Builtins,
        runtime: &dyn WasmRuntime,
        disk: &dyn Disk,
    ) -> Loaded {
        // Named entries first: a `.wasm` file that declares a name they
        // loaded is refused, not swapped in. `resolve` decides the order.
        let resolution = self.resolve(entries, builtins);
        let mut enabled = resolution.unloaded(entries);
        let mut slots: Vec<Slot> = entries.iter().map(|_| Slot::Skipped).collect();
        #[expect(
            clippy::result_large_err,
            reason = "the walk's closures answer the LoadFailure the entry carries (see read_entry)"
        )]
        let outcomes = resolution.walk(
            |_, resolved| self.read_entry(resolved, runtime, disk),
            |resolved, done, verdict| self.settle_entry(resolved, done, verdict, runtime),
        );
        for (i, outcome) in outcomes {
            match outcome {
                Ok((name, done)) => {
                    if matches!(resolution.entries()[i], Resolved::File(_)) {
                        enabled[i].name = name;
                    }
                    enabled[i].loaded = true;
                    slots[i] = Slot::Loaded(Box::new(done));
                }
                Err(failure) => {
                    enabled[i].failure = Some(failure);
                    slots[i] = Slot::Failed;
                }
            }
        }
        // A repeated `.wasm` file entry takes its first's declared name.
        for (i, resolved) in resolution.entries().iter().enumerate() {
            if let Resolved::Again { first } = resolved
                && enabled[*first].file().is_some()
            {
                enabled[i].name = enabled[*first].name.clone();
            }
        }

        let mut declarations = Vec::new();
        for (i, slot) in slots.into_iter().enumerate() {
            if let Slot::Loaded(done) = slot {
                enabled[i].warnings = done.notices.into_iter().chain(done.warnings).collect();
                declarations.push(done.declaration);
            }
        }
        Loaded {
            enabled,
            declarations,
        }
    }

    /// What the load of `enabled` (as [`Installed::load`] left them, in entry
    /// order) reports, in its one order: entry by entry, its failure's
    /// diagnostic or its warnings; the lock's E033 once, just before the
    /// first entry it left unloaded. Reads nothing.
    pub fn load_diagnostics<'a>(
        &'a self,
        enabled: &'a [EnabledExtension],
    ) -> Vec<(LoadSubject, &'a Diagnostic)> {
        let mut reported = Vec::new();
        let mut lock_reported = false;
        for (i, entry) in enabled.iter().enumerate() {
            match &entry.failure {
                Some(failure) => {
                    if failure.problem == LoadProblem::LockUnreadable
                        && !std::mem::replace(&mut lock_reported, true)
                        && let Some(problem) = self.lock.problem()
                    {
                        reported.push((LoadSubject::Lock, problem));
                    }
                    reported.push((LoadSubject::Entry(i), &failure.diagnostic));
                }
                None => reported.extend(entry.warnings.iter().map(|w| (LoadSubject::Entry(i), w))),
            }
        }
        reported
    }

    /// `problem` as the failure of the entry `subject` names.
    fn failure(&self, subject: &Subject, problem: LoadProblem) -> LoadFailure {
        LoadFailure {
            diagnostic: problem.diagnostic(subject, self),
            problem,
        }
    }

    /// The module at `path`, read once and never statted first: the crate's
    /// one module read. A file that is not there is `missing(path)`; any
    /// other read error (a permission, a directory) is `ModuleUnreadable`.
    pub(crate) fn read_module(
        &self,
        disk: &dyn Disk,
        path: &Path,
        missing: impl FnOnce(PathBuf) -> LoadProblem,
    ) -> Result<Module, LoadProblem> {
        disk.read(path).map(Module::new).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                missing(path.to_path_buf())
            } else {
                LoadProblem::ModuleUnreadable {
                    path: path.to_path_buf(),
                    reason: error.to_string(),
                }
            }
        })
    }

    /// `entry`'s module, read once, when it is the one the entry pins.
    pub(crate) fn check_module(
        &self,
        entry: &LockFileEntry,
        disk: &dyn Disk,
    ) -> Result<Module, LoadProblem> {
        let module = self.read_module(disk, &self.module_path(&entry.name), |path| {
            LoadProblem::ModuleMissing { path }
        })?;
        if !entry.wasm_hash.is_empty() && entry.wasm_hash != module.digest() {
            return Err(LoadProblem::Changed {
                locked: entry.wasm_hash.clone(),
                actual: module.digest().to_string(),
            });
        }
        Ok(module)
    }

    /// Read one resolved entry: its bytes once, loaded under its key, its declaration once. Returns
    /// the name it declares with what it brings; [`Installed::settle_entry`] admits or refuses it
    /// (`Resolved::admit`, by [`Resolution::walk`]).
    ///
    /// When it returns `Err`, nothing this call loaded stays loaded.
    #[expect(
        clippy::result_large_err,
        reason = "Failed<Diagnostic> / LoadFailure carry the whole diagnostic the surfaces present (clippy.toml admits a bare Diagnostic only)"
    )]
    fn read_entry(
        &self,
        resolved: &Resolved,
        runtime: &dyn WasmRuntime,
        disk: &dyn Disk,
    ) -> Result<(Declared, Done), LoadFailure> {
        let subject = subject(resolved);
        let fail = |problem: LoadProblem| self.failure(&subject, problem);
        let mut notices = Vec::new();
        let module;
        let (key, bytes): (&str, &[u8]) = match resolved {
            Resolved::Builtin { name, bytes } => (name, bytes),
            Resolved::Locked { name, entry } => {
                module = self.check_module(entry, disk).map_err(fail)?;
                if entry.wasm_hash.is_empty() {
                    notices.push(
                        Diagnostic::new(
                            codes::W149,
                            format!(
                                "extension '{name}': its lock entry pins no hash, so its binary was loaded without being checked"
                            ),
                        )
                        .with_suggestion(format!(
                            "pin it by installing it again: {}",
                            self.reinstall(name)
                        )),
                    );
                }
                (name, module.bytes())
            }
            Resolved::Unloadable { problem, .. } => return Err(fail(problem.clone())),
            Resolved::File(file) => {
                module = self
                    .read_module(disk, &file.file, |path| LoadProblem::FileMissing { path })
                    .map_err(fail)?;
                (file.key, module.bytes())
            }
            Resolved::Again { .. } => unreachable!("left out of the load order"),
        };
        runtime
            .load(key, bytes)
            .map_err(|reason| fail(LoadProblem::NotAComponent { reason }))?;
        let read = load_declaration(runtime, key).map_err(|error| {
            runtime.unload(key);
            fail(LoadProblem::NoDeclaration {
                reason: error.to_string(),
            })
        })?;
        Ok((
            Declared::of(&read.declaration),
            Done {
                declaration: read.declaration,
                notices,
                warnings: read.warnings,
            },
        ))
    }

    /// Settle one entry [`Installed::read_entry`] loaded under its key, by the walk's verdict: the
    /// name it loads under (a named entry's name; a `.wasm` file entry's declared name), renamed
    /// from the key when they differ, or the problem refusing it, which leaves nothing loaded
    /// under the key.
    #[expect(
        clippy::result_large_err,
        reason = "Failed<Diagnostic> / LoadFailure carry the whole diagnostic the surfaces present (clippy.toml admits a bare Diagnostic only)"
    )]
    fn settle_entry(
        &self,
        resolved: &Resolved,
        done: Done,
        verdict: Result<&str, LoadProblem>,
        runtime: &dyn WasmRuntime,
    ) -> Result<(String, Done), LoadFailure> {
        let key = match resolved {
            Resolved::Builtin { name, .. } | Resolved::Locked { name, .. } => name,
            Resolved::File(file) => file.key,
            Resolved::Unloadable { .. } | Resolved::Again { .. } => {
                unreachable!("only a loaded entry is settled")
            }
        };
        match verdict {
            Ok(name) => {
                if name != key {
                    runtime.rename(key, name);
                }
                Ok((name.to_string(), done))
            }
            Err(problem) => {
                runtime.unload(key);
                Err(self.failure(&subject(resolved), problem))
            }
        }
    }

    /// `name`'s lock entry when its module on disk is the one it pins (any
    /// module, for an entry that pins no hash): the check the extension load
    /// makes ([`Self::check_module`]), without loading it.
    pub fn verified(&self, name: &str, disk: &dyn Disk) -> Option<&LockFileEntry> {
        let entry = self
            .lock
            .entries()
            .iter()
            .find(|e| e.name.as_str() == name)?;
        self.check_module(entry, disk).ok().map(|_| entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_loaded_enables_each_entry_as_written() {
        let entries = ["@acme/a".to_string(), "x=ext.wasm".to_string()];
        let loaded = Installed::none().unloaded(&entries, &Builtins::none());
        let names: Vec<&str> = loaded.enabled.iter().map(|e| e.name.as_str()).collect();
        let files: Vec<Option<&str>> = loaded.enabled.iter().map(|e| e.file()).collect();
        assert_eq!(names, ["@acme/a", "x"]);
        assert_eq!(files, [None, Some("ext.wasm")]);
        assert!(loaded.enabled.iter().all(|e| e.failure.is_none()));
        assert!(loaded.declarations.is_empty());
    }
}
