use super::*;

/// Formal's migration exposed this: an extension serving describes via
/// `raw_category` (static data) is declared by them exactly as the
/// builders declare, and its handshake carries no flags.
#[test]
fn raw_categories_are_declared() {
    let mut b = ContributionsBuilder::new(ExtensionMeta::new("@acme/formal", "1.0.0"));
    b.raw_category(
        DeclaredCategory::Entities,
        serde_json::json!([{ "name": "Property" }]),
    );
    b.raw_category(
        DeclaredCategory::ValidationRules,
        serde_json::json!([{
            "code": "F100",
            "severity": "warning",
            "message_template": "m",
            "check": "custom"
        }]),
    );

    let declaration = b.declaration();
    assert!(declaration.declares(DeclaredCategory::Entities));
    assert!(declaration.declares(DeclaredCategory::ValidationRules));
    assert!(!declaration.declares(DeclaredCategory::Collectors));
    let value: serde_json::Value = serde_json::from_str(&b.handshake_json()).unwrap();
    assert!(value.get("contribution_flags").is_none());
    assert!(value.get("providers").is_none());
}

#[test]
fn a_collector_can_capture_stdout() {
    let mut k = CollectorBuilder::new("cargo-test");
    let plain = serde_json::to_value(&k.descriptor).unwrap();
    assert!(plain.get("capture").is_none());
    k.capture_stdout();
    let captured = serde_json::to_value(&k.descriptor).unwrap();
    assert_eq!(captured["capture"], serde_json::json!("stdout"));
}

#[test]
fn a_starter_template_rides_the_handshake() {
    let mut b = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
    let without: serde_json::Value = serde_json::from_str(&b.handshake_json()).unwrap();
    assert!(without.get("starter_template").is_none(), "{without}");

    b.starter_template("spec \"{project}\" {}\n");
    let with: serde_json::Value = serde_json::from_str(&b.handshake_json()).unwrap();
    assert_eq!(
        with["starter_template"],
        serde_json::json!("spec \"{project}\" {}\n")
    );
}

#[test]
fn a_migration_hook_rides_the_handshake() {
    let mut b = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
    let without: serde_json::Value = serde_json::from_str(&b.handshake_json()).unwrap();
    assert!(without.get("migration_hook").is_none(), "{without}");

    b.migration_hook_handler("migrate_acme", |_| Ok(()));
    let with: serde_json::Value = serde_json::from_str(&b.handshake_json()).unwrap();
    assert_eq!(with["migration_hook"], serde_json::json!("migrate_acme"));
}
