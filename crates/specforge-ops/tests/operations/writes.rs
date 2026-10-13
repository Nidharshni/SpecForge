//! What each writing operation reports it wrote ([`Writes`]) is what it
//! changed on disk: a snapshot diff of the project, file by file.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use specforge_ops::Writes;
use specforge_ops::extension::{
    self, AddOutcome, AddRequest, RemoveRequest, Source, StrandedEntity, Trust,
};
use specforge_ops::testing::{self, declaring, serving_builtin};
use specforge_ops::view::ProjectView;
use specforge_project::CompiledProject;
use specforge_wasm::testing::InProcessRuntime;
use std::sync::Arc;
use tempfile::TempDir;

type Snapshot = BTreeMap<PathBuf, Vec<u8>>;

/// Every file under `root` with its bytes; empty when `root` is absent.
fn files_under(root: &Path) -> Snapshot {
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
fn changed_since(root: &Path, before: &Snapshot) -> Vec<String> {
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

/// `writes` as the root-relative names a surface lists.
fn listed(writes: &Writes, root: &Path) -> Vec<String> {
    writes.names_under(root)
}

/// A project directory whose `specforge.json` enables `extensions`, with
/// `files` written (path, text).
fn project(extensions: &[&str], files: &[(&str, &str)]) -> TempDir {
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
struct Blobs {
    dir: TempDir,
}

impl Blobs {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("greet.wasm"), testing::GREET).unwrap();
        std::fs::write(dir.path().join("greet-v.wasm"), testing::GREET_VARIANT).unwrap();
        std::fs::write(dir.path().join("probe.wasm"), testing::PROBE).unwrap();
        Blobs { dir }
    }

    fn greet(&self) -> PathBuf {
        self.dir.path().join("greet.wasm")
    }

    /// `@sdk/greet` 0.1.0 with other bytes.
    fn variant(&self) -> PathBuf {
        self.dir.path().join("greet-v.wasm")
    }

    fn probe(&self) -> PathBuf {
        self.dir.path().join("probe.wasm")
    }
}

/// What the tests serve in process: the blobs, and the builtin
/// `@specforge/product` as a look-alike.
fn candidates() -> InProcessRuntime {
    serving_builtin(
        testing::candidates(),
        "@specforge/product",
        declaring("@specforge/product", "1.0.0", &[]),
    )
}

const GREET: &str = "@sdk/greet";
const MODULE: &str = ".specforge/extensions/@sdk/greet/extension.wasm";

fn add(root: &Path, source: Source) -> Result<extension::Added, specforge_ops::OpError> {
    extension::add(
        &AddRequest {
            root: &specforge_common::ProjectRoot::of(root).unwrap(),
            extensions: vec![source.into()],
            allow_unsigned: false,
            trust: Trust::Refuse,
            dry_run: false,
        },
        &specforge_ops::registry::Unconfigured("test"),
        &candidates(),
    )
}

/// The project at `root`, compiled in process with [`candidates`].
fn compiled(root: &Path) -> CompiledProject {
    CompiledProject::compile(
        &specforge_common::ProjectRoot::of(root).unwrap(),
        Some(Arc::new(candidates())),
    )
}

/// The project at `root`, compiled with the component runtime that loads
/// the builtins.
fn compiled_with_builtins(root: &Path) -> CompiledProject {
    let runtime = specforge_component::ComponentRuntime::with_user_cache();
    CompiledProject::compile(
        &specforge_common::ProjectRoot::of(root).unwrap(),
        Some(Arc::new(runtime)),
    )
}

fn remove(root: &Path, name: &str, dry_run: bool) -> extension::RemoveOutcome {
    remove_over(&compiled(root), name, dry_run)
}

fn remove_over(project: &CompiledProject, name: &str, dry_run: bool) -> extension::RemoveOutcome {
    let request = RemoveRequest {
        name,
        force: false,
        dry_run,
    };
    extension::remove(&ProjectView::of(project), &request).unwrap()
}

const MESSY: &str = "behavior messy \"Messy\" {\ncontract \"The system MUST work\"\n}\n";
const TIDY: &str = "behavior tidy \"Tidy\" {\n  contract \"The system MUST work\"\n}\n";

#[cfg(unix)]
#[test]
fn format_writes_are_the_files_it_rewrote() {
    use specforge_ops::format::{self, Mode, Request};
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
    })
    .unwrap();
    assert!(outcome.writes().is_empty());
}

