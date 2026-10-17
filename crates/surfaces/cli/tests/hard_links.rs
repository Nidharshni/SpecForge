//! A hard link planted at the name a cache is staged under is never written through (ADR 0144).
//! The CLI is unbounded (ADR 0092 D6), so only the write can be made safe. The links sit at the temp
//! name the writers used before ADR 0144 and at the one `replace_file` uses now.

use crate::support::{Cli, TestProject};
use specforge_project::testing::specs;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Hard links to one outside file at each of `names` under `dir`; the file's path.
#[cfg(unix)]
fn plant(dir: &Path, names: &[&str]) -> (TempDir, std::path::PathBuf) {
    let outside = TempDir::new().unwrap();
    let victim = outside.path().join("victim.txt");
    fs::write(&victim, "ORIGINAL").unwrap();
    fs::create_dir_all(dir).unwrap();
    for name in names {
        fs::hard_link(&victim, dir.join(name)).unwrap();
    }
    (outside, victim)
}

#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "a mutation writes no file through a hard link to a file outside the project"
)]
#[ignore = "flipped by T7: the build cache is staged with fs::write"]
fn check_cache_temp_is_not_written_through_a_hard_link() {
    let dir = TestProject::new().raw_config(
        r#"{"name":"t","version":"0.1.0","spec_root":"spec","extensions":["@specforge/software"]}"#,
    )
    .file("spec/main.spec", specs::SOFTWARE);
    let (_outside, victim) = plant(
        dir.root(),
        &[".specforge-cache.json.tmp", "specforge-cache.json.tmp"],
    );

    Cli::on(&dir)
        .command(&["check", "--cache"])
        .assert()
        .success();

    assert_eq!(fs::read_to_string(&victim).unwrap(), "ORIGINAL");
    assert!(dir.root().join("specforge-cache.json").is_file());
}

#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "a mutation writes no file through a hard link to a file outside the project"
)]
#[ignore = "flipped by T7: the schema cache is staged with fs::write"]
fn export_schema_cache_temp_is_not_written_through_a_hard_link() {
    let dir = TestProject::new().raw_config(
        r#"{"name":"t","version":"0.1.0","spec_root":"spec","extensions":["@specforge/software"]}"#,
    )
    .file("spec/main.spec", specs::SOFTWARE);
    let (_outside, victim) = plant(
        &dir.root().join(".specforge"),
        &[".schema-cache.tmp", "schema-cache.json.tmp"],
    );

    Cli::on(&dir).command(&["export"]).assert().success();

    assert_eq!(fs::read_to_string(&victim).unwrap(), "ORIGINAL");
}
