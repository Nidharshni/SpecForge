use super::*;

fn matches(pattern: &str, path: &str) -> bool {
    Pattern(pattern).matches(path)
}

#[test]
fn pattern_star_stays_in_a_segment_and_double_star_crosses() {
    assert!(matches("*.rs", "a.rs"));
    assert!(!matches("*.rs", "a/b.rs"));
    assert!(matches("**/*.rs", "a.rs"));
    assert!(matches("**/*.rs", "crates/x/src/b.rs"));
    assert!(!matches("**/*.rs", "crates/x/src/b.spec"));
    assert!(matches("crates/*/src/**", "crates/x/src/a/b.rs"));
    assert!(!matches("crates/*/src/**", "crates/x/tests/a.rs"));
    assert!(!matches("crates/*/src/**", "crates/x/y/src/a.rs"));
    assert!(matches("Cargo.toml", "Cargo.toml"));
    assert!(!matches("Cargo.toml", "gate/Cargo.toml"));
    assert!(matches("**/Cargo.lock", "Cargo.lock"));
    assert!(matches("**/Cargo.lock", "extensions/product/Cargo.lock"));
    assert!(matches(
        "extensions/*/Cargo.toml",
        "extensions/product/Cargo.toml"
    ));
}

#[test]
fn matching_returns_the_changed_paths_a_pattern_reaches() {
    let changes = ChangeSet::of(["b.rs", "a.spec", "crates/x/src/lib.rs"]);
    assert_eq!(
        changes.matching(&["**/*.rs"]),
        ["b.rs", "crates/x/src/lib.rs"]
    );
    assert_eq!(changes.matching(&["**/*.spec", "nothing"]), ["a.spec"]);
    assert!(changes.matching(&["docs/**"]).is_empty());
}
