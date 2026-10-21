//! The ownership rule through `Database`, its interface.

use crate::db::{Database, PackageVersion, TokenRecord};
use crate::ownership::WriteRefused;
use specforge_package::PackageName;

/// A token of `account` (with `filter`), which the database must know.
fn writer(db: &Database, account: &str, filter: Option<&str>) -> TokenRecord {
    let hash = format!("hash-{account}-{}", filter.unwrap_or("all"));
    db.insert_token(&hash, account, filter, "t", None, false)
        .unwrap();
    db.validate_token(&hash).unwrap()
}

fn name(text: &str) -> PackageName {
    PackageName::parse(text).unwrap()
}

fn pv(name: &str) -> PackageVersion {
    PackageVersion {
        name: name.to_string(),
        version: "1.0.0".to_string(),
        sha256: "h".to_string(),
        size_bytes: 1,
        description: String::new(),
        keywords: String::new(),
        publisher: "p".to_string(),
        published_at: "2026-01-01".to_string(),
        signature: None,
        manifest: String::new(),
    }
}

fn database() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    (dir, db)
}

#[test]
fn a_token_writes_into_an_unclaimed_scope_its_filter_covers() {
    let (_dir, db) = database();
    assert_eq!(db.may_write(&writer(&db, "a", None), &name("@x/y")), Ok(()));
    assert_eq!(
        db.may_write(&writer(&db, "a", Some("@x")), &name("@x/y")),
        Ok(())
    );
}

#[test]
fn a_token_outside_its_filter_is_refused_before_the_owner_is_read() {
    let (_dir, db) = database();
    let a = writer(&db, "a", None);
    db.publish(&pv("@x/y"), &name("@x/y"), &a).unwrap();
    // Outside the filter *and* owned by another: the filter is told first.
    assert_eq!(
        db.may_write(&writer(&db, "b", Some("@web")), &name("@x/z")),
        Err(WriteRefused::Outside {
            filter: "@web".to_string(),
            package: "@x/z".to_string()
        })
    );
}

#[test]
fn a_scope_owned_by_another_is_refused_naming_its_owner() {
    let (_dir, db) = database();
    db.publish(&pv("@x/y"), &name("@x/y"), &writer(&db, "a", None))
        .unwrap();
    let refused = db
        .may_write(&writer(&db, "b", None), &name("@x/z"))
        .unwrap_err();
    assert!(
        matches!(&refused, WriteRefused::Owned { scope, .. } if scope == "@x"),
        "{refused:?}"
    );
}

#[test]
fn the_owner_writes_again_into_its_scope() {
    let (_dir, db) = database();
    let a = writer(&db, "a", None);
    assert_eq!(db.publish(&pv("@x/y"), &name("@x/y"), &a), Ok(true));
    assert_eq!(db.may_write(&a, &name("@x/z")), Ok(()));
    assert_eq!(db.publish(&pv("@x/z"), &name("@x/z"), &a), Ok(false));
}

#[test]
fn a_second_token_of_the_owning_account_writes_into_its_scope() {
    let (_dir, db) = database();
    let first = writer(&db, "acme", None);
    let second = writer(&db, "acme", Some("@x"));
    db.publish(&pv("@x/y"), &name("@x/y"), &first).unwrap();
    assert_eq!(db.may_write(&second, &name("@x/z")), Ok(()));
    assert_eq!(db.publish(&pv("@x/z"), &name("@x/z"), &second), Ok(false));
}

#[test]
fn a_schema_1_data_directory_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.db");
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("CREATE TABLE packages (id INTEGER PRIMARY KEY); PRAGMA user_version = 1;")
        .unwrap();
    let error = Database::open(&path).err().expect("schema 1 is refused");
    assert!(
        error.contains("schema 1") && error.contains("--data-dir"),
        "{error}"
    );
}

#[test]
fn account_names_are_lowercase_words() {
    for good in ["acme", "a", "team-7", &"x".repeat(39)] {
        assert_eq!(super::account_name(good), Ok(good));
    }
    for bad in ["", "-a", "Acme", "a b", "a_b", &"x".repeat(40)] {
        assert!(super::account_name(bad).is_err(), "{bad}");
    }
}
