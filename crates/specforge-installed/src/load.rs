//! Turning a project's `extensions` entries into loaded extensions and
//! their declarations (ADR 0028): a builtin from its embedded bytes, an
//! installed extension from its pinned module, a `.wasm` file entry from its
//! file under the name it declares. What does not load is a typed
//! [`LoadFailure`] on its entry with one diagnostic; the runtime keeps no
//! load state.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use specforge_common::{Diagnostic, ExtensionEntry, codes};
use specforge_protocol_types::ExtensionDeclaration;
use specforge_wasm::protocol::load_declaration;
use specforge_wasm::{ExtensionCalls, WasmRuntime};

use crate::Installed;
use crate::lock::LockFileEntry;
use crate::module::Module;
use crate::resolve::{FileEntry, Resolved};

/// The builtin extensions the host embeds: name and component bytes.
#[derive(Debug, Clone, Copy)]
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

impl Loaded {
    /// What `entries` enable when nothing loads (no runtime): each entry
    /// [`EnabledExtension::unloaded`], no declaration, no diagnostic.
    pub fn unloaded(entries: &[String]) -> Loaded {
        Loaded {
            enabled: entries
                .iter()
                .map(|entry| EnabledExtension::unloaded(entry))
                .collect(),
            declarations: Vec::new(),
        }
    }
}

