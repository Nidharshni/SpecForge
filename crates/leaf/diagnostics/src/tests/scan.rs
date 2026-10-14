use super::is_third_party;
use specforge_repo_root::repo_root;
use std::path::{Path, PathBuf};

/// Replace test-only items (`#[cfg(test)]` / `#[test]` and the item that
/// follows, brace-matched) with blank lines so line numbers survive.
fn strip_test_items(src: &str) -> Vec<&str> {
    let lines: Vec<&str> = src.lines().collect();
    let mut out = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.starts_with("#[cfg(test)]") || trimmed == "#[test]" {
            let mut j = i + 1;
            let mut depth: i64 = 0;
            let mut started = false;
            while j < lines.len() {
                for ch in lines[j].chars() {
                    match ch {
                        '{' => {
                            depth += 1;
                            started = true;
                        }
                        '}' => depth -= 1,
                        _ => {}
                    }
                }
                if (started && depth <= 0) || (!started && lines[j].trim_end().ends_with(';')) {
                    break;
                }
                j += 1;
            }
            let end = j.min(lines.len() - 1);
            out.extend(std::iter::repeat_n("", end - i + 1));
            i = end + 1;
            continue;
        }
        out.push(lines[i]);
        i += 1;
    }
    out
}

/// The end of a code-shaped token starting at `b[i]`: one uppercase
/// letter, optional `-AREA` segments, then three digits (`E001`, `R013`,
/// `R-RES-005`, `E-PUB-001`), not followed by another word character.
/// Any such family is found, so an emitter can't hide a code behind a
/// new shape; the catalog decides which families are allowed.
fn code_shape_end(b: &[u8], i: usize) -> Option<usize> {
    if !b.get(i)?.is_ascii_uppercase() {
        return None;
    }
    let mut j = i + 1;
    let mut segments = 0;
    while b.get(j) == Some(&b'-') && b.get(j + 1).is_some_and(u8::is_ascii_uppercase) {
        j += 1;
        while b.get(j).is_some_and(u8::is_ascii_uppercase) {
            j += 1;
        }
        segments += 1;
    }
    if segments > 0 {
        if b.get(j) != Some(&b'-') {
            return None;
        }
        j += 1;
    }
    if !(j..j + 3).all(|k| b.get(k).is_some_and(u8::is_ascii_digit)) {
        return None;
    }
    let end = j + 3;
    let word = |c: &u8| c.is_ascii_alphanumeric() || *c == b'_';
    (!b.get(end).is_some_and(word)).then_some(end)
}

/// Find diagnostic codes in string literals on a line, each with the
/// byte range around it: a whole literal (`"E028"`, `"R-RES-005"`), a
/// code printed in brackets after a severity (`"error[E048]: ..."`), or
/// a literal that starts with the code (`"W097: ..."`). Single-letter
/// families are limited to E, W, I, A, R and F, so strings like
/// `"X509"` aren't codes.
pub(super) fn code_literals(line: &str) -> Vec<(&str, std::ops::Range<usize>)> {
    let b = line.as_bytes();
    let mut found = Vec::new();
    for i in 1..b.len() {
        let Some(end) = code_shape_end(b, i) else {
            continue;
        };
        let code = &line[i..end];
        if code.len() == 4 && !matches!(b[i], b'E' | b'W' | b'I' | b'A' | b'R' | b'F') {
            continue;
        }
        let after = b.get(end).copied();
        let quoted = b[i - 1] == b'"' && matches!(after, Some(b'"') | Some(b':'));
        let bracketed = b[i - 1] == b'['
            && after == Some(b']')
            && ["error", "warning", "info"]
                .iter()
                .any(|w| line[..i - 1].ends_with(w));
        if quoted || bracketed {
            found.push((code, i - 1..end + 1));
        }
    }
    found
}

pub(super) fn walk(dir: &Path, files: &mut Vec<PathBuf>) {
    const SKIP: &[&str] = &[
        "tests",
        "target",
        "node_modules",
        ".git",
        "benches",
        "examples",
    ];
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if !SKIP.contains(&name.as_str()) {
                walk(&path, files);
            }
        } else {
            files.push(path);
        }
    }
}

