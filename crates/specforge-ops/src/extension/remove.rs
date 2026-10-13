//! `specforge remove` and `specforge.remove_extension`.

use super::lock_plan::{Ask, Disabled, LockPlan, Purpose};
use super::{NOT_FOUND, Origin, ProjectExtension, Selection};
use crate::view::ProjectView;
use crate::{OpError, OpErrorKind, Writes};
use serde::Serialize;
use specforge_common::ExtensionEntry;
use specforge_common::shape::Shape;
use specforge_graph::Graph;
use specforge_installed::{Installed, LockFileEntry, LockState};
use specforge_package::PackageName;
use specforge_project::EnabledExtension;
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

/// What a removal reads of its project: the view, its root, its kinds and
/// its graph.
struct Removing<'a> {
    view: &'a ProjectView<'a>,
    root: &'a Path,
    force: bool,
    dry_run: bool,
    kinds: &'a KindRegistry,
    graph: &'a Graph,
    /// The project's installed extensions, as the compile read them.
    installed: &'a Installed,
}

/// A stranded entity (CONTEXT.md): one whose kind only the removed
/// extension declares; the next compile reports it E024.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Shape)]
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
    /// Its version as the inventory gives it ([`ProjectExtension::version`]).
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

impl RemoveOutcome {
    /// The document both surfaces print.
    pub fn document(&self) -> RemoveDocument {
        RemoveDocument {
            action: if self.dry_run {
                RemoveAction::Planned
            } else {
                RemoveAction::Removed
            },
            extension: self.name.clone(),
            source: self.origin.source(),
            version: self.version.clone(),
            stranded: self.stranded.clone(),
        }
    }
}

/// What `specforge remove --format json` and `specforge.remove_extension`
/// print (`McpRemoveExtensionResult`).
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct RemoveDocument {
    pub action: RemoveAction,
    pub extension: String,
    /// `builtin`, the lock entry's source, `file:<path>` or `unknown`
    /// ([`Origin::source`]).
    pub source: String,
    /// The locked version; for a builtin, the loaded one. Absent when
    /// neither is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The entities the removal strands, in id order.
    pub stranded: Vec<StrandedEntity>,
}

/// Whether the removal was made or only previewed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Shape)]
#[serde(rename_all = "snake_case")]
pub enum RemoveAction {
    Removed,
    /// A dry run: nothing changed.
    Planned,
}

/// Remove `name` from the project the view was compiled from: a locked
/// install is uninstalled (binary, lock entry and `specforge.json` entry);
/// a builtin is disabled (its `specforge.json` entry); a `.wasm` file
/// entry, named by the extension it loaded as, its entry or its path, is
/// dropped from `specforge.json` (the file and the lock are left alone); an
/// enabled entry that does not load (E028) is dropped by its name.
/// Refused with E027 while a member of the project's peer set (the
/// extensions its entries load) would be left without a non-optional peer
/// it requires, unless `force`; with `extension_conflict` when more than
/// one entry enables `name`.
///
/// Every refusal is decided from the view before anything is written: a
/// `specforge.json` the compile could not edit (a problem that
/// [`blocks_edits`](specforge_common::ConfigProblem::blocks_edits), which
/// the compile reported as E069) refuses every removal with
/// `config_invalid`, without reading the file again. A removal is all or
/// nothing: the binary, the lock entry and the `specforge.json` entry go as
/// one change, and a failure at any step puts every file back (the error's
/// [`OpError::writes`] names what the rollback could not, normally
/// nothing). Without a root: `no_project`. The removal is one lock-plan change
/// ([`LockPlan::removing`]): a dry run is the same plan checked and not
/// committed.
pub fn remove(view: &ProjectView, req: &RemoveRequest) -> Result<RemoveOutcome, OpError> {
    let root = view.project_root()?;
    if let Some(problem) = view
        .env()
        .config_problems()
        .iter()
        .find(|problem| problem.blocks_edits())
    {
        return Err(crate::config::refusal(problem));
    }
    let inventory = super::inventory(view);
    let removing = &Removing {
        view,
        root,
        force: req.force,
        dry_run: req.dry_run,
        kinds: view.registries().kinds(),
        graph: view.graph(),
        installed: view.installed(),
    };
    match inventory.select(req.name) {
        Selection::File { entry, extension } => remove_file(removing, entry, extension),
        Selection::Named(extension) => remove_named(removing, extension),
        Selection::Ambiguous(entries) => Err(ambiguous(req.name, &entries)),
        Selection::Nothing => Err(nothing_named(req.name, view.installed().lock())),
    }
}

