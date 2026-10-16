//! What each writing operation reports it wrote ([`Writes`]) is what it
//! changed on disk: a snapshot diff of the project, file by file.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use specforge_ops::Writes;
use specforge_ops::extension::{self, AddOutcome, AddRequest, RemoveRequest, StrandedEntity};
use specforge_ops::registry::Trust;
use specforge_ops::testing::{self, declaring, serving_builtin};
use specforge_ops::view::ProjectView;
use specforge_project::CompiledProject;
use specforge_project::testing::compile_at;
use specforge_wasm::testing::InProcessRuntime;
use std::sync::Arc;
use tempfile::TempDir;

pub(crate) type Snapshot = BTreeMap<PathBuf, Vec<u8>>;

/// Every file under `root` with its bytes; empty when `root` is absent.
pub(crate) fn files_under(root: &Path) -> Snapshot {
    let mut files = Snapshot::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for path in entries.map(|entry| entry.unwrap().path()) {
            if path.is_dir() {
                dirs.push(path);
            } else {
                let bytes = std::fs::read(&path).unwrap();
                files.insert(path, bytes);
            }
        }
    }
    files
}

/// The files whose bytes differ between `before` and now, added and
/// removed ones included, relative to `root`, sorted.
pub(crate) fn changed_since(root: &Path, before: &Snapshot) -> Vec<String> {
    let after = files_under(root);
    let mut changed: Vec<String> = before
        .keys()
        .chain(after.keys())
        .filter(|path| before.get(*path) != after.get(*path))
        .map(|path| path.strip_prefix(root).unwrap().display().to_string())
        .collect();
    changed.sort();
    changed.dedup();
    changed
}

pub(crate) fn listed(writes: &Writes, root: &Path) -> Vec<String> {
    let names = writes.named(Some(root));
    names.iter().map(ToString::to_string).collect()
}

/// A project directory whose `specforge.json` enables `extensions`, with
/// `files` written (path, text).
pub(crate) fn project(extensions: &[&str], files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    let config = serde_json::json!({"name": "w", "version": "0.1.0", "extensions": extensions});
    std::fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

/// The binaries a test installs from, as files: `@sdk/greet` (declaring
/// the kind `greeting`), the same extension with other bytes, and
/// `@test/probe`. Served in process by [`candidates`].
pub(crate) struct Blobs {
    dir: TempDir,
}

impl Blobs {
    pub(crate) fn new() -> Self {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("greet.wasm"), testing::GREET).unwrap();
        std::fs::write(dir.path().join("greet-v.wasm"), testing::GREET_VARIANT).unwrap();
        std::fs::write(dir.path().join("probe.wasm"), testing::PROBE).unwrap();
        Blobs { dir }
    }

    pub(crate) fn greet(&self) -> PathBuf {
        self.dir.path().join("greet.wasm")
    }

    /// `@sdk/greet` 0.1.0 with other bytes.
    pub(crate) fn variant(&self) -> PathBuf {
        self.dir.path().join("greet-v.wasm")
    }

    pub(crate) fn probe(&self) -> PathBuf {
        self.dir.path().join("probe.wasm")
    }
}

/// What the tests serve in process: the blobs, and the builtin
/// `@specforge/product` as a look-alike.
pub(crate) fn candidates() -> InProcessRuntime {
    serving_builtin(
        testing::candidates(),
        "@specforge/product",
        declaring("@specforge/product", "1.0.0", &[]),
    )
}

pub(crate) const GREET: &str = "@sdk/greet";
pub(crate) const MODULE: &str = ".specforge/extensions/@sdk/greet/extension.wasm";

pub(crate) fn add(
    root: &Path,
    specifier: impl AsRef<Path>,
) -> Result<extension::Added, specforge_ops::Refusal<extension::AddField>> {
    extension::add(
        &AddRequest {
            root: &specforge_common::ProjectRoot::of(root).unwrap(),
            extensions: vec![specifier.as_ref().display().to_string()],
            base: None,
            allow_unsigned: false,
            trust: Trust::Refuse,
            dry_run: false,
            containment: &specforge_ops::Containment::unbounded(),
        },
        &specforge_ops::registry::Unconfigured("test"),
        &candidates(),
    )
}

