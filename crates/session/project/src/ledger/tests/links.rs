//! A stamp records the link chain it followed (behavior `invalidate_changed_files`; ADR 0129).
#![cfg(unix)]

use std::fs;
use std::time::{Duration, SystemTime};

use specforge_registry::{Disk, PlainDisk};

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

    let (plain_entry, link_entry) = (
        Entry::stamp(&plain, &PlainDisk),
        Entry::stamp(&link, &PlainDisk),
    );
    assert!(plain_entry.is_some());
    assert_ne!(plain_entry, link_entry);
    assert_eq!(link_entry, Entry::stamp(&link, &PlainDisk));
}

/// A disk that refuses every read, as a bounded one refuses a source outside its bound.
struct Refusing;

impl Disk for Refusing {
    fn exists(&self, _: &std::path::Path) -> bool {
        false
    }

    fn list(&self, _: &std::path::Path) -> Option<Vec<String>> {
        None
    }

    fn read(&self, _: &std::path::Path) -> std::io::Result<Vec<u8>> {
        Err(std::io::ErrorKind::PermissionDenied.into())
    }
}

#[test]
fn a_racy_stamp_hashes_through_the_disk_it_is_handed_not_below_it() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("f");
    fs::write(&file, "one").unwrap();
    // Just written: racy, so the stamp hashes the content.
    let entry = Entry::stamp(&file, &Refusing).unwrap();
    let modified = fs::metadata(&file).unwrap().modified().unwrap();
    fs::write(&file, "two").unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(modified)
        .unwrap();

    // The refused read hashes as the refusal, so the bytes below the bound are never read.
    assert_eq!(Some(entry), Entry::stamp(&file, &Refusing));
    assert_ne!(
        Entry::stamp(&file, &PlainDisk),
        Entry::stamp(&file, &Refusing)
    );
}
