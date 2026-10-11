//! `specforge remove` and `specforge.remove_extension`.

use super::{NOT_FOUND, Origin, builtin_name};
use crate::view::ProjectView;
use crate::{OpError, OpErrorKind, Writes};
use specforge_common::{ExtensionEntry, codes};
use specforge_graph::Graph;
use specforge_installed::{Installed, LockFile, LockState};
use specforge_project::EnabledExtension;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_protocol_types::PackageName;
use specforge_registry::KindRegistry;
use std::path::Path;

/// What to remove from the project the view was compiled from.
pub struct RemoveRequest<'a> {
    pub name: &'a str,
    /// Remove even when another extension requires it.
    pub force: bool,
    /// Report what would be removed; change nothing.
    pub dry_run: bool,
}

/// What a removal reads of its project: the view's root, what each
/// `specforge.json` entry enabled in the compile (how a `.wasm` file entry
/// is found by the name its component declared), the declarations it
/// loaded, its kinds and its graph.
struct Removing<'a> {
    root: &'a Path,
    name: &'a str,
    force: bool,
    dry_run: bool,
    enabled: &'a [EnabledExtension],
    loaded: &'a [ExtensionDeclaration],
    kinds: &'a KindRegistry,
    graph: &'a Graph,
    /// The project's installed extensions, as the compile read them.
    installed: &'a Installed,
}

/// A stranded entity (CONTEXT.md): one whose kind only the removed
/// extension declares; the next compile reports it E024.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrandedEntity {
    pub entity_id: String,
    pub kind: String,
}

impl StrandedEntity {
    /// `<kind> '<id>' uses a kind only <extension> defines`.
    pub fn warning(&self, extension: &str) -> String {
        format!(
            "{} '{}' uses a kind only {extension} defines",
            self.kind, self.entity_id
        )
    }
}

/// What a removal did (or, on a dry run, would do).
#[derive(Debug, Clone, PartialEq)]
pub struct RemoveOutcome {
    pub name: String,
    /// The locked version; for a builtin, the loaded one, if it loaded.
    pub version: Option<String>,
    pub origin: Origin,
    /// The entities the removal strands, in id order: their kind only the
    /// removed extension defines, so they fail E024 on the next compile.
    pub stranded: Vec<StrandedEntity>,
    pub dry_run: bool,
    /// The files the removal changed: `specforge.json` when an entry was
    /// dropped, and for an uninstall `specforge.lock` and each file deleted
    /// under the extension's directory. Nothing for a dry run.
    pub writes: Writes,
}

