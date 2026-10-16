use std::path::Path;

use specforge_common::ProjectConfig;

use crate::ledger::inputs::*;

fn config(extensions: &[&str]) -> ProjectConfig {
    ProjectConfig {
        extensions: extensions.iter().map(|e| e.to_string()).collect(),
        ..ProjectConfig::default()
    }
}

#[test]
fn a_value_made_twice_from_the_same_config_is_equal() {
    let dir = tempfile::TempDir::new().unwrap();
    let config = config(&["@acme/local=ext/local.wasm"]);
    assert_eq!(
        SessionInputs::opened(dir.path(), &config),
        SessionInputs::opened(dir.path(), &config)
    );
    assert_ne!(
        SessionInputs::opened(dir.path(), &config),
        SessionInputs::opened(dir.path(), &self::config(&[]))
    );
}

#[test]
fn named_files_change_the_value() {
    let dir = tempfile::TempDir::new().unwrap();
    let opened = SessionInputs::opened(dir.path(), &config(&[]));
    let named = vec![dir.path().join("docs/guide.md")];
    let with = opened.with_named(named.clone());
    assert_ne!(opened, with);
    assert_eq!(with, opened.with_named(named));
    assert_eq!(with.with_named(Vec::new()), opened);
    // The missing file's directory is stamped with it.
    let files: Vec<&Path> = with.check_files().collect();
    assert!(
        files.contains(&dir.path().join("docs").as_path()),
        "{files:?}"
    );
    assert!(files.contains(&dir.path().join("docs/guide.md").as_path()));
}

#[test]
fn classify_compares_canonical_forms() {
    let dir = tempfile::TempDir::new().unwrap();
    let real = dir.path().join("real");
    std::fs::create_dir_all(real.join("spec")).unwrap();
    let link = dir.path().join("link");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real, &link).unwrap();
    #[cfg(not(unix))]
    let link = real.clone();
    let inputs = SessionInputs::opened(&link, &config(&[]));
    std::fs::write(real.join("specforge.json"), "{}").unwrap();

    // The same file, however the directory is spelled.
    for config in [real.join("specforge.json"), link.join("specforge.json")] {
        let reach = inputs.reach([config.as_path()]);
        assert_eq!(reach.inputs.len(), 1, "{reach:?}");
        assert_eq!(reach.inputs[0].1[0].0, Kind::Environment);
    }
    assert_eq!(
        inputs.reach([real.join("a.spec").as_path()]).regions,
        vec!["a.spec".to_string()]
    );
}

#[test]
fn detached_inputs_reach_nothing() {
    let inputs = SessionInputs::detached();
    assert_eq!(inputs.root(), None);
    assert_eq!(inputs.spec_root(), None);
    assert!(inputs.watch_roots().is_empty());
    assert!(inputs.watched().is_empty());
    assert!(inputs.discover().is_empty());
    assert!(!inputs.excludes("target/x.spec"));
    assert_eq!(inputs.environment_files().count(), 0);
    assert_eq!(inputs.check_files().count(), 0);
    let reach = inputs.reach([Path::new("/x/a.spec"), Path::new("/x/specforge.json")]);
    assert!(reach.regions.is_empty() && reach.inputs.is_empty() && reach.pending.is_empty());
    assert_eq!(inputs.with_named(vec!["/x/y".into()]), inputs);
}

#[test]
fn watched_paths_are_spelled_from_the_root_as_opened() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("spec")).unwrap();
    std::fs::create_dir_all(root.join("docs")).unwrap();
    let config = ProjectConfig {
        spec_root: Some("spec".to_string()),
        extensions: vec!["@acme/local=ext/local.wasm".to_string()],
        ..ProjectConfig::default()
    };
    // Named as the checks join them: through `spec/..`.
    let inputs =
        SessionInputs::opened(root, &config).with_named(vec![root.join("spec/../docs/guide.md")]);

    assert_eq!(
        inputs.watched(),
        vec![
            Watched::Sources(root.join("spec")),
            Watched::Directories(root.to_path_buf()),
            Watched::File(root.join("specforge.json")),
            Watched::File(root.join("specforge.lock")),
            Watched::File(root.join("ext/local.wasm")),
            Watched::File(root.join("docs/guide.md")),
            Watched::Listing(root.join("docs")),
        ]
    );
}

#[test]
fn a_watched_path_outside_the_root_is_canonical() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join("proj");
    std::fs::create_dir_all(&root).unwrap();
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let inputs = SessionInputs::opened(&root, &ProjectConfig::default())
        .with_named(vec![root.join("../outside/guide.md")]);

    let canonical_outside = std::fs::canonicalize(&outside).unwrap();
    let watched = inputs.watched();
    assert!(
        watched.contains(&Watched::File(canonical_outside.join("guide.md"))),
        "{watched:?}"
    );
    assert!(
        watched.contains(&Watched::Listing(canonical_outside.clone())),
        "{watched:?}"
    );
    // The root itself is spelled as opened.
    assert_eq!(watched[0], Watched::Sources(root));
}

#[test]
fn a_module_in_a_missing_directory_is_pending_as_an_environment_input() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join("proj");
    std::fs::create_dir_all(&root).unwrap();
    let far = dir.path().join("far");
    std::fs::create_dir_all(&far).unwrap();
    let module = far.join("deep/mods/ext.wasm");
    let inputs = SessionInputs::opened(
        &root,
        &config(&[&format!("@acme/far={}", module.display())]),
    );
    let far = std::fs::canonicalize(&far).unwrap();

    // The nearest existing ancestor is watched for its own entries.
    assert!(
        inputs.watch_roots().contains(&WatchRoot {
            dir: far.clone(),
            recursive: false
        }),
        "{:?}",
        inputs.watch_roots()
    );
    // A directory created on the way is the module's change; one
    // beside it is nothing.
    assert_eq!(
        inputs.reach([far.join("deep").as_path()]).pending,
        vec![(far.join("deep"), Kind::Environment)]
    );
    assert!(
        inputs
            .reach([far.join("other").as_path()])
            .pending
            .is_empty()
    );
    // Below the first directory on the way is not this ancestor's.
    assert!(
        inputs
            .reach([far.join("deep/mods").as_path()])
            .pending
            .is_empty()
    );
    // Once the directory exists, it is watched whole.
    std::fs::create_dir_all(far.join("deep/mods")).unwrap();
    let inputs = SessionInputs::opened(
        &root,
        &config(&[&format!("@acme/far={}", module.display())]),
    );
    assert!(
        inputs.watch_roots().contains(&WatchRoot {
            dir: far.join("deep/mods"),
            recursive: true
        }),
        "{:?}",
        inputs.watch_roots()
    );
}

#[test]
fn a_missing_root_has_no_watch_roots() {
    let dir = tempfile::TempDir::new().unwrap();
    let inputs = SessionInputs::opened(&dir.path().join("nope"), &config(&[]));
    assert!(inputs.watch_roots().is_empty());
    assert!(inputs.discover_at("").is_empty());
}
