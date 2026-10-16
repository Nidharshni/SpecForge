//! A checked plan written as one change, all or nothing (ADR 0028 D4), the publisher keys pinned last
//! (ADR 0059).

use std::path::{Path, PathBuf};

use super::{Ask, Checked, Disabled, Purpose};
use crate::registry::{KeyPin, Registry};
use crate::{Containment, Held, OpError, OpErrorKind, Writes};

/// What writes an [`Also`] file, given the file it was resolved to.
pub(crate) type WriteHeld<'f> = Box<dyn FnOnce(&Held) -> Result<(), OpError> + 'f>;

/// A file the change's last step writes after `specforge.json`'s edit and before the pins (`init`'s
/// config, `.gitignore` and starter). Put back, or removed when it was not there, when the change
/// fails. `write` is handed the file `path` resolved to (ADR 0112): it opens nothing else.
pub(crate) struct Also<'f> {
    pub(crate) path: PathBuf,
    pub(crate) write: WriteHeld<'f>,
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

    /// A preview answers without writing. A commit writes, all or nothing (ADR 0028 D4): each
    /// staged module placed and each uninstalled one moved aside, the lock once (when a module
    /// changed), then the last step: `specforge.json`'s edit (the appended entries, the dropped
    /// ones) and last the publisher keys its fetches accepted (ADR 0059). A plan that stages no
    /// module writes no lock. A failure at any step, the pins' included (R-TRUST-006), puts every
    /// file back and pins nothing. The error's writes name what could not be put back (normally
    /// nothing).
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
        let edited_config = edits_config && (!names.is_empty() || !disabled.is_empty());
        let also_paths: Vec<PathBuf> = also.iter().map(|also| also.path.clone()).collect();
        // Held before anything is written: what is written is what is checked here.
        let also_held = also_paths
            .iter()
            .map(|path| containment.resolve(path))
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
            let enabling = edits_config && !names.is_empty();
            let config_after = if enabling || !disabled.is_empty() {
                config_edit(root, &names, &disabled, enabling, &mut enabled)?
            } else {
                None
            };
            let last = || -> Result<(), OpError> {
                if let Some(bytes) = &config_after {
                    let file = config_file()?;
                    file.replace(bytes)
                        .map_err(|e| crate::config::write_failed(file, &e))?;
                }
                for (file, held) in also.into_iter().zip(&also_held) {
                    (file.write)(held)?;
                }
                record(pins)
            };
            let mut change = if changes_lock {
                installed
                    .change()
                    .map_err(|why| OpError::of(OpErrorKind::SchemaMismatch, why))?
            } else {
                installed.change_entries()
            };
            for (module, pin) in installs {
                change.install(module, pin);
            }
            for name in &uninstalls {
                change.uninstall(name);
            }
            let committed = change
                .files(
                    edited_config
                        .then(|| config_path.clone())
                        .into_iter()
                        .chain(also_paths),
                )
                .commit_with(last, |why| OpError::of(OpErrorKind::Internal, why))
                .map_err(|failed| failed.error.with_writes(Writes::of(failed.left)))?;
            writes = Writes::of(committed.changed);
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

/// Pin what a committed plan's fetches decided, through the port that fetched them.
fn record(pins: Option<(&dyn Registry, Vec<KeyPin>)>) -> Result<(), OpError> {
    match pins {
        Some((registry, pins)) if !pins.is_empty() => registry.pin(&pins),
        _ => Ok(()),
    }
}