/// Rust production source under `top`: (path from the workspace root,
/// the file's lines with test items blanked). Files named `tests.rs` or
/// `*_tests.rs` are test modules and are left out.
fn production_source(top: &str) -> Vec<(String, Vec<String>)> {
    let root = repo_root();
    let mut files = Vec::new();
    walk(&root.join(top), &mut files);
    files.sort();
    let mut out = Vec::new();
    for path in files {
        let rel = path.strip_prefix(root).unwrap_or(&path);
        let rel = rel.to_string_lossy().replace('\\', "/");
        let name = rel.rsplit('/').next().unwrap_or("");
        if !name.ends_with(".rs")
            || !rel.split('/').any(|part| part == "src")
            || name == "tests.rs"
            || name.ends_with("_tests.rs")
        {
            continue;
        }
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        let lines = strip_test_items(&src)
            .into_iter()
            .map(str::to_string)
            .collect();
        out.push((rel, lines));
    }
    out
}

/// The catalog itself: the table (every code) and `lib.rs` (the
/// retired codes), so the scanners skip them.
const CATALOG_FILES: &[&str] = &[
    "crates/leaf/diagnostics/src/catalog.rs",
    "crates/leaf/diagnostics/src/lib.rs",
];

/// The host's production source: the compiler, the CLI, the servers, the
/// build tooling. It names a code only through `codes::*`. A guest's
/// source ([`guest_owners`]) is read as one.
pub(super) fn host_source() -> Vec<(String, Vec<String>)> {
    let owners = guest_owners();
    let mut files = Vec::new();
    for top in ["crates", "xtask", "integrations"] {
        files.extend(production_source(top));
    }
    // A stale CATALOG_FILES would scan the catalog as an emitter, and
    // every entry would look emitted: each skip must match one file.
    let mut catalog_skipped = Vec::new();
    files.retain(|(rel, _)| {
        if CATALOG_FILES.contains(&rel.as_str()) {
            catalog_skipped.push(rel.clone());
            return false;
        }
        !owners.iter().any(|(dir, _)| rel.starts_with(dir.as_str()))
    });
    catalog_skipped.sort();
    assert_eq!(
        catalog_skipped, CATALOG_FILES,
        "the scanners must skip exactly the catalog files"
    );
    files
}