/// Remove `name` from the project the view was compiled from: a locked
/// install is uninstalled (binary, lock entry and `specforge.json` entry);
/// a builtin is disabled (its `specforge.json` entry); a `.wasm` file
/// entry, named by the extension it loaded as, its entry or its path, is
/// dropped from `specforge.json` (the file and the lock are left alone).
/// Refused with E027 while another loaded or locked extension requires it
/// as a non-optional peer, unless `force`; with `extension_conflict` when
/// more than one entry enables `name`.
///
/// Every refusal is decided from the view before anything is written: a
/// `specforge.json` the compile could not edit (a problem that
/// [`blocks_edits`](specforge_common::ConfigProblem::blocks_edits), which
/// the compile reported as E069) refuses every removal with
/// `config_invalid`, without reading the file again. A removal is all or
/// nothing: the binary, the lock entry and the `specforge.json` entry go as
/// one change, and a failure at any step puts every file back (the error's
/// [`OpError::writes`] names what the rollback could not, normally
/// nothing). Without a root: `no_project`.
pub fn remove(view: &ProjectView, req: &RemoveRequest) -> Result<RemoveOutcome, OpError> {
    let root = view.project_root()?;
    if let Some(problem) = view
        .env()
        .config_problems
        .iter()
        .find(|problem| problem.blocks_edits())
    {
        return Err(crate::config::refusal(problem));
    }
    let req = &Removing {
        root,
        name: req.name,
        force: req.force,
        dry_run: req.dry_run,
        enabled: &view.env().enabled,
        loaded: view.registries().declarations(),
        kinds: &view.registries().kinds,
        graph: view.graph(),
        installed: view.installed(),
    };
    // The `.wasm` file entries `name` names, and whether a named entry
    // (legacy `name@version` duplicates included) enables it too.
    let files: Vec<&EnabledExtension> = req
        .enabled
        .iter()
        .filter(|e| {
            e.file
                .as_deref()
                .is_some_and(|path| e.entry == req.name || path == req.name || e.name == req.name)
        })
        .collect();
    let named: Vec<&EnabledExtension> = req
        .enabled
        .iter()
        .filter(|e| e.file.is_none() && e.name == req.name)
        .collect();
    match (files.as_slice(), named.is_empty()) {
        ([], _) => {}
        ([file], true) => return remove_file(req, file),
        _ => {
            // In the order specforge.json lists them.
            let entries: Vec<&str> = req
                .enabled
                .iter()
                .filter(|e| files.contains(e) || named.contains(e))
                .map(|e| e.entry.as_str())
                .collect();
            let quoted: Vec<String> = entries.iter().map(|e| format!("'{e}'")).collect();
            let error = OpError::new(
                OpErrorKind::Conflict,
                "extension_conflict",
                format!(
                    "'{}' is enabled by {} specforge.json entries: {}",
                    req.name,
                    quoted.len(),
                    quoted.join(", ")
                ),
            )
            .with_suggestion(
                "remove one entry by its text as specforge.json writes it, or edit specforge.json",
            );
            return Err(
                error.with_data(serde_json::json!({"extension": req.name, "entries": entries}))
            );
        }
    }
    if ExtensionEntry::parse(req.name)
        .file(Path::new(""))
        .is_some()
    {
        return Err(OpError::new(
            OpErrorKind::ExtensionNotFound,
            NOT_FOUND,
            format!(
                "extension '{}' is not enabled: no specforge.json entry names that file",
                req.name
            ),
        )
        .with_suggestion("`specforge extensions` lists what the project enables")
        .with_data(serde_json::json!({ "extension": req.name })));
    }

    // What is deleted below is a directory named by this argument: it must
    // be a package name (E072), before anything is read or removed.
    let package = PackageName::parse(req.name)
        .map_err(|why| OpError::from(specforge_common::package::invalid(&why)))?;

    let lock = req.installed.lock().file();
    let locked = req
        .installed
        .lock()
        .entries()
        .iter()
        .find(|e| e.name.as_str() == req.name);

    let (version, origin) = match (locked, builtin_name(req.name)) {
        (Some(entry), _) => (
            Some(entry.version.clone()),
            Origin::Installed {
                source: entry.source.clone(),
            },
        ),
        (None, Some(builtin)) => {
            // Enabled when an entry of the compile's config names it.
            if !req
                .enabled
                .iter()
                .any(|e| e.file.is_none() && e.name == builtin)
            {
                return Err(not_installed(req.name, None));
            }
            let loaded = req.loaded.iter().find(|d| d.name() == builtin);
            (loaded.map(|d| d.version().to_string()), Origin::Builtin)
        }
        (None, None) => {
            let why = match req.installed.lock() {
                LockState::Absent => Some("no lock file found"),
                LockState::Unreadable(problem) => Some(problem.message.as_str()),
                LockState::Read(_) => None,
            };
            return Err(not_installed(req.name, why));
        }
    };

    refuse_if_required(req, req.name, lock)?;

    let stranded = stranded(req.graph, req.kinds, req.name);
    let mut outcome = RemoveOutcome {
        name: req.name.to_string(),
        version,
        stranded,
        dry_run: req.dry_run,
        origin,
        writes: Writes::none(),
    };
    if req.dry_run {
        return Ok(outcome);
    }

    // An installed extension: its binary, its lock entry and its
    // specforge.json entry go as one change, all or nothing. Anything else
    // (a builtin) is its specforge.json entry alone; a project without
    // specforge.json (an install only the lock knows) has no entry to drop.
    let drop_entry = || match crate::config::remove_extension(req.root, req.name) {
        Err(e) if e.code == "config_not_found" => Ok(false),
        other => other,
    };
    let config_file = req.root.join(crate::config::CONFIG_FILE);
    if matches!(outcome.origin, Origin::Installed { .. }) && lock.is_some() {
        // Dependents are checked above, over the loaded declarations and the lock.
        let mut change = req.installed.change().map_err(OpError::from)?;
        change.uninstall(&package);
        let committed = change
            .commit_with(&config_file, drop_entry)
            .map_err(|failed| failed.error.with_writes(Writes::of(failed.left)))?;
        outcome.writes = Writes::of(committed.changed);
    } else {
        let dropped = drop_entry()?;
        outcome.writes.record_if(dropped, config_file);
    }
    Ok(outcome)
}

