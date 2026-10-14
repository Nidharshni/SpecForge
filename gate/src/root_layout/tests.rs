use super::*;

fn tracked(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|p| p.to_string()).collect()
}

const LIST: &[(&str, &str)] = &[("README.md", "front page"), ("crates", "workspace")];

#[test]
fn the_tracked_top_level_matching_the_list_passes() {
    assert_eq!(
        disagreements(
            LIST,
            &tracked(&["README.md", "crates/a/src/lib.rs", "crates/b/Cargo.toml"])
        ),
        Vec::<String>::new()
    );
}

#[test]
fn an_unlisted_top_level_entry_is_reported() {
    assert_eq!(
        disagreements(
            LIST,
            &tracked(&[
                "README.md",
                "crates/a/x.rs",
                "plans/MASTER.md",
                "plans/b.md"
            ])
        ),
        ["`plans` is tracked at the top level but not on the root list"]
    );
}

#[test]
fn a_listed_entry_no_longer_tracked_is_reported() {
    assert_eq!(
        disagreements(LIST, &tracked(&["crates/a/x.rs"])),
        ["the root list names `README.md`, which the repository does not track"]
    );
}

#[test]
fn the_root_list_is_sorted_unique_and_gives_every_reason() {
    let names: Vec<&str> = ROOT.iter().map(|(name, _)| *name).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(names, sorted);
    assert!(ROOT.iter().all(|(_, why)| !why.is_empty()));
}
