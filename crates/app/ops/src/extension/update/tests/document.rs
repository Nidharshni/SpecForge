use super::*;

#[specforge_test(
    behavior = "update_all_extensions",
    verify = "the update document lists every locked extension's status, held back when another failed"
)]
fn update_document_lists_each_status() {
    let row = |name: &str, status| ExtensionUpdate {
        name: name.into(),
        status,
    };
    let outcome = UpdateOutcome {
        extensions: vec![
            row(
                "@a/moved",
                UpdateStatus::Updated {
                    from: "1.0.0".into(),
                    to: "1.1.0".into(),
                    sha256: "ab".into(),
                    publisher: Publisher::Unsigned,
                },
            ),
            row(
                "@a/same",
                UpdateStatus::UpToDate {
                    version: "2.0.0".into(),
                },
            ),
            row(
                "@a/local",
                UpdateStatus::NotFromRegistry {
                    source: "local:x.wasm".into(),
                },
            ),
            row(
                "@a/gone",
                UpdateStatus::Failed(
                    OpError::coded(
                        crate::OpErrorKind::PreconditionFailed,
                        codes::E063,
                        "no registry",
                    )
                    .with_suggestion("add one"),
                ),
            ),
        ],
    };
    let ended_at = chrono::DateTime::parse_from_rfc3339("2026-10-11T12:00:00.123Z")
        .unwrap()
        .with_timezone(&chrono::Utc);

    let document = serde_json::to_value(outcome.document(ended_at)).unwrap();

    assert_eq!(document["ok"], false);
    assert_eq!(
        document["extensions"],
        serde_json::json!([
            {"status": "held_back", "name": "@a/moved", "from": "1.0.0", "to": "1.1.0"},
            {"status": "up_to_date", "name": "@a/same", "version": "2.0.0"},
            {"status": "not_from_registry", "name": "@a/local", "source": "local:x.wasm"},
            {"status": "failed", "name": "@a/gone", "code": "E063", "message": "no registry",
             "suggestion": "add one"},
        ])
    );
    assert_eq!(
        document["batch_update_completed"],
        serde_json::json!({"updatedCount": 0, "failedCount": 1, "skippedCount": 3,
            "timestamp": "2026-10-11T12:00:00.123Z"})
    );
}
