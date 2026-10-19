//! What `init` writes, and what a failed one puts back.

use std::path::Path;
use std::sync::Arc;

use specforge_ops::extension::{self, AddOutcome};
use specforge_ops::testing::{self, declaring};
use specforge_project::testing::compile_at;
use tempfile::TempDir;

use crate::writes::{
    Blobs, MODULE, Snapshot, add, candidates, changed_since, files_under, listed, project,
};
use specforge_wasm::testing::InProcessRuntime;

fn runtime() -> Arc<specforge_component::ComponentRuntime> {
    specforge_project::testing::component_runtime()
}

/// What `init` scaffolds in `dir` with `extensions`, the candidates read in
/// process.
fn init(dir: &Path, extensions: &[String]) -> specforge_ops::authoring::init::Outcome {
    use specforge_ops::authoring::init;
    let request = init::Request {
        dir,
        name: Some("demo"),
        version: init::DEFAULT_VERSION,
        extensions,
        caller: specforge_ops::testing::caller(),
    };
    init::init(&request, &candidates()).unwrap()
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
    use specforge_ops::authoring::init;
    let scratch = TempDir::new().unwrap();
    // The structural starter, and the templates the software and product
    // builtins contribute (`{version}` in them is the project's version).
    let starter_of = |extensions: &[String], version: &str| {
        let dir = scratch.path().join(format!(
            "{}-{version}",
            extensions.join("_").replace(['/', '@'], "-")
        ));
        let request = init::Request {
            dir: &dir,
            name: Some("demo"),
            version,
            extensions,
            caller: specforge_ops::testing::caller(),
        };
        init::init(&request, &*runtime()).unwrap();
        let config: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("specforge.json")).unwrap()).unwrap();
        assert_eq!(config["version"], version);
        std::fs::read_to_string(dir.join("spec/hello.spec")).unwrap()
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
    use specforge_ops::{OpErrorKind, authoring::init};
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
        caller: specforge_ops::testing::caller(),
    };

    let error = init::init(&request, &candidates()).unwrap_err();

    assert_eq!(error.error.code, init::STARTER_EXISTS, "{error:?}");
    assert_eq!(error.error.kind, OpErrorKind::Conflict);
    assert_eq!(files_under(&dir), before, "the file is byte-identical");
}

#[specforge_test_macros::test(
    behavior = "non_interactive_init",
    verify = "init refuses local extensions the lock plan refuses before it writes anything"
)]
fn init_refuses_local_extensions_over_an_unreadable_lock_before_writing() {
    use specforge_ops::authoring::init;
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
        caller: specforge_ops::testing::caller(),
    };

    let error = init::init(&request, &candidates()).unwrap_err();

    assert_eq!(error.error.code, "E033", "{error:?}");
    assert_eq!(files_under(&dir), before);
}

#[specforge_test_macros::test(
    behavior = "scaffold_new_project",
    verify = "a failed init leaves the directory as it was, files that were there included"
)]
fn a_failed_init_puts_back_what_was_there() {
    use specforge_ops::authoring::init;
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
        caller: specforge_ops::testing::caller(),
    };

    let error = init::init(&request, &candidates()).unwrap_err();

    assert_eq!(error.error.code, "E032", "{error:?}");
    assert!(error.error.writes.is_empty(), "{:?}", error.error.writes);
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

#[specforge_test_macros::test(
    behavior = "scaffold_new_project",
    verify = "a failed init leaves the directory as it was, files that were there included"
)]
fn a_failed_init_leaves_an_empty_directory_empty() {
    use specforge_ops::authoring::init;
    let blobs = Blobs::new();
    let scratch = TempDir::new().unwrap();
    let dir = scratch.path().join("victim");
    std::fs::create_dir_all(&dir).unwrap();
    // `.specforge` is a file: no module can be placed under it.
    std::fs::write(dir.join(".specforge"), "not a directory\n").unwrap();
    let extensions = [blobs.greet().display().to_string()];
    let request = init::Request {
        dir: &dir,
        name: Some("demo"),
        version: init::DEFAULT_VERSION,
        extensions: &extensions,
        caller: specforge_ops::testing::caller(),
    };

    let error = init::init(&request, &candidates()).unwrap_err();

    assert_eq!(error.error.code, "E032", "{error:?}");
    assert!(error.error.writes.is_empty(), "{:?}", error.error.writes);
    let left: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(left, [std::ffi::OsString::from(".specforge")]);
    assert_eq!(
        std::fs::read_to_string(dir.join(".specforge")).unwrap(),
        "not a directory\n"
    );
}

