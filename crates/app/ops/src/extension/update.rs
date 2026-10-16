//! `specforge update`: move registry installs to newer published versions,
//! through the lock plan `add` uses (fetch, integrity, trust, the peer rule
//! over the lock the whole update leaves, the handshake's name and version
//! check, install).
//!
//! An update is all or nothing (`update_all_extensions`): every newer
//! package is fetched and checked before anything is written, and if one
//! fails, nothing is applied. The binaries and the lock are one change
//! (`specforge_installed::Change`): a failure puts the previous ones back.
//! The publisher keys the update's fetches accepted are pinned with it, once it is written: an
//! update that is not applied pins nothing (ADR 0059).

use super::lock_plan::{Ask, Fetching, Install, LockPlan, Opened};
use crate::registry::{Acceptance, NO_REGISTRY, Publisher, Registry};
use crate::{Caller, OpError, OpErrorKind};
use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_common::{Code, codes};
use specforge_installed::{Installed, LockState};
use specforge_package::PackageName;
use specforge_package::{LockedVersion, Reach};
use specforge_wasm::WasmRuntime;

/// The code `update` reports when the project has no lock file.
pub const NO_LOCK: Code = codes::E033;

/// What to update, and how.
#[derive(Debug, Clone)]
pub struct UpdateRequest<'a> {
    pub root: &'a specforge_common::ProjectRoot,
    /// One extension by name; every locked extension when `None`.
    pub name: Option<&'a str>,
    /// How far each extension may move: within its locked version's caret range, or to any
    /// newer version (`--major`). Never to its locked version or below.
    pub reach: Reach,
    /// Accept a registry package with no publisher signature.
    pub allow_unsigned: bool,
    /// What the update may write (`caller.containment()`, ADR 0104: a file it would write that
    /// lands outside, by a symlink in the project, refuses it) and who can be asked about a
    /// publisher key change (`caller.asking()`, ADR 0110).
    pub caller: Caller<'a>,
}

/// What became of one locked extension.
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateStatus {
    /// Moved from `from` to `to` (only when the outcome is applied; see
    /// [`UpdateOutcome::applied`]).
    Updated {
        from: String,
        to: String,
        sha256: String,
        publisher: Publisher,
    },
    /// Nothing published is newer than its locked version within the request's reach (its locked
    /// version may no longer be published).
    UpToDate { version: String },
    /// Not a registry install (`source` is the lock's, e.g.
    /// `local:<path>`): a registry never replaces it (ADR 0004 D3-b).
    NotFromRegistry { source: String },
    /// Its newer version could not be fetched or checked.
    Failed(OpError),
}

/// One locked extension and what became of it.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionUpdate {
    pub name: String,
    pub status: UpdateStatus,
}

/// What an update did, extension by extension, in lock order.
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateOutcome {
    pub extensions: Vec<ExtensionUpdate>,
}

impl UpdateOutcome {
    /// Whether the updates were written: no extension failed. When one
    /// did, the [`UpdateStatus::Updated`] entries are what would have
    /// changed, and nothing did.
    pub fn applied(&self) -> bool {
        self.failures().next().is_none()
    }

    /// The extensions that moved (or, unapplied, would have).
    pub fn updated(&self) -> impl Iterator<Item = (&str, &str, &str)> {
        self.extensions.iter().filter_map(|e| match &e.status {
            UpdateStatus::Updated { from, to, .. } => {
                Some((e.name.as_str(), from.as_str(), to.as_str()))
            }
            _ => None,
        })
    }

    /// The extensions that failed, with why.
    pub fn failures(&self) -> impl Iterator<Item = (&str, &OpError)> {
        self.extensions.iter().filter_map(|e| match &e.status {
            UpdateStatus::Failed(error) => Some((e.name.as_str(), error)),
            _ => None,
        })
    }

    /// The document `specforge update` prints, stamped `ended_at` (the
    /// adapter passes `Utc::now()`, a test a fixed time).
    pub fn document(&self, ended_at: chrono::DateTime<chrono::Utc>) -> UpdateDocument {
        let applied = self.applied();
        let counts = self.batch_update_completed();
        UpdateDocument {
            ok: applied,
            extensions: self
                .extensions
                .iter()
                .map(|e| {
                    let name = e.name.clone();
                    match &e.status {
                        UpdateStatus::Updated {
                            from,
                            to,
                            sha256,
                            publisher,
                        } if applied => UpdateRow::Updated {
                            name,
                            from: from.clone(),
                            to: to.clone(),
                            sha256: sha256.clone(),
                            key_id: publisher.key_id().map(str::to_string),
                            publisher: publisher.acceptance(),
                        },
                        UpdateStatus::Updated { from, to, .. } => UpdateRow::HeldBack {
                            name,
                            from: from.clone(),
                            to: to.clone(),
                        },
                        UpdateStatus::UpToDate { version } => UpdateRow::UpToDate {
                            name,
                            version: version.clone(),
                        },
                        UpdateStatus::NotFromRegistry { source } => UpdateRow::NotFromRegistry {
                            name,
                            source: source.clone(),
                        },
                        UpdateStatus::Failed(error) => UpdateRow::Failed {
                            name,
                            code: error.code.to_string(),
                            message: error.message.clone(),
                            suggestion: error.suggestion.clone(),
                        },
                    }
                })
                .collect(),
            extension_upgrade_completed: self.extension_upgrade_completed(),
            batch_update_completed: BatchUpdateEvent {
                updated_count: counts.updated_count,
                failed_count: counts.failed_count,
                skipped_count: counts.skipped_count,
                timestamp: ended_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            },
        }
    }

