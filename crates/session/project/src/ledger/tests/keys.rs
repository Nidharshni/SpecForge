//! One key for a source, however its path is spelled (ADR 0106 D3).

use crate::ledger::SessionInputs;
use crate::ledger::key::{discovered, source_key};

#[cfg(unix)]
#[test]
fn a_source_key_keeps_a_symlinks_own_name() {
    let dir = tempfile::TempDir::new().unwrap();
    let dir = std::fs::canonicalize(dir.path()).unwrap();
    let real = dir.join("real");
    std::fs::create_dir_all(real.join("shared")).unwrap();
    std::fs::write(real.join("shared/r.spec"), "").unwrap();
    // The root spelled two ways: through `link` and as `real`.
    std::os::unix::fs::symlink(&real, dir.join("link")).unwrap();
    std::os::unix::fs::symlink("shared/r.spec", real.join("alias.spec")).unwrap();

    assert_eq!(
        source_key(&dir.join("link"), &real.join("alias.spec")),
        "alias.spec"
    );
}

#[cfg(unix)]
#[test]
fn a_walked_path_and_its_spellings_have_one_key() {
    let dir = tempfile::TempDir::new().unwrap();
    let dir = std::fs::canonicalize(dir.path()).unwrap();
    let real = dir.join("elsewhere");
    std::fs::create_dir_all(&real).unwrap();
    std::fs::write(real.join("a.spec"), "").unwrap();
    // The spec root reached through a symlinked alias.
    let alias = dir.join("alias");
    std::os::unix::fs::symlink(&real, &alias).unwrap();

    let config = specforge_common::ProjectConfig::default();
    let inputs = SessionInputs::opened(&alias, &config);
    let reach = inputs.reach([real.join("a.spec").as_path()]);

    assert_eq!(discovered(&alias, &alias.join("a.spec")), "a.spec");
    assert_eq!(source_key(&alias, &real.join("a.spec")), "a.spec");
    assert_eq!(source_key(&alias, &alias.join("a.spec")), "a.spec");
    assert_eq!(reach.regions, ["a.spec"]);
}
