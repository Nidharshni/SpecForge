//! A checked plan written as one change, all or nothing (ADR 0028 D4), the publisher keys pinned last
//! (ADR 0059).

use std::path::{Path, PathBuf};

use super::{Ask, Checked, Disabled, Purpose};
use crate::change::{Cause, ProjectChange};
use crate::registry::{KeyPin, Registry};
use crate::{Containment, Held, OpError, OpErrorKind, Writes};

/// What stages an [`Also`] file: the file it was resolved to, and the project change to stage it in.
pub(crate) type StageHeld<'f> =
    Box<dyn FnOnce(Held, &mut ProjectChange) -> Result<(), OpError> + 'f>;

/// A file the change writes after `specforge.json`'s edit and before the pins (`init`'s config,
/// `.gitignore` and starter), in the same project change (ADR 0128): put back, or removed when it
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

/// What a finished plan did for one ask.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Asked {
    /// The entry of the extension it asked for was appended by this change (would be, for a
    /// preview); `false` when an entry already enabled it, or another writer added it since the
    /// plan read the file.
    pub(crate) appended: bool,
    /// The builtins enabled because its extension requires them, dependencies first.
    pub(crate) peers_enabled: Vec<&'static str>,
}

impl Done {
    /// What the plan did for `ask` (nothing, for an ask the plan changed nothing for).
    pub(crate) fn asked(&self, ask: Ask) -> Asked {
        self.asked
            .iter()
            .find(|(asked, _)| *asked == ask)
            .map(|(_, asked)| asked.clone())
            .unwrap_or_default()
    }
}

/// `specforge.json` as the plan leaves it: the names enabled and the entries dropped, applied to the
/// array in memory, one read and one render (`None` when the file would be as it is). A missing
/// `specforge.json` is nothing to edit for a plan that only drops entries: a project without one (an
/// install only the lock knows) has no entry to drop. `enabled` is what enabling changed.
fn config_edit(
    root: &Path,
    names: &[&str],
    disabled: &[Disabled],
    enabling: bool,
    enabled: &mut Option<crate::config::Enabled>,
) -> Result<Option<Vec<u8>>, OpError> {
    let edit = crate::config::edit_extensions(root, |extensions| {
        let mut changed = false;
        if enabling {
            let now = crate::config::enable(extensions, names);
            changed |= !now.appended.is_empty();
            *enabled = Some(now);
        }
        for dropped in disabled {
            changed |= match dropped {
                Disabled::Named(name) => crate::config::remove_extension(extensions, name),
                Disabled::Entry(entry) => crate::config::remove_entry(extensions, entry),
            };
        }
        changed
    });
    match edit {
        Err(error) if error.code == "config_not_found" && !enabling => Ok(None),
        other => other,
    }
}

impl Checked<'_> {
    /// What `specforge.json` enables as the change leaves it, in order: the entries it held
    /// without the dropped ones, then the appended ones, dependencies first.
    pub(crate) fn entries(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter(|entry| !self.disabled.iter().any(|disabled| disabled.drops(entry)))
            .cloned()
            .chain(self.appended.iter().map(|appended| appended.name.clone()))
            .collect()
    }

    /// A preview answers without writing. A commit writes, all or nothing (ADR 0028 D4, ADR 0128):
    /// each staged module placed and each uninstalled one moved aside, the lock once (when a module
    /// changed), then, as the installed change's last step, the project change: `specforge.json`'s
    /// edit (the appended entries, the dropped ones) and the `also` files written in order, and last
    /// the publisher keys its fetches accepted (ADR 0059). A plan that stages no module writes no
    /// lock and opens no installed change (ADR 0041 B4). A failure at any step, the pins' included
    /// (R-TRUST-006), puts every file back and pins nothing. The error's writes name what could not
    /// be put back (normally nothing).
    ///
    /// Before anything is written, and for a preview too, every file the change would write is
    /// resolved through `containment` (ADR 0104): the config, the `also` files, the lock and each
    /// installed or uninstalled module. One that lands outside it, by a symlink in the project,
    /// refuses the change (`outside_root`), nothing written (ADR 0092 D8).
    pub(crate) fn finish(
        self,
        root: &Path,
        also: Vec<Also<'_>>,
        containment: &Containment,
    ) -> Result<Done, OpError> {
        let final_entries = self.entries();
        let Checked {
            installed,
            changes_lock,
            installs,
            uninstalls,
            disabled,
            purpose,
            edits_config,
            pins,
            appended,
            wanted,
            ..
        } = self;
        let names: Vec<&str> = appended
            .iter()
            .map(|appended| appended.name.as_str())
            .collect();
        // Held before anything is written: what is written is what is checked here.
        let also_held = also
            .iter()
            .map(|also| containment.resolve(&also.path))
            .collect::<Result<Vec<Held>, _>>()?;
        let config_path = root.join(crate::config::CONFIG_FILE);
        let config_held = (edits_config || !disabled.is_empty())
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
        let config_file = || {
            config_held.as_ref().ok_or_else(|| {
                OpError::new(
                    OpErrorKind::Internal,
                    "config_not_held",
                    "the change edits specforge.json without holding it",
                )
            })
        };
        let mut enabled = None;
        let mut writes = Writes::none();
        if purpose == Purpose::Commit {
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
            let enabling = edits_config && !names.is_empty();
            let config_after = if enabling || !disabled.is_empty() {
                config_edit(root, &names, &disabled, enabling, &mut enabled)?
            } else {
                None
            };
            let mut files = ProjectChange::new();
            if let Some(after) = config_after {
                let held = config_file()?.clone();
                let before = held.read_if_there().map_err(|e| unreadable(&held, &e))?;
                files.stage(held, before, after);
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

        let written = |name: &str| match purpose {
            Purpose::Commit if !edits_config => names.contains(&name),
            Purpose::Commit => enabled
                .as_ref()
                .is_some_and(|enabled| enabled.appended.iter().any(|a| a == name)),
            Purpose::Preview => names.contains(&name),
        };
        let mut asked: Vec<(Ask, Asked)> = Vec::new();
        for (ask, name) in &wanted {
            if asked.iter().any(|(known, _)| known == ask) {
                continue;
            }
            asked.push((
                *ask,
                Asked {
                    appended: written(name),
                    // Each is reported once, by the ask whose extension first required it.
                    peers_enabled: appended
                        .iter()
                        .filter(|entry| entry.ask == *ask && entry.required_by.is_some())
                        .filter(|entry| written(&entry.name))
                        .filter_map(|entry| super::super::builtin_name(&entry.name))
                        .collect(),
                },
            ));
        }
        Ok(Done {
            writes,
            enabled: enabled
                .as_ref()
                .map_or(final_entries.len(), |enabled| enabled.total),
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
            "file_changed",
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

/// The refusal of reading `file`, which the change would write, to stage it.
fn unreadable(file: &Held, error: &std::io::Error) -> OpError {
    OpError::new(
        OpErrorKind::of_io(error),
        "file_unreadable",
        format!("failed to read {}: {error}", file.spelled.display()),
    )
}

/// Pin what a committed plan's fetches decided, through the port that fetched them.
fn record(pins: Option<(&dyn Registry, Vec<KeyPin>)>) -> Result<(), OpError> {
    match pins {
        Some((registry, pins)) if !pins.is_empty() => registry.pin(&pins),
        _ => Ok(()),
    }
}