    /// One `extension_upgrade_completed` (`spec/events/wasm-lifecycle.spec`)
    /// per extension the update moved, in lock order: none when it was not
    /// applied, since nothing then changed.
    pub fn extension_upgrade_completed(&self) -> Vec<UpgradeCompleted> {
        if !self.applied() {
            return Vec::new();
        }
        self.updated()
            .map(|(name, from, to)| UpgradeCompleted {
                extension_name: name.to_string(),
                previous_version: from.to_string(),
                new_version: to.to_string(),
            })
            .collect()
    }

    /// The `batch_update_completed` event's counts
    /// (`spec/events/wasm-extensions.spec`). An extension counts as
    /// updated only when the update was applied; one an aborted update
    /// would have moved counts as skipped, since nothing changed. The
    /// adapter that emits the event stamps its `timestamp`.
    pub fn batch_update_completed(&self) -> BatchUpdateCompleted {
        let failed_count = self.failures().count();
        let updated_count = if failed_count == 0 {
            self.updated().count()
        } else {
            0
        };
        BatchUpdateCompleted {
            updated_count,
            failed_count,
            skipped_count: self.extensions.len() - updated_count - failed_count,
        }
    }
}

/// What `specforge update --format json` prints: whether the update was
/// written, what became of each locked extension, an
/// `extension_upgrade_completed` payload per extension it moved and the
/// `batch_update_completed` event's payload.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct UpdateDocument {
    /// No extension failed, so the update was written (`specforge update`
    /// exits 1 otherwise).
    pub ok: bool,
    /// Every locked extension the update looked at, in lock order.
    pub extensions: Vec<UpdateRow>,
    /// One per extension moved, in lock order; empty when nothing was
    /// applied.
    pub extension_upgrade_completed: Vec<UpgradeCompleted>,
    pub batch_update_completed: BatchUpdateEvent,
}

/// `extension_upgrade_completed` (`spec/events/wasm-lifecycle.spec`): one
/// extension an applied update moved, from which version to which.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Shape)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeCompleted {
    pub extension_name: String,
    pub previous_version: String,
    pub new_version: String,
}

/// One locked extension and what became of it.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum UpdateRow {
    /// Moved from `from` to `to`.
    Updated {
        name: String,
        from: String,
        to: String,
        sha256: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        key_id: Option<String>,
        publisher: Acceptance,
    },
    /// Would have moved, but another extension failed, so nothing was
    /// applied.
    HeldBack {
        name: String,
        from: String,
        to: String,
    },
    /// Nothing published is newer than its locked version within the request's reach (its locked
    /// version may no longer be published).
    UpToDate { name: String, version: String },
    /// Not a registry install; a registry never replaces it (ADR 0004 D3-b).
    NotFromRegistry { name: String, source: String },
    /// Its newer version could not be fetched, checked or installed.
    Failed {
        name: String,
        code: String,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        suggestion: Option<String>,
    },
}

/// `batch_update_completed` as `specforge update --format json` carries it
/// (`spec/events/wasm-extensions.spec`): the counts and when the update
/// ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Shape)]
#[serde(rename_all = "camelCase")]
pub struct BatchUpdateEvent {
    pub updated_count: usize,
    pub failed_count: usize,
    pub skipped_count: usize,
    /// RFC 3339, UTC, milliseconds.
    pub timestamp: String,
}

/// The payload of `batch_update_completed`, which an update that ran to
/// the end (applied or rolled back) produces, its timestamp aside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchUpdateCompleted {
    /// Extensions moved to a newer version.
    pub updated_count: usize,
    /// Extensions whose newer version could not be fetched, checked or
    /// installed.
    pub failed_count: usize,
    /// Extensions left as they were: up to date, not from a registry, or
    /// held back because another failed.
    pub skipped_count: usize,
}