#[test]
fn rename_writes_are_the_files_it_edited() {
    use specforge_ops::navigate::Navigator;
    use specforge_ops::rename;
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

    let writes = rename::apply(&plan, root).unwrap();

    assert_eq!(listed(&writes, root), ["limit.spec", "login.spec"]);
    assert_eq!(changed_since(root, &before), ["limit.spec", "login.spec"]);
}

/// The component runtime that loads the builtins, with the per-user cache.
fn runtime() -> specforge_component::ComponentRuntime {
    specforge_component::ComponentRuntime::with_user_cache()
}

/// What `init` scaffolds in `dir` with `extensions`, the candidates read in
/// process.
fn init(dir: &Path, extensions: &[String]) -> specforge_ops::init::Outcome {
    use specforge_ops::init;
    let request = init::Request {
        dir,
        name: Some("demo"),
        version: init::DEFAULT_VERSION,
        extensions,
    };
    let plan = init::plan(&request, &candidates()).unwrap();
    init::apply(dir, plan, &candidates()).unwrap()
}

#[test]
fn init_writes_include_gitignore_only_when_it_changed() {
    let scratch = TempDir::new().unwrap();

    let fresh = scratch.path().join("fresh");
    let outcome = init(&fresh, &[]);
    assert_eq!(
        listed(&outcome.writes, &fresh),
        [".gitignore", "spec/hello.spec", "specforge.json"]
    );
    assert_eq!(
        changed_since(&fresh, &Snapshot::new()),
        listed(&outcome.writes, &fresh)
    );

    // A .gitignore holding every entry is left as it is.
    let beside = scratch.path().join("beside");
    std::fs::create_dir_all(&beside).unwrap();
    std::fs::write(
        beside.join(".gitignore"),
        "specforge-infer.json\nspecforge-report.json\n.specforge/\n",
    )
    .unwrap();
    let before = files_under(&beside);
    let outcome = init(&beside, &[]);
    assert_eq!(
        listed(&outcome.writes, &beside),
        ["spec/hello.spec", "specforge.json"]
    );
    assert_eq!(
        changed_since(&beside, &before),
        listed(&outcome.writes, &beside)
    );
}

#[specforge_test_macros::test(
    behavior = "scaffold_starter_spec_file",
    verify = "the starter spec's version is the project's"
)]
fn the_starter_states_the_project_version() {
    use specforge_ops::init;
    let scratch = TempDir::new().unwrap();
    // The structural starter, and the templates the software and product
    // builtins contribute (`{version}` in them is the project's version).
    let starter_of = |extensions: &[String], version: &str| {
        let request = init::Request {
            dir: &scratch
                .path()
                .join(format!("{}-{version}", extensions.len())),
            name: Some("demo"),
            version,
            extensions,
        };
        let plan = init::plan(&request, &runtime()).unwrap();
        assert_eq!(plan.config["version"], version);
        plan.starter
    };

    let none: Vec<String> = Vec::new();
    let software = vec!["@specforge/software".to_string()];
    let product = vec!["@specforge/product".to_string()];
    for extensions in [&none, &software, &product] {
        let given = starter_of(extensions, "2.3.0");
        assert!(
            given.contains("version \"2.3.0\""),
            "{extensions:?}: {given}"
        );
        assert!(!given.contains("0.1.0"), "{extensions:?}: {given}");
        assert!(!given.contains("{version}"), "{extensions:?}: {given}");
        let default = starter_of(extensions, init::DEFAULT_VERSION);
        assert!(
            default.contains("version \"0.1.0\""),
            "{extensions:?}: {default}"
        );
    }
}

