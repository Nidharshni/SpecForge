use super::*;
use crate::extension::lock_plan::Asked as Answer;

#[test]
fn the_plan_judges_and_answers_one_set_of_entries() {
    let runtime = crate::testing::probes();
    let installed = nowhere(Vec::new());
    let mut plan = LockPlan::adding(
        &installed,
        opened(&["@acme/foo@1.0.0", " ./a.wasm "]),
        &runtime,
        fetching(no_registry()),
        crate::testing::caller(),
        Purpose::Commit,
        false,
    )
    .unwrap();
    plan.disable(Ask(0), Disabled::Entry("./a.wasm".into()));
    plan.enable(Ask(0), "@acme/bar");

    let judged = plan.entries_after();
    let items = plan.config.items_after(
        |entry| Disabled::Entry("./a.wasm".into()).drops(entry),
        &plan.appended,
    );

    assert_eq!(judged, ["@acme/foo@1.0.0", "@acme/bar"]);
    assert_eq!(
        items,
        [
            serde_json::json!("@acme/foo@1.0.0"),
            serde_json::json!("@acme/bar")
        ]
    );
}

#[test]
fn a_non_string_item_is_kept_where_it_was() {
    let read = specforge_common::parse_project_config(
        Path::new("specforge.json"),
        r#"{"extensions": ["@acme/foo@1.0.0", 7, "@acme/gone"]}"#,
    );
    let opened = Opened::read(&read.config, true);

    let items = opened.items_after(|entry| entry == "@acme/gone", &[]);

    assert_eq!(opened.entries(), ["@acme/foo@1.0.0", "@acme/gone"]);
    assert_eq!(
        items,
        [serde_json::json!("@acme/foo@1.0.0"), serde_json::json!(7)]
    );
}

/// A plan over the `specforge.json` holding `config`, enabling `names`, checked.
fn enabling<'a>(
    installed: &'a Installed,
    runtime: &'a InProcessRuntime,
    root: &Path,
    purpose: Purpose,
    names: &[&str],
) -> Checked<'a> {
    let mut plan = LockPlan::adding(
        installed,
        opened_at(root),
        runtime,
        fetching(no_registry()),
        crate::testing::caller(),
        purpose,
        false,
    )
    .unwrap();
    for (at, name) in names.iter().enumerate() {
        plan.enable(Ask(at), name);
    }
    plan.check().ok().unwrap()
}

fn software() -> InProcessRuntime {
    crate::testing::serving_builtin(
        runtime(),
        "@specforge/software",
        declaring("@specforge/software", "1.0.0", &[]),
    )
}

fn project_with(config: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("specforge.json"), config).unwrap();
    dir
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "add extension appends to extensions list"
)]
fn a_commit_writes_the_entries_the_plan_judged_and_keeps_the_rest() {
    let dir = project_with(
        r#"{"name":"p","version":"0.1.0","spec_root":"spec","extensions":["@acme/foo@1.0.0",7]}"#,
    );
    let (runtime, installed) = (software(), Installed::at(dir.path()));
    let checked = enabling(
        &installed,
        &runtime,
        dir.path(),
        Purpose::Commit,
        &["@specforge/software"],
    );
    let judged = checked.entries();

    let done = checked.finish().unwrap();

    let text = std::fs::read_to_string(dir.path().join("specforge.json")).unwrap();
    let written: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(judged, ["@acme/foo@1.0.0", "@specforge/software"]);
    assert_eq!(
        written["extensions"],
        serde_json::json!(["@acme/foo@1.0.0", 7, "@specforge/software"])
    );
    assert_eq!(written["name"], "p");
    assert_eq!(written["spec_root"], "spec");
    assert_eq!(
        text,
        String::from_utf8(crate::config::render(&written).unwrap()).unwrap()
    );
    assert_eq!(done.enabled, 2);
    assert!(matches!(done.asked(Ask(0)), Answer::Changes { .. }));
}

#[test]
fn an_unchanged_set_of_entries_writes_nothing() {
    let dir = project_with(r#"{"extensions":["@specforge/software@1.0.0"]}"#);
    let (runtime, installed) = (software(), Installed::at(dir.path()));

    let done = enabling(
        &installed,
        &runtime,
        dir.path(),
        Purpose::Commit,
        &["@specforge/software"],
    )
    .finish()
    .unwrap();

    assert_eq!(
        std::fs::read_to_string(dir.path().join("specforge.json")).unwrap(),
        r#"{"extensions":["@specforge/software@1.0.0"]}"#
    );
    assert!(done.writes.is_empty());
    assert_eq!(done.asked(Ask(0)), Answer::Unchanged { locked: None });
}

#[test]
fn a_preview_over_a_changed_config_answers_what_was_judged() {
    let dir = project_with(r#"{"extensions":[]}"#);
    let (runtime, installed) = (software(), Installed::at(dir.path()));
    let checked = enabling(
        &installed,
        &runtime,
        dir.path(),
        Purpose::Preview,
        &["@specforge/software"],
    );
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"extensions":["@acme/x"]}"#,
    )
    .unwrap();

    let done = checked.finish().unwrap();

    assert!(matches!(done.asked(Ask(0)), Answer::Changes { .. }));
    assert!(done.writes.is_empty());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("specforge.json")).unwrap(),
        r#"{"extensions":["@acme/x"]}"#
    );
}

