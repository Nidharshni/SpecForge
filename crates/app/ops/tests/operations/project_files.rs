//! Every project file an operation opens goes through a held name (ADR 0112 D6). A request field
//! makes a forgotten bound fail to compile; this makes a forgotten *open* fail a test: no
//! `std::fs` open in `src/` outside `containment/file.rs` but the listed ones, each with its
//! reason, and a listed one that no longer matches fails too.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The calls that open, create, write, rename, remove or copy a file.
const NEEDLES: &[&str] = &[
    "fs::write(",
    "fs::read(",
    "fs::read_to_string(",
    "fs::rename(",
    "fs::remove_file(",
    "fs::remove_dir_all(",
    "fs::create_dir_all(",
    "fs::copy(",
    "File::create(",
    "File::open(",
    "OpenOptions::new(",
];

/// `(file under src/, needle, how many lines, why it is not a project file held by a containment)`.
const ALLOWED: &[(&str, &str, usize, &str)] = &[
    (
        "analysis/collect/consent.rs",
        "fs::read_to_string(",
        1,
        "the user's consent store, not a project file",
    ),
    (
        "analysis/collect/consent.rs",
        "fs::create_dir_all(",
        1,
        "the user's consent store, not a project file",
    ),
    (
        "analysis/collect/consent.rs",
        "fs::write(",
        1,
        "the user's consent store, not a project file",
    ),
    (
        "solver.rs",
        "fs::write(",
        1,
        "the solver's script, in the system temp directory",
    ),
    (
        "solver.rs",
        "fs::remove_file(",
        1,
        "the solver's script, in the system temp directory",
    ),
    (
        "schema_version/mod.rs",
        "fs::read_to_string(",
        1,
        "the export's schema record: written by `specforge export` only (the CLI is unbounded)",
    ),
    (
        "schema_version/mod.rs",
        "fs::create_dir_all(",
        1,
        "the export's schema record: written by `specforge export` only",
    ),
    (
        "schema_version/mod.rs",
        "fs::write(",
        1,
        "the export's schema record: written by `specforge export` only",
    ),
    (
        "schema_version/mod.rs",
        "fs::rename(",
        1,
        "the export's schema record: written by `specforge export` only",
    ),
    (
        "extension/publish.rs",
        "fs::read(",
        1,
        "an author's built binary, `specforge publish` only",
    ),
    (
        "extension/candidate.rs",
        "fs::read_to_string(",
        1,
        "a held crate directory's Cargo.toml: only its name is read, and the binary it names is held",
    ),
    (
        "read/infer/lint.rs",
        "fs::read_to_string(",
        1,
        "an inference source file: routed by plan 02 of round 12",
    ),
    (
        "read/infer/scan/mod.rs",
        "fs::read_to_string(",
        1,
        "an inference source file: routed by plan 02 of round 12",
    ),
    (
        "read/infer/manifest.rs",
        "fs::read(",
        1,
        "an inference source file's hash (`compute_content_hash`): routed by plan 02 of round 12",
    ),
];

/// Files and directories that are test support, or the module itself.
fn exempt(file: &str) -> bool {
    file == "containment/file.rs"
        || file.ends_with("tests.rs")
        || file
            .split('/')
            .any(|part| part == "tests" || part == "testing")
        || file == "testing.rs"
        || file == "extension/fixtures.rs"
}

/// The needles a non-comment line of `source` (up to its first `#[cfg(test)]`) contains.
fn opens(source: &str) -> Vec<(usize, &'static str)> {
    let mut found = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        // The test module: `#[cfg(test)]` at the left margin, then a `mod`.
        if line.starts_with("#[cfg(test)]")
            && lines
                .get(index + 1)
                .is_some_and(|next| next.contains("mod "))
        {
            break;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        for needle in NEEDLES {
            if line.contains(needle) {
                found.push((index + 1, *needle));
            }
        }
    }
    found
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}

/// What the check refuses in `files` (file, source): a line not on the list, or a listed entry
/// that no longer matches.
fn unlisted(files: &[(String, String)], allowed: &[(&str, &str, usize, &str)]) -> Vec<String> {
    let mut counts: BTreeMap<(String, &str), usize> = BTreeMap::new();
    let mut problems = Vec::new();
    let mut lines: BTreeMap<(String, &str), Vec<usize>> = BTreeMap::new();
    for (file, source) in files.iter().filter(|(file, _)| !exempt(file)) {
        for (line, needle) in opens(source) {
            *counts.entry((file.clone(), needle)).or_default() += 1;
            lines.entry((file.clone(), needle)).or_default().push(line);
        }
    }
    for ((file, needle), found) in &counts {
        let listed = allowed
            .iter()
            .find(|(f, n, ..)| f == file && n == needle)
            .map_or(0, |(_, _, count, _)| *count);
        if *found != listed {
            problems.push(format!(
                "src/{file}: {found} `{needle}` (lines {:?}), {listed} listed: open it through a Held (ADR 0112), or list it in ALLOWED with its reason",
                lines[&(file.clone(), *needle)]
            ));
        }
    }
    for (file, needle, count, reason) in allowed {
        if *count > 0 && !counts.contains_key(&((*file).to_string(), *needle)) {
            problems.push(format!(
                "ALLOWED lists src/{file} `{needle}` ({reason}), which no longer matches: delete the entry"
            ));
        }
    }
    problems
}

#[test]
fn every_file_an_operation_opens_is_held() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut paths = Vec::new();
    rust_files(&src, &mut paths);
    paths.sort();
    let files: Vec<(String, String)> = paths
        .iter()
        .map(|path| {
            let name = path
                .strip_prefix(&src)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            (name, std::fs::read_to_string(path).unwrap())
        })
        .collect();
    let problems = unlisted(&files, ALLOWED);
    assert!(
        problems.is_empty(),
        "{} opens bypass the held-file module:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

#[test]
fn a_new_open_outside_the_module_fails_the_check() {
    let files = vec![(
        "read/new.rs".to_string(),
        "fn f(root: &Path) { std::fs::write(root.join(\"x\"), b\"\").unwrap(); }\n".to_string(),
    )];
    let problems = unlisted(&files, &[]);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("src/read/new.rs"), "{problems:?}");

    // Listed with its count, it passes; a stale entry fails.
    let listed = [("read/new.rs", "fs::write(", 1, "test")];
    assert!(unlisted(&files, &listed).is_empty());
    let stale = [("read/gone.rs", "fs::write(", 1, "test")];
    assert_eq!(unlisted(&files, &stale).len(), 2);

    // Comments, and everything after the test module, are not opens.
    let quiet = vec![(
        "read/quiet.rs".to_string(),
        "// std::fs::write(a)\n#[cfg(test)]\nmod t { fn f() { std::fs::write(a); } }\n".to_string(),
    )];
    assert!(unlisted(&quiet, &[]).is_empty());
}
