//! A check run stamps each file it reads, then reads it; what it read is the check inputs
//! (ADR 0106 D4, behavior `bring_session_up_to_date`).

use std::fs;

use specforge_common::ProjectConfig;
use specforge_registry::{Disk, PlainDisk};
use specforge_test_macros::test as specforge_test;

use crate::ledger::{Ledger, Reading, SessionInputs, Watched};

fn opened() -> (tempfile::TempDir, Ledger) {
    let dir = tempfile::TempDir::new().unwrap();
    let dir_path = std::fs::canonicalize(dir.path()).unwrap();
    let ledger = Ledger::over(SessionInputs::opened(&dir_path, &ProjectConfig::default()));
    (dir, ledger)
}

#[specforge_test(
    behavior = "bring_session_up_to_date",
    verify = "a source or check input written between its stamp and its read is seen next time"
)]
fn a_check_input_written_between_its_stamp_and_its_read_is_seen_next_time() {
    let (dir, mut ledger) = opened();
    let doc = std::fs::canonicalize(dir.path()).unwrap().join("guide.md");
    fs::write(&doc, "old").unwrap();
    let run = ledger.checks();
    run.read(Reading::File, &doc, |path| {
        fs::write(path, "new, and longer").unwrap();
        fs::read(path).ok()
    });
    run.finish();
    assert_eq!(ledger.stale().check_inputs, [doc]);
}

#[test]
fn a_missing_check_input_records_its_directory() {
    let (dir, mut ledger) = opened();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    let missing = root.join("docs/guide.md");
    let run = ledger.checks();
    assert!(!run.read(Reading::File, &missing, std::path::Path::exists));
    run.read(Reading::Listing, &root.join("docs"), |_| ());
    run.finish();
    assert!(
        ledger
            .inputs()
            .watched()
            .contains(&Watched::Listing(root.join("docs")))
    );
    let other = root.join("docs/other.md");
    fs::write(&other, "x").unwrap();
    assert_eq!(ledger.changes([other.as_path()]).check_inputs, [other]);
}

#[test]
fn an_unfinished_check_run_records_nothing() {
    let (dir, mut ledger) = opened();
    let doc = dir.path().join("guide.md");
    let before = ledger.inputs().clone();
    let run = ledger.checks();
    run.read(Reading::File, &doc, |_| ());
    drop(run);
    assert_eq!(ledger.inputs(), &before);
}

#[test]
fn a_run_that_reads_what_the_last_read_moves_nothing() {
    let (dir, mut ledger) = opened();
    let one = dir.path().join("one.md");
    let two = dir.path().join("two.md");
    fs::write(&one, "1").unwrap();
    fs::write(&two, "2").unwrap();
    let read = |ledger: &mut Ledger, file: &std::path::Path| {
        let run = ledger.checks();
        run.read(Reading::File, file, |_| ());
        run.finish()
    };
    assert!(read(&mut ledger, &one));
    assert!(!read(&mut ledger, &one));
    assert!(read(&mut ledger, &two));
}

#[test]
fn a_run_that_reads_a_path_twice_moves_nothing() {
    let (dir, mut ledger) = opened();
    let one = dir.path().join("one.md");
    let two = dir.path().join("two.md");
    let run_of = |ledger: &mut Ledger, files: &[&std::path::Path]| {
        let run = ledger.checks();
        for file in files {
            run.read(Reading::File, file, |_| ());
        }
        run.finish()
    };
    assert!(run_of(&mut ledger, &[&one, &two, &one]));
    // The same set, asked in another order and with repeats: no change.
    assert!(!run_of(&mut ledger, &[&two, &one, &two]));
}

#[test]
fn the_plain_disk_reads_the_file_system() {
    let dir = tempfile::TempDir::new().unwrap();
    fs::write(dir.path().join("a.md"), "a").unwrap();
    fs::create_dir(dir.path().join("sub")).unwrap();
    let file = dir.path().join("a.md");
    assert!(PlainDisk.exists(&file));
    assert!(PlainDisk.exists(&dir.path().join("sub")));
    assert_eq!(
        PlainDisk.list(dir.path()),
        Some(vec!["a.md".to_string(), "sub".to_string()])
    );
    assert_eq!(PlainDisk.read(&file).unwrap(), b"a".to_vec());
    let missing = PlainDisk.read(&dir.path().join("none.md")).unwrap_err();
    assert_eq!(missing.kind(), std::io::ErrorKind::NotFound);
}
