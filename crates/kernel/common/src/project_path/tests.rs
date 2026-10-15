use super::*;

#[test]
fn a_file_under_the_root_is_relative_with_slashes() {
    let path = ProjectPath::of(Path::new("/p"), Path::new("/p/spec/a.spec"));
    assert_eq!(path.as_str(), "spec/a.spec");
    assert_eq!(path.to_string(), "spec/a.spec");
    assert_eq!(
        ProjectPath::of(Path::new("/p"), Path::new("/p/specforge.json")).as_str(),
        "specforge.json"
    );
}

#[test]
fn it_serializes_as_a_string_and_joins_back_onto_the_root() {
    let path = ProjectPath::of(Path::new("/p"), Path::new("/p/spec/a.spec"));
    assert_eq!(serde_json::to_value(&path).unwrap(), "spec/a.spec");
    assert_eq!(path.under(Path::new("/p")), Path::new("/p/spec/a.spec"));
    let outside = ProjectPath("/elsewhere/x.wasm".to_string());
    assert_eq!(
        outside.under(Path::new("/p")),
        Path::new("/elsewhere/x.wasm")
    );
}

#[cfg(unix)]
#[test]
fn a_root_spelled_through_a_link_names_the_same_file() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real");
    std::fs::create_dir_all(real.join("spec")).unwrap();
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    // Written through the link, named from the real root; a removed file (no longer on disk) included.
    assert_eq!(
        ProjectPath::of(&real, &link.join("spec/a.spec")).as_str(),
        "spec/a.spec"
    );
    assert_eq!(
        ProjectPath::of(&real, &link.join("gone.wasm")).as_str(),
        "gone.wasm"
    );
    assert_eq!(
        ProjectPath::of(&link, &real.join("spec/a.spec")).as_str(),
        "spec/a.spec"
    );
}

#[test]
fn a_removed_file_is_named_through_its_nearest_existing_ancestor() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("spec")).unwrap();
    let removed = dir.path().join("spec/deep/gone/x.spec");
    assert_eq!(
        ProjectPath::of(dir.path(), &removed).as_str(),
        "spec/deep/gone/x.spec"
    );
}

#[test]
fn a_file_outside_the_root_is_absolute_and_canonical() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    let elsewhere = dir.path().join("elsewhere");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&elsewhere).unwrap();

    let named = root.join("../elsewhere/x.wasm");
    let expected = std::fs::canonicalize(&elsewhere).unwrap().join("x.wasm");
    assert_eq!(
        ProjectPath::of(&root, &named).as_str(),
        expected.display().to_string()
    );
    assert_eq!(
        ProjectPath::absolute(&named).as_str(),
        expected.display().to_string()
    );
}