/// The refusal for an argument that more than one entry enables, naming
/// the entries in the order specforge.json lists them.
fn ambiguous(name: &str, entries: &[&EnabledExtension]) -> OpError {
    let entries: Vec<&str> = entries.iter().map(|e| e.entry.as_str()).collect();
    let quoted: Vec<String> = entries.iter().map(|e| format!("'{e}'")).collect();
    let error = OpError::new(
        OpErrorKind::Conflict,
        "extension_conflict",
        format!(
            "'{name}' is enabled by {} specforge.json entries: {}",
            quoted.len(),
            quoted.join(", ")
        ),
    )
    .with_suggestion(
        "remove one entry by its text as specforge.json writes it, or edit specforge.json",
    );
    error.with_data(serde_json::json!({"extension": name, "entries": entries}))
}

/// The refusal for an argument the project has nothing under: a file no
/// entry names; E072 for a name that could never be installed; else
/// `extension_not_found`, saying why nothing is locked when there is no
/// readable lock.
fn nothing_named(name: &str, lock: &LockState) -> OpError {
    if ExtensionEntry::parse(name).file(Path::new("")).is_some() {
        return OpError::new(
            OpErrorKind::ExtensionNotFound,
            NOT_FOUND,
            format!("extension '{name}' is not enabled: no specforge.json entry names that file"),
        )
        .with_suggestion("`specforge extensions` lists what the project enables")
        .with_data(serde_json::json!({ "extension": name }));
    }
    if let Err(why) = PackageName::parse(name) {
        return OpError::of(
            OpErrorKind::InvalidInput,
            specforge_common::package::invalid(&why),
        );
    }
    let why = match lock {
        LockState::Absent => Some("no lock file found"),
        LockState::Unreadable(problem) => Some(problem.message.as_str()),
        LockState::Read(_) => None,
    };
    not_installed(name, why)
}

/// Remove the extension `extension` names: its lock entry, binary and
/// `specforge.json` entries as one change when it is locked; else its
/// `specforge.json` entries (a builtin, or an entry that does not load).
fn remove_named(req: &Removing, extension: &ProjectExtension) -> Result<RemoveOutcome, OpError> {
    // Neither builtin nor locked while the lock can't be read: whether it
    // is installed is not known, so nothing is removed.
    if extension.origin == Origin::Unknown
        && let Some(problem) = req.installed.lock().problem()
    {
        return Err(not_installed(
            extension.name,
            Some(problem.message.as_str()),
        ));
    }
    // Loaded, but neither enabled nor locked (a view assembled in a test).
    if !extension.enabled() && extension.lock.is_none() {
        return Err(not_installed(extension.name, None));
    }
    let writes = change(
        req,
        extension.lock,
        Disabled::Named(extension.name.to_string()),
    )?;
    Ok(RemoveOutcome {
        name: extension.name.to_string(),
        version: extension.version().map(str::to_string),
        origin: extension.origin.clone(),
        stranded: stranded(req.graph, req.kinds, extension.name),
        dry_run: req.dry_run,
        writes,
    })
}

/// The removal as one lock-plan change: `locked`'s package is uninstalled (its binary and lock
/// entry) and the entries `disabled` selects are dropped from `specforge.json`, judged by the
/// peer rule over the project's peer set as the removal leaves it, then written all or nothing
/// (nothing for a dry run). The files it wrote.
fn change(
    req: &Removing,
    locked: Option<&LockFileEntry>,
    disabled: Disabled,
) -> Result<Writes, OpError> {
    let purpose = if req.dry_run {
        Purpose::Preview
    } else {
        Purpose::Commit
    };
    let mut plan = LockPlan::removing(req.view, purpose, req.force, locked.is_some())?;
    // Its directory is named by the lock entry's package name, which the lock read validated
    // (ADR 0036): nothing outside `.specforge/extensions` can be deleted.
    if let Some(locked) = locked {
        plan.uninstall(Ask(0), &locked.name);
    }
    plan.disable(Ask(0), disabled);
    let done = plan
        .check()
        .map_err(|refused| refused.decisive().1)?
        .finish(req.root)?;
    Ok(done.writes)
}

