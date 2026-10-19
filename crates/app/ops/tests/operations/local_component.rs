//! The extension a path names is read by one module (ADR 0172): add, init, publish and extension
//! validate refuse the same path with the same code and message.

use std::path::Path;

use crate::writes::project;
use specforge_ops::authoring::init;
use specforge_ops::extension::{self, AddRequest, publish, validate};
use specforge_ops::registry::Unconfigured;
use specforge_ops::registry::testing::MemoryRegistry;
use specforge_ops::testing::candidates;
use tempfile::TempDir;

/// The `(code, message)` each verb answers for `path`: add, init, publish, extension validate.
fn refusals(scratch: &Path, path: &Path) -> [(String, String); 4] {
    let existing = project(&[], &[]);
    let by_add = extension::add(
        &AddRequest {
            root: &specforge_common::ProjectRoot::of(existing.path()).unwrap(),
            extensions: vec![path.display().to_string()],
            base: None,
            allow_unsigned: false,
            dry_run: true,
            caller: specforge_ops::testing::caller(),
        },
        &Unconfigured("add"),
        &candidates(),
    )
    .unwrap_err()
    .error;

    let fresh = scratch.join(format!(
        "init-{}",
        path.file_name().unwrap().to_string_lossy()
    ));
    let extensions = [path.display().to_string()];
    let by_init = init::init(
        &init::Request {
            dir: &fresh,
            name: Some("demo"),
            version: init::DEFAULT_VERSION,
            extensions: &extensions,
            caller: specforge_ops::testing::caller(),
        },
        &candidates(),
    )
    .unwrap_err()
    .error;

    let by_publish = publish(path, &MemoryRegistry::new(), &candidates())
        .result
        .unwrap_err();
    let by_validate = validate(path, &candidates()).unwrap_err();
    [by_add, by_init, by_publish, by_validate].map(|e| (e.code.to_string(), e.message))
}

#[test]
#[ignore = "flipped by T7"]
fn every_verb_refuses_a_path_alike() {
    let t = TempDir::new().unwrap();
    let scratch = t.path();
    let missing = scratch.join("missing.wasm");

    let krate = scratch.join("krate");
    std::fs::create_dir_all(&krate).unwrap();
    std::fs::write(
        krate.join("Cargo.toml"),
        "[package]\nname = \"k\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    let several = scratch.join("several");
    let release = several.join("target/wasm32-wasip2/release");
    std::fs::create_dir_all(&release).unwrap();
    std::fs::write(release.join("a.wasm"), b"a").unwrap();
    std::fs::write(release.join("b.wasm"), b"b").unwrap();

    let not_built = |dir: &Path| {
        format!(
            "no built component in {}/target/wasm32-wasip2/release: build the extension first",
            dir.display()
        )
    };
    let holds_several = |dir: &Path| {
        format!(
            "{}/target/wasm32-wasip2/release holds several components (a.wasm, b.wasm): name the component to use",
            dir.display()
        )
    };

    #[cfg_attr(not(unix), expect(unused_mut, reason = "the alias case is unix only"))]
    let mut cases = vec![
        (
            missing.clone(),
            format!("no extension binary at {}", missing.display()),
        ),
        (krate.clone(), not_built(&krate)),
        (several.clone(), holds_several(&several)),
    ];
    #[cfg(unix)]
    {
        let alias = scratch.join("alias");
        std::os::unix::fs::symlink(&several, &alias).unwrap();
        cases.push((alias.clone(), holds_several(&alias)));
    }

    for (path, message) in cases {
        let expected = ("E040".to_string(), message);
        assert_eq!(
            refusals(scratch, &path),
            [(); 4].map(|()| expected.clone()),
            "{}",
            path.display()
        );
    }
}

#[cfg(unix)]
#[test]
#[ignore = "flipped by T7"]
fn an_unreadable_component_is_refused_alike() {
    use std::os::unix::fs::PermissionsExt;

    let t = TempDir::new().unwrap();
    let locked = t.path().join("locked.wasm");
    std::fs::write(&locked, specforge_ops::testing::GREET).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read(&locked).is_ok() {
        return; // root reads anyway
    }

    let got = refusals(t.path(), &locked);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).unwrap();

    let expected = (
        "E040".to_string(),
        format!(
            "failed to read {}: Permission denied (os error 13)",
            locked.display()
        ),
    );
    assert_eq!(got, [(); 4].map(|()| expected.clone()));
}
