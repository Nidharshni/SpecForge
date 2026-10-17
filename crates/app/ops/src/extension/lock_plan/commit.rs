//! A checked plan written as one change, all or nothing (ADR 0028 D4), the publisher keys pinned last
//! (ADR 0059).

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{Ask, Checked, Opened, Purpose};
use crate::change::{Cause, ProjectChange};
use crate::registry::{KeyPin, Registry};
use crate::{Held, OpError, OpErrorKind, Writes};

/// What stages an [`Also`] file: the file it was resolved to, and the project change to stage it in.
pub(crate) type StageHeld<'f> =
    Box<dyn FnOnce(Held, &mut ProjectChange) -> Result<(), OpError> + 'f>;

/// A file the change writes after `specforge.json`'s edit and before the pins (`init`'s
/// `.gitignore` and starter, handed over with [`Checked::writing`]), in the same project change (ADR 0128): put back, or removed when it
/// was not there, when the change fails. `stage` is handed the file `path` resolved to (ADR 0112):
/// it reads and stages nothing else. `code` is the refusal of writing it.
pub(crate) struct Also<'f> {
    pub(crate) path: PathBuf,
    pub(crate) stage: StageHeld<'f>,
    pub(crate) code: &'static str,
}

/// What a finished plan did, or for a preview would do.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Done {
    /// The files written; none for a preview.
    pub(crate) writes: Writes,
    /// How many extensions `specforge.json` enables after the change (would, for a preview).
    pub(crate) enabled: usize,
    asked: Vec<(Ask, Asked)>,
}

/// What a finished plan did for one ask, or for a preview would do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Asked {
    /// The change enables what the ask named, and installs it for an install, after the builtins
    /// its extension requires: `peers_enabled`, dependencies first, each reported once, by the ask
    /// whose extension first required it.
    Changes { peers_enabled: Vec<&'static str> },
    /// The project already has it: an entry of `specforge.json` enables it. `locked` is the
    /// version the lock pins, `None` for an entry only. The change leaves it as it is.
    Unchanged { locked: Option<String> },
}

impl Done {
    /// What the plan did for `ask`.
    ///
    /// # Panics
    /// For an ask the operation never made.
    pub(crate) fn asked(&self, ask: Ask) -> Asked {
        self.asked
            .iter()
            .find(|(asked, _)| *asked == ask)
            .map(|(_, asked)| asked.clone())
            .expect("an ask the operation made is answered")
    }
}