/// The project at `root`, compiled in process with [`candidates`].
pub(crate) fn compiled(root: &Path) -> CompiledProject {
    compile_at(root, Some(Arc::new(candidates())))
}

/// The project at `root`, compiled with the component runtime that loads
/// the builtins.
fn compiled_with_builtins(root: &Path) -> CompiledProject {
    compile_at(root, Some(specforge_project::testing::component_runtime()))
}

fn remove(root: &Path, name: &str, dry_run: bool) -> extension::RemoveOutcome {
    remove_over(&compiled(root), name, dry_run)
}

fn remove_over(project: &CompiledProject, name: &str, dry_run: bool) -> extension::RemoveOutcome {
    let request = RemoveRequest {
        name,
        force: false,
        dry_run,
        containment: &specforge_ops::Containment::unbounded(),
    };
    extension::remove(&ProjectView::of(project), &request).unwrap()
}

const MESSY: &str = "behavior messy \"Messy\" {\ncontract \"The system MUST work\"\n}\n";
const TIDY: &str = "behavior tidy \"Tidy\" {\n  contract \"The system MUST work\"\n}\n";

#[cfg(unix)]
#[test]
fn format_writes_are_the_files_it_rewrote() {
    use specforge_ops::authoring::format::{self, Mode, Request};
    use std::os::unix::fs::PermissionsExt;
    let dir = project(
        &[],
        &[
            ("a.spec", MESSY),
            ("b.spec", &MESSY.replace("messy", "other")),
            ("c.spec", TIDY),
        ],
    );
    let root = dir.path();
    let locked = root.join("b.spec");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o444)).unwrap();
    let before = files_under(root);

    let outcome = format::run(&Request {
        root: &specforge_common::ProjectRoot::of(root).unwrap(),
        paths: &[],
        mode: Mode::Write,
        containment: &specforge_ops::Containment::unbounded(),
    })
    .unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).unwrap();

    // c.spec is already formatted; b.spec could not be written.
    assert_eq!(outcome.failures.len(), 1, "{:?}", outcome.failures);
    assert_eq!(listed(&outcome.writes(), root), ["a.spec"]);
    assert_eq!(changed_since(root, &before), ["a.spec"]);

    // A check writes nothing.
    let outcome = format::run(&Request {
        root: &specforge_common::ProjectRoot::of(root).unwrap(),
        paths: &[],
        mode: Mode::Check,
        containment: &specforge_ops::Containment::unbounded(),
    })
    .unwrap();
    assert!(outcome.writes().is_empty());
}

#[test]
fn rename_writes_are_the_files_it_edited() {
    use specforge_ops::authoring::rename;
    use specforge_ops::read::navigate::Navigator;
    let limit = "invariant session_limit \"Limit\" {\n  guarantee \"x\"\n}\n";
    let login = "behavior login \"Login\" {\n  invariants [session_limit]\n}\n";
    let other = "behavior other \"Other\" {\n}\n";
    let dir = project(
        &["@specforge/software"],
        &[
            ("limit.spec", limit),
            ("login.spec", login),
            ("other.spec", other),
        ],
    );
    let root = dir.path();
    let project = compiled_with_builtins(root);
    let read = |f: &str| std::fs::read_to_string(root.join(f)).ok();
    let plan = rename::plan(
        &Navigator::new(ProjectView::of(&project), read),
        "session_limit",
        "session_cap",
    )
    .unwrap();
    let before = files_under(root);

    let writes = rename::apply(&plan, root, &specforge_ops::Containment::unbounded()).unwrap();

    assert_eq!(listed(&writes, root), ["limit.spec", "login.spec"]);
    assert_eq!(changed_since(root, &before), ["limit.spec", "login.spec"]);
}