/// Remove the `.wasm` file entry `file`: only its `specforge.json` entry
/// goes. The extension it loaded as (if it loaded) is what dependents and
/// stranded entities are checked against.
fn remove_file(
    req: &Removing,
    file: &EnabledExtension,
    extension: &ProjectExtension,
) -> Result<RemoveOutcome, OpError> {
    let writes = change(req, None, Disabled::Entry(file.entry.clone()))?;
    Ok(RemoveOutcome {
        name: file.name.clone(),
        version: extension.version().map(str::to_string),
        origin: Origin::File {
            path: file.file.clone().unwrap_or_default(),
        },
        stranded: stranded(req.graph, req.kinds, &file.name),
        dry_run: req.dry_run,
        writes,
    })
}

fn not_installed(name: &str, why: Option<&str>) -> OpError {
    let mut message = format!("extension '{name}' is not installed");
    if let Some(why) = why {
        message.push_str(&format!(" ({why})"));
    }
    OpError::new(OpErrorKind::ExtensionNotFound, NOT_FOUND, message)
        .with_data(serde_json::json!({ "extension": name }))
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
                warnings: Vec::new(),
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
                    root: &specforge_common::ProjectRoot::of(fixture.dir.path()).unwrap(),
                    extensions: vec![Source::Builtin("@specforge/product").into()],
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
                    root: &specforge_common::ProjectRoot::of(fixture.dir.path()).unwrap(),
                    name: None,
                    reach: specforge_package::Reach::Compatible,
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

    #[specforge_test(
        behavior = "remove_extension",
        verify = "an enabled entry that is not installed is removed from specforge.json by its name"
    )]
    fn an_enabled_entry_that_is_not_installed_is_removed_by_its_name() {
        let fixture = Fixture::new().config(&["@specforge/product", "@acme/missing@1.2.0"]);
        write_config(
            &fixture,
            r#"{"extensions": ["@specforge/product", "@acme/missing@1.2.0"]}"#,
        );
        let before = files_under(fixture.dir.path());

        let request = RemoveRequest {
            name: "@acme/missing",
            force: false,
            dry_run: true,
        };
        let planned = remove(&fixture.view(), &request).unwrap();
        assert_eq!(planned.origin, Origin::Unknown);
        assert_eq!(planned.version.as_deref(), Some("1.2.0"));
        assert!(planned.writes.is_empty());
        assert_eq!(files_under(fixture.dir.path()), before);

        let outcome = remove(&fixture.view(), &removing("@acme/missing")).unwrap();

        assert_eq!(outcome.origin, Origin::Unknown);
        assert_eq!(outcome.version.as_deref(), Some("1.2.0"));
        assert_eq!(
            outcome.writes,
            Writes::of([fixture.dir.path().join("specforge.json")])
        );
        let config: serde_json::Value =
            serde_json::from_str(&text(fixture.dir.path().join("specforge.json"))).unwrap();
        assert_eq!(
            config["extensions"],
            serde_json::json!(["@specforge/product"])
        );

        // Without a legacy version the entry has none.
        let plain = Fixture::new().config(&["@acme/missing"]);
        write_config(&plain, r#"{"extensions": ["@acme/missing"]}"#);
        let outcome = remove(&plain.view(), &removing("@acme/missing")).unwrap();
        assert_eq!((outcome.origin, outcome.version), (Origin::Unknown, None));
    }

    #[test]
    fn an_entry_that_is_no_package_name_is_removed_by_its_text() {
        let fixture = Fixture::new().config(&["Bad Name"]);
        write_config(&fixture, r#"{"extensions": ["Bad Name"]}"#);

        let outcome = remove(&fixture.view(), &removing("Bad Name")).unwrap();

        assert_eq!(outcome.origin, Origin::Unknown);
        let config: serde_json::Value =
            serde_json::from_str(&text(fixture.dir.path().join("specforge.json"))).unwrap();
        assert_eq!(config["extensions"], serde_json::json!([]));
    }

    #[test]
    fn an_entry_of_unknown_origin_is_not_removed_under_an_unreadable_lock() {
        let mut fixture = Fixture::new().config(&["@acme/x"]);
        write_config(&fixture, r#"{"extensions": ["@acme/x"]}"#);
        std::fs::write(
            specforge_installed::lock_path(fixture.dir.path()),
            "not valid json {{{",
        )
        .unwrap();
        fixture = fixture.read_lock();
        let before = files_under(fixture.dir.path());

        let refused = remove(&fixture.view(), &removing("@acme/x")).unwrap_err();

        assert_eq!(refused.code, NOT_FOUND);
        assert!(refused.message.contains("corrupt lock file"), "{refused:?}");
        assert_eq!(files_under(fixture.dir.path()), before);
    }

    #[test]
    fn a_builtin_named_by_a_stray_lock_entry_is_uninstalled_as_a_builtin() {
        let fixture = installed(
            Fixture::new().config(&["@specforge/product"]),
            "@specforge/product",
        );
        write_config(&fixture, r#"{"extensions": ["@specforge/product"]}"#);

        let outcome = remove(&fixture.view(), &removing("@specforge/product")).unwrap();

        assert_eq!(outcome.origin, Origin::Builtin);
        let lock = text(specforge_installed::lock_path(fixture.dir.path()));
        assert!(!lock.contains("@specforge/product"), "{lock}");
        let config: serde_json::Value =
            serde_json::from_str(&text(fixture.dir.path().join("specforge.json"))).unwrap();
        assert_eq!(config["extensions"], serde_json::json!([]));
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
                warnings: Vec::new(),
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
        let fixture = Fixture::new();
        write_config(&fixture, r#"{"extensions": []}"#);
        std::fs::write(
            specforge_installed::lock_path(fixture.dir.path()),
            "not valid json {{{",
        )
        .unwrap();
        let fixture = fixture.read_lock();

        let refused = remove(&fixture.view(), &removing("@acme/x")).unwrap_err();

        assert_eq!(refused.code, NOT_FOUND);
        assert!(refused.message.contains("corrupt lock file"), "{refused:?}");
        // Nothing was written: the corrupt file is as it was.
        assert_eq!(
            text(specforge_installed::lock_path(fixture.dir.path())),
            "not valid json {{{"
        );
    }

    #[specforge_test(
        behavior = "remove_extension",
        verify = "the remove document names the extension's source and version whatever its origin"
    )]
    fn remove_document_names_source_and_version() {
        use specforge_installed::LockSource;

        let outcome = |origin, version: Option<&str>, dry_run| RemoveOutcome {
            name: "@acme/x".into(),
            version: version.map(String::from),
            origin,
            stranded: vec![StrandedEntity {
                entity_id: "a".into(),
                kind: "greeting".into(),
            }],
            dry_run,
            writes: Writes::default(),
        };
        let document = |outcome: RemoveOutcome| serde_json::to_value(outcome.document()).unwrap();
        let stranded = serde_json::json!([{"entity_id": "a", "kind": "greeting"}]);

        assert_eq!(
            document(outcome(Origin::Builtin, Some("1.0.0"), false)),
            serde_json::json!({"action": "removed", "extension": "@acme/x", "source": "builtin",
                "version": "1.0.0", "stranded": stranded})
        );
        assert_eq!(
            document(outcome(
                Origin::Installed {
                    source: LockSource::Registry
                },
                Some("0.1.0"),
                false
            )),
            serde_json::json!({"action": "removed", "extension": "@acme/x", "source": "registry",
                "version": "0.1.0", "stranded": stranded})
        );
        assert_eq!(
            document(outcome(
                Origin::File {
                    path: "x.wasm".into()
                },
                None,
                false
            )),
            serde_json::json!({"action": "removed", "extension": "@acme/x", "source": "file:x.wasm",
                "stranded": stranded})
        );
        assert_eq!(
            document(outcome(Origin::Unknown, None, true))["action"],
            "planned"
        );
    }
}