/// Remove the `.wasm` file entry `file`: only its `specforge.json` entry
/// goes. The extension it loaded as (if it loaded) is what dependents and
/// stranded entities are checked against.
fn remove_file(req: &Removing, file: &EnabledExtension) -> Result<RemoveOutcome, OpError> {
    let declaration = req.loaded.iter().find(|d| d.name() == file.name);
    if declaration.is_some() {
        refuse_if_required(req, &file.name, req.installed.lock().file())?;
    }
    let stranded = stranded(req.graph, req.kinds, &file.name);
    let mut outcome = RemoveOutcome {
        name: file.name.clone(),
        version: declaration.map(|d| d.version().to_string()),
        stranded,
        dry_run: req.dry_run,
        origin: Origin::File {
            path: file.file.clone().unwrap_or_default(),
        },
        writes: Writes::none(),
    };
    if !req.dry_run {
        let dropped = crate::config::remove_entry(req.root, &file.entry)?;
        outcome
            .writes
            .record_if(dropped, req.root.join(crate::config::CONFIG_FILE));
    }
    Ok(outcome)
}

/// E027 when another loaded or locked extension requires `name` as a
/// non-optional peer, unless the request forces it.
fn refuse_if_required(req: &Removing, name: &str, lock: Option<&LockFile>) -> Result<(), OpError> {
    let dependents = dependents(name, req.loaded, lock);
    if dependents.is_empty() || req.force {
        return Ok(());
    }
    Err(OpError::diagnostic(
        codes::E027,
        format!(
            "cannot uninstall '{}': required by {}",
            name,
            dependents.join(", ")
        ),
    )
    .with_suggestion("use --force to uninstall anyway, or remove dependent extensions first"))
}

fn not_installed(name: &str, why: Option<&str>) -> OpError {
    let mut message = format!("extension '{name}' is not installed");
    if let Some(why) = why {
        message.push_str(&format!(" ({why})"));
    }
    OpError::new(OpErrorKind::ExtensionNotFound, NOT_FOUND, message)
        .with_data(serde_json::json!({ "extension": name }))
}