/// Update the extensions `req` names in the project at `req.root.dir()`.
///
/// Fails outright (nothing asked, nothing written) with `config_invalid`
/// when `specforge.json` cannot be used, with E033 when the
/// project has no lock file, with `extension_not_found` (E072 for a name
/// that is no package name) when the name it was given is not locked, and with E063 when a registry install needs a
/// registry and none is configured. Fails with the change's error (E032
/// naming the package whose binary could not be placed, E033 for the lock)
/// when applying it fails; everything is put back, and what could not be is
/// the error's writes.
pub fn update(
    req: &UpdateRequest,
    registry: &dyn Registry,
    runtime: &dyn WasmRuntime,
) -> Result<UpdateOutcome, OpError> {
    // A project whose specforge.json cannot be used is refused before
    // anything is read or written, as `add` and `remove` refuse it.
    let read = crate::config::usable(req.root.dir())?;
    let (config, found) = (read.config, read.found);
    let installed = Installed::at(req.root.dir());
    let lock = match installed.lock() {
        LockState::Read(lock) => lock.clone(),
        LockState::Absent => {
            return Err(OpError::coded(
                OpErrorKind::PreconditionFailed,
                NO_LOCK,
                "no lock file found. Run `specforge add` first.",
            ));
        }
        LockState::Unreadable(problem) => {
            return Err(OpError::coded(
                OpErrorKind::PreconditionFailed,
                NO_LOCK,
                format!("{}. Run `specforge add` first.", problem.message),
            ));
        }
    };

    // A name the lock does not hold is refused before any registry is asked.
    if let Some(name) = req.name
        && !lock.entries.iter().any(|entry| entry.name.as_str() == name)
    {
        return Err(not_locked(name));
    }

    // Every newer package is fetched and staged; every upgrade is then judged against the lock as
    // the whole update leaves it, whatever order the lock lists the extensions in.
    let mut plan = LockPlan::updating(
        &installed,
        Opened::read(&config, found),
        runtime,
        Fetching {
            registry,
            allow_unsigned: req.allow_unsigned,
            asking: req.caller.asking(),
        },
    )?;
    let mut extensions = Vec::new();
    for entry in lock
        .entries
        .iter()
        .filter(|e| req.name.is_none_or(|n| e.name.as_str() == n))
    {
        let status = if !entry.source.is_registry() {
            UpdateStatus::NotFromRegistry {
                source: entry.source.to_string(),
            }
        } else {
            match newer(req.reach, registry, &plan, &entry.name, &entry.version) {
                Ok(None) => UpdateStatus::UpToDate {
                    version: entry.version.clone(),
                },
                Ok(Some(install)) => {
                    let status = UpdateStatus::Updated {
                        from: entry.version.clone(),
                        to: install.version().to_string(),
                        sha256: install.sha256().to_string(),
                        publisher: install
                            .publisher()
                            .cloned()
                            .expect("a fetched install has a publisher"),
                    };
                    plan.install(Ask(extensions.len()), install);
                    status
                }
                Err(error) if error.is(NO_REGISTRY) => return Err(error),
                Err(error) => UpdateStatus::Failed(error),
            }
        };
        extensions.push(ExtensionUpdate {
            name: entry.name.to_string(),
            status,
        });
    }

    let checked = match plan.check() {
        Ok(checked) => Some(checked),
        Err(refused) => {
            for (Ask(row), error) in refused.failures() {
                extensions[row].status = UpdateStatus::Failed(error);
            }
            None
        }
    };
    let outcome = UpdateOutcome { extensions };
    // Apply: one change that places every binary and writes the lock once. A failure puts the
    // previous binaries and lock back.
    if let Some(checked) =
        checked.filter(|_| outcome.applied() && outcome.updated().next().is_some())
    {
        checked.finish(req.root.dir(), Vec::new(), req.caller.containment())?;
    }
    Ok(outcome)
}

/// The refusal of an update by a name the lock does not hold: E072 for a name that is no package
/// name, else `extension_not_found`, telling a builtin's name that builtins update with specforge.
fn not_locked(name: &str) -> OpError {
    if let Err(why) = PackageName::parse(name) {
        return OpError::of(
            OpErrorKind::InvalidInput,
            specforge_common::package::invalid(&why),
        );
    }
    let error = super::not_installed(name, None);
    if super::builtin_name(name).is_some() {
        error.with_suggestion("builtins are part of specforge: update specforge to update them")
    } else {
        error
    }
}

/// The package `name` (locked at `locked`) moves to, fetched, or `None` when it is up to date:
/// its upgrade target within `reach` ([`super::upgrade`]).
fn newer<'a>(
    reach: Reach,
    registry: &dyn Registry,
    plan: &LockPlan<'a>,
    name: &PackageName,
    locked: &str,
) -> Result<Option<Install<'a>>, OpError> {
    let Some(target) = super::upgrade(registry, name, &LockedVersion::read(locked), reach)? else {
        return Ok(None);
    };
    plan.fetch(name, &target).map(Some)
}

#[cfg(test)]
mod tests;
