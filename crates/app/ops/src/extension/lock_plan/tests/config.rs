use super::*;

#[test]
fn the_plan_judges_and_answers_one_set_of_entries() {
    let runtime = crate::testing::probes();
    let installed = nowhere(Vec::new());
    let mut plan = LockPlan::adding(
        &installed,
        opened(&["@acme/foo@1.0.0", " ./a.wasm "]),
        &runtime,
        fetching(no_registry()),
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
    let opened = Opened::read(&read.config);

    let items = opened.items_after(|entry| entry == "@acme/gone", &[]);

    assert_eq!(opened.entries(), ["@acme/foo@1.0.0", "@acme/gone"]);
    assert_eq!(
        items,
        [serde_json::json!("@acme/foo@1.0.0"), serde_json::json!(7)]
    );
}
