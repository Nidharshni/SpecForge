//! `clean-target`: free the disk a long session fills. Cargo never deletes the rlibs, test binaries and
//! dep-info a changed hash replaced, so `<target>/<profile>/deps` keeps every build of every crate. This keeps
//! the newest file of each (stem, extension) pair, removes `incremental/` (off in `.cargo/config.toml`, so only
//! stale ones exist) and `blob-stage/` (rebuilt by `revendor`), and reports what it freed. `--dry-run` only reports.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::SystemTime;

use gate::Repo;

fn main() {
    let dry = std::env::args().any(|a| a == "--dry-run");
    let repo = Repo::discover().unwrap_or_else(|e| {
        eprintln!("clean-target: {e}");
        std::process::exit(2);
    });
    let target = &repo.target_dir;
    let mut freed = 0u64;
    for profile in fs::read_dir(target).into_iter().flatten().flatten() {
        freed += dedupe(&profile.path().join("deps"), dry);
        freed += remove(&profile.path().join("incremental"), dry);
    }
    freed += remove(&target.join("blob-stage"), dry);
    let verb = if dry { "would free" } else { "freed" };
    eprintln!("clean-target: {verb} {:.1} GB", freed as f64 / 1e9);
}

/// `name-<16 hex>.ext` splits into (`name`, `.ext`); a file with no hash is none of ours to dedupe.
fn key(name: &str) -> Option<(String, String)> {
    let dot = name.find('.').unwrap_or(name.len());
    let (stem, ext) = name.split_at(dot);
    let (base, hash) = stem.rsplit_once('-')?;
    (hash.len() == 16 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| (base.to_string(), ext.to_string()))
}

/// One file of a group: when it was written, where it is, and how big.
type Built = (SystemTime, std::path::PathBuf, u64);

fn dedupe(deps: &Path, dry: bool) -> u64 {
    let mut groups: HashMap<(String, String), Vec<Built>> = HashMap::new();
    for entry in fs::read_dir(deps).into_iter().flatten().flatten() {
        let (Ok(meta), Some(k)) = (entry.metadata(), key(&entry.file_name().to_string_lossy()))
        else {
            continue;
        };
        if meta.is_file() {
            groups.entry(k).or_default().push((
                meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                entry.path(),
                meta.len(),
            ));
        }
    }
    let mut freed = 0;
    for mut files in groups.into_values() {
        files.sort();
        files.pop();
        for (_, path, len) in files {
            if dry || fs::remove_file(&path).is_ok() {
                freed += len;
            }
        }
    }
    freed
}

fn size(path: &Path) -> u64 {
    fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .fold(0, |sum, e| {
            let p = e.path();
            sum + if p.is_dir() {
                size(&p)
            } else {
                e.metadata().map_or(0, |m| m.len())
            }
        })
}

fn remove(path: &Path, dry: bool) -> u64 {
    let len = size(path);
    if !dry {
        let _ = fs::remove_dir_all(path);
    }
    len
}
