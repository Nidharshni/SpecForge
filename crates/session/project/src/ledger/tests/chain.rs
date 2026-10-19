//! A name read through its links: the hops, and what a stamp records of them (ADR 0129, ADR 0173).
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use std::time::{Duration, SystemTime};

use specforge_registry::PlainDisk;

use crate::ledger::chain::{followed, hops};
use crate::ledger::key::in_canonical_dir;
use crate::ledger::stamps::Entry;

fn base() -> (tempfile::TempDir, PathBuf) {
    let t = tempfile::TempDir::new().unwrap();
    let dir = fs::canonicalize(t.path()).unwrap();
    (t, dir)
}

#[test]
fn a_plain_file_and_a_missing_path_have_no_hops() {
    let (_t, dir) = base();
    fs::write(dir.join("plain"), "x").unwrap();
    assert!(hops(&dir.join("plain")).is_empty());
    assert!(hops(&dir.join("missing")).is_empty());
    assert!(followed(&dir.join("missing")).is_none());
}

#[test]
fn a_relative_link_is_joined_onto_its_own_directory() {
    let (_t, dir) = base();
    symlink("sub/b", dir.join("a")).unwrap();
    assert_eq!(hops(&dir.join("a")), [dir.join("sub/b")]);
}

#[test]
fn an_absolute_link_replaces_the_path() {
    let (_t, dir) = base();
    symlink("/abs/b", dir.join("a")).unwrap();
    assert_eq!(hops(&dir.join("a")), [Path::new("/abs/b")]);
}

#[test]
fn a_dangling_link_still_names_its_target() {
    let (_t, dir) = base();
    symlink("gone", dir.join("a")).unwrap();
    assert_eq!(hops(&dir.join("a")), [dir.join("gone")]);
    assert!(followed(&dir.join("a")).is_none());
}

#[test]
fn a_loop_stops_after_forty_hops() {
    let (_t, dir) = base();
    symlink("b", dir.join("a")).unwrap();
    symlink("a", dir.join("b")).unwrap();
    assert_eq!(hops(&dir.join("a")).len(), 40);
}

#[test]
fn a_dotdot_target_is_left_to_the_file_system() {
    let (_t, dir) = base();
    fs::create_dir_all(dir.join("out/v1")).unwrap();
    fs::write(dir.join("out/c"), "real").unwrap();
    symlink("../c", dir.join("out/v1/b")).unwrap();
    symlink(dir.join("out/v1"), dir.join("alias")).unwrap();

    let hops = hops(&dir.join("alias/b"));
    assert_eq!(hops, [dir.join("alias/../c")]);
    assert_eq!(fs::read_to_string(&hops[0]).unwrap(), "real");
}

#[test]
fn followed_gives_the_metadata_of_the_file_reached() {
    let (_t, dir) = base();
    fs::write(dir.join("real"), "four").unwrap();
    symlink(dir.join("real"), dir.join("link")).unwrap();
    let (metadata, chain) = followed(&dir.join("link")).unwrap();
    assert_eq!(
        metadata.len(),
        fs::metadata(dir.join("real")).unwrap().len()
    );
    let landings: Vec<_> = hops(&dir.join("link"))
        .iter()
        .map(|hop| in_canonical_dir(hop))
        .collect();
    assert_eq!(chain, landings);
}

#[test]
fn a_stamp_moves_when_a_directory_link_on_the_way_to_a_hop_moves() {
    let (_t, dir) = base();
    let time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
    for v in ["v1", "v2"] {
        fs::create_dir(dir.join(v)).unwrap();
        fs::write(dir.join(v).join("b"), "same").unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(dir.join(v).join("b"))
            .unwrap()
            .set_modified(time)
            .unwrap();
    }
    symlink("alias/b", dir.join("link")).unwrap();
    symlink(dir.join("v1"), dir.join("alias")).unwrap();

    let before = Entry::stamp(&dir.join("link"), &PlainDisk);
    assert!(before.is_some());
    fs::remove_file(dir.join("alias")).unwrap();
    symlink(dir.join("v2"), dir.join("alias")).unwrap();
    assert_ne!(before, Entry::stamp(&dir.join("link"), &PlainDisk));
}
