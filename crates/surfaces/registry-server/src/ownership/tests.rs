//! The ownership rule through `Database`, its interface.

use crate::db::{Database, PackageVersion, TokenRecord};
use crate::ownership::WriteRefused;
use specforge_package::PackageName;

fn writer(hash: &str, filter: Option<&str>) -> TokenRecord {
    TokenRecord {
        token_hash: hash.to_string(),
        scope: filter.map(str::to_string),
        label: "t".to_string(),
        created_at: "2026-01-01".to_string(),
        expires_at: None,
        admin: false,
    }
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
    assert_eq!(db.may_write(&writer("a", None), &name("@x/y")), Ok(()));
    assert_eq!(
        db.may_write(&writer("a", Some("@x")), &name("@x/y")),
        Ok(())
    );
}

#[test]
fn a_token_outside_its_filter_is_refused_before_the_owner_is_read() {
    let (_dir, db) = database();
    let a = writer("a", None);
    db.publish(&pv("@x/y"), &name("@x/y"), &a).unwrap();
    // Outside the filter *and* owned by another: the filter is told first.
    assert_eq!(
        db.may_write(&writer("b", Some("@web")), &name("@x/z")),
        Err(WriteRefused::Outside {
            filter: "@web".to_string(),
            package: "@x/z".to_string()
        })
    );
}

#[test]
fn a_scope_owned_by_another_is_refused_naming_its_owner() {
    let (_dir, db) = database();
    db.publish(&pv("@x/y"), &name("@x/y"), &writer("a", None))
        .unwrap();
    let refused = db.may_write(&writer("b", None), &name("@x/z")).unwrap_err();
    assert!(
        matches!(&refused, WriteRefused::Owned { scope, .. } if scope == "@x"),
        "{refused:?}"
    );
}

#[test]
fn the_owner_writes_again_into_its_scope() {
    let (_dir, db) = database();
    let a = writer("a", None);
    assert_eq!(db.publish(&pv("@x/y"), &name("@x/y"), &a), Ok(true));
    assert_eq!(db.may_write(&a, &name("@x/z")), Ok(()));
    assert_eq!(db.publish(&pv("@x/z"), &name("@x/z"), &a), Ok(false));
}
