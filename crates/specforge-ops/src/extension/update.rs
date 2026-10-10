//! `specforge update`: move registry installs to newer published versions,
//! through the steps `add` uses (fetch, integrity, trust, the ADR-0001
//! diamond gate, the handshake's name and version check, install).
//!
//! An update is all or nothing (`update_all_extensions`): every newer
//! package is fetched and checked before anything is written, and if one
//! fails, nothing is applied. The binaries and the lock are one change
//! (`specforge_installed::Change`): a failure puts the previous ones back.
//! (A publisher key
//! pinned while checking a signature stays pinned: it records trust, not
//! a change to the project.)

use super::add::{Checked, fetch_checked};
use super::diamond::broken_requirers;
use super::{Trust, published_versions};
use crate::registry::{NO_REGISTRY, Publisher, Registry};
use crate::{OpError, OpErrorKind, Writes};
use specforge_common::{Code, codes};
use specforge_installed::{Installed, LockFile, LockSource, LockState, Pin};
use specforge_protocol_types::PackageName;
use specforge_protocol_types::package::VersionRequirement;
use specforge_wasm::WasmRuntime;
use std::path::Path;

/// The code `update` reports when the project has no lock file.
pub const NO_LOCK: Code = codes::E033;

/// What to update, and how.
#[derive(Debug, Clone)]
pub struct UpdateRequest<'a> {
    pub root: &'a Path,
    /// One extension by name; every locked extension when `None`.
    pub name: Option<&'a str>,
    /// Allow a new major version (`--major`); otherwise an extension only
    /// moves within its locked version's caret range.
    pub major: bool,
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
    /// Already at the newest version the request allows.
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

