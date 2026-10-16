//! One change to what is installed: binaries and `specforge.lock` written together or not at all
//! (ADR 0028), and the caller's last step run inside it. The project's own files (`specforge.json`,
//! `.gitignore`, the starter) are the caller's: its last step is a project change that puts back what
//! it wrote and reports what it could not (ADR 0128).
//!
//! The change is staged in memory. Committing it moves every module aside
//! under `.specforge/extensions/.staging` before it places the new one, so a
//! previous binary is never deleted before its replacement is in place,
//! writes the lock once (only when a step was staged), runs the caller's last
//! step, and, on any failure, puts every module and the lock back as it was
//! and removes the directories the change created.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use specforge_common::{Diagnostic, codes};
use specforge_package::PackageName;
use specforge_protocol_types::PeerDependency;

use crate::Installed;
use crate::layout::{MODULE_FILE, STAGING};
use crate::lock::{LockFile, LockFileEntry, LockSource, LockState, write_lock_file};
use crate::module::Module;

/// What a lock entry records about an install, its hash aside (the module's
/// digest is).
#[derive(Debug, Clone, PartialEq)]
pub struct Pin {
    pub name: PackageName,
    pub version: String,
    pub source: LockSource,
    pub key_id: Option<String>,
    pub peers: Vec<PeerDependency>,
}

impl Pin {
    /// The lock entry this pin records for a module whose SHA-256 is `sha256`.
    pub fn entry(&self, sha256: &str) -> LockFileEntry {
        LockFileEntry {
            name: self.name.clone(),
            version: self.version.clone(),
            source: self.source.clone(),
            wasm_hash: sha256.to_string(),
            key_id: self.key_id.clone(),
            peer_dependencies: self.peers.clone(),
        }
    }
}

/// One staged step.
enum Step {
    Install { name: PackageName, module: Module },
    Uninstall { name: PackageName },
}

/// A change to what is installed, staged in memory until it is committed.
pub struct Change<'a> {
    installed: &'a Installed,
    /// The lock as the change will write it.
    lock: LockFile,
    steps: Vec<Step>,
}

/// A committed change: the files whose bytes it changed or that it deleted
/// (module files and `specforge.lock`), in path order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Committed {
    pub changed: Vec<PathBuf>,
}

/// A change that failed and was rolled back. `left` names what the rollback
/// could not put back (normally nothing); an error that is a diagnostic says
/// so in its message.
#[derive(Debug)]
pub struct Failed<E> {
    pub error: E,
    pub left: Vec<PathBuf>,
}

impl Installed {
    /// Begin changing what is installed; the change starts from the lock as
    /// read (empty when absent). Refused with E033 when `specforge.lock` is
    /// there but can't be read: writing one now would drop what it records.
    pub fn change(&self) -> Result<Change<'_>, Diagnostic> {
        Ok(Change {
            installed: self,
            lock: self.writable_lock()?,
            steps: Vec::new(),
        })
    }

    /// The lock a change starts from: the lock as read, empty when absent. E033 when
    /// `specforge.lock` is there but can't be read: writing one now would drop what it records.
    pub fn writable_lock(&self) -> Result<LockFile, Diagnostic> {
        match &self.lock {
            LockState::Read(lock) => Ok(lock.clone()),
            LockState::Absent => Ok(LockFile::default()),
            LockState::Unreadable(problem) => Err(problem.clone().with_suggestion(
                "fix specforge.lock, or delete it and install the extensions again; \
                 nothing was changed"
                    .to_string(),
            )),
        }
    }
}