#[test]
fn a_config_that_appeared_or_no_longer_parses_is_changed() {
    let dir = project_with(r#"{"extensions":["@acme/x"]}"#);
    let held = crate::Containment::unbounded()
        .resolve(&dir.path().join("specforge.json"))
        .unwrap();
    let opened_none = Opened::read(&specforge_common::ProjectConfig::default(), false);
    let opened_file = opened_at(dir.path());

    // A file that was not there when the plan opened is changed.
    let error = opened_none.unchanged(&held, "add").unwrap_err();
    assert_eq!(error.code, crate::FILE_CHANGED);
    assert_eq!(
        error.message,
        "cannot add: specforge.json changed since it was read; nothing was written"
    );
    assert_eq!(error.suggestion.as_deref(), Some("run add again"));
    // Formatting is not a change; a document that no longer parses is.
    std::fs::write(held.spelled.clone(), "{ \"extensions\" :\n [\"@acme/x\"] }").unwrap();
    assert!(opened_file.unchanged(&held, "add").is_ok());
    std::fs::write(held.spelled.clone(), "{").unwrap();
    assert_eq!(
        opened_file.unchanged(&held, "add").unwrap_err().code,
        crate::FILE_CHANGED
    );
    // A file that went is changed; no file, when none was opened, is not.
    std::fs::remove_file(held.spelled.clone()).unwrap();
    assert_eq!(
        opened_file.unchanged(&held, "add").unwrap_err().code,
        crate::FILE_CHANGED
    );
    assert!(opened_none.unchanged(&held, "add").is_ok());
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "add extension appends to extensions list"
)]
fn an_entry_whose_name_prefixes_an_enabled_one_is_appended() {
    let dir = project_with(r#"{"name":"p","extensions":["@specforge/software-ish@1.0.0"]}"#);
    let (runtime, installed) = (software(), Installed::at(dir.path()));

    let done = enabling(
        &installed,
        &runtime,
        dir.path(),
        Purpose::Commit,
        &["@specforge/software"],
    )
    .finish()
    .unwrap();

    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("specforge.json")).unwrap())
            .unwrap();
    assert_eq!(
        written["extensions"],
        serde_json::json!(["@specforge/software-ish@1.0.0", "@specforge/software"])
    );
    assert!(matches!(done.asked(Ask(0)), Answer::Changes { .. }));
}

#[test]
fn a_named_removal_drops_every_entry_naming_exactly_it() {
    let read = specforge_common::parse_project_config(
        Path::new("specforge.json"),
        r#"{"name":"p","extensions":["@acme/foo@1.0.0","@acme/foobar","@acme/foo","@acme/foo=./foo.wasm"]}"#,
    );
    let opened = Opened::read(&read.config, true);
    let named = Disabled::Named("@acme/foo".into());

    let items = opened.items_after(|entry| named.drops(entry), &[]);

    assert_eq!(
        items,
        [
            serde_json::json!("@acme/foobar"),
            serde_json::json!("@acme/foo=./foo.wasm")
        ],
        "a file entry is not dropped by name"
    );
}

#[test]
fn an_entry_removal_drops_only_the_entries_written_so() {
    let read = specforge_common::parse_project_config(
        Path::new("specforge.json"),
        r#"{"extensions":["./ext/a.wasm"," ./ext/a.wasm ","./ext/a.wasm@1","@acme/foo"]}"#,
    );
    let opened = Opened::read(&read.config, true);
    let entry = Disabled::Entry(" ./ext/a.wasm ".into());

    let items = opened.items_after(|name| entry.drops(name), &[]);

    assert_eq!(
        items,
        [
            serde_json::json!("./ext/a.wasm@1"),
            serde_json::json!("@acme/foo")
        ]
    );
}

#[cfg(unix)]
#[test]
fn a_config_write_goes_to_the_file_it_was_handed() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("real.json");
    std::fs::write(&target, r#"{"name":"p","extensions":[]}"#).unwrap();
    std::os::unix::fs::symlink(&target, dir.path().join("specforge.json")).unwrap();
    let (runtime, installed) = (software(), Installed::at(dir.path()));

    enabling(
        &installed,
        &runtime,
        dir.path(),
        Purpose::Commit,
        &["@specforge/software"],
    )
    .finish()
    .unwrap();

    // The target is rewritten and the link is kept.
    assert!(
        std::fs::symlink_metadata(dir.path().join("specforge.json"))
            .unwrap()
            .is_symlink()
    );
    assert!(
        std::fs::read_to_string(&target)
            .unwrap()
            .contains("@specforge/software")
    );
}