/// The component runtime that loads the builtins, with the per-user cache.
#[test]
fn enabling_a_builtin_writes_the_config_only() {
    let dir = project(&[], &[]);
    let root = dir.path();
    let before = files_under(root);

    let added = add(root, "@specforge/product").unwrap();

    assert!(matches!(
        added.added[0].outcome,
        AddOutcome::Builtin { changed: true, .. }
    ));
    assert_eq!(listed(&added.writes, root), ["specforge.json"]);
    assert_eq!(changed_since(root, &before), ["specforge.json"]);
    assert_eq!(
        added.extensions_enabled,
        specforge_common::read_project_config(root)
            .config
            .extensions
            .len()
    );
    assert!(added.extensions_enabled >= 1);
}

#[test]
fn enabling_an_enabled_builtin_writes_nothing() {
    let dir = project(&["@specforge/product"], &[]);
    let root = dir.path();
    let before = files_under(root);

    let added = add(root, "@specforge/product").unwrap();

    assert!(matches!(
        added.added[0].outcome,
        AddOutcome::Builtin { changed: false, .. }
    ));
    assert!(added.writes.is_empty());
    assert_eq!(changed_since(root, &before), Vec::<String>::new());
    assert_eq!(added.extensions_enabled, 1);
}

#[test]
fn installing_a_local_extension_writes_module_lock_and_config() {
    let blobs = Blobs::new();
    let dir = project(&[], &[]);
    let root = dir.path();
    let before = files_under(root);

    let added = add(root, blobs.greet()).unwrap();

    assert!(matches!(
        added.added[0].outcome,
        AddOutcome::Installed { .. }
    ));
    let three = [MODULE, "specforge.json", "specforge.lock"];
    assert_eq!(listed(&added.writes, root), three);
    assert_eq!(changed_since(root, &before), three);
    assert_eq!(added.extensions_enabled, 1);

    // The same blob again: already present, nothing written.
    let before = files_under(root);
    let again = add(root, blobs.greet()).unwrap();
    assert!(matches!(
        again.added[0].outcome,
        AddOutcome::AlreadyPresent { .. }
    ));
    assert!(again.writes.is_empty());
    assert_eq!(changed_since(root, &before), Vec::<String>::new());
}

#[specforge_test_macros::test(
    behavior = "install_wasm_extension",
    verify = "a failed install puts back the binary, specforge.lock and specforge.json it changed"
)]
fn an_add_that_fails_after_placing_its_module_writes_nothing() {
    let blobs = Blobs::new();
    let dir = project(&[], &[]);
    let root = dir.path();
    // The lock is written through a sibling file; a directory in its place
    // makes the lock write fail after the module is placed.
    std::fs::create_dir_all(root.join("specforge.lock.tmp")).unwrap();
    let before = files_under(root);

    let error = add(root, blobs.greet()).unwrap_err();

    assert_eq!(error.error.code, "E033", "{error:?}");
    assert!(
        error.error.message.contains("specforge.lock.tmp"),
        "the message names the file it could not write: {error:?}"
    );
    assert!(error.error.writes.is_empty(), "{:?}", error.error.writes);
    assert_eq!(changed_since(root, &before), Vec::<String>::new());
}

pub(crate) const PROBE: &str = "@test/probe";
pub(crate) const PROBE_MODULE: &str = ".specforge/extensions/@test/probe/extension.wasm";

/// Make the lock write fail: it goes through a sibling file, and a
/// directory in its place cannot be written.
fn unwritable_lock(root: &Path) {
    std::fs::create_dir_all(root.join("specforge.lock.tmp")).unwrap();
}