#[test]
fn init_with_a_local_extension_writes_its_module_and_lock() {
    let blobs = Blobs::new();
    let scratch = TempDir::new().unwrap();
    let dir = scratch.path().join("with-greet");

    let outcome = init(&dir, &[blobs.greet().display().to_string()]);

    assert_eq!(
        listed(&outcome.writes, &dir),
        [
            ".gitignore",
            MODULE,
            "spec/hello.spec",
            "specforge.json",
            "specforge.lock"
        ]
    );
    assert_eq!(
        changed_since(&dir, &Snapshot::new()),
        listed(&outcome.writes, &dir)
    );
}

#[specforge_test_macros::test(
    behavior = "scaffold_new_project",
    verify = "init refuses a directory whose starter file exists, writing nothing"
)]
fn init_refuses_a_directory_whose_starter_file_exists() {
    use specforge_ops::{OpErrorKind, init};
    let scratch = TempDir::new().unwrap();
    let dir = scratch.path().join("victim");
    std::fs::create_dir_all(dir.join("spec")).unwrap();
    std::fs::write(dir.join("spec/hello.spec"), "term mine \"Mine\" {\n}\n").unwrap();
    let before = files_under(&dir);
    let request = init::Request {
        dir: &dir,
        name: Some("demo"),
        version: init::DEFAULT_VERSION,
        extensions: &[],
    };

    let error = init::plan(&request, &candidates()).unwrap_err();

    assert_eq!(error.code, init::STARTER_EXISTS, "{error:?}");
    assert_eq!(error.kind, OpErrorKind::Conflict);
    assert_eq!(files_under(&dir), before, "the file is byte-identical");
}

#[specforge_test_macros::test(
    behavior = "non_interactive_init",
    verify = "init refuses local extensions the lock plan refuses before it writes anything"
)]
fn init_refuses_local_extensions_over_an_unreadable_lock_before_writing() {
    use specforge_ops::init;
    let blobs = Blobs::new();
    let scratch = TempDir::new().unwrap();
    let dir = scratch.path().join("victim");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("specforge.lock"), "garbage\n").unwrap();
    let before = files_under(&dir);
    let extensions = [blobs.greet().display().to_string()];
    let request = init::Request {
        dir: &dir,
        name: Some("demo"),
        version: init::DEFAULT_VERSION,
        extensions: &extensions,
    };

    let error = init::plan(&request, &candidates()).unwrap_err();

    assert_eq!(error.code, "E033", "{error:?}");
    assert_eq!(files_under(&dir), before);
}

#[specforge_test_macros::test(
    behavior = "scaffold_new_project",
    verify = "a failed init leaves the directory as it was, files that were there included"
)]
fn a_failed_init_puts_back_what_was_there() {
    use specforge_ops::init;
    let blobs = Blobs::new();
    let scratch = TempDir::new().unwrap();
    let dir = scratch.path().join("victim");
    std::fs::create_dir_all(dir.join(".specforge")).unwrap();
    std::fs::write(dir.join(".specforge/keep.txt"), "keep\n").unwrap();
    // `.specforge/extensions` is a file: no module can be placed under it.
    std::fs::write(dir.join(".specforge/extensions"), "not a directory\n").unwrap();
    let empty_lock = "{\"lockfile_version\": 1, \"entries\": []}";
    std::fs::write(dir.join("specforge.lock"), empty_lock).unwrap();
    let extensions = [blobs.greet().display().to_string()];
    let request = init::Request {
        dir: &dir,
        name: Some("demo"),
        version: init::DEFAULT_VERSION,
        extensions: &extensions,
    };

    let plan = init::plan(&request, &candidates()).unwrap();
    let error = init::apply(&dir, plan, &candidates()).unwrap_err();

    assert_eq!(error.code, "E032", "{error:?}");
    assert!(error.writes.is_empty(), "{:?}", error.writes);
    assert_eq!(
        std::fs::read_to_string(dir.join(".specforge/keep.txt")).unwrap(),
        "keep\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join(".specforge/extensions")).unwrap(),
        "not a directory\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("specforge.lock")).unwrap(),
        empty_lock
    );
    for gone in ["spec", "specforge.json", ".gitignore"] {
        assert!(!dir.join(gone).exists(), "{gone} is removed again");
    }
}