/// The extensions that require `name` as a non-optional peer: loaded ones
/// (their handshake) and locked ones (the peers recorded at install).
fn dependents(name: &str, loaded: &[ExtensionDeclaration], lock: Option<&LockFile>) -> Vec<String> {
    let requires = |peers: &[specforge_registry::PeerDependency]| {
        peers.iter().any(|p| p.name == name && !p.optional)
    };
    let mut out: Vec<String> = loaded
        .iter()
        .filter(|d| d.name() != name && requires(d.peers()))
        .map(|d| d.name().to_string())
        .chain(
            lock.iter()
                .flat_map(|lock| &lock.entries)
                .filter(|e| e.name.as_str() != name && requires(&e.peer_dependencies))
                .map(|e| e.name.to_string()),
        )
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The entities whose kind only `extension` defines, in id order.
fn stranded(graph: &Graph, kinds: &KindRegistry, extension: &str) -> Vec<StrandedEntity> {
    // `Graph::nodes` is in id order.
    graph
        .nodes()
        .into_iter()
        .filter(|node| {
            kinds
                .get(node.kind.raw.as_str())
                .is_some_and(|kind| kind.source_extension == extension)
        })
        .map(|node| StrandedEntity {
            entity_id: node.id.raw.to_string(),
            kind: node.kind.raw.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::testing::Fixture;
    use specforge_common::ConfigProblem;
    use specforge_test_macros::test as specforge_test;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    /// Every file under `root` with its bytes: what a refused removal must
    /// leave as it was.
    fn files_under(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut files = BTreeMap::new();
        let mut dirs = vec![root.to_path_buf()];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dirs.push(path);
                } else {
                    files.insert(path.clone(), std::fs::read(&path).unwrap());
                }
            }
        }
        files
    }

    /// Where the installed extension `name` lives under `root`.
    fn package_dir(root: &Path, name: &str) -> PathBuf {
        Installed::unread(root).package_dir(&PackageName::parse(name).unwrap())
    }

    /// The text of the file at `path`.
    fn text(path: PathBuf) -> String {
        String::from_utf8(std::fs::read(path).unwrap()).unwrap()
    }

    fn removing(name: &str) -> RemoveRequest<'_> {
        RemoveRequest {
            name,
            force: false,
            dry_run: false,
        }
    }

    fn write_config(fixture: &Fixture, text: &str) {
        std::fs::write(fixture.dir.path().join("specforge.json"), text).unwrap();
    }

    /// `name` installed at the fixture's root: a lock entry and a binary.
    fn installed(fixture: Fixture, name: &str) -> Fixture {
        let fixture = fixture.lock(&[(name, "1.0.0", "registry")]);
        let dir = package_dir(fixture.dir.path(), name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("extension.wasm"), b"\0asm").unwrap();
        fixture
    }

    #[test]
    fn a_rootless_view_cannot_remove() {
        let fixture = Fixture::new().config(&["@specforge/product"]);
        write_config(&fixture, r#"{"extensions": ["@specforge/product"]}"#);

        let error = remove(&fixture.rootless_view(), &removing("@specforge/product")).unwrap_err();

        assert_eq!(error.code, "no_project");
        assert_eq!(
            text(fixture.dir.path().join("specforge.json")),
            r#"{"extensions": ["@specforge/product"]}"#
        );
    }

    #[specforge_test(
        behavior = "remove_extension",
        verify = "extension is removed from extensions list"
    )]
    fn a_builtin_named_by_the_view_is_removed_from_the_config() {
        let fixture = Fixture::new().config(&["@specforge/product"]);
        write_config(&fixture, r#"{"extensions": ["@specforge/product"]}"#);

        let outcome = remove(&fixture.view(), &removing("@specforge/product")).unwrap();

        assert_eq!(outcome.origin, Origin::Builtin);
        let config: serde_json::Value =
            serde_json::from_str(&text(fixture.dir.path().join("specforge.json"))).unwrap();
        assert_eq!(config["extensions"], serde_json::json!([]));

        // What the view enabled decides, not the file: a view that enabled
        // nothing refuses, whatever is on disk.
        let other = Fixture::new();
        write_config(&other, r#"{"extensions": ["@specforge/product"]}"#);
        let error = remove(&other.view(), &removing("@specforge/product")).unwrap_err();
        assert_eq!(error.code, NOT_FOUND);
    }

    #[specforge_test(
        behavior = "remove_extension",
        verify = "a name more than one specforge.json entry enables is refused as ambiguous, naming the entries"
    )]
    fn a_name_two_entries_enable_is_refused() {
        let fixture = Fixture::new().enabled(vec![
            EnabledExtension::unloaded("@sdk/greet"),
            EnabledExtension {
                entry: "greet.wasm".into(),
                name: "@sdk/greet".into(),
                file: Some("greet.wasm".into()),
                failure: None,
            },
        ]);
        write_config(&fixture, r#"{"extensions": ["@sdk/greet", "greet.wasm"]}"#);
        let before = files_under(fixture.dir.path());

        let error = remove(&fixture.view(), &removing("@sdk/greet")).unwrap_err();

        assert_eq!(error.code, "extension_conflict");
        assert_eq!(
            error.data.unwrap()["entries"],
            serde_json::json!(["@sdk/greet", "greet.wasm"])
        );
        assert_eq!(files_under(fixture.dir.path()), before);
    }

    #[specforge_test(
        behavior = "remove_extension",
        verify = "a removal with an unreadable specforge.json is config_invalid and changes nothing"
    )]
    fn a_non_object_config_refuses_the_removal_before_any_write() {
        let fixture = installed(Fixture::new(), "@acme/x").config_problems(vec![
            ConfigProblem::NotAnObject {
                path: PathBuf::from("./specforge.json"),
            },
        ]);
        write_config(&fixture, "[1,2]");
        let before = files_under(fixture.dir.path());

        for dry_run in [false, true] {
            let request = RemoveRequest {
                name: "@acme/x",
                force: true,
                dry_run,
            };
            let error = remove(&fixture.view(), &request).unwrap_err();

            assert_eq!(error.code, "config_invalid");
            assert_eq!(error.message, "./specforge.json must be a JSON object");
            assert_eq!(files_under(fixture.dir.path()), before);
        }
    }

    #[specforge_test(
        behavior = "management_operations_over_the_project_view",
        verify = "add, update and remove refuse an unusable specforge.json with one refusal, before they write"
    )]
    fn remove_refuses_as_add_and_update_do() {
        use crate::config::testing::UNUSABLE;
        use crate::extension::{AddRequest, Source, Trust, UpdateRequest, add, update};

        for config in UNUSABLE {
            let fixture = installed(Fixture::new(), "@acme/x");
            write_config(&fixture, config);
            let read = specforge_common::read_project_config(fixture.dir.path());
            // The compile reported these problems; the others read them.
            let fixture = fixture.config_problems(read.problems.clone());
            let before = files_under(fixture.dir.path());
            let unconfigured = crate::registry::Unconfigured("both");

            let removed = remove(&fixture.view(), &removing("@acme/x")).unwrap_err();
            let added = add(
                &AddRequest {
                    root: fixture.dir.path(),
                    source: Source::Builtin("@specforge/product"),
                    allow_unsigned: false,
                    trust: Trust::Refuse,
                    dry_run: false,
                },
                &unconfigured,
                &specforge_wasm::testing::InProcessRuntime::new(),
            )
            .unwrap_err();
            let updated = update(
                &UpdateRequest {
                    root: fixture.dir.path(),
                    name: None,
                    major: false,
                    allow_unsigned: true,
                    trust: Trust::Refuse,
                },
                &unconfigured,
                &specforge_wasm::testing::InProcessRuntime::new(),
            )
            .unwrap_err();

            assert_eq!(removed.code, "config_invalid", "{config}");
            assert_eq!(removed.kind, OpErrorKind::SchemaMismatch, "{config}");
            assert_eq!(removed.message, read.problems[0].to_string(), "{config}");
            assert_eq!(added, removed, "{config}");
            assert_eq!(updated, removed, "{config}");
            assert_eq!(files_under(fixture.dir.path()), before, "{config}");
        }
    }

    #[specforge_test(
        behavior = "remove_extension",
        verify = "a removal with an unreadable specforge.json is config_invalid and changes nothing"
    )]
    fn a_non_string_item_does_not_block_a_removal() {
        let fixture = Fixture::new()
            .config(&["@specforge/product"])
            .config_problems(vec![ConfigProblem::ItemNotAString {
                path: PathBuf::from("./specforge.json"),
                key: "extensions",
                index: 1,
                json: "42".into(),
            }]);
        write_config(&fixture, r#"{"extensions": ["@specforge/product", 42]}"#);

        let outcome = remove(&fixture.view(), &removing("@specforge/product")).unwrap();

        assert_eq!(outcome.origin, Origin::Builtin);
        let config: serde_json::Value =
            serde_json::from_str(&text(fixture.dir.path().join("specforge.json"))).unwrap();
        assert_eq!(config["extensions"], serde_json::json!([42]));
    }

    #[test]
    fn an_install_is_dropped_from_the_config_before_it_is_uninstalled() {
        let fixture = installed(Fixture::new().config(&["@acme/x"]), "@acme/x");
        write_config(&fixture, r#"{"extensions": ["@acme/x"]}"#);
        // specforge.json can't be written: the lock and the binary stay.
        let config = fixture.dir.path().join("specforge.json");
        let mut readonly = std::fs::metadata(&config).unwrap().permissions();
        readonly.set_readonly(true);
        std::fs::set_permissions(&config, readonly.clone()).unwrap();
        let before = files_under(fixture.dir.path());

        let refused = remove(&fixture.view(), &removing("@acme/x"));

        #[allow(
            clippy::permissions_set_readonly_false,
            reason = "restoring the test's file"
        )]
        readonly.set_readonly(false);
        std::fs::set_permissions(&config, readonly).unwrap();
        if refused.is_ok() {
            // Running as a user who writes read-only files (root): the
            // order cannot be observed this way.
            return;
        }
        assert_eq!(refused.unwrap_err().code, "config_write_failed");
        assert_eq!(files_under(fixture.dir.path()), before);

        // Writable: all three go.
        let outcome = remove(&fixture.view(), &removing("@acme/x")).unwrap();
        assert!(matches!(outcome.origin, Origin::Installed { .. }));
        assert!(!package_dir(fixture.dir.path(), "@acme/x").exists());
        let lock = text(specforge_installed::lock_path(fixture.dir.path()));
        assert!(!lock.contains("@acme/x"), "{lock}");
    }

    #[specforge_test(
        behavior = "management_operations_over_the_project_view",
        verify = "list, doctor and remove read the lock the compile read, once"
    )]
    fn remove_reads_the_lock_the_compile_read_not_the_disk() {
        let fixture = installed(Fixture::new().config(&["@acme/x"]), "@acme/x");
        write_config(&fixture, r#"{"extensions": ["@acme/x"]}"#);
        // The file changed after the compile read it: the removal is the
        // compile's, and writes the lock back whole.
        std::fs::write(
            specforge_installed::lock_path(fixture.dir.path()),
            "not valid json {{{",
        )
        .unwrap();

        let outcome = remove(&fixture.view(), &removing("@acme/x")).unwrap();

        assert!(matches!(outcome.origin, Origin::Installed { .. }));
        assert!(!package_dir(fixture.dir.path(), "@acme/x").exists());
        let lock = text(specforge_installed::lock_path(fixture.dir.path()));
        assert!(lock.contains("lockfile_version"), "{lock}");
        assert!(!lock.contains("@acme/x"), "{lock}");
    }

    #[test]
    fn pin_remove_refuses_an_enabled_entry_that_is_not_installed() {
        let fixture = Fixture::new().config(&["@specforge/product", "@acme/missing"]);
        write_config(
            &fixture,
            r#"{"extensions": ["@specforge/product", "@acme/missing"]}"#,
        );
        let before = files_under(fixture.dir.path());

        let error = remove(&fixture.view(), &removing("@acme/missing")).unwrap_err();

        assert_eq!(error.code, NOT_FOUND);
        assert_eq!(
            error.message,
            "extension '@acme/missing' is not installed (no lock file found)"
        );
        assert_eq!(files_under(fixture.dir.path()), before);
    }

    #[test]
    fn pin_remove_refuses_an_entry_that_is_no_package_name_with_e072() {
        let fixture = Fixture::new().config(&["Bad Name"]);
        write_config(&fixture, r#"{"extensions": ["Bad Name"]}"#);
        let before = files_under(fixture.dir.path());

        let error = remove(&fixture.view(), &removing("Bad Name")).unwrap_err();

        assert_eq!(error.code, "E072");
        assert_eq!(files_under(fixture.dir.path()), before);
    }

    #[specforge_test(
        behavior = "remove_extension",
        verify = "a .wasm file entry is removed by the name it declares or by its entry as written, leaving its file in place"
    )]
    fn remove_selects_a_file_entry_by_its_text_its_path_or_its_name() {
        let fixture = Fixture::new()
            .enabled(vec![EnabledExtension {
                entry: "@sdk/greet=greet.wasm".into(),
                name: "@sdk/greet".into(),
                file: Some("greet.wasm".into()),
                failure: None,
            }])
            .declarations(vec![Fixture::declaration("@sdk/greet", "0.1.0")]);
        write_config(&fixture, r#"{"extensions": ["@sdk/greet=greet.wasm"]}"#);

        for argument in ["@sdk/greet=greet.wasm", "greet.wasm", "@sdk/greet"] {
            let request = RemoveRequest {
                name: argument,
                force: false,
                dry_run: true,
            };
            let outcome = remove(&fixture.view(), &request).unwrap();

            assert_eq!(outcome.name, "@sdk/greet", "{argument}");
            assert_eq!(
                outcome.origin,
                Origin::File {
                    path: "greet.wasm".into()
                },
                "{argument}"
            );
            assert_eq!(outcome.version.as_deref(), Some("0.1.0"), "{argument}");
            assert!(outcome.writes.is_empty(), "{argument}");
        }
    }

    #[test]
    fn an_unreadable_lock_is_why_nothing_is_installed() {
        let mut fixture = Fixture::new();
        write_config(&fixture, r#"{"extensions": []}"#);
        std::fs::write(
            specforge_installed::lock_path(fixture.dir.path()),
            "not valid json {{{",
        )
        .unwrap();
        fixture.env.installed = Installed::at(fixture.dir.path());

        let refused = remove(&fixture.view(), &removing("@acme/x")).unwrap_err();

        assert_eq!(refused.code, NOT_FOUND);
        assert!(refused.message.contains("corrupt lock file"), "{refused:?}");
        // Nothing was written: the corrupt file is as it was.
        assert_eq!(
            text(specforge_installed::lock_path(fixture.dir.path())),
            "not valid json {{{"
        );
    }
}