/// Where an extension's code is written as text, and whose it is. A
/// guest has no host constants (it does not link this crate: the
/// builtins' blobs would change with every explanation), so its codes
/// are string literals, attributed by directory and nothing else. The
/// coverage crate is shared source of the testing extension (ADR 0004
/// D2-f).
fn guest_owners() -> Vec<(String, String)> {
    let mut owners = vec![(
        "crates/leaf/coverage/src".to_string(),
        "@specforge/testing".to_string(),
    )];
    let mut extensions: Vec<String> = std::fs::read_dir(repo_root().join("extensions"))
        .expect("extensions/")
        .flatten()
        .filter(|entry| entry.path().join("src").is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    extensions.sort();
    owners.extend(extensions.into_iter().map(|name| {
        (
            format!("extensions/{name}/src"),
            format!("@specforge/{name}"),
        )
    }));
    owners.sort();
    owners
}

/// One code literal an extension's source writes.
pub(super) struct GuestSite {
    pub(super) code: String,
    pub(super) owner: String,
    pub(super) location: String,
    /// The literal is compared against (`d.code == "E059"`, a `matches!`
    /// pattern), so it doesn't keep a catalog entry alive.
    pub(super) consumer: bool,
}

/// Every code literal in the guests' production source, with its
/// directory's owner. The third-party ranges are never catalogued.
pub(super) fn guest_sites() -> Vec<GuestSite> {
    let mut sites = Vec::new();
    for (dir, owner) in guest_owners() {
        for (rel, lines) in production_source(&dir) {
            for (index, line) in lines.iter().enumerate() {
                for (code, range) in code_literals(line) {
                    if is_third_party(code) {
                        continue;
                    }
                    sites.push(GuestSite {
                        code: code.to_string(),
                        owner: owner.clone(),
                        location: format!("{rel}:{}", index + 1),
                        consumer: is_literal_consumer(line, &range),
                    });
                }
            }
        }
    }
    sites
}

/// Whether the code literal on `line` at `range` is only compared
/// against rather than reported: an operand of `==`/`!=`, a match or
/// `matches!` pattern (`"E003" | "E025" =>`). A guest's `c.rule("W041", ..)`
/// or `PassDiagnostic::new("A001", ..)` reports it.
pub(super) fn is_literal_consumer(line: &str, range: &std::ops::Range<usize>) -> bool {
    let before = line[..range.start].trim_end();
    let after = line[range.end..].trim_start();
    let alternative = |s: &str| s.starts_with('|') && !s.starts_with("||");
    before.ends_with("==")
        || before.ends_with("!=")
        || after.starts_with("==")
        || after.starts_with("!=")
        || (before.ends_with('|') && !before.ends_with("||"))
        || alternative(after)
        || (after.starts_with("=>") && !before.ends_with("=>"))
        || line.contains("matches!(")
}

/// The `codes::X` references on a line: the constant's name and whether
/// the reference only looks for the code (`d.is(codes::E001)`,
/// `codes::E003.matches(..)`, an operand of `==`) rather than reports
/// it. A reference inside a table of codes to look for (`[Code; N]`,
/// `&[(Code, _)]`) is a consumer too: `in_table` says the line is
/// inside one.
pub(super) fn code_references(line: &str, in_table: bool) -> Vec<(String, bool)> {
    let b = line.as_bytes();
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = line[from..].find("codes::") {
        let start = from + at;
        from = start + "codes::".len();
        if start > 0 && word(b[start - 1]) {
            continue;
        }
        let end = from
            + line[from..]
                .bytes()
                .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == b'_')
                .count();
        if end == from {
            continue;
        }
        let before = line[..start].trim_end();
        let after = line[end..].trim_start();
        let consumer = in_table
            || before.ends_with(".is(")
            || before.ends_with("==")
            || before.ends_with("!=")
            || after.starts_with("==")
            || after.starts_with("!=")
            || after.starts_with(".matches(");
        found.push((line[from..end].to_string(), consumer));
    }
    found
}

/// Whether `line` opens a table of codes to look for (it ends at the
/// line that closes the array with `];`).
pub(super) fn opens_table(line: &str) -> bool {
    line.contains(": [Code;") || line.contains("&[(Code,")
}

/// Every host reference to a code constant, with where it is and
/// whether it is a consumer. The constant's name is the code with `-`
/// spelled `_`.
pub(super) fn host_references() -> Vec<(String, String, bool)> {
    let mut references = Vec::new();
    for (rel, lines) in host_source() {
        let mut in_table = false;
        for (index, line) in lines.iter().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            in_table |= opens_table(line);
            for (ident, consumer) in code_references(line, in_table) {
                references.push((
                    ident.replace('_', "-"),
                    format!("{rel}:{}", index + 1),
                    consumer,
                ));
            }
            if in_table && line.trim_end().ends_with("];") {
                in_table = false;
            }
        }
    }
    references
}

/// Every code-shaped word in `text` (see [`code_shape_end`]; single-letter
/// families E, W, I, A, R and F), with its line number.
pub(super) fn cited_codes(text: &str) -> Vec<(usize, &str)> {
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'-';
    let mut found = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let b = line.as_bytes();
        for i in 0..b.len() {
            if i > 0 && word(b[i - 1]) {
                continue;
            }
            let Some(end) = code_shape_end(b, i) else {
                continue;
            };
            let single = end - i == 4;
            if !single || matches!(b[i], b'E' | b'W' | b'I' | b'A' | b'R' | b'F') {
                found.push((index + 1, &line[i..end]));
            }
        }
    }
    found
}