#[test]
fn enabling_a_builtin_writes_the_config_only() {
    let dir = project(&[], &[]);
    let root = dir.path();
    let before = files_under(root);

    let added = add(root, Source::Builtin("@specforge/product")).unwrap();

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

    let added = add(root, Source::Builtin("@specforge/product")).unwrap();

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

    let added = add(root, Source::Local(blobs.greet())).unwrap();

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
    let again = add(root, Source::Local(blobs.greet())).unwrap();
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

    let error = add(root, Source::Local(blobs.greet())).unwrap_err();

    assert_eq!(error.code, "E033", "{error:?}");
    assert!(
        error.message.contains("specforge.lock.tmp"),
        "the message names the file it could not write: {error:?}"
    );
    assert!(error.writes.is_empty(), "{:?}", error.writes);
    assert_eq!(changed_since(root, &before), Vec::<String>::new());
}

const PROBE: &str = "@test/probe";
const PROBE_MODULE: &str = ".specforge/extensions/@test/probe/extension.wasm";

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
    add(root, Source::Local(blobs.greet())).unwrap();
    let variant = blobs.variant();
    let before = files_under(root);
    unwritable_lock(root);

    let error = add(root, Source::Local(variant)).unwrap_err();
    std::fs::remove_dir(root.join("specforge.lock.tmp")).unwrap();

    assert_eq!(error.code, "E033", "{error:?}");
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
    add(root, Source::Local(blobs.greet())).unwrap();
    add(root, Source::Local(blobs.probe())).unwrap();
    std::fs::write(root.join("specforge.lock"), "not a lock {{{").unwrap();
    let variant = blobs.variant();
    let before = files_under(root);

    let error = add(root, Source::Local(variant)).unwrap_err();

    assert_eq!(error.code, "E033", "{error:?}");
    assert!(error.writes.is_empty());
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
    add(root, Source::Local(blobs.greet())).unwrap();
    add(root, Source::Local(blobs.probe())).unwrap();
    unwritable_lock(root);
    let before = files_under(root);
    let project = compiled(root);

    let error = extension::remove(
        &ProjectView::of(&project),
        &RemoveRequest {
            name: PROBE,
            force: false,
            dry_run: false,
        },
    )
    .unwrap_err();

    assert_eq!(error.code, "E033", "{error:?}");
    assert!(error.writes.is_empty(), "{:?}", error.writes);
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
    add(root, Source::Local(blobs.greet())).unwrap();
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
    add(root, Source::Local(blobs.greet())).unwrap();

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