/// Update the extensions `req` names in the project at `req.root`.
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
    crate::config::usable(req.root)?;
    let installed = Installed::at(req.root);
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

    // Plan: resolve and check every newer package against the lock as it
    // will be, before anything is written.
    let mut staged = lock.clone();
    let mut planned: Vec<(String, Checked)> = Vec::new();
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
            match plan_one(req, registry, runtime, &staged, &entry.name, &entry.version) {
                Ok(None) => UpdateStatus::UpToDate {
                    version: entry.version.clone(),
                },
                Ok(Some(checked)) => {
                    let status = UpdateStatus::Updated {
                        from: entry.version.clone(),
                        to: checked.binary.candidate().version().to_string(),
                        sha256: checked.package.sha256.clone(),
                        publisher: checked.package.publisher.clone(),
                    };
                    if let Some(staged_entry) =
                        staged.entries.iter_mut().find(|e| e.name == entry.name)
                    {
                        staged_entry.version = checked.binary.candidate().version().to_string();
                        staged_entry.peer_dependencies =
                            checked.binary.candidate().peers().to_vec();
                    }
                    planned.push((entry.name.to_string(), checked));
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

    // An update must also leave the extensions that require it satisfied.
    for (dependent, peer) in broken_dependents(&staged, &planned, registry) {
        if let Some(e) = extensions.iter_mut().find(|e| e.name == peer.0) {
            let failed = OpError::new(
                peer.1.kind,
                peer.1.code.clone(),
                format!("updating {} breaks {dependent}: {}", peer.0, peer.1.message),
            );
            e.status = UpdateStatus::Failed(failed);
        }
    }

    let outcome = UpdateOutcome { extensions };
    if !outcome.applied() || planned.is_empty() {
        return Ok(outcome);
    }

    // Apply: one change that places every binary and writes the lock once.
    // A failure puts the previous binaries and lock back.
    let mut change = installed.change().map_err(OpError::from)?;
    for (_, checked) in &planned {
        change.install(
            checked.binary.module().clone(),
            Pin {
                name: checked.package.name.clone(),
                version: checked.binary.candidate().version().to_string(),
                source: LockSource::Registry,
                key_id: checked.package.publisher.key_id().map(str::to_string),
                peers: checked.binary.candidate().peers().to_vec(),
            },
        );
    }
    change
        .commit()
        .map_err(|failed| OpError::from(failed.error).with_writes(Writes::of(failed.left)))?;
    Ok(outcome)
}

/// The newer package `name` (locked at `current`) moves to, checked, or
/// `None` when it is up to date.
fn plan_one(
    req: &UpdateRequest,
    registry: &dyn Registry,
    runtime: &dyn WasmRuntime,
    staged: &LockFile,
    package: &PackageName,
    current: &str,
) -> Result<Option<Checked>, OpError> {
    // Within the caret range of the locked version unless --major: a new
    // major version is a breaking change the user opts into.
    let requirement = match semver::Version::parse(current) {
        Ok(current) if !req.major => VersionRequirement::compatible_with(&current),
        _ => VersionRequirement::Latest,
    };
    let latest = super::resolve_requirement(registry, package, &requirement)?;
    if latest.to_string() == current {
        return Ok(None);
    }
    // The package's own locked peers are the ones it replaces.
    let mut others = staged.clone();
    others.entries.retain(|e| e.name != *package);
    fetch_checked(
        registry,
        runtime,
        &others,
        package,
        &latest,
        req.allow_unsigned,
        req.trust,
    )
    .map(Some)
}

/// Each locked extension a planned update leaves outside its declared peer
/// range: `(dependent, (updated peer, why))`.
fn broken_dependents(
    staged: &LockFile,
    planned: &[(String, Checked)],
    registry: &dyn Registry,
) -> Vec<(String, (String, OpError))> {
    let names: Vec<&str> = planned.iter().map(|(name, _)| name.as_str()).collect();
    let published = published_versions(registry);
    let mut broken: Vec<(String, (String, OpError))> = names
        .iter()
        .flat_map(|name| {
            broken_requirers(staged, name, &names, Some(&published))
                .into_iter()
                .map(move |(dependent, error)| (dependent, (name.to_string(), error)))
        })
        .collect();
    // In lock order, as the dependents are reported.
    broken.sort_by_key(|(dependent, _)| {
        staged
            .entries
            .iter()
            .position(|e| e.name.as_str() == dependent)
    });
    broken
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::{declaration_of, entry, greet, installed, project};
    use super::*;
    use crate::registry::testing::{MemoryRegistry, Published, declaration};
    use specforge_installed::{hex_sha256, lock_path};
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

    fn request(root: &Path, major: bool) -> UpdateRequest<'_> {
        UpdateRequest {
            root,
            name: None,
            major,
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

        let outcome = update(&request(dir.path(), true), &registry, &runtime()).unwrap();

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

        let outcome = update(&request(dir.path(), false), &registry, &runtime()).unwrap();

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

        let outcome = update(&request(dir.path(), true), &registry, &runtime()).unwrap();

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

        let outcome = update(&request(dir.path(), true), &registry, &runtime()).unwrap();

        assert!(!outcome.applied());
        let (name, error) = outcome.failures().next().unwrap();
        assert_eq!(name, "@sdk/greet");
        assert!(error.message.contains("breaks @acme/user"), "{error:?}");
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

        let result = update(&request(dir.path(), true), &registry, &runtime());
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

        let result = update(&request(dir.path(), true), &registry, &runtime());
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

    #[test]
    fn a_local_install_is_never_asked_about() {
        let dir = project(vec![entry("@sdk/greet", "0.0.9", "local:greet.wasm", &[])]);
        let registry = MemoryRegistry::new().serving(old("@sdk/greet", "9.9.9"));

        let outcome = update(&request(dir.path(), true), &registry, &runtime()).unwrap();

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
            &request(empty.path(), false),
            &MemoryRegistry::new(),
            &runtime(),
        )
        .unwrap_err();
        assert!(error.is(NO_LOCK), "{error:?}");

        let dir = project(vec![entry("@sdk/greet", "0.0.9", "registry", &[])]);
        let unconfigured = crate::registry::Unconfigured("update");
        let error = update(&request(dir.path(), false), &unconfigured, &runtime()).unwrap_err();
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
                let error =
                    update(&request(dir.path(), false), &unconfigured, &runtime()).unwrap_err();

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

    /// Pin (plan 01 T0): which extension an update judges against which version depends on the
    /// order the lock lists them in. T1 flips it.
    #[test]
    fn pin_the_lock_order_decides_an_update_that_moves_a_peer_and_its_dependent() {
        let runtime = coupled_runtime();
        // a -> 2.0.0 (wants b ^2) and b -> 2.0.0: fine together.
        let together = || {
            MemoryRegistry::new()
                .serving(served(&runtime, A2))
                .serving(served(&runtime, B2))
        };
        let a_first = coupled(true);
        let outcome = update(&request(a_first.path(), true), &together(), &runtime).unwrap();
        let failures: Vec<(&str, &str)> = outcome
            .failures()
            .map(|(n, e)| (n, e.code.as_ref()))
            .collect();
        assert_eq!(
            failures,
            [("@x/a", "R-RES-006"), ("@x/b", "R-RES-005")],
            "a is judged against b 1.0.0, and b against the a it replaces"
        );
        let b_first = coupled(false);
        assert!(
            update(&request(b_first.path(), true), &together(), &runtime)
                .unwrap()
                .applied()
        );

        // a -> 1.1.0 (still wants b ^1) and b -> 2.0.0: never satisfiable together.
        let apart = || {
            MemoryRegistry::new()
                .serving(served(&runtime, A1_1))
                .serving(old("@x/b", "1.0.0"))
                .serving(served(&runtime, B2))
        };
        let a_first = coupled(true);
        let outcome = update(&request(a_first.path(), true), &apart(), &runtime).unwrap();
        assert!(outcome.applied(), "{outcome:?}");
        let lock = specforge_installed::read_lock_file(&lock_path(a_first.path())).unwrap();
        let locked: Vec<(&str, &str)> = lock
            .entries
            .iter()
            .map(|e| (e.name.as_str(), e.version.as_str()))
            .collect();
        assert_eq!(
            locked,
            [("@x/a", "1.1.0"), ("@x/b", "2.0.0")],
            "a's peer ^1 is left unsatisfied"
        );
        let b_first = coupled(false);
        assert!(
            !update(&request(b_first.path(), true), &apart(), &runtime)
                .unwrap()
                .applied()
        );
    }
}
