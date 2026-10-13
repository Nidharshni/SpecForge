//! What a command left on disk, compared with the files its JSON output
//! says it wrote (`files_written`, ADR 0022).

/// Every file under `root` with its bytes.
pub(crate) fn files_under(
    root: &std::path::Path,
) -> std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
    let mut files = std::collections::BTreeMap::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for path in entries.map(|entry| entry.unwrap().path()) {
            if path.is_dir() {
                dirs.push(path);
            } else {
                let bytes = std::fs::read(&path).unwrap();
                files.insert(path, bytes);
            }
        }
    }
    files
}

/// The files under `root` whose bytes differ from `before`, added and
/// removed ones included, relative to `root`, sorted.
pub(crate) fn changed_since(
    root: &std::path::Path,
    before: &std::collections::BTreeMap<std::path::PathBuf, Vec<u8>>,
) -> Vec<String> {
    let after = files_under(root);
    let mut changed: Vec<String> = before
        .keys()
        .chain(after.keys())
        .filter(|path| before.get(*path) != after.get(*path))
        .map(|path| path.strip_prefix(root).unwrap().display().to_string())
        .collect();
    changed.sort();
    changed.dedup();
    changed
}

/// A JSON output's `files_written`.
pub(crate) fn files_written(json: &serde_json::Value) -> Vec<String> {
    json["files_written"]
        .as_array()
        .unwrap_or_else(|| panic!("no files_written in {json}"))
        .iter()
        .map(|f| f.as_str().unwrap().to_string())
        .collect()
}