fn migration(
    root: &specforge_common::ProjectRoot,
    dry_run: bool,
) -> specforge_ops::migrate::Request<'_> {
    specforge_ops::migrate::Request {
        root,
        target: specforge_parser::CURRENT_FORMAT_VERSION,
        dry_run,
        no_backup: false,
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

    let preview = specforge_ops::migrate::run(
        &migration(&specforge_common::ProjectRoot::of(root).unwrap(), true),
        None,
    )
    .unwrap();
    assert!(preview.writes.is_empty());
    assert_eq!(changed_since(root, &before), Vec::<String>::new());

    let outcome = specforge_ops::migrate::run(
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

    let outcome = specforge_ops::migrate::run(
        &migration(&specforge_common::ProjectRoot::of(root).unwrap(), false),
        Some(std::sync::Arc::new(runtime)),
    )
    .unwrap();

    assert!(outcome.rollback.is_some());
    assert_eq!(listed(&outcome.writes, root), ["old.spec.bak"]);
    assert_eq!(changed_since(root, &before), ["old.spec.bak"]);
}

#[specforge_test_macros::test(
    behavior = "non_interactive_init",
    verify = "init enables a builtin after the builtins it requires, as add does"
)]
fn init_enables_a_builtin_after_the_builtins_it_requires() {
    use specforge_ops::init;
    let scratch = TempDir::new().unwrap();
    let extensions = vec!["@specforge/formal".to_string()];
    let request = init::Request {
        dir: &scratch.path().join("formal"),
        name: Some("demo"),
        version: init::DEFAULT_VERSION,
        extensions: &extensions,
    };

    let plan = init::plan(&request, &runtime()).unwrap();

    assert_eq!(
        plan.extensions,
        [
            "@specforge/software",
            "@specforge/formal",
            "@specforge/testing"
        ]
    );
    assert!(
        plan.starter.contains("software specification"),
        "{}",
        plan.starter
    );
}

#[specforge_test_macros::test(
    behavior = "non_interactive_init",
    verify = "init enables a builtin after the builtins it requires, as add does"
)]
fn init_enables_each_builtin_after_the_builtins_it_requires() {
    use specforge_ops::init;
    let scratch = TempDir::new().unwrap();
    let runtime = runtime();
    for (i, builtin) in specforge_project::builtins().names().enumerate() {
        let extensions = vec![builtin.to_string()];
        let request = init::Request {
            dir: &scratch.path().join(format!("p{i}")),
            name: Some("demo"),
            version: init::DEFAULT_VERSION,
            extensions: &extensions,
        };

        let plan = init::plan(&request, &runtime).unwrap();

        for enabled in &plan.extensions {
            let Some(enabled) = extension::builtin_name(enabled) else {
                continue;
            };
            let position = |name: &str| plan.extensions.iter().position(|e| e == name);
            for required in extension::Candidate::builtin(&runtime, enabled)
                .unwrap()
                .peers()
                .iter()
                .filter(|peer| !peer.optional)
            {
                assert!(
                    position(&required.name) < position(enabled)
                        && position(&required.name).is_some(),
                    "{builtin}: {enabled} requires {} first: {:?}",
                    required.name,
                    plan.extensions
                );
            }
        }
    }
}

#[test]
fn init_refuses_a_local_binary_that_claims_a_builtins_name_before_writing() {
    use specforge_ops::init;
    let scratch = TempDir::new().unwrap();
    let wasm = scratch.path().join("product.wasm");
    std::fs::write(&wasm, b"\0asm impostor").unwrap();
    let impostor = candidates().binary(
        b"\0asm impostor",
        declaring("@specforge/product", "9.9.9", &[]),
    );
    let dir = scratch.path().join("never");
    let extensions = vec![wasm.display().to_string()];
    let request = init::Request {
        dir: &dir,
        name: Some("demo"),
        version: init::DEFAULT_VERSION,
        extensions: &extensions,
    };

    let error = init::plan(&request, &impostor).unwrap_err();

    assert_eq!(error.code, "extension_not_found");
    assert_eq!(
        error.message,
        format!(
            "unresolvable extension '{}': the extension declares the name of the builtin '@specforge/product'",
            wasm.display()
        )
    );
    assert!(!dir.exists());
}