#[test]
fn a_failed_add_over_an_install_keeps_the_pinned_binary() {
    let blobs = Blobs::new();
    let dir = project(&[], &[]);
    let root = dir.path();
    add(root, blobs.greet()).unwrap();
    let variant = blobs.variant();
    let before = files_under(root);
    unwritable_lock(root);

    let error = add(root, variant).unwrap_err();
    std::fs::remove_dir(root.join("specforge.lock.tmp")).unwrap();

    assert_eq!(error.error.code, "E033", "{error:?}");
    assert_eq!(changed_since(root, &before), Vec::<String>::new());
    let diagnostics = compiled(root).diagnostics();
    let refused: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.message.contains(GREET) && ["E033", "E070"].contains(&d.code.as_str()))
        .map(|d| d.code.as_str())
        .collect();
    assert!(refused.is_empty(), "{diagnostics:?}");
}

#[specforge_test_macros::test(
    behavior = "read_lock_file",
    verify = "an unreadable lock file is E033 and is never replaced by a change"
)]
fn an_add_over_an_unreadable_lock_is_refused() {
    let blobs = Blobs::new();
    let dir = project(&[], &[]);
    let root = dir.path();
    add(root, blobs.greet()).unwrap();
    add(root, blobs.probe()).unwrap();
    std::fs::write(root.join("specforge.lock"), "not a lock {{{").unwrap();
    let variant = blobs.variant();
    let before = files_under(root);

    let error = add(root, variant).unwrap_err();

    assert_eq!(error.error.code, "E033", "{error:?}");
    assert!(error.error.writes.is_empty());
    assert_eq!(changed_since(root, &before), Vec::<String>::new());
    let lock = std::fs::read_to_string(root.join("specforge.lock")).unwrap();
    assert_eq!(lock, "not a lock {{{", "the lock is byte-identical");
}

#[specforge_test_macros::test(
    behavior = "remove_extension",
    verify = "a removal that fails changes nothing"
)]
fn a_failed_remove_changes_nothing() {
    let blobs = Blobs::new();
    let dir = project(&[], &[]);
    let root = dir.path();
    add(root, blobs.greet()).unwrap();
    add(root, blobs.probe()).unwrap();
    unwritable_lock(root);
    let before = files_under(root);
    let project = compiled(root);

    let error = extension::remove(
        &ProjectView::of(&project),
        &RemoveRequest {
            name: PROBE,
            force: false,
            dry_run: false,
            containment: &specforge_ops::Containment::unbounded(),
        },
    )
    .unwrap_err();

    assert_eq!(error.error.code, "E033", "{error:?}");
    assert!(error.error.writes.is_empty(), "{:?}", error.error.writes);
    assert_eq!(changed_since(root, &before), Vec::<String>::new());
    let config = std::fs::read_to_string(root.join("specforge.json")).unwrap();
    assert!(config.contains(PROBE), "{config}");
    let lock = std::fs::read_to_string(root.join("specforge.lock")).unwrap();
    assert!(lock.contains(PROBE), "{lock}");
    assert!(root.join(PROBE_MODULE).exists());
}

#[test]
fn removing_an_install_writes_its_module_lock_and_config() {
    let blobs = Blobs::new();
    let dir = project(&[], &[]);
    let root = dir.path();
    add(root, blobs.greet()).unwrap();
    let before = files_under(root);

    let outcome = remove(root, GREET, false);

    let three = [MODULE, "specforge.json", "specforge.lock"];
    assert_eq!(listed(&outcome.writes, root), three);
    assert_eq!(changed_since(root, &before), three);
}

#[test]
fn disabling_a_builtin_writes_the_config_only() {
    let dir = project(&["@specforge/product"], &[]);
    let root = dir.path();
    let before = files_under(root);

    let outcome = remove_over(&compiled_with_builtins(root), "@specforge/product", false);

    assert_eq!(listed(&outcome.writes, root), ["specforge.json"]);
    assert_eq!(changed_since(root, &before), ["specforge.json"]);

    // A dry run writes nothing.
    let dir = project(&["@specforge/product"], &[]);
    let before = files_under(dir.path());
    let preview = remove_over(
        &compiled_with_builtins(dir.path()),
        "@specforge/product",
        true,
    );
    assert!(preview.writes.is_empty());
    assert_eq!(changed_since(dir.path(), &before), Vec::<String>::new());
}

