//! A stamp records the link chain it followed (behavior `invalidate_changed_files`; ADR 0129).
#![cfg(unix)]

use std::fs;
use std::time::{Duration, SystemTime};

use crate::ledger::stamps::Entry;

fn same_time(paths: &[&std::path::Path]) {
    let time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
    for path in paths {
        let file = fs::OpenOptions::new().write(true).open(path).unwrap();
        file.set_modified(time).unwrap();
    }
}

#[test]
fn an_entry_for_a_link_differs_from_a_plain_file_with_the_same_len_and_mtime() {
    let dir = tempfile::TempDir::new().unwrap();
    let (plain, real, link) = (
        dir.path().join("plain"),
        dir.path().join("real"),
        dir.path().join("link"),
    );
    fs::write(&plain, "text").unwrap();
    fs::write(&real, "text").unwrap();
    same_time(&[&plain, &real]);
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let (plain_entry, link_entry) = (Entry::stamp(&plain), Entry::stamp(&link));
    assert!(plain_entry.is_some());
    assert_ne!(plain_entry, link_entry);
    assert_eq!(link_entry, Entry::stamp(&link));
}
