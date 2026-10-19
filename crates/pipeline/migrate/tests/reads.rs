//! A migration reads each source through the read it is handed (ADR 0158).

use std::io;
use std::path::Path;

use specforge_migrate::migrate_project;
use specforge_parser::CURRENT_FORMAT_VERSION;

#[test]
fn a_file_the_read_refuses_fails_and_is_not_written() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("spec/a.spec");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let old = "// specforge-format: 0.9\nbehavior alpha \"Alpha\" {\n}\n";
    std::fs::write(&file, old).unwrap();
    let refusal = "it has 2 names, so no bound can say where its content is";
    let summary = migrate_project(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        std::slice::from_ref(&file),
        &CURRENT_FORMAT_VERSION,
        &|_: &Path| Err(io::Error::new(io::ErrorKind::InvalidInput, refusal)),
    );
    assert_eq!(summary.failed_count, 1, "{summary:?}");
    assert_eq!(
        summary.results[0].error.as_deref(),
        Some(format!("failed to read file: {refusal}").as_str())
    );
    assert!(summary.diffs.is_empty() && summary.backups.is_empty() && summary.rewrites.is_empty());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), old);
    assert_eq!(
        std::fs::read_dir(file.parent().unwrap()).unwrap().count(),
        1
    );
}