#[specforge_test_macros::test(
    behavior = "non_interactive_init",
    verify = "init enables a builtin after the builtins it requires, as add does"
)]
fn init_enables_a_builtin_after_the_builtins_it_requires() {
    use specforge_ops::authoring::init;
    let scratch = TempDir::new().unwrap();
    let extensions = vec!["@specforge/formal".to_string()];
    let request = init::Request {
        dir: &scratch.path().join("formal"),
        name: Some("demo"),
        version: init::DEFAULT_VERSION,
        extensions: &extensions,
        caller: specforge_ops::testing::caller(),
    };

    let outcome = init::init(&request, &*runtime()).unwrap();

    assert_eq!(
        outcome.extensions,
        [
            "@specforge/software",
            "@specforge/formal",
            "@specforge/testing"
        ]
    );
    let starter = std::fs::read_to_string(scratch.path().join("formal/spec/hello.spec")).unwrap();
    assert!(starter.contains("software specification"), "{starter}");
}

#[specforge_test_macros::test(
    behavior = "non_interactive_init",
    verify = "init enables a builtin after the builtins it requires, as add does"
)]
fn init_enables_each_builtin_after_the_builtins_it_requires() {
    use specforge_ops::authoring::init;
    let scratch = TempDir::new().unwrap();
    let runtime = runtime();
    for (i, builtin) in specforge_project::builtins().names().enumerate() {
        let extensions = vec![builtin.to_string()];
        let request = init::Request {
            dir: &scratch.path().join(format!("p{i}")),
            name: Some("demo"),
            version: init::DEFAULT_VERSION,
            extensions: &extensions,
            caller: specforge_ops::testing::caller(),
        };

        let extensions = init::init(&request, &*runtime).unwrap().extensions;

        for enabled in &extensions {
            let Some(enabled) = extension::builtin_name(enabled) else {
                continue;
            };
            let position = |name: &str| extensions.iter().position(|e| e == name);
            for required in extension::Candidate::builtin(&*runtime, enabled)
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
                    extensions
                );
            }
        }
    }
}

#[test]
fn init_refuses_a_local_binary_that_claims_a_builtins_name_before_writing() {
    use specforge_ops::authoring::init;
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
        caller: specforge_ops::testing::caller(),
    };

    let error = init::init(&request, &impostor).unwrap_err();

    // As `add` refuses it: the extension conflicts with the builtin.
    assert_eq!(error.error.code, "extension_conflict");
    assert_eq!(
        error.error.message,
        "the extension declares the name of the builtin '@specforge/product'"
    );
    assert!(!dir.exists());
}

#[specforge_test_macros::test(
    behavior = "install_wasm_extension",
    verify = "add, init and publish read a candidate's declaration in the runtime their surface passes"
)]
fn init_reads_a_local_file_once() {
    use specforge_ops::authoring::init;
    let blobs = Blobs::new();
    let scratch = TempDir::new().unwrap();
    let dir = scratch.path().join("once");
    let extensions = vec![blobs.greet().display().to_string()];
    let request = init::Request {
        dir: &dir,
        name: Some("demo"),
        version: init::DEFAULT_VERSION,
        extensions: &extensions,
        caller: specforge_ops::testing::caller(),
    };
    let runtime = candidates();
    let handshakes = |runtime: &InProcessRuntime| {
        runtime
            .calls()
            .iter()
            .filter(|call| call.extension == "__candidate" && call.export == "__handshake")
            .count()
    };

    init::init(&request, &runtime).unwrap();
    assert_eq!(
        handshakes(&runtime),
        1,
        "init reads the file once, then installs it"
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
    let added = add(&sub, "@specforge/product").unwrap();

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
    use specforge_ops::authoring::init;
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
        caller: specforge_ops::testing::caller(),
    };

    let error = init::init(&request, &testing::probes()).unwrap_err();

    assert_eq!(error.error.code, "E027", "{error:?}");
    // Named by the specifier that asked for it, as `add` names it among several.
    assert!(
        error.error.message.starts_with("adding '")
            && error.error.message.ends_with(
                "extension '@probe/needs-software' requires peer dependency '@specforge/software' ^9.0 but version 1.0.0 is installed"
            ),
        "{error:?}"
    );
    assert!(!dir.join("specforge.json").exists());
}

#[specforge_test_macros::test(
    behavior = "non_interactive_init",
    verify = "init enables the builtins a local file requires before it"
)]
fn init_enables_the_builtins_a_local_file_requires() {
    use specforge_ops::authoring::init;
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
        caller: specforge_ops::testing::caller(),
    };
    let runtime = testing::probes();

    let outcome = init::init(&request, &runtime).unwrap();

    assert_eq!(
        outcome.extensions,
        [
            "@specforge/software",
            "@probe/needs-software",
            "@specforge/testing"
        ]
    );
    let compiled = compile_at(&dir, Some(Arc::new(runtime)));
    assert!(
        !compiled.diagnostics().iter().any(|d| d.code == "E027"),
        "{:?}",
        compiled.diagnostics()
    );
}