#[specforge_test_macros::test(
    behavior = "install_wasm_extension",
    verify = "add, init and publish read a candidate's declaration in the runtime their surface passes"
)]
fn init_reads_a_local_file_once() {
    use specforge_ops::init;
    let blobs = Blobs::new();
    let scratch = TempDir::new().unwrap();
    let dir = scratch.path().join("once");
    let extensions = vec![blobs.greet().display().to_string()];
    let request = init::Request {
        dir: &dir,
        name: Some("demo"),
        version: init::DEFAULT_VERSION,
        extensions: &extensions,
    };
    let runtime = candidates();
    let handshakes = |runtime: &InProcessRuntime| {
        runtime
            .calls()
            .iter()
            .filter(|call| call.extension == "__candidate" && call.export == "__handshake")
            .count()
    };

    let plan = init::plan(&request, &runtime).unwrap();
    assert_eq!(handshakes(&runtime), 1);

    init::apply(&dir, plan, &runtime).unwrap();
    assert_eq!(
        handshakes(&runtime),
        1,
        "apply installs without reading again"
    );
}

#[specforge_test_macros::test(
    behavior = "add_extension_to_existing_project",
    verify = "add extension appends to extensions list"
)]
fn add_from_a_subdirectory_adds_to_the_project_it_is_in() {
    let dir = project(&[], &[]);
    let root = dir.path();
    let sub = root.join("spec/docs");
    std::fs::create_dir_all(&sub).unwrap();
    let before = files_under(root);

    // The project the sub-path is in is the one that gains the extension.
    let added = add(&sub, Source::Builtin("@specforge/product")).unwrap();

    assert!(matches!(
        added.added[0].outcome,
        AddOutcome::Builtin { changed: true, .. }
    ));
    assert_eq!(listed(&added.writes, root), ["specforge.json"]);
    assert_eq!(changed_since(root, &before), ["specforge.json"]);
    assert!(!sub.join("specforge.json").exists());
}

#[specforge_test_macros::test(
    behavior = "non_interactive_init",
    verify = "init refuses local extensions the lock plan refuses before it writes anything"
)]
fn init_refuses_a_local_file_the_embedded_builtin_does_not_satisfy() {
    use specforge_ops::init;
    let scratch = TempDir::new().unwrap();
    let needs = scratch.path().join("needs.wasm");
    std::fs::write(&needs, testing::NEEDS_SOFTWARE_9).unwrap();
    let dir = scratch.path().join("never");
    let extensions = vec![
        needs.display().to_string(),
        "@specforge/software".to_string(),
    ];
    let request = init::Request {
        dir: &dir,
        name: Some("demo"),
        version: init::DEFAULT_VERSION,
        extensions: &extensions,
    };

    let error = init::plan(&request, &testing::probes()).unwrap_err();

    assert_eq!(error.code, "E027", "{error:?}");
    assert_eq!(
        error.message,
        "extension '@probe/needs-software' requires peer dependency '@specforge/software' ^9.0 but version 1.0.0 is installed"
    );
    assert!(!dir.join("specforge.json").exists());
}

#[specforge_test_macros::test(
    behavior = "non_interactive_init",
    verify = "init enables the builtins a local file requires before it"
)]
fn init_enables_the_builtins_a_local_file_requires() {
    use specforge_ops::init;
    let scratch = TempDir::new().unwrap();
    let needs = scratch.path().join("needs.wasm");
    std::fs::write(&needs, testing::NEEDS_SOFTWARE_1).unwrap();
    let dir = scratch.path().join("proj");
    let extensions = vec![needs.display().to_string()];
    let request = init::Request {
        dir: &dir,
        name: Some("demo"),
        version: init::DEFAULT_VERSION,
        extensions: &extensions,
    };
    let runtime = testing::probes();

    let plan = init::plan(&request, &runtime).unwrap();

    assert_eq!(
        plan.extensions,
        [
            "@specforge/software",
            "@probe/needs-software",
            "@specforge/testing"
        ]
    );
    init::apply(&dir, plan, &runtime).unwrap();
    let compiled = CompiledProject::compile(
        &specforge_common::ProjectRoot::of(&dir).unwrap(),
        Some(Arc::new(runtime)),
    );
    assert!(
        !compiled.diagnostics().iter().any(|d| d.code == "E027"),
        "{:?}",
        compiled.diagnostics()
    );
}