impl<'a> Checked<'a> {
    /// Also write `also` with the change, after `specforge.json` and before the pins (ADR 0128 D4,
    /// ADR 0137).
    pub(crate) fn writing(mut self, also: Vec<Also<'a>>) -> Checked<'a> {
        self.also = also;
        self
    }

    /// The `extensions` items the change leaves, in order: the items the plan opened with
    /// without the dropped ones, then the appended ones, dependencies first (ADR 0126).
    fn items(&self) -> Vec<Value> {
        self.config.items_after(
            |entry| self.disabled.iter().any(|disabled| disabled.drops(entry)),
            &self.appended,
        )
    }

    /// What `specforge.json` enables as the change leaves it, in order.
    pub(crate) fn entries(&self) -> Vec<String> {
        Opened::entries_of(&self.items())
    }

    /// A preview answers without writing. A commit writes, all or nothing (ADR 0028 D4, ADR 0128):
    /// each staged module placed and each uninstalled one moved aside, the lock once (when a module
    /// changed), then, as the installed change's last step, the project change: `specforge.json` with
    /// the entries the plan judged (ADR 0126) and the files of [`Self::writing`] written in order, and last
    /// the publisher keys its fetches accepted (ADR 0059). A plan that stages no module writes no
    /// lock and opens no installed change (ADR 0041 B4). A failure at any step, the pins' included
    /// (R-TRUST-006), puts every file back and pins nothing. The error's writes name what could not
    /// be put back (normally nothing).
    ///
    /// Before anything is written, a commit refuses `file_changed` when `specforge.json` no longer
    /// holds the document the plan opened with (ADR 0126); a preview writes nothing and is not checked.
    ///
    /// A change that changes nothing (and writes no `also` file) answers here: nothing is resolved,
    /// compared or written (ADR 0138).
    ///
    /// Before anything is written, and for a preview too, every file the change would write is
    /// resolved through the caller's containment narrowed to the project (ADR 0104, ADR 0137): the config, the `also` files, the lock and each
    /// installed or uninstalled module. One that lands outside it, by a symlink in the project,
    /// refuses the change (`outside_root`), nothing written (ADR 0092 D8).
    pub(crate) fn finish(self) -> Result<Done, OpError> {
        let items = self.items();
        let final_entries = Opened::entries_of(&items);
        let Checked {
            installed,
            changes_lock,
            installs,
            uninstalls,
            purpose,
            operation,
            config,
            pins,
            appended,
            asked,
            changes,
            lent,
            also,
            ..
        } = self;
        if !changes && also.is_empty() {
            return Ok(Done {
                writes: Writes::none(),
                enabled: final_entries.len(),
                asked,
            });
        }
        // The project root must lie inside what the caller lent: narrowing never widens (ADR 0137).
        lent.resolve(installed.root())?;
        let containment = lent.narrowed(installed.root());
        let document = config.written(&items);
        // Held before anything is written: what is written is what is checked here.
        let also_held = also
            .iter()
            .map(|also| containment.resolve(&also.path))
            .collect::<Result<Vec<Held>, _>>()?;
        let config_path = installed.root().join(crate::config::CONFIG_FILE);
        let config_held = config
            .holds_a_file()
            .then(|| containment.resolve(&config_path))
            .transpose()?;
        let mut written_by_change: Vec<PathBuf> = Vec::new();
        if changes_lock {
            written_by_change.push(installed.lock_path());
        }
        written_by_change.extend(
            installs
                .iter()
                .map(|(_, pin)| installed.module_path(&pin.name)),
        );
        written_by_change.extend(uninstalls.iter().map(|name| installed.module_path(name)));
        for path in &written_by_change {
            containment.resolve(path)?;
        }
        let mut writes = Writes::none();
        if purpose == Purpose::Commit {
            // Nothing is written yet: a file that changed since the plan opened it refuses here (ADR 0126).
            let config_before = config_held
                .as_ref()
                .map(|held| config.unchanged(held, operation))
                .transpose()?
                .flatten();
            // The installed change opens first: an unreadable lock is E033 before anything is judged.
            let change = if changes_lock {
                Some(
                    installed
                        .change()
                        .map_err(|why| OpError::of(OpErrorKind::SchemaMismatch, why))?,
                )
            } else {
                None
            };
            let mut files = ProjectChange::new();
            if let (Some(document), Some(held)) = (&document, &config_held) {
                files.stage(
                    held.clone(),
                    config_before,
                    crate::config::render(document)?,
                );
            }
            let mut also_codes: Vec<(PathBuf, &'static str)> = Vec::new();
            for (file, held) in also.into_iter().zip(also_held) {
                also_codes.push((held.spelled.clone(), file.code));
                (file.stage)(held, &mut files)?;
            }
            let config_spelled = config_held.as_ref().map(|held| held.spelled.clone());
            let commit_files = |files: ProjectChange| -> Result<Writes, OpError> {
                files.commit_then(|| record(pins)).map_err(|failed| {
                    failed.error(|cause| refused(cause, config_spelled.as_deref(), &also_codes))
                })
            };
            match change {
                Some(mut change) => {
                    for (module, pin) in installs {
                        change.install(module, pin);
                    }
                    for name in &uninstalls {
                        change.uninstall(name);
                    }
                    let mut project = Writes::none();
                    let committed = change
                        .commit_with(
                            || {
                                project = commit_files(files)?;
                                Ok(())
                            },
                            |why| OpError::of(OpErrorKind::Internal, why),
                        )
                        .map_err(|failed| {
                            let mut left = failed.error.writes.clone();
                            left.merge(Writes::of(failed.left));
                            failed.error.with_writes(left)
                        })?;
                    writes = Writes::of(committed.changed);
                    writes.merge(project);
                }
                None => writes = commit_files(files)?,
            }
        }

        let asked = asked
            .into_iter()
            .map(|(ask, answer)| {
                let answer = match answer {
                    // Each is reported once, by the ask whose extension first required it.
                    Asked::Changes { .. } => Asked::Changes {
                        peers_enabled: appended
                            .iter()
                            .filter(|entry| entry.ask == ask && entry.required_by.is_some())
                            .filter_map(|entry| super::super::builtin_name(&entry.name))
                            .collect(),
                    },
                    unchanged => unchanged,
                };
                (ask, answer)
            })
            .collect();
        Ok(Done {
            writes,
            enabled: final_entries.len(),
            asked,
        })
    }
}

/// The refusal of a project change that failed: `config` is the spelled `specforge.json` the plan edited, and
/// `also` the spelled files the caller staged with their codes.
fn refused(
    cause: Cause<OpError>,
    config: Option<&Path>,
    also: &[(PathBuf, &'static str)],
) -> OpError {
    match cause {
        Cause::Changed { file } => OpError::new(
            OpErrorKind::Conflict,
            crate::FILE_CHANGED,
            format!(
                "{} changed while the change was being written; nothing was written",
                file.display()
            ),
        ),
        Cause::Unreadable { file, error } => OpError::new(
            OpErrorKind::of_io(&error),
            "file_unreadable",
            format!("failed to read {}: {error}", file.display()),
        ),
        Cause::Unwritable { file, error } => {
            let code = if config == Some(file.as_path()) {
                "config_write_failed"
            } else {
                also.iter()
                    .find(|(path, _)| *path == file)
                    .map_or("file_unreadable", |(_, code)| code)
            };
            OpError::new(
                OpErrorKind::of_io(&error),
                code,
                format!("failed to write {}: {error}", file.display()),
            )
        }
        Cause::Then(error) => error,
    }
}

/// Pin what a committed plan's fetches decided, through the port that fetched them.
fn record(pins: Option<(&dyn Registry, Vec<KeyPin>)>) -> Result<(), OpError> {
    match pins {
        Some((registry, pins)) if !pins.is_empty() => registry.pin(&pins),
        _ => Ok(()),
    }
}
