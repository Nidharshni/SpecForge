use super::*;

#[test]
fn a_record_for_an_unknown_entity_is_stray_with_a_close_match() {
    let mut project = Project::new();
    add_entity(&mut project, "widget");
    write_report(&project, &["widget", "wodget", "zzzzzzzz"]);
    let outcome = project.run(&pass("contracts")).unwrap();
    assert_eq!(
        outcome.stray_records,
        vec![
            StrayRecord {
                entity_id: "wodget".to_string(),
                near: Some("widget".to_string()),
            },
            StrayRecord {
                entity_id: "zzzzzzzz".to_string(),
                near: None,
            },
        ]
    );
    assert_eq!(
        json_of(&outcome)["stray_records"],
        json!([
            {"entity_id": "wodget", "near": "widget"},
            {"entity_id": "zzzzzzzz"}
        ])
    );
}

#[test]
fn stray_records_are_never_promoted_by_strict_and_change_neither_ok_nor_the_list() {
    let mut project = Project::new();
    add_entity(&mut project, "widget");
    write_report(&project, &["wodget"]);
    let lax = project.run(&pass("contracts")).unwrap();
    let strict = project
        .run(&AnalyzeOptions {
            strict: true,
            ..pass("contracts")
        })
        .unwrap();
    assert!(lax.ok() && strict.ok());
    assert_eq!(lax.stray_records, strict.stray_records);
    assert_eq!(strict.stray_records.len(), 1);
    assert!(strict.passes.iter().all(|p| p.findings.is_empty()));
}

#[test]
fn the_stray_records_key_is_absent_when_there_are_none() {
    let mut project = Project::new();
    add_entity(&mut project, "widget");
    write_report(&project, &["widget"]);
    let outcome = project.run(&pass("contracts")).unwrap();
    assert!(outcome.stray_records.is_empty());
    assert!(json_of(&outcome).get("stray_records").is_none());

    let none = Project::new().run(&pass("contracts")).unwrap();
    assert!(json_of(&none).get("stray_records").is_none());
}