#[test]
fn stranded_lists_the_entities_by_id() {
    let blobs = Blobs::new();
    let dir = project(
        &[],
        &[
            ("b.spec", "greeting zeta \"Zeta\" {\n  style warm\n}\n"),
            ("a.spec", "greeting hello \"Hello\" {\n  style warm\n}\n"),
        ],
    );
    let root = dir.path();
    add(root, blobs.greet()).unwrap();

    let outcome = remove(root, GREET, true);

    assert_eq!(
        outcome.stranded,
        [
            StrandedEntity {
                entity_id: "hello".to_string(),
                kind: "greeting".to_string(),
            },
            StrandedEntity {
                entity_id: "zeta".to_string(),
                kind: "greeting".to_string(),
            },
        ]
    );
    assert_eq!(
        outcome.stranded[0].warning(GREET),
        format!("greeting 'hello' uses a kind only {GREET} defines")
    );
}

const OLD: &str = "// specforge-format: 0.9\nbehavior gamma \"Gamma\" {\n}\n";

/// What a request with no bound carries.
pub(crate) static UNBOUNDED: specforge_ops::Containment = specforge_ops::Containment::unbounded();

fn migration(
    root: &specforge_common::ProjectRoot,
    dry_run: bool,
) -> specforge_ops::authoring::migrate::Request<'_> {
    specforge_ops::authoring::migrate::Request {
        root,
        target: specforge_parser::CURRENT_FORMAT_VERSION,
        dry_run,
        no_backup: false,
        containment: &UNBOUNDED,
    }
}

#[test]
fn a_migration_writes_each_file_and_its_backup() {
    let dir = project(
        &[],
        &[
            ("old.spec", OLD),
            ("older.spec", &OLD.replace("gamma", "delta")),
            ("current.spec", TIDY),
        ],
    );
    let root = dir.path();
    let before = files_under(root);

    let preview = specforge_ops::authoring::migrate::run(
        &migration(&specforge_common::ProjectRoot::of(root).unwrap(), true),
        None,
    )
    .unwrap();
    assert!(preview.writes.is_empty());
    assert_eq!(changed_since(root, &before), Vec::<String>::new());

    let outcome = specforge_ops::authoring::migrate::run(
        &migration(&specforge_common::ProjectRoot::of(root).unwrap(), false),
        None,
    )
    .unwrap();

    let written = [
        ".specforge/migration.json",
        "old.spec",
        "old.spec.bak",
        "older.spec",
        "older.spec.bak",
    ];
    assert_eq!(listed(&outcome.writes, root), written);
    assert_eq!(changed_since(root, &before), written);
}

#[test]
fn a_rolled_back_migration_keeps_only_its_backups() {
    let dir = project(&["@t/x"], &[("old.spec", OLD)]);
    let root = dir.path();
    specforge_installed::testing::install(root, &["@t/x"]);
    let before = files_under(root);
    let runtime = specforge_wasm::testing::InProcessRuntime::new().with(|| {
        let mut c = specforge_extension_sdk::ContributionsBuilder::new(
            specforge_extension_sdk::ExtensionMeta::new("@t/x", "1.0.0"),
        );
        c.migration_hook_handler("hook", |_| Err("the hook failed".into()));
        c
    });

    let outcome = specforge_ops::authoring::migrate::run(
        &migration(&specforge_common::ProjectRoot::of(root).unwrap(), false),
        Some(std::sync::Arc::new(runtime)),
    )
    .unwrap();

    assert!(outcome.rollback.is_some());
    assert_eq!(listed(&outcome.writes, root), ["old.spec.bak"]);
    assert_eq!(changed_since(root, &before), ["old.spec.bak"]);
}