impl Change<'_> {
    /// Install `module` as `pin.name`: its binary and its lock entry,
    /// replacing what it had.
    pub fn install(&mut self, module: Module, pin: Pin) {
        let entry = pin.entry(module.digest());
        self.lock.put(entry);
        self.steps.push(Step::Install {
            name: pin.name,
            module,
        });
    }

    /// Uninstall `name`: its directory and its lock entry. A name the lock
    /// does not hold stages nothing.
    pub fn uninstall(&mut self, name: &PackageName) {
        if self.lock.without(name) {
            self.steps.push(Step::Uninstall { name: name.clone() });
        }
    }

    /// Write the change: every binary placed or moved aside, then the lock
    /// once. All or nothing (see [`Self::commit_with`]).
    #[expect(
        clippy::result_large_err,
        reason = "Failed<Diagnostic> / LoadFailure carry the whole diagnostic the surfaces present (clippy.toml admits a bare Diagnostic only)"
    )]
    pub fn commit(self) -> Result<Committed, Failed<Diagnostic>> {
        self.run(|| Ok(()), std::convert::identity)
    }

    /// Place every staged module, write the lock once (only when a step was staged), then `last`.
    /// All or nothing: a failure at any step, `last`'s included, puts back every module and the
    /// lock and removes the directories the change created. `last` puts back the files it wrote
    /// itself when it fails, and reports what it could not in its own error (ADR 0128).
    /// [`Committed::changed`] lists each module and the lock whose bytes changed. E032 for a binary
    /// that can't be placed or moved aside, E033 for a lock that can't be written (as `own` makes
    /// them an `E`), `last`'s own error otherwise.
    pub fn commit_with<E>(
        self,
        last: impl FnOnce() -> Result<(), E>,
        own: impl FnOnce(Diagnostic) -> E,
    ) -> Result<Committed, Failed<E>> {
        self.run(last, own)
    }

    fn run<E>(
        self,
        last: impl FnOnce() -> Result<(), E>,
        own: impl FnOnce(Diagnostic) -> E,
    ) -> Result<Committed, Failed<E>> {
        let Change {
            installed,
            lock,
            steps,
        } = self;
        let staging = installed.extensions_dir().join(STAGING);
        // A process killed mid-commit can leave modules here: sweep them.
        let _ = std::fs::remove_dir_all(&staging);

        let lock_path = installed.lock_path();
        let lock_before = std::fs::read(&lock_path).ok();
        let created = Absent::of(installed);
        let mut journal = Journal::default();
        let mut changed = BTreeSet::new();

        let placed = steps.iter().enumerate().try_for_each(|(i, step)| {
            apply_step(installed, &staging, i, step, &mut journal, &mut changed)
        });
        let writes_lock = (!steps.is_empty()).then_some(lock);
        let outcome: Result<(), Cause<E>> = placed
            .map_err(Cause::Diagnostic)
            .and_then(|()| match &writes_lock {
                Some(lock) => write_lock_file(lock, &lock_path).map_err(Cause::Diagnostic),
                None => Ok(()),
            })
            .and_then(|()| last().map_err(Cause::Last));

        match outcome {
            Ok(()) => {
                if writes_lock.is_some() && std::fs::read(&lock_path).ok() != lock_before {
                    changed.insert(lock_path);
                }
                journal.finish(installed, &staging);
                Ok(Committed {
                    changed: changed.into_iter().collect(),
                })
            }
            Err(cause) => {
                let mut left = Vec::new();
                if writes_lock.is_some() {
                    restore_bytes(&lock_path, lock_before.as_deref(), &mut left);
                }
                journal.undo(installed, &mut left);
                left.sort();
                left.dedup();
                if left.is_empty() {
                    let _ = std::fs::remove_dir_all(&staging);
                }
                created.remove();
                let error = match cause {
                    Cause::Last(error) => error,
                    Cause::Diagnostic(diagnostic) => own(says_what_is_left(diagnostic, &left)),
                };
                Err(Failed { error, left })
            }
        }
    }
}

/// The directories a change may create that did not exist when it began: the project root, and the
/// extensions directory and its parents.
struct Absent(Vec<PathBuf>);

impl Absent {
    fn of(installed: &Installed) -> Absent {
        let root = installed.root();
        let mut watched: BTreeSet<PathBuf> = BTreeSet::from([root.to_path_buf()]);
        for start in [installed.extensions_dir()] {
            if !start.starts_with(root) {
                watched.insert(start);
                continue;
            }
            let mut dir = start.as_path();
            while dir != root {
                watched.insert(dir.to_path_buf());
                match dir.parent() {
                    Some(parent) => dir = parent,
                    None => break,
                }
            }
        }
        let mut absent: Vec<PathBuf> = watched.into_iter().filter(|dir| !dir.exists()).collect();
        // Deepest first: a child goes before the directory that holds it.
        absent.sort_by_key(|dir| std::cmp::Reverse(dir.components().count()));
        Absent(absent)
    }

    /// Remove each one that is empty (never recursively: one that holds anything is left, since
    /// something the change did not write is in it).
    fn remove(self) {
        for dir in self.0 {
            let _ = std::fs::remove_dir(dir);
        }
    }
}

/// Why a commit stopped.
enum Cause<E> {
    /// A step of the change itself (E032, E033).
    Diagnostic(Diagnostic),
    /// The caller's last step (the project change and the pins).
    Last(E),
}

/// `diagnostic`, saying what the rollback could not put back.
fn says_what_is_left(mut diagnostic: Diagnostic, left: &[PathBuf]) -> Diagnostic {
    if !left.is_empty() {
        let paths: Vec<String> = left.iter().map(|p| p.display().to_string()).collect();
        diagnostic.message = format!(
            "{}; the rollback could not put back: {}",
            diagnostic.message,
            paths.join(", ")
        );
    }
    diagnostic
}

