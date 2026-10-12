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

use super::Trust;
use super::lock_plan::{Fetch, Install, LockPlan, Verb};
use crate::registry::{Acceptance, NO_REGISTRY, Publisher, Registry};
use crate::{OpError, OpErrorKind};
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
    pub trust: Trust,
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
/// project has no lock file, and with E063 when a registry install needs a
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
    crate::config::usable(req.root.dir())?;
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

    // Every newer package is fetched and staged; every upgrade is then judged against the lock as
    // the whole update leaves it, whatever order the lock lists the extensions in.
    let mut plan = LockPlan::new(&installed, Verb::Update)?;
    let fetch = Fetch {
        installed: &installed,
        registry,
        runtime,
        allow_unsigned: req.allow_unsigned,
        trust: req.trust,
    };
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
            match newer(req.reach, &fetch, &entry.name, &entry.version) {
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
                    plan.stage(install);
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
            for (name, error) in refused.failures {
                if let Some(e) = extensions.iter_mut().find(|e| e.name == name.as_str()) {
                    e.status = UpdateStatus::Failed(error);
                }
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
        checked.commit()?;
    }
    Ok(outcome)
}

/// The package `name` (locked at `locked`) moves to, fetched, or `None` when it is up to date:
/// its upgrade target within `reach` ([`super::upgrade`]).
fn newer<'a>(
    reach: Reach,
    fetch: &Fetch<'a>,
    name: &PackageName,
    locked: &str,
) -> Result<Option<Install<'a>>, OpError> {
    let Some(target) = super::upgrade(fetch.registry, name, &LockedVersion::read(locked), reach)?
    else {
        return Ok(None);
    };
    fetch.install(name, &target).map(Some)
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::{declaration_of, entry, greet, installed, project};
    use super::*;
    use crate::registry::testing::{Asked, MemoryRegistry, Published, declaration};
    use specforge_installed::{LockSource, hex_sha256, lock_path};
    use specforge_test_macros::test as specforge_test;

    /// What the tests serve in process: `@sdk/greet` and `@test/probe`.
    fn runtime() -> specforge_wasm::testing::InProcessRuntime {
        crate::testing::candidates()
    }

    fn name(text: &str) -> PackageName {
        PackageName::parse(text).unwrap()
    }

    /// `@sdk/greet` 0.1.0, a real extension binary, published with the declaration it declares.
    fn greet_published() -> Published {
        Published::new(greet(), declaration_of(&greet()))
    }

    /// `name@version` published with bytes that are no extension.
    fn old(name: &str, version: &str) -> Published {
        Published::new(b"\0asm old".to_vec(), declaration(name, version, &[]))
    }

    fn request(root: &specforge_common::ProjectRoot, reach: Reach) -> UpdateRequest<'_> {
        UpdateRequest {
            root,
            name: None,
            reach,
            allow_unsigned: true,
            trust: Trust::Refuse,
        }
    }

    fn status_of<'a>(outcome: &'a UpdateOutcome, name: &str) -> &'a UpdateStatus {
        &outcome
            .extensions
            .iter()
            .find(|e| e.name == name)
            .unwrap()
            .status
    }

    #[specforge_test(
        behavior = "update_all_extensions",
        verify = "lock file records new binary hashes after update"
    )]
    fn an_update_installs_the_newer_binary_and_locks_its_hash() {
        let dir = project(vec![entry("@sdk/greet", "0.0.9", "registry", &[])]);
        let registry = MemoryRegistry::new()
            .serving(old("@sdk/greet", "0.0.9"))
            .serving(greet_published());

        let outcome = update(
            &request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                Reach::Major,
            ),
            &registry,
            &runtime(),
        )
        .unwrap();

        assert!(outcome.applied(), "{outcome:?}");
        assert_eq!(
            outcome.updated().collect::<Vec<_>>(),
            [("@sdk/greet", "0.0.9", "0.1.0")]
        );
        assert_eq!(
            outcome.batch_update_completed(),
            BatchUpdateCompleted {
                updated_count: 1,
                failed_count: 0,
                skipped_count: 0,
            }
        );
        let lock = specforge_installed::read_lock_file(&lock_path(dir.path())).unwrap();
        assert_eq!(lock.entries[0].version, "0.1.0");
        assert_eq!(lock.entries[0].wasm_hash, hex_sha256(&greet()));
        assert_eq!(lock.entries[0].source, LockSource::Registry);
        let installed = installed(dir.path(), "@sdk/greet");
        assert_eq!(std::fs::read(installed).unwrap(), greet());
    }

    #[specforge_test(
        behavior = "update_all_extensions",
        verify = "major version skipped without --major flag"
    )]
    fn a_new_major_version_needs_major() {
        // ^0.0.9 admits only 0.0.9: 0.1.0 is a breaking change.
        let dir = project(vec![entry("@sdk/greet", "0.0.9", "registry", &[])]);
        let registry = MemoryRegistry::new()
            .serving(old("@sdk/greet", "0.0.9"))
            .serving(greet_published());

        let outcome = update(
            &request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                Reach::Compatible,
            ),
            &registry,
            &runtime(),
        )
        .unwrap();

        // Asked for what ^0.0.9 admits: 0.1.0 is not it.
        assert_eq!(registry.listed(), [name("@sdk/greet")]);
        assert_eq!(
            status_of(&outcome, "@sdk/greet"),
            &UpdateStatus::UpToDate {
                version: "0.0.9".into()
            }
        );
        assert_eq!(
            specforge_installed::read_lock_file(&lock_path(dir.path()))
                .unwrap()
                .entries[0]
                .version,
            "0.0.9"
        );
    }

    #[specforge_test(
        behavior = "update_all_extensions",
        verify = "failed upgrade rolls back all changes"
    )]
    fn one_failed_upgrade_applies_none() {
        let dir = project(vec![
            entry("@sdk/greet", "0.0.9", "registry", &[]),
            entry("@acme/liar", "1.0.0", "registry", &[]),
        ]);
        let lock_before = std::fs::read(lock_path(dir.path())).unwrap();
        // @acme/liar 1.1.0 is published as @acme/liar but its binary is greet's: it
        // declares other than was published, E028, as add refuses it.
        let registry = MemoryRegistry::new()
            .serving(greet_published())
            .serving(Published::new(
                greet(),
                declaration("@acme/liar", "1.1.0", &[]),
            ));

        let outcome = update(
            &request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                Reach::Major,
            ),
            &registry,
            &runtime(),
        )
        .unwrap();

        assert!(!outcome.applied());
        let failures: Vec<(&str, &str)> = outcome
            .failures()
            .map(|(n, e)| (n, e.code.as_ref()))
            .collect();
        assert_eq!(failures, [("@acme/liar", "E028")]);
        // greet would have moved but did not: it is skipped, not updated.
        assert_eq!(
            outcome.batch_update_completed(),
            BatchUpdateCompleted {
                updated_count: 0,
                failed_count: 1,
                skipped_count: 1,
            }
        );
        assert_eq!(std::fs::read(lock_path(dir.path())).unwrap(), lock_before);
        let installed = installed(dir.path(), "@sdk/greet");
        assert_eq!(std::fs::read(installed).unwrap(), b"old");
    }

    #[specforge_test(
        behavior = "update_all_extensions",
        verify = "peer dependency conflicts detected before applying"
    )]
    fn an_update_that_breaks_a_dependent_is_refused() {
        let dir = project(vec![
            entry("@sdk/greet", "0.0.9", "registry", &[]),
            entry(
                "@acme/user",
                "1.0.0",
                "local:user.wasm",
                &[("@sdk/greet", "^0.0.9")],
            ),
        ]);
        let lock_before = std::fs::read(lock_path(dir.path())).unwrap();
        let registry = MemoryRegistry::new().serving(greet_published());

        let outcome = update(
            &request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                Reach::Major,
            ),
            &registry,
            &runtime(),
        )
        .unwrap();

        assert!(!outcome.applied());
        let (name, error) = outcome.failures().next().unwrap();
        assert_eq!(name, "@sdk/greet");
        assert!(
            error
                .message
                .starts_with("updating '@sdk/greet' 0.1.0 breaks '@acme/user': "),
            "{error:?}"
        );
        assert_eq!(std::fs::read(lock_path(dir.path())).unwrap(), lock_before);
    }

    #[cfg(unix)]
    #[specforge_test(
        behavior = "update_all_extensions",
        verify = "failed upgrade rolls back all changes"
    )]
    fn a_lock_that_cannot_be_written_puts_the_old_binaries_back() {
        use std::os::unix::fs::PermissionsExt;
        let dir = project(vec![entry("@sdk/greet", "0.0.9", "registry", &[])]);
        let lock_before = std::fs::read(lock_path(dir.path())).unwrap();
        let registry = MemoryRegistry::new().serving(greet_published());
        // The binaries can be placed, the lock beside them cannot be written.
        let mode = |m| std::fs::Permissions::from_mode(m);
        std::fs::set_permissions(dir.path(), mode(0o555)).unwrap();
        if std::fs::write(dir.path().join("probe"), b"").is_ok() {
            // Running as a user permissions don't bind (root): nothing to test.
            std::fs::set_permissions(dir.path(), mode(0o755)).unwrap();
            return;
        }

        let result = update(
            &request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                Reach::Major,
            ),
            &registry,
            &runtime(),
        );
        std::fs::set_permissions(dir.path(), mode(0o755)).unwrap();

        let error = result.unwrap_err();
        assert_eq!(error.code, "E033", "{error:?}");
        assert_eq!(std::fs::read(lock_path(dir.path())).unwrap(), lock_before);
        let installed = installed(dir.path(), "@sdk/greet");
        assert_eq!(std::fs::read(installed).unwrap(), b"old");
    }

    #[cfg(unix)]
    #[specforge_test(
        behavior = "update_all_extensions",
        verify = "a write that fails while applying fails the update, naming what it could not write, and nothing is applied"
    )]
    fn a_write_that_fails_while_applying_fails_the_update() {
        use std::os::unix::fs::PermissionsExt;
        let dir = project(vec![
            entry("@sdk/greet", "0.0.9", "registry", &[]),
            entry("@test/probe", "0.0.9", "registry", &[]),
        ]);
        let lock_before = std::fs::read(lock_path(dir.path())).unwrap();
        let probe = crate::testing::PROBE.to_vec();
        let registry = MemoryRegistry::new()
            .serving(greet_published())
            .serving(Published::new(probe.clone(), declaration_of(&probe)));
        // The second package's directory cannot take a new file.
        let probe_dir = dir.path().join(".specforge/extensions/@test");
        let mode = |m| std::fs::Permissions::from_mode(m);
        std::fs::set_permissions(&probe_dir, mode(0o555)).unwrap();
        if std::fs::write(probe_dir.join("probe-check"), b"").is_ok() {
            // Running as a user permissions don't bind (root): nothing to test.
            std::fs::set_permissions(&probe_dir, mode(0o755)).unwrap();
            return;
        }

        let result = update(
            &request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                Reach::Major,
            ),
            &registry,
            &runtime(),
        );
        std::fs::set_permissions(&probe_dir, mode(0o755)).unwrap();

        let error = result.unwrap_err();
        assert_eq!(error.code, "E032", "{error:?}");
        assert!(error.message.contains("'@test/probe'"), "{error:?}");
        assert_eq!(std::fs::read(lock_path(dir.path())).unwrap(), lock_before);
        assert_eq!(
            std::fs::read(installed(dir.path(), "@sdk/greet")).unwrap(),
            b"old"
        );
        assert_eq!(
            std::fs::read(installed(dir.path(), "@test/probe")).unwrap(),
            b"old"
        );
    }

    /// Whether `registry` was asked to fetch anything.
    fn fetched(registry: &MemoryRegistry) -> bool {
        registry
            .asked()
            .iter()
            .any(|asked| matches!(asked, Asked::Fetch(..)))
    }

    #[specforge_test(
        behavior = "update_all_extensions",
        verify = "an update never moves an extension to its locked version or below, and a locked version no longer published is up to date"
    )]
    fn major_never_moves_a_pre_release_lock_to_a_lower_release() {
        // --major admits any version above 0.2.0-beta.1; 0.1.0 is below it.
        let dir = project(vec![entry("@sdk/greet", "0.2.0-beta.1", "registry", &[])]);
        let lock_before = std::fs::read(lock_path(dir.path())).unwrap();
        let registry = MemoryRegistry::new()
            .serving(greet_published())
            .serving(old("@sdk/greet", "0.2.0-beta.1"));

        let outcome = update(
            &request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                Reach::Major,
            ),
            &registry,
            &runtime(),
        )
        .unwrap();

        assert_eq!(
            status_of(&outcome, "@sdk/greet"),
            &UpdateStatus::UpToDate {
                version: "0.2.0-beta.1".into()
            }
        );
        assert!(!fetched(&registry), "{:?}", registry.asked());
        assert!(outcome.extension_upgrade_completed().is_empty());
        assert_eq!(std::fs::read(lock_path(dir.path())).unwrap(), lock_before);
    }

    #[specforge_test(
        behavior = "update_all_extensions",
        verify = "an update never moves an extension to its locked version or below, and a locked version no longer published is up to date"
    )]
    fn a_vanished_lock_with_nothing_newer_is_up_to_date() {
        // 0.3.0 is no longer published; only 0.1.0 is.
        for reach in [Reach::Compatible, Reach::Major] {
            let dir = project(vec![entry("@sdk/greet", "0.3.0", "registry", &[])]);
            let lock_before = std::fs::read(lock_path(dir.path())).unwrap();
            let registry = MemoryRegistry::new().serving(greet_published());

            let outcome = update(
                &request(
                    &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                    reach,
                ),
                &registry,
                &runtime(),
            )
            .unwrap();

            assert!(outcome.applied(), "{reach:?}: {outcome:?}");
            assert_eq!(
                status_of(&outcome, "@sdk/greet"),
                &UpdateStatus::UpToDate {
                    version: "0.3.0".into()
                },
                "{reach:?}"
            );
            assert!(!fetched(&registry), "{reach:?}: {:?}", registry.asked());
            assert!(outcome.extension_upgrade_completed().is_empty());
            assert_eq!(std::fs::read(lock_path(dir.path())).unwrap(), lock_before);
        }
    }

    #[specforge_test(
        behavior = "update_all_extensions",
        verify = "an update never moves an extension to its locked version or below, and a locked version no longer published is up to date"
    )]
    fn a_vanished_lock_holds_back_no_other_upgrade() {
        // @x/gone 1.2.0 is no longer published; only 1.0.0 is: it is up to date, and greet's
        // upgrade (0.1.0-alpha.1 -> 0.1.0, inside ^0.1.0-alpha.1) is applied.
        let dir = project(vec![
            entry("@sdk/greet", "0.1.0-alpha.1", "registry", &[]),
            entry("@x/gone", "1.2.0", "registry", &[]),
        ]);
        let registry = MemoryRegistry::new()
            .serving(greet_published())
            .serving(old("@x/gone", "1.0.0"));

        let outcome = update(
            &request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                Reach::Compatible,
            ),
            &registry,
            &runtime(),
        )
        .unwrap();

        assert!(outcome.applied(), "{outcome:?}");
        assert_eq!(
            outcome.updated().collect::<Vec<_>>(),
            [("@sdk/greet", "0.1.0-alpha.1", "0.1.0")]
        );
        assert_eq!(
            status_of(&outcome, "@x/gone"),
            &UpdateStatus::UpToDate {
                version: "1.2.0".into()
            }
        );
        assert_eq!(
            outcome.batch_update_completed(),
            BatchUpdateCompleted {
                updated_count: 1,
                failed_count: 0,
                skipped_count: 1,
            }
        );
        assert_eq!(
            outcome.extension_upgrade_completed(),
            [UpgradeCompleted {
                extension_name: "@sdk/greet".into(),
                previous_version: "0.1.0-alpha.1".into(),
                new_version: "0.1.0".into(),
            }]
        );
        let lock = specforge_installed::read_lock_file(&lock_path(dir.path())).unwrap();
        let locked: Vec<(&str, &str)> = lock
            .entries
            .iter()
            .map(|e| (e.name.as_str(), e.version.as_str()))
            .collect();
        assert_eq!(locked, [("@sdk/greet", "0.1.0"), ("@x/gone", "1.2.0")]);
    }

    #[specforge_test(
        behavior = "update_all_extensions",
        verify = "newer versions detected from registry"
    )]
    fn an_unreadable_locked_version_moves_to_what_add_installs() {
        // A lock edited by hand to a version that is no SemVer: either reach moves it to what
        // `latest` picks (the highest release), which is what `specforge add <name>` installs.
        for reach in [Reach::Compatible, Reach::Major] {
            let dir = project(vec![entry("@sdk/greet", "not-semver", "registry", &[])]);
            let registry = MemoryRegistry::new()
                .serving(greet_published())
                .serving(old("@sdk/greet", "0.2.0-beta.1"));

            let outcome = update(
                &request(
                    &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                    reach,
                ),
                &registry,
                &runtime(),
            )
            .unwrap();

            assert!(outcome.applied(), "{reach:?}: {outcome:?}");
            assert_eq!(
                outcome.updated().collect::<Vec<_>>(),
                [("@sdk/greet", "not-semver", "0.1.0")],
                "{reach:?}"
            );
        }
    }

    const T1_5: &[u8] = b"\0asm in-process @x/t 1.5.0";
    const T2: &[u8] = b"\0asm in-process @x/t 2.0.0";
    const P_BETA_2: &[u8] = b"\0asm in-process @x/p 2.0.0-beta.2";

    #[specforge_test(
        behavior = "update_all_extensions",
        verify = "semver-compatible upgrades applied"
    )]
    fn an_update_moves_to_the_newest_version_within_its_reach() {
        let runtime = crate::testing::candidates()
            .binary(T1_5, declaring("@x/t", "1.5.0", &[]))
            .binary(T2, declaring("@x/t", "2.0.0", &[]));
        for (reach, want) in [(Reach::Compatible, "1.5.0"), (Reach::Major, "2.0.0")] {
            let dir = project(vec![entry("@x/t", "1.4.0", "registry", &[])]);
            let registry = MemoryRegistry::new()
                .serving(old("@x/t", "1.4.0"))
                .serving(served(&runtime, T1_5))
                .serving(served(&runtime, T2));

            let outcome = update(
                &request(
                    &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                    reach,
                ),
                &registry,
                &runtime,
            )
            .unwrap();

            assert!(outcome.applied(), "{reach:?}: {outcome:?}");
            assert_eq!(
                outcome.updated().collect::<Vec<_>>(),
                [("@x/t", "1.4.0", want)],
                "{reach:?}"
            );
        }
    }

    #[specforge_test(
        behavior = "update_all_extensions",
        verify = "an update never moves an extension to its locked version or below, and a locked version no longer published is up to date"
    )]
    fn a_pre_release_lock_moves_along_its_own_track() {
        let runtime =
            crate::testing::candidates().binary(P_BETA_2, declaring("@x/p", "2.0.0-beta.2", &[]));
        for reach in [Reach::Compatible, Reach::Major] {
            let dir = project(vec![entry("@x/p", "2.0.0-beta.1", "registry", &[])]);
            let registry = MemoryRegistry::new()
                .serving(old("@x/p", "1.4.0"))
                .serving(old("@x/p", "2.0.0-beta.1"))
                .serving(served(&runtime, P_BETA_2));

            let outcome = update(
                &request(
                    &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                    reach,
                ),
                &registry,
                &runtime,
            )
            .unwrap();

            assert!(outcome.applied(), "{reach:?}: {outcome:?}");
            assert_eq!(
                outcome.updated().collect::<Vec<_>>(),
                [("@x/p", "2.0.0-beta.1", "2.0.0-beta.2")],
                "{reach:?}"
            );
        }
    }

    #[test]
    fn a_local_install_is_never_asked_about() {
        let dir = project(vec![entry("@sdk/greet", "0.0.9", "local:greet.wasm", &[])]);
        let registry = MemoryRegistry::new().serving(old("@sdk/greet", "9.9.9"));

        let outcome = update(
            &request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                Reach::Major,
            ),
            &registry,
            &runtime(),
        )
        .unwrap();

        assert!(registry.asked().is_empty());
        assert_eq!(
            status_of(&outcome, "@sdk/greet"),
            &UpdateStatus::NotFromRegistry {
                source: "local:greet.wasm".into()
            }
        );
    }

    #[test]
    fn no_lock_and_no_registry_fail_outright() {
        let empty = tempfile::tempdir().unwrap();
        let error = update(
            &request(
                &specforge_common::ProjectRoot::of(empty.path()).unwrap(),
                Reach::Compatible,
            ),
            &MemoryRegistry::new(),
            &runtime(),
        )
        .unwrap_err();
        assert!(error.is(NO_LOCK), "{error:?}");

        let dir = project(vec![entry("@sdk/greet", "0.0.9", "registry", &[])]);
        let unconfigured = crate::registry::Unconfigured("update");
        let error = update(
            &request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                Reach::Compatible,
            ),
            &unconfigured,
            &runtime(),
        )
        .unwrap_err();
        assert!(error.is(NO_REGISTRY), "{error:?}");
    }

    #[specforge_test(
        behavior = "management_operations_over_the_project_view",
        verify = "add, update and remove refuse an unusable specforge.json with one refusal, before they write"
    )]
    fn an_unusable_config_is_refused_before_the_lock_is_read() {
        use crate::config::testing::{UNUSABLE, files_under};

        for config in UNUSABLE {
            // With a lock and without one: the config is refused first.
            for locked in [true, false] {
                let dir = if locked {
                    project(vec![entry("@sdk/greet", "0.0.9", "registry", &[])])
                } else {
                    tempfile::tempdir().unwrap()
                };
                std::fs::write(dir.path().join("specforge.json"), config).unwrap();
                let before = files_under(dir.path());
                let read = specforge_common::read_project_config(dir.path());
                let refused = crate::config::refusal(&read.problems[0]);

                let unconfigured = crate::registry::Unconfigured("update");
                let error = update(
                    &request(
                        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                        Reach::Compatible,
                    ),
                    &unconfigured,
                    &runtime(),
                )
                .unwrap_err();

                assert_eq!(error, refused, "{config}, locked: {locked}");
                assert_eq!(files_under(dir.path()), before, "{config}");
            }
        }
    }

    use crate::testing::declaring;
    use specforge_wasm::testing::InProcessRuntime;

    const A1_1: &[u8] = b"\0asm in-process @x/a 1.1.0";
    const A2: &[u8] = b"\0asm in-process @x/a 2.0.0";
    const B2: &[u8] = b"\0asm in-process @x/b 2.0.0";

    /// `@x/a` 1.1.0 (wants `@x/b` ^1), `@x/a` 2.0.0 (wants ^2) and `@x/b` 2.0.0, served in process.
    fn coupled_runtime() -> InProcessRuntime {
        crate::testing::candidates()
            .binary(A1_1, declaring("@x/a", "1.1.0", &[("@x/b", "^1", false)]))
            .binary(A2, declaring("@x/a", "2.0.0", &[("@x/b", "^2", false)]))
            .binary(B2, declaring("@x/b", "2.0.0", &[]))
    }

    /// `wasm` published with the declaration it declares in `runtime`.
    fn served(runtime: &InProcessRuntime, wasm: &[u8]) -> Published {
        Published::new(
            wasm.to_vec(),
            crate::publish::declare(runtime, wasm).unwrap().0,
        )
    }

    /// A project locking `@x/a` 1.0.0 (wanting `@x/b` ^1) and `@x/b` 1.0.0, `@x/a` first or second.
    fn coupled(a_first: bool) -> tempfile::TempDir {
        let a = entry("@x/a", "1.0.0", "registry", &[("@x/b", "^1")]);
        let b = entry("@x/b", "1.0.0", "registry", &[]);
        project(if a_first { vec![a, b] } else { vec![b, a] })
    }

    #[specforge_test(
        behavior = "update_all_extensions",
        verify = "every upgrade is judged against the lock as the whole update leaves it, whatever order the lock lists them in"
    )]
    fn an_update_is_judged_against_the_lock_it_leaves_in_any_lock_order() {
        let runtime = coupled_runtime();
        for a_first in [true, false] {
            // a -> 2.0.0 (wants b ^2) and b -> 2.0.0: fine together.
            let together = || {
                MemoryRegistry::new()
                    .serving(served(&runtime, A2))
                    .serving(served(&runtime, B2))
            };
            let dir = coupled(a_first);
            let outcome = update(
                &request(
                    &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                    Reach::Major,
                ),
                &together(),
                &runtime,
            )
            .unwrap();
            assert!(outcome.applied(), "a first: {a_first}: {outcome:?}");
            let lock = specforge_installed::read_lock_file(&lock_path(dir.path())).unwrap();
            let locked: Vec<(&str, &str)> = lock
                .entries
                .iter()
                .map(|e| (e.name.as_str(), e.version.as_str()))
                .collect();
            let mut want = [("@x/a", "2.0.0"), ("@x/b", "2.0.0")];
            if !a_first {
                want.reverse();
            }
            assert_eq!(locked, want, "the lock keeps its order");

            // a -> 1.1.0 (still wants b ^1) and b -> 2.0.0: never satisfiable together.
            let apart = MemoryRegistry::new()
                .serving(served(&runtime, A1_1))
                .serving(old("@x/b", "1.0.0"))
                .serving(served(&runtime, B2));
            let dir = coupled(a_first);
            let lock_before = std::fs::read(lock_path(dir.path())).unwrap();
            let outcome = update(
                &request(
                    &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                    Reach::Major,
                ),
                &apart,
                &runtime,
            )
            .unwrap();
            assert!(!outcome.applied(), "a first: {a_first}: {outcome:?}");
            let failures: Vec<(&str, &str, &str)> = outcome
                .failures()
                .map(|(n, e)| (n, e.code.as_ref(), e.message.as_str()))
                .collect();
            assert_eq!(
                failures,
                [(
                    "@x/a",
                    "R-RES-006",
                    "version diamond: '@x/a' requires peer '@x/b' ^1 but this update locks 2.0.0; @x/b 1.0.0 would satisfy every requirer"
                )]
            );
            assert!(matches!(
                status_of(&outcome, "@x/b"),
                UpdateStatus::Updated { .. }
            ));
            assert_eq!(std::fs::read(lock_path(dir.path())).unwrap(), lock_before);
        }
    }

    #[specforge_test(
        behavior = "update_all_extensions",
        verify = "the update document lists every locked extension's status, held back when another failed"
    )]
    fn update_document_lists_each_status() {
        let row = |name: &str, status| ExtensionUpdate {
            name: name.into(),
            status,
        };
        let outcome = UpdateOutcome {
            extensions: vec![
                row(
                    "@a/moved",
                    UpdateStatus::Updated {
                        from: "1.0.0".into(),
                        to: "1.1.0".into(),
                        sha256: "ab".into(),
                        publisher: Publisher::Unsigned,
                    },
                ),
                row(
                    "@a/same",
                    UpdateStatus::UpToDate {
                        version: "2.0.0".into(),
                    },
                ),
                row(
                    "@a/local",
                    UpdateStatus::NotFromRegistry {
                        source: "local:x.wasm".into(),
                    },
                ),
                row(
                    "@a/gone",
                    UpdateStatus::Failed(
                        OpError::diagnostic(codes::E063, "no registry").with_suggestion("add one"),
                    ),
                ),
            ],
        };
        let ended_at = chrono::DateTime::parse_from_rfc3339("2026-10-11T12:00:00.123Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        let document = serde_json::to_value(outcome.document(ended_at)).unwrap();

        assert_eq!(document["ok"], false);
        assert_eq!(
            document["extensions"],
            serde_json::json!([
                {"status": "held_back", "name": "@a/moved", "from": "1.0.0", "to": "1.1.0"},
                {"status": "up_to_date", "name": "@a/same", "version": "2.0.0"},
                {"status": "not_from_registry", "name": "@a/local", "source": "local:x.wasm"},
                {"status": "failed", "name": "@a/gone", "code": "E063", "message": "no registry",
                 "suggestion": "add one"},
            ])
        );
        assert_eq!(
            document["batch_update_completed"],
            serde_json::json!({"updatedCount": 0, "failedCount": 1, "skippedCount": 3,
                "timestamp": "2026-10-11T12:00:00.123Z"})
        );
    }
}