/// A `.gitignore` that is a symlink leaving the directory: a bounded init refuses before writing,
/// the CLI's (unbounded) writes through it, as the user's own link (ADR 0092 D6, D8).
#[cfg(unix)]
#[test]
fn init_writes_no_file_through_a_gitignore_that_leaves_its_directory() {
    use specforge_ops::Containment;
    use specforge_ops::authoring::init;
    let scratch = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let outside = elsewhere.path().join("victim");
    let request = |dir: &Path, containment: &Containment| {
        let request = init::Request {
            dir,
            name: Some("demo"),
            version: init::DEFAULT_VERSION,
            extensions: &[],
            caller: specforge_ops::testing::bounded(containment),
        };
        init::init(&request, &candidates())
    };
    let linked = |name: &str| {
        let dir = scratch.path().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&outside, "one\n").unwrap();
        std::os::unix::fs::symlink(&outside, dir.join(".gitignore")).unwrap();
        dir
    };

    let dir = linked("bounded");
    let refused = request(&dir, &Containment::within(scratch.path())).unwrap_err();
    assert_eq!(refused.error.code, specforge_ops::OUTSIDE_ROOT);
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "one\n");
    assert!(!dir.join("specforge.json").exists());

    let dir = linked("unbounded");
    request(&dir, &Containment::unbounded()).unwrap();
    assert_ne!(std::fs::read_to_string(&outside).unwrap(), "one\n");
}

/// A `.gitignore` linked to a file in a sibling directory the caller's bound still holds: only
/// init's own narrowing to the directory it creates refuses it.
#[cfg(unix)]
#[test]
fn init_writes_nothing_outside_its_directory_whatever_bound_its_caller_lends() {
    use specforge_ops::Containment;
    use specforge_ops::authoring::init;
    let scratch = TempDir::new().unwrap();
    let victim = scratch.path().join("sibling/victim");
    std::fs::create_dir_all(victim.parent().unwrap()).unwrap();
    std::fs::write(&victim, "sibling-original\n").unwrap();
    let dir = scratch.path().join("new");
    std::fs::create_dir_all(&dir).unwrap();
    std::os::unix::fs::symlink(&victim, dir.join(".gitignore")).unwrap();
    let within = Containment::within(scratch.path());
    let request = init::Request {
        dir: &dir,
        name: Some("demo"),
        version: init::DEFAULT_VERSION,
        extensions: &[],
        caller: specforge_ops::testing::bounded(&within),
    };

    let refused = init::init(&request, &candidates()).unwrap_err();

    assert_eq!(
        refused.error.code,
        specforge_ops::OUTSIDE_ROOT,
        "{refused:?}"
    );
    assert_eq!(
        std::fs::read_to_string(&victim).unwrap(),
        "sibling-original\n"
    );
    assert!(!dir.join("specforge.json").exists());
}

#[test]
fn init_writes_the_entries_its_plan_judged() {
    use specforge_ops::authoring::init;
    let scratch = TempDir::new().unwrap();
    let dir = scratch.path().join("formal");
    let extensions = vec!["@specforge/formal".to_string()];
    let request = init::Request {
        dir: &dir,
        name: Some("demo"),
        version: init::DEFAULT_VERSION,
        extensions: &extensions,
        caller: specforge_ops::testing::caller(),
    };

    let outcome = init::init(&request, &*runtime()).unwrap();

    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("specforge.json")).unwrap())
            .unwrap();
    let expected = [
        "@specforge/software",
        "@specforge/formal",
        "@specforge/testing",
    ];
    assert_eq!(outcome.extensions, expected);
    assert_eq!(written["extensions"], serde_json::json!(expected));
    let mut keys: Vec<&str> = written
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["$schema", "extensions", "name", "spec_root", "version"]
    );
}

/// A `.gitignore` init cannot read is a read failure, not a write one (ADR 0155).
#[cfg(unix)]
#[test]
#[ignore = "flipped by T5"]
fn an_init_that_cannot_read_gitignore_is_unreadable() {
    use specforge_ops::authoring::init;
    use std::os::unix::fs::PermissionsExt;
    let scratch = TempDir::new().unwrap();
    let dir = scratch.path().join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    let gitignore = dir.join(".gitignore");
    std::fs::write(&gitignore, "x\n").unwrap();
    std::fs::set_permissions(&gitignore, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read(&gitignore).is_ok() {
        return; // permissions do not bind (root)
    }
    let request = init::Request {
        dir: &dir,
        name: Some("demo"),
        version: init::DEFAULT_VERSION,
        extensions: &[],
        caller: specforge_ops::testing::caller(),
    };

    let refused = init::init(&request, &candidates()).unwrap_err();
    std::fs::set_permissions(&gitignore, std::fs::Permissions::from_mode(0o644)).unwrap();

    let error = refused.error;
    assert_eq!(error.code, specforge_ops::FILE_UNREADABLE, "{error:?}");
    assert_eq!(
        error.kind,
        specforge_ops::OpErrorKind::PermissionDenied,
        "{error:?}"
    );
    assert!(
        error.message.starts_with("failed to read .gitignore: "),
        "{}",
        error.message
    );
    assert_eq!(files_under(&dir).len(), 1, "only the .gitignore");
}