/// What the commit did to the directories, to undo it.
#[derive(Default)]
struct Journal {
    /// A directory moved to `to`, which was at `from`.
    moved: Vec<(PathBuf, PathBuf)>,
    /// A directory placed where there was none or where a moved one was.
    placed: Vec<PathBuf>,
}

impl Journal {
    /// Put the directories back: the placed ones go, the moved ones return.
    fn undo(self, installed: &Installed, left: &mut Vec<PathBuf>) {
        for dir in self.placed.iter().rev() {
            if std::fs::remove_dir_all(dir).is_err() && dir.exists() {
                left.push(dir.clone());
            } else {
                remove_empty_scope(installed, dir);
            }
        }
        for (from, to) in self.moved.iter().rev() {
            if let Some(parent) = from.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if std::fs::rename(to, from).is_err() {
                left.push(from.clone());
            }
        }
    }

    /// The change is whole: drop what was moved aside, and the scope
    /// directories (`@scope`) that an uninstall emptied.
    fn finish(self, installed: &Installed, staging: &Path) {
        let _ = std::fs::remove_dir_all(staging);
        for (from, _) in &self.moved {
            if !from.exists() {
                remove_empty_scope(installed, from);
            }
        }
    }
}

/// Remove the directory above `package_dir` when it is a scope that holds
/// nothing, and is not the extensions directory itself.
fn remove_empty_scope(installed: &Installed, package_dir: &Path) {
    let extensions = installed.extensions_dir();
    if let Some(scope) = package_dir.parent()
        && scope != extensions
        && scope.starts_with(&extensions)
    {
        // Fails (and is left) when the scope still holds an extension.
        let _ = std::fs::remove_dir(scope);
    }
}

/// Put the lock at `path` back to `before`: its bytes, or gone when it was not there.
fn restore_bytes(path: &Path, before: Option<&[u8]>, left: &mut Vec<PathBuf>) {
    let now = std::fs::read(path).ok();
    if now.as_deref() == before {
        return;
    }
    let restored = match before {
        Some(bytes) => std::fs::write(path, bytes),
        None => std::fs::remove_file(path),
    };
    if restored.is_err() {
        left.push(path.to_path_buf());
    }
}

/// Do one step: stage the new module or move the old directory aside, then
/// place it.
fn apply_step(
    installed: &Installed,
    staging: &Path,
    i: usize,
    step: &Step,
    journal: &mut Journal,
    changed: &mut BTreeSet<PathBuf>,
) -> Result<(), Diagnostic> {
    match step {
        Step::Install { name, module } => {
            let dir = installed.package_dir(name);
            let target = installed.module_path(name);
            let before = std::fs::read(&target).ok();

            let staged = staging.join(i.to_string());
            std::fs::create_dir_all(&staged).map_err(|e| {
                failed(format!(
                    "failed to create the staging directory for '{name}': {e}"
                ))
            })?;
            std::fs::write(staged.join(MODULE_FILE), module.bytes())
                .map_err(|e| failed(format!("failed to write .wasm binary for '{name}': {e}")))?;

            if dir.exists() {
                // What the previous directory held besides the module is
                // deleted with it.
                for file in files_in(&dir) {
                    if file != target {
                        changed.insert(file);
                    }
                }
                let aside = staging.join(format!("{i}.previous"));
                std::fs::rename(&dir, &aside).map_err(|e| {
                    failed(format!(
                        "failed to move the previous '{name}' aside from '{}': {e}",
                        dir.display()
                    ))
                })?;
                journal.moved.push((dir.clone(), aside));
            }
            if let Some(parent) = dir.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            std::fs::rename(&staged, &dir)
                .map_err(|e| failed(format!("failed to finalize installation of '{name}': {e}")))?;
            journal.placed.push(dir);
            if before.as_deref() != Some(module.bytes()) {
                changed.insert(target);
            }
        }
        Step::Uninstall { name } => {
            let dir = installed.package_dir(name);
            if dir.exists() {
                changed.extend(files_in(&dir));
                let aside = staging.join(format!("{i}.removed"));
                std::fs::create_dir_all(staging).map_err(|e| {
                    failed(format!(
                        "failed to create the staging directory for '{name}': {e}"
                    ))
                })?;
                std::fs::rename(&dir, &aside).map_err(|e| {
                    failed(format!(
                        "failed to remove extension directory '{}': {e}",
                        dir.display()
                    ))
                })?;
                journal.moved.push((dir, aside));
            }
        }
    }
    Ok(())
}

fn failed(message: String) -> Diagnostic {
    Diagnostic::new(codes::E032, message)
        .with_suggestion("check the permissions of .specforge/extensions and try again".to_string())
}

/// Every file under `dir` (none when it does not exist): what deleting it
/// deletes.
fn files_in(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut dirs = vec![dir.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for path in entries.flatten().map(|entry| entry.path()) {
            if path.is_dir() {
                dirs.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files
}
