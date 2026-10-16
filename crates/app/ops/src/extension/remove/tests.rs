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

static UNBOUNDED: crate::Containment = crate::Containment::unbounded();

fn removing(name: &str) -> RemoveRequest<'_> {
    RemoveRequest {
        name,
        force: false,
        dry_run: false,
        containment: &UNBOUNDED,
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

    assert_eq!(error.error.code, "no_project");
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
    assert_eq!(error.error.code, NOT_FOUND);
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

    assert_eq!(error.error.code, "extension_conflict");
    assert_eq!(
        error.error.data.unwrap()["entries"],
        serde_json::json!(["@sdk/greet", "greet.wasm"])
    );
    assert_eq!(files_under(fixture.dir.path()), before);
}

#[specforge_test(
    behavior = "remove_extension",
    verify = "a removal with an unreadable specforge.json is config_invalid and changes nothing"
)]
fn a_non_object_config_refuses_the_removal_before_any_write() {
    let fixture =
        installed(Fixture::new(), "@acme/x").config_problems(vec![ConfigProblem::NotAnObject {
            path: PathBuf::from("./specforge.json"),
        }]);
    write_config(&fixture, "[1,2]");
    let before = files_under(fixture.dir.path());

    for dry_run in [false, true] {
        let request = RemoveRequest {
            name: "@acme/x",
            force: true,
            dry_run,
            containment: &UNBOUNDED,
        };
        let error = remove(&fixture.view(), &request).unwrap_err();

        assert_eq!(error.error.code, "config_invalid");
        assert_eq!(
            error.error.message,
            "./specforge.json must be a JSON object"
        );
        assert_eq!(files_under(fixture.dir.path()), before);
    }
}

#[specforge_test(
    behavior = "management_operations_over_the_project_view",
    verify = "add, update and remove refuse an unusable specforge.json with one refusal, before they write"
)]
fn remove_refuses_as_add_and_update_do() {
    use crate::config::testing::UNUSABLE;
    use crate::extension::{AddRequest, Source, UpdateRequest, add, update};
    use crate::registry::Trust;

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
                containment: &UNBOUNDED,
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
                containment: &UNBOUNDED,
            },
            &unconfigured,
            &specforge_wasm::testing::InProcessRuntime::new(),
        )
        .unwrap_err();

        assert_eq!(removed.error.code, "config_invalid", "{config}");
        assert_eq!(removed.error.kind, OpErrorKind::SchemaMismatch, "{config}");
        assert_eq!(
            removed.error.message,
            read.problems[0].to_string(),
            "{config}"
        );
        assert_eq!(added.error, removed.error, "{config}");
        assert_eq!(updated, removed.error, "{config}");
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

    #[expect(
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
    assert_eq!(refused.unwrap_err().error.code, "config_write_failed");
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
        containment: &UNBOUNDED,
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

    assert_eq!(refused.error.code, NOT_FOUND);
    assert!(
        refused.error.message.contains("corrupt lock file"),
        "{refused:?}"
    );
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
            containment: &UNBOUNDED,
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

    assert_eq!(refused.error.code, NOT_FOUND);
    assert!(
        refused.error.message.contains("corrupt lock file"),
        "{refused:?}"
    );
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

#[specforge_test(
    invariant = "mcp_structured_error_responses",
    verify = "an operation states the request field each refusal is about, and a refusal of the project or the environment is about none"
)]
fn every_refusal_of_a_name_is_about_it() {
    let about = Some(RemoveField::Name);
    let subject = |fixture: &Fixture, name: &str| {
        let refusal = remove(&fixture.view(), &removing(name)).unwrap_err();
        (refusal.error.code.to_string(), refusal.subject)
    };

    // A name that could never be installed, and one the project has nothing under.
    let fixture = Fixture::new();
    assert_eq!(subject(&fixture, "@@bad"), ("E072".to_string(), about));
    assert_eq!(subject(&fixture, "@nope/x"), (NOT_FOUND.to_string(), about));

    // A name two entries enable.
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
    assert_eq!(
        subject(&fixture, "@sdk/greet"),
        ("extension_conflict".to_string(), about)
    );

    // A refusal of the project is about no field: an unusable config, no project.
    let fixture =
        installed(Fixture::new(), "@acme/x").config_problems(vec![ConfigProblem::NotAnObject {
            path: PathBuf::from("./specforge.json"),
        }]);
    assert_eq!(
        subject(&fixture, "@acme/x"),
        ("config_invalid".to_string(), None)
    );
    let fixture = Fixture::new();
    let refusal = remove(&fixture.rootless_view(), &removing("@acme/x")).unwrap_err();
    assert_eq!(
        (refusal.error.code.as_ref(), refusal.subject),
        ("no_project", None)
    );
}
