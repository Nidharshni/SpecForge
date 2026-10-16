use std::path::{Path, PathBuf};

use specforge_common::ProjectConfig;

use crate::ledger::inputs::*;
use crate::ledger::{Ledger, Reading};

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
    let read = vec![dir.path().join("specforge.json")];
    assert_eq!(
        SessionInputs::opened(dir.path(), &config, read.clone()),
        SessionInputs::opened(dir.path(), &config, read.clone())
    );
    assert_ne!(
        SessionInputs::opened(dir.path(), &config, read),
        SessionInputs::opened(dir.path(), &config, Vec::new())
    );
}

/// `inputs` after one check run read `files` (a missing one records its directory).
fn after_checks(inputs: SessionInputs, files: &[PathBuf]) -> SessionInputs {
    let mut ledger = Ledger::over(inputs);
    let run = ledger.checks();
    for file in files {
        if !run.read(Reading::File, file, Path::exists)
            && let Some(parent) = file.parent()
        {
            run.read(Reading::Listing, parent, |_| ());
        }
    }
    run.finish();
    ledger.inputs().clone()
}

#[test]
fn named_files_change_the_value() {
    let dir = tempfile::TempDir::new().unwrap();
    let opened = SessionInputs::opened(dir.path(), &config(&[]), Vec::new());
    let named = vec![dir.path().join("docs/guide.md")];
    let with = after_checks(opened.clone(), &named);
    assert_ne!(opened, with);
    assert_eq!(with, after_checks(opened.clone(), &named));
    assert_eq!(after_checks(with, &[]), opened);
    // The missing file's directory is recorded with it.
    let watched = after_checks(opened, &named).watched();
    assert!(watched.contains(&Watched::Listing(dir.path().join("docs"))));
    assert!(watched.contains(&Watched::File(dir.path().join("docs/guide.md"))));
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
    let inputs = SessionInputs::opened(&link, &config(&[]), vec![link.join("specforge.json")]);
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
    assert!(!inputs.excludes("target/x.spec"));
    let reach = inputs.reach([Path::new("/x/a.spec"), Path::new("/x/specforge.json")]);
    assert!(reach.regions.is_empty() && reach.inputs.is_empty() && reach.pending.is_empty());
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
    let inputs = after_checks(
        SessionInputs::opened(
            root,
            &config,
            vec![
                root.join("specforge.json"),
                root.join("specforge.lock"),
                root.join("ext/local.wasm"),
            ],
        ),
        &[root.join("spec/../docs/guide.md")],
    );

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
    let inputs = after_checks(
        SessionInputs::opened(&root, &ProjectConfig::default(), Vec::new()),
        &[root.join("../outside/guide.md")],
    );

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
        vec![module.clone()],
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
        vec![module.clone()],
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
    let inputs = SessionInputs::opened(&dir.path().join("nope"), &config(&[]), Vec::new());
    assert!(inputs.watch_roots().is_empty());
    assert!(inputs.discover_at("").is_empty());
}

#[cfg(unix)]
#[test]
fn an_input_is_reached_by_its_link_and_by_each_hop() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join("root");
    let out = dir.path().join("out");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("a.md"), "a").unwrap();
    std::fs::write(out.join("b.md"), "b").unwrap();
    std::os::unix::fs::symlink(out.join("a.md"), root.join("l.md")).unwrap();

    // A check reads the link through the checks' disk, which records each hop.
    let mut ledger = Ledger::over(SessionInputs::opened(&root, &config(&[]), Vec::new()));
    let run = ledger.checks();
    assert!(specforge_registry::Disk::exists(
        &crate::ledger::disk::RunDisk(&run),
        &root.join("l.md")
    ));
    run.finish();
    let after = ledger.inputs().clone();

    // The link, and the file it points to, each reach the check input.
    for changed in [root.join("l.md"), out.join("a.md")] {
        let reach = after.reach([changed.as_path()]);
        assert_eq!(reach.inputs.len(), 1, "{changed:?}: {reach:?}");
        assert_eq!(reach.inputs[0].1[0].0, Kind::Check, "{reach:?}");
    }
    // A sibling of the target is not an input.
    let reach = after.reach([out.join("b.md").as_path()]);
    assert!(reach.inputs.is_empty(), "{reach:?}");
}

#[cfg(unix)]
#[test]
fn a_source_is_reached_by_each_hop_its_read_followed() {
    use crate::ledger::Reads;

    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join("root");
    let out = dir.path().join("out");
    std::fs::create_dir_all(root.join("links")).unwrap();
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("real.spec"), "term a_one \"A\" {}\n").unwrap();
    std::fs::write(out.join("b.md"), "b").unwrap();
    std::os::unix::fs::symlink(out.join("real.spec"), root.join("links/l1")).unwrap();
    std::os::unix::fs::symlink("links/l1", root.join("a.spec")).unwrap();

    let mut ledger = Ledger::over(SessionInputs::opened(&root, &config(&[]), Vec::new()));
    ledger.source(&root, "a.spec");
    let after = ledger.inputs().clone();

    // Each hop, and a directory above one, reaches the source.
    for changed in [
        root.join("links/l1"),
        out.join("real.spec"),
        root.join("links"),
    ] {
        let reach = after.reach([changed.as_path()]);
        assert_eq!(
            reach.linked,
            std::collections::BTreeSet::from(["a.spec".to_string()]),
            "{changed:?}: {reach:?}"
        );
    }
    // A sibling of the target is not a hop.
    assert!(after.reach([out.join("b.md").as_path()]).linked.is_empty());
}