/// What one `specforge.json` `extensions` entry enables, as the load left it.
#[derive(Debug, Clone, PartialEq)]
pub struct EnabledExtension {
    /// The entry as specforge.json writes it (trimmed).
    pub entry: String,
    /// A named entry's name; a `.wasm` file entry's declared name once it
    /// loaded, else the name written before `=`, else the path.
    pub name: String,
    /// The path a `.wasm` file entry names, as written.
    pub file: Option<String>,
    /// Why it did not load. `None`: it loaded, or nothing was loaded
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
    /// What `entry` names when nothing is loaded (no runtime).
    pub fn unloaded(entry: &str) -> Self {
        match ExtensionEntry::parse(entry) {
            ExtensionEntry::Named(name) => EnabledExtension {
                entry: entry.trim().to_string(),
                name: name.to_string(),
                file: None,
                failure: None,
                warnings: Vec::new(),
            },
            ExtensionEntry::File { name, path } => EnabledExtension {
                entry: entry.trim().to_string(),
                name: name.unwrap_or(path).to_string(),
                file: Some(path.to_string()),
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

impl LoadProblem {
    /// The one diagnostic that reports this problem for `subject`.
    fn diagnostic(&self, subject: &Subject, installed: &Installed) -> Diagnostic {
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
            LoadProblem::NotAComponent { reason } => Diagnostic::new(
                codes::E028,
                format!("extension '{name}': failed to load Wasm module: {reason}"),
            ),
            LoadProblem::NoDeclaration { reason } => Diagnostic::new(
                codes::E028,
                format!("extension '{name}': protocol loading failed: {reason}"),
            ),
            LoadProblem::FileMissing { .. }
            | LoadProblem::NotTheNameWritten { .. }
            | LoadProblem::AlreadyLoaded { .. } => {
                unreachable!("a named entry names no file")
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
        let build = "build it with specforge-extension-sdk for wasm32-wasip2".to_string();
        match self {
            LoadProblem::FileMissing { path: missing } => refused(
                format!("the file {} does not exist", missing.display()),
                "build the extension, or correct the path (a relative path is relative to the project root)"
                    .to_string(),
            ),
            LoadProblem::NotAComponent { reason } => refused(
                format!(
                    "{} does not load as an extension component: {reason}",
                    file.display()
                ),
                build,
            ),
            LoadProblem::NoDeclaration { reason } => refused(
                format!("{} answers no handshake: {reason}", file.display()),
                build,
            ),
            LoadProblem::NotTheNameWritten { declared, written } => refused(
                format!("{} declares '{declared}', not '{written}'", file.display()),
                format!("write \"{declared}={path}\", or just \"{path}\""),
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
                build,
            ),
            LoadProblem::NotInstalled
            | LoadProblem::LockUnreadable
            | LoadProblem::NotAPackageName { .. }
            | LoadProblem::ModuleMissing { .. }
            | LoadProblem::Changed { .. }
            | LoadProblem::NotItsLockEntry { .. } => {
                unreachable!("a file entry is not installed")
            }
        }
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
    ) -> Loaded {
        let mut enabled = Loaded::unloaded(entries).enabled;
        let mut slots: Vec<Slot> = entries.iter().map(|_| Slot::Skipped).collect();

        // Named entries first: a `.wasm` file that declares a name they
        // loaded is refused, not swapped in. `resolve` decides the order.
        let resolution = self.resolve(entries, builtins);
        let mut loaded_names = HashSet::new();
        for (i, resolved) in resolution.in_load_order() {
            let outcome = match resolved {
                Resolved::Builtin { name, bytes } => self
                    .load_bytes(name, bytes, None, runtime)
                    .map(|done| ((*name).to_string(), done)),
                Resolved::Locked { name, entry } => self
                    .check_module(entry)
                    .map_err(|problem| self.failure(&Subject::Named(name), problem))
                    .and_then(|module| self.load_bytes(name, module.bytes(), Some(entry), runtime))
                    .map(|done| ((*name).to_string(), done)),
                Resolved::Unloadable { name, problem } => {
                    Err(self.failure(&Subject::Named(name), problem.clone()))
                }
                Resolved::File(file) => {
                    let subject = Subject::File {
                        key: file.key,
                        file: &file.file,
                        path: file.path,
                    };
                    self.load_file(file, &loaded_names, runtime)
                        .map_err(|problem| self.failure(&subject, problem))
                }
                Resolved::Again { .. } => unreachable!("left out of the load order"),
            };
            match outcome {
                Ok((declared, done)) => {
                    if matches!(resolved, Resolved::File(_)) {
                        enabled[i].name = declared.clone();
                    }
                    loaded_names.insert(declared);
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
                && enabled[*first].file.is_some()
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

    /// Load the extension a named entry names from `bytes`: a builtin's embedded bytes, or the
    /// pinned module of the lock `entry` it is installed by.
    fn load_bytes(
        &self,
        name: &str,
        bytes: &[u8],
        entry: Option<&LockFileEntry>,
        runtime: &dyn WasmRuntime,
    ) -> Result<Done, LoadFailure> {
        let fail = |problem: LoadProblem| self.failure(&Subject::Named(name), problem);
        let mut notices = Vec::new();
        if entry.is_some_and(|entry| entry.wasm_hash.is_empty()) {
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
        runtime
            .load(name, bytes)
            .map_err(|reason| fail(LoadProblem::NotAComponent { reason }))?;
        let loaded = load_declaration(runtime, name).map_err(|error| {
            fail(LoadProblem::NoDeclaration {
                reason: error.to_string(),
            })
        })?;
        // An installed binary is the extension its lock entry names.
        if entry.is_some() && loaded.declaration.name() != name {
            runtime.unload(name);
            return Err(fail(LoadProblem::NotItsLockEntry {
                declared: loaded.declaration.name().to_string(),
            }));
        }
        Ok(Done {
            declaration: loaded.declaration,
            notices,
            warnings: loaded.warnings,
        })
    }

    /// `entry`'s module, read once, when it is the one the entry pins.
    pub(crate) fn check_module(&self, entry: &LockFileEntry) -> Result<Module, LoadProblem> {
        let path = self.module_path(&entry.name);
        let module = Module::read(&path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                LoadProblem::ModuleMissing { path: path.clone() }
            } else {
                LoadProblem::ModuleUnreadable {
                    path: path.clone(),
                    reason: error.to_string(),
                }
            }
        })?;
        if !entry.wasm_hash.is_empty() && entry.wasm_hash != module.digest() {
            return Err(LoadProblem::Changed {
                locked: entry.wasm_hash.clone(),
                actual: module.digest().to_string(),
            });
        }
        Ok(module)
    }

    /// Load the component a `.wasm` file entry names under the name it
    /// declares, which is returned with its declaration.
    fn load_file(
        &self,
        entry: &FileEntry,
        loaded_names: &HashSet<String>,
        runtime: &dyn WasmRuntime,
    ) -> Result<(String, Done), LoadProblem> {
        let (key, file) = (entry.key, entry.file.as_path());
        if !file.is_file() {
            return Err(LoadProblem::FileMissing {
                path: file.to_path_buf(),
            });
        }
        let module = Module::read(file).map_err(|error| LoadProblem::NotAComponent {
            reason: format!("cannot read it: {error}"),
        })?;
        // Loaded under the entry until its handshake says what it is.
        runtime
            .load(key, module.bytes())
            .map_err(|reason| LoadProblem::NotAComponent { reason })?;
        let declared = match ExtensionCalls::new(runtime).handshake(key) {
            Ok(handshake) => handshake.response.name,
            Err(error) => {
                runtime.unload(key);
                return Err(LoadProblem::NoDeclaration {
                    reason: error.to_string(),
                });
            }
        };
        if let Err(problem) = entry.admit(&declared, |name| loaded_names.contains(name)) {
            runtime.unload(key);
            return Err(problem);
        }
        runtime.rename(key, &declared);
        match load_declaration(runtime, &declared) {
            Ok(loaded) => Ok((
                declared,
                Done {
                    declaration: loaded.declaration,
                    notices: Vec::new(),
                    warnings: loaded.warnings,
                },
            )),
            Err(error) => {
                runtime.unload(&declared);
                Err(LoadProblem::NoDeclaration {
                    reason: error.to_string(),
                })
            }
        }
    }

    /// `name`'s lock entry when its module on disk is the one it pins (any
    /// module, for an entry that pins no hash). What "already installed"
    /// means for `add`.
    pub fn verified(&self, name: &str) -> Option<&LockFileEntry> {
        let entry = self
            .lock
            .entries()
            .iter()
            .find(|e| e.name.as_str() == name)?;
        self.check_module(entry).ok().map(|_| entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_loaded_enables_each_entry_as_written() {
        let loaded = Loaded::unloaded(&["@acme/a".to_string(), "x=ext.wasm".to_string()]);
        let names: Vec<&str> = loaded.enabled.iter().map(|e| e.name.as_str()).collect();
        let files: Vec<Option<&str>> = loaded.enabled.iter().map(|e| e.file.as_deref()).collect();
        assert_eq!(names, ["@acme/a", "x"]);
        assert_eq!(files, [None, Some("ext.wasm")]);
        assert!(loaded.enabled.iter().all(|e| e.failure.is_none()));
        assert!(loaded.declarations.is_empty());
    }
}
