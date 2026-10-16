use super::*;

#[test]
fn the_source_path_rule() {
    for (given, recorded) in [
        ("a.rs", "a.rs"),
        ("./a.rs", "a.rs"),
        ("src\\a.rs", "src/a.rs"),
        ("src//a.rs", "src/a.rs"),
        ("src/./a.rs", "src/a.rs"),
        ("src/", "src"),
    ] {
        assert_eq!(
            SourceKey::of(given).map(|k| k.as_str().to_string()),
            Ok(recorded.to_string()),
            "{given}"
        );
    }
    for outside in [
        "/x", "\\x", "C:\\x", "c:/x", "../x", "a/../b", "", ".", "./",
    ] {
        assert_eq!(SourceKey::of(outside), Err(OutsideRoot), "{outside:?}");
    }
    // A source root may be the project root itself.
    assert_eq!(SourceKey::root(".").unwrap().as_str(), ".");
    assert_eq!(SourceKey::root("./src").unwrap().as_str(), "src");
    for outside in ["", "/x", "../x"] {
        assert_eq!(SourceKey::root(outside), Err(OutsideRoot), "{outside:?}");
    }
}

#[test]
fn content_hash_is_sha256() {
    let hash = content_hash(b"fn main() {}");
    assert_eq!(hash.len(), 64);
    assert_eq!(
        content_hash(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}
