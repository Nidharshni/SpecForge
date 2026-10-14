//! Diagnostic code registry: the single canonical catalog of every code the
//! compiler, the CLI, and the first-party extensions emit.
//!
//! `specforge explain <CODE>` and the MCP `specforge.explain` tool print an
//! entry, MCP diagnostics and the LSP carry its title and docs link, doctor
//! quotes its explanation, and `docs/diagnostics.md` is generated from
//! [`CATALOG`]. The catalog is one table (`catalog.rs`) that generates both
//! [`CATALOG`] and a typed constant for every core code ([`codes`]): the
//! host names a core code as `codes::W112`, a [`Code`] whose level is the
//! catalogued one. The tests at the bottom of this file fail when an emitted
//! code is missing from the catalog, is attributed to the wrong owner, or
//! when the catalog lists a code nothing emits.

#[macro_use]
mod catalog;
mod code;
mod extension;

pub use catalog::{CATALOG, codes};
#[doc(hidden)]
pub use code::prefix_states;
pub use code::{Code, GradedCode};
pub use extension::{CodeMisuse, check_extension_code};

/// One diagnostic code and what it means.
#[derive(Debug, Clone, Copy)]
pub struct CodeEntry {
    /// `E###` (error), `W###` (warning), `I###` (info), `A###` (an
    /// `analyze` pass finding, whose severity the pass sets), or the
    /// registry client's `R###` / `R-<AREA>-###`.
    pub code: &'static str,
    /// Short human-readable name.
    pub title: &'static str,
    /// `"core"` for the compiler/CLI, or the emitting extension
    /// (`"@specforge/formal"`, ...).
    pub owner: &'static str,
    /// The severity a diagnostic of this code has when it is reported. An
    /// `E`/`W`/`I` prefix states it (the table does not compile otherwise);
    /// an `A###` finding's severity is set by its pass.
    pub level: Level,
    /// What triggers the diagnostic and how to fix it.
    pub explanation: &'static str,
}

/// A catalogued code's severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Level {
    Error,
    Warning,
    Info,
    /// `A###` analyze findings: the pass picks the severity per finding.
    SetByPass,
}

impl Level {
    /// How `specforge explain` and `docs/diagnostics.md` name the level.
    pub fn describe(self) -> &'static str {
        match self {
            Level::Error => "error",
            Level::Warning => "warning",
            Level::Info => "info",
            Level::SetByPass => "set by the analyze pass",
        }
    }
}

/// Width used when wrapping explanations for the terminal and the docs page.
pub const WRAP_WIDTH: usize = 80;

/// The page on the canonical repository (the workspace's Cargo
/// `repository`, ADR 0004 D6-b) that documents every catalogued code.
pub const DOCS_URL: &str = concat!(
    env!("CARGO_PKG_REPOSITORY"),
    "/blob/main/docs/diagnostics.md"
);

/// The docs link for `code`: its section of [`DOCS_URL`], or `None` when
/// the catalog has no entry for it (a third-party code, or anything
/// outside the catalog).
pub fn docs_href(code: &str) -> Option<String> {
    lookup(code).map(|entry| format!("{DOCS_URL}#{}", entry.code.to_lowercase()))
}

/// Look up a code (case-insensitive).
pub fn lookup(code: &str) -> Option<&'static CodeEntry> {
    let normalized = code.to_uppercase();
    CATALOG
        .binary_search_by(|entry| entry.code.cmp(normalized.as_str()))
        .ok()
        .map(|index| &CATALOG[index])
}

/// The catalog entry that describes a diagnostic of `code` reported by
/// `origin` (the extension that reported it; `None` for the host's own): the
/// entry when the host reported it, or when the entry's owner is that very
/// extension. A code an extension squats (W150) is not described by its
/// owner's entry: its title, explanation and docs link would be another
/// diagnostic's. Ownership only, not level: a warning `--strict` promoted to
/// an error keeps its description.
pub fn describes(code: &str, origin: Option<&str>) -> Option<&'static CodeEntry> {
    let entry = lookup(code)?;
    match origin {
        None => Some(entry),
        Some(extension) => (entry.owner == extension).then_some(entry),
    }
}

/// Greedy word wrap; never splits a word.
pub fn wrap(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut line_len = 0;
    for word in text.split_whitespace() {
        if line_len > 0 && line_len + 1 + word.len() > width {
            out.push('\n');
            line_len = 0;
        } else if line_len > 0 {
            out.push(' ');
            line_len += 1;
        }
        out.push_str(word);
        line_len += word.len();
    }
    out
}

/// Render `docs/diagnostics.md` from [`CATALOG`] (see the `explain_docs_sync` test).
pub fn render_docs() -> String {
    let mut out = String::from(DOCS_HEADER);
    for entry in CATALOG {
        out.push_str(&format!(
            "\n## {code}\n\n```\n{code}: {title}\n\n{body}\n\nOwner: {owner}\nLevel: {level}\n```\n",
            code = entry.code,
            title = entry.title,
            body = wrap(entry.explanation, WRAP_WIDTH),
            owner = entry.owner,
            level = entry.level.describe(),
        ));
    }
    out.push_str(
        "\n## Retired codes\n\nThese codes are no longer emitted, and are never reused for another \
         meaning.\n\n| Code | Replaced by |\n|------|-------------|\n",
    );
    for (old, new) in RETIRED {
        let new = match new {
            Some(code) => format!("[{code}](#{})", code.to_lowercase()),
            None => "(nothing)".to_string(),
        };
        out.push_str(&format!("| {old} | {new} |\n"));
    }
    out
}

const DOCS_HEADER: &str = "# SpecForge Diagnostic Codes

<!-- Generated file: do not edit by hand. -->

This page is generated from the catalog table in `crates/leaf/diagnostics/src/catalog.rs`,
the single registry of diagnostic codes; `specforge explain <CODE>` prints the
same text. Every code emitted by the compiler, the CLI, or a first-party
extension has exactly one entry, and each entry names its owner: `core` for the
compiler and CLI, or the `@specforge/<name>` extension that emits it. The
compiler and CLI name each code through a typed constant generated from this
table (`specforge_diagnostics::codes`), and a test fails when a code an
extension emits is missing here, belongs to another owner, or when a listed
code is never emitted.

Codes follow the pattern `E###` (error), `W###` (warning) and `I###` (info);
`A###` codes are `specforge analyze` findings, whose severity the pass sets.
The registry client keeps its own family, `R###` and `R-<AREA>-###`, whose
prefix doesn't state the severity; no other family is accepted. Each entry's
`Level` is the severity a diagnostic of that code has when it is reported;
`specforge check --strict` raises warnings to errors afterwards. A code an
extension reports at another level, or a code it does not own, is reported as
W150. The ranges
`E900`-`E998`, `W900`-`W998` and `I900`-`I998` are reserved for third-party
extensions and never appear in this catalog; `I999` is a core code.

Regenerate this page after editing the catalog table:

```sh
SPECFORGE_BLESS=1 cargo test -p specforge-diagnostics explain_docs_sync
```
";

/// Codes that are no longer emitted, with the code that replaced them (if
/// any). A retired code is never reused for another meaning.
pub const RETIRED: &[(&str, Option<&str>)] = &[
    ("E017", None),
    ("E018", None),
    ("E020", None),
    ("E023", None),
    ("E029", None),
    ("E035", None),
    ("E037", None),
    ("E038", None),
    ("E047", Some("W139")),
    ("E053", None),
    ("E060", None),
    ("I006", None),
    ("W011", None),
    ("W024", None),
    ("W025", None),
    ("W026", None),
    ("W028", None),
    ("W050", Some("E061")),
    ("W051", Some("E061")),
    ("W052", Some("E061")),
    ("W062", Some("E073")),
    ("W063", None),
    ("W077", Some("E061")),
    ("W078", Some("E061")),
    ("W079", Some("E061")),
    ("W080", Some("E061")),
    ("W083", Some("E061")),
    ("W084", Some("E061")),
    ("W085", Some("E061")),
    ("W095", Some("E061")),
    ("W099", None),
    ("W111", None),
    ("W114", None),
    ("W116", None),
    ("W117", None),
    ("W119", None),
    ("W120", None),
    ("W121", Some("E061")),
    ("W122", None),
    ("W125", Some("E061")),
    ("W146", None),
];

/// Look up a retired code (case-insensitive): `Some(replacement)`.
pub fn retired(code: &str) -> Option<Option<&'static str>> {
    let normalized = code.to_uppercase();
    RETIRED
        .iter()
        .find(|(old, _)| *old == normalized)
        .map(|(_, new)| *new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_repo_root::repo_root;
    use specforge_test_macros::test as specforge_test;
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::{Path, PathBuf};

    /// Third-party extensions own E900–E998 / W900–W998 / I900–I998.
    /// The registry client's code areas (`R-RES-005`), catalogued as they
    /// are (ADR 0004 D6-c). The grammar is frozen: no new area, no new family.
    const REGISTRY_AREAS: &[&str] = &["AUTH", "LOGIN", "OPS", "RES", "TRUST"];

    /// `E###`/`W###`/`I###`/`A###`, or the registry client's `R###` and
    /// `R-<AREA>-###`.
    fn is_allowed_shape(code: &str) -> bool {
        let digits = |s: &str| s.len() == 3 && s.bytes().all(|c| c.is_ascii_digit());
        if let Some(rest) = code.strip_prefix("R-") {
            return rest
                .split_once('-')
                .is_some_and(|(area, n)| REGISTRY_AREAS.contains(&area) && digits(n));
        }
        code.len() == 4
            && matches!(code.as_bytes()[0], b'E' | b'W' | b'I' | b'A' | b'R')
            && digits(&code[1..])
    }

    fn is_third_party(code: &str) -> bool {
        let b = code.as_bytes();
        if b.len() != 4 || !matches!(b[0], b'E' | b'W' | b'I') {
            return false;
        }
        let n: u32 = code[1..].parse().unwrap_or(0);
        (900..=998).contains(&n)
    }

    /// C4-10: docs/diagnostics.md is generated from [`CATALOG`] and linked
    /// from LSP `codeDescription.href`. Set `SPECFORGE_BLESS=1` to rewrite it.
    #[test]
    fn explain_docs_sync() {
        let path = repo_root().join("docs/diagnostics.md");
        let rendered = render_docs();
        if std::env::var("SPECFORGE_BLESS").as_deref() == Ok("1") {
            std::fs::write(&path, &rendered).expect("write docs/diagnostics.md");
            return;
        }
        let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            on_disk == rendered,
            "docs/diagnostics.md is out of date with the explain catalog; regenerate it with \
             `SPECFORGE_BLESS=1 cargo test -p specforge-diagnostics explain_docs_sync`"
        );
    }

    #[test]
    fn catalog_is_sorted_unique_and_well_formed() {
        let mut problems = Vec::new();
        for pair in CATALOG.windows(2) {
            if pair[0].code >= pair[1].code {
                problems.push(format!(
                    "{} must come before {} (CATALOG is sorted by code, without duplicates)",
                    pair[1].code, pair[0].code
                ));
            }
        }
        for entry in CATALOG {
            if !is_allowed_shape(entry.code) {
                problems.push(format!(
                    "{}: not of the form E###/W###/I###/A###; R### and R-<AREA>-### (AREA one \
                     of {}) are kept only for the registry client, so a new code takes the \
                     next free E/W/I/A number",
                    entry.code,
                    REGISTRY_AREAS.join(", ")
                ));
            } else if is_third_party(entry.code) {
                problems.push(format!(
                    "{}: the 900-998 range is reserved for third-party extensions",
                    entry.code
                ));
            }
            if entry.title.is_empty() || entry.explanation.is_empty() {
                problems.push(format!("{}: empty title or explanation", entry.code));
            }
            if entry.owner != "core" && !entry.owner.starts_with("@specforge/") {
                problems.push(format!(
                    "{}: owner `{}` must be `core` or `@specforge/<name>`",
                    entry.code, entry.owner
                ));
            }
        }
        for (old, new) in RETIRED {
            if lookup(old).is_some() {
                problems.push(format!("{old} is retired, so it can't be in CATALOG"));
            }
            if new.is_some_and(|code| lookup(code).is_none()) {
                problems.push(format!(
                    "{old} is retired to {new:?}, which isn't in CATALOG"
                ));
            }
        }
        assert!(
            problems.is_empty(),
            "CATALOG problems:\n  {}",
            problems.join("\n  ")
        );
    }

    /// The catalog describes a diagnostic the host reported, or one its
    /// owner reported; ownership decides, not the level it was reported at.
    #[specforge_test(
        invariant = "diagnostic_code_uniqueness",
        verify = "an extension reports only its own catalogued codes, or third-party codes whose prefix states their level"
    )]
    fn describes_reads_ownership_not_level() {
        // The host's own: described.
        assert_eq!(
            describes("E001", None).map(|e| e.title),
            Some("Parse error")
        );
        // A code the extension owns, whatever level `--strict` gave it.
        assert_eq!(
            describes("W004", Some("@specforge/testing")).map(|e| e.code),
            Some("W004")
        );
        // A code another owner has: not described.
        assert!(describes("E001", Some("@acme/squat")).is_none());
        assert!(describes("W004", Some("@acme/squat")).is_none());
        assert!(describes("E001", Some("@specforge/testing")).is_none());
        // A code the catalog does not have: nothing to describe.
        assert!(describes("W950", Some("@acme/x")).is_none());
        assert!(describes("W950", None).is_none());
    }

    #[test]
    fn lookup_is_case_insensitive() {
        assert_eq!(lookup("e001").map(|e| e.code), Some("E001"));
        assert!(lookup("E900").is_none());
    }

    /// The table builds each constant and its catalog entry from the same
    /// tokens, so one constant of every shape stands for all of them.
    #[specforge_test(
        invariant = "diagnostic_code_uniqueness",
        verify = "a core code's constant carries its catalogued level and owner"
    )]
    fn code_constants_are_their_catalog_entries() {
        for (code, level) in [
            (codes::W112, Level::Warning),
            (codes::E001, Level::Error),
            (codes::I999, Level::Info),
            (codes::R003, Level::Warning),
            (codes::R_RES_005, Level::Error),
        ] {
            let entry = lookup(code.id()).expect("a constant names a catalogued code");
            assert_eq!(entry.code, code.id());
            assert_eq!(entry.owner, "core", "{code}");
            assert_eq!(entry.level, code.level(), "{code}");
            assert_eq!(code.level(), level, "{code}");
        }
        assert_eq!(codes::R_RES_005.id(), "R-RES-005");
        let graded = lookup(codes::A010.id()).expect("A010 is catalogued");
        assert_eq!(
            (graded.code, graded.owner, graded.level),
            ("A010", "core", Level::SetByPass)
        );
    }

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
    fn code_literals(line: &str) -> Vec<(&str, std::ops::Range<usize>)> {
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

    fn walk(dir: &Path, files: &mut Vec<PathBuf>) {
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

    /// The catalog itself: the table (every code) and this file (the
    /// retired codes), so the scanners skip them.
    const CATALOG_FILES: &[&str] = &[
        "crates/leaf/diagnostics/src/catalog.rs",
        "crates/leaf/diagnostics/src/lib.rs",
    ];

    /// The host's production source: the compiler, the CLI, the servers, the
    /// build tooling. It names a code only through `codes::*`. A guest's
    /// source ([`guest_owners`]) is read as one.
    fn host_source() -> Vec<(String, Vec<String>)> {
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
    struct GuestSite {
        code: String,
        owner: String,
        location: String,
        /// The literal is compared against (`d.code == "E059"`, a `matches!`
        /// pattern), so it doesn't keep a catalog entry alive.
        consumer: bool,
    }

    /// Every code literal in the guests' production source, with its
    /// directory's owner. The third-party ranges are never catalogued.
    fn guest_sites() -> Vec<GuestSite> {
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
    fn is_literal_consumer(line: &str, range: &std::ops::Range<usize>) -> bool {
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
    fn code_references(line: &str, in_table: bool) -> Vec<(String, bool)> {
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
    fn opens_table(line: &str) -> bool {
        line.contains(": [Code;") || line.contains("&[(Code,")
    }

    /// Every host reference to a code constant, with where it is and
    /// whether it is a consumer. The constant's name is the code with `-`
    /// spelled `_`.
    fn host_references() -> Vec<(String, String, bool)> {
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
    fn cited_codes(text: &str) -> Vec<(usize, &str)> {
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

    /// C3: hand-written docs cite only catalogued codes. `docs/diagnostics.md`
    /// is generated from the catalog, and ADRs are dated records that quote
    /// the codes of their time; the third-party ranges are never catalogued.
    #[test]
    fn hand_written_docs_cite_only_catalogued_codes() {
        let docs = repo_root().join("docs");
        let mut files = Vec::new();
        walk(&docs, &mut files);
        files.sort();
        let mut problems = Vec::new();
        for path in files {
            let rel = path.strip_prefix(&docs).unwrap_or(&path);
            let rel = rel.to_string_lossy().replace('\\', "/");
            if !rel.ends_with(".md") || rel == "diagnostics.md" || rel.starts_with("adr/") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            for (line, code) in cited_codes(&text) {
                if lookup(code).is_none() && !is_third_party(code) {
                    problems.push(format!("docs/{rel}:{line} cites {code}"));
                }
            }
        }
        assert!(
            problems.is_empty(),
            "docs cite codes the catalog doesn't have; use the catalogued code (see \
             docs/diagnostics.md) or drop the citation:\n  {}",
            problems.join("\n  ")
        );
    }

    /// The host names a core code only through its constant
    /// (`codes::W112`), so its severity is the catalog's and a wrong
    /// prefix/level pair does not compile. A code-shaped string literal in
    /// the host's production source (outside test items and comments) is
    /// that rule broken: `Code::catalogued("W999", ..)`, `"E003: ..."` in a
    /// message, `severity[E048]` in a format string. The third-party ranges
    /// (a scaffold's `W900`) are never catalogued and are not codes of
    /// the host's.
    #[test]
    fn host_source_names_codes_through_constants() {
        let files = host_source();
        assert!(
            files.len() >= 100,
            "scanned {} host files; is the workspace root right?",
            files.len()
        );
        let mut problems = Vec::new();
        for (rel, lines) in &files {
            for (index, line) in lines.iter().enumerate() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                for (code, _) in code_literals(line) {
                    if !is_third_party(code) {
                        problems.push(format!(
                            "{rel}:{} writes {code} as text; name it `codes::{}`",
                            index + 1,
                            code.replace('-', "_")
                        ));
                    }
                }
            }
        }
        assert!(
            problems.is_empty(),
            "host source writes diagnostic codes as text ({} sites):\n  {}",
            problems.len(),
            problems.join("\n  ")
        );
    }

    /// `Diagnostic::untyped` is the narrow door for a code that arrives as
    /// text: an extension's code a host crossing point did not build from a
    /// constant, an `OpError` turned back into a diagnostic. Only the files
    /// that convert such values call it, so a new call site is a decision
    /// this list shows.
    #[test]
    fn untyped_is_called_only_where_codes_cross() {
        const UNTYPED_FILES: &[&str] = &[
            // The definition (`new`, `graded` and `from_extension` build through it).
            "crates/kernel/common/src/diagnostic.rs",
            // The host's own E006 rules carry their code as text, like an extension's.
            "crates/pipeline/registry/src/rules/check.rs",
            // An MCP failure's code is text from an `OpError` or a message.
            "crates/surfaces/mcp/src/tool.rs",
        ];
        let mut calling: Vec<String> = host_source()
            .into_iter()
            .filter(|(_, lines)| {
                lines
                    .iter()
                    .any(|l| !l.trim_start().starts_with("//") && l.contains("untyped("))
            })
            .map(|(rel, _)| rel)
            .collect();
        calling.sort();
        assert_eq!(
            calling, UNTYPED_FILES,
            "`Diagnostic::untyped` is called from these files only (a code that arrives as \
             text); a core code is built with `Diagnostic::new(codes::X, ..)`"
        );
    }

    /// C2: a code that is only compared against isn't emitted, so a catalog
    /// entry can't outlive its last emitter through a consumer.
    #[test]
    fn consumer_references_do_not_count_as_emitting() {
        let consumers = [
            (r#"            if !d.is(codes::E001) {"#, false),
            (
                r#"        Err(e) if e.is(codes::E059) => err_invalid("#,
                false,
            ),
            (r#"        .filter(|d| !d.is(codes::E027))"#, false),
            (r#"        if codes::E003.matches(&diag.code) {"#, false),
            (r#"        if diag.code == codes::E003.id() {"#, false),
            (
                r#"pub const CONFLICT_CODES: [Code; 2] = [codes::E017, codes::W018];"#,
                false,
            ),
            (r#"    (codes::E003, ErrorCode::EntityNotFound),"#, true),
            (r#"            codes::E026,"#, true),
        ];
        for (line, in_table) in consumers {
            let found = code_references(line, in_table || opens_table(line));
            assert!(!found.is_empty(), "`{line}` has a reference");
            for (ident, consumer) in found {
                assert!(consumer, "{ident} in `{line}` is a consumer");
            }
        }
        let emitters = [
            r#"            Diagnostic::new(codes::W139, format!("claim"))"#,
            r#"        return Err(fail(codes::E034, "bad"))"#,
            r#"const BUDGET_TOO_SMALL: Code = codes::E062;"#,
            r#"    Diagnostic::graded(codes::A010, Severity::Info, "m")"#,
            r#"            Kind::Missing => codes::E025,"#,
            r#"            let code = specforge_common::codes::R_TRUST_002;"#,
        ];
        for line in emitters {
            let found = code_references(line, opens_table(line));
            assert!(!found.is_empty(), "`{line}` has a reference");
            for (ident, consumer) in found {
                assert!(!consumer, "{ident} in `{line}` is emitted");
            }
        }
        let guest_consumers = [
            r#"        Err(e) if e.code == "E059" => err_invalid("#,
            r#"            if d.code != "E001" {"#,
            r#"        if !matches!(diag.code.as_str(), "E003" | "E025") {"#,
            r#"            "E003" | "E025" => fix(diag),"#,
        ];
        for line in guest_consumers {
            for (code, range) in code_literals(line) {
                assert!(
                    is_literal_consumer(line, &range),
                    "{code} in `{line}` is a consumer"
                );
            }
        }
        let guest_emitters = [
            r#"        c.rule("W041", |r| {"#,
            r#"                code: "E028".to_string(),"#,
            r#"      "code": "W041","#,
            r#"            Kind::Missing => "E025","#,
        ];
        for line in guest_emitters {
            for (code, range) in code_literals(line) {
                assert!(
                    !is_literal_consumer(line, &range),
                    "{code} in `{line}` is emitted"
                );
            }
        }
    }

    /// The catalog is enforced: every code a guest reports is registered
    /// under its extension, every core code is built by the host, and every
    /// registered code is reported somewhere. The host's reports are its
    /// `codes::X` references (a constant exists only for a core code, so the
    /// owner needs no guess); the guests' are string literals, owned by
    /// their directory ([`guest_owners`]).
    #[specforge_test(
        invariant = "diagnostic_code_uniqueness",
        verify = "Diagnostic Code Uniqueness guarantee holds"
    )]
    fn explain_catalog_matches_emitted_codes() {
        let catalog: BTreeMap<&str, &CodeEntry> = CATALOG.iter().map(|e| (e.code, e)).collect();
        let host = host_references();
        let guests = guest_sites();
        assert!(
            host.len() >= 150,
            "scanner found {} host code references; is the workspace root right?",
            host.len()
        );
        assert!(
            guests.len() >= 90,
            "scanner found {} guest code literals; is the workspace root right?",
            guests.len()
        );

        let mut problems = Vec::new();
        let mut emitted = BTreeSet::new();
        for (code, location, consumer) in &host {
            match catalog.get(code.as_str()) {
                None => problems.push(format!(
                    "{code} referenced at {location} is not in the CATALOG"
                )),
                // A consumer looks for a code some other site reports.
                Some(_) if *consumer => {}
                Some(entry) => {
                    if entry.owner != "core" {
                        problems.push(format!(
                            "{code} built at {location} belongs to `{}`, not core",
                            entry.owner
                        ));
                    }
                    emitted.insert(code.as_str());
                }
            }
        }
        for site in &guests {
            match catalog.get(site.code.as_str()) {
                None => problems.push(format!(
                    "{} reported at {} is not in the CATALOG; add a table entry with owner `{}`",
                    site.code, site.location, site.owner
                )),
                // A consumer may test for another owner's code, but the code
                // it looks for must exist.
                Some(_) if site.consumer => {}
                Some(entry) if entry.owner != site.owner => problems.push(format!(
                    "{} reported at {} belongs to `{}`, but the CATALOG says owner `{}`; each code \
                     has one owner, so pick a free code for this extension or fix the entry",
                    site.code, site.location, site.owner, entry.owner
                )),
                Some(_) => {
                    emitted.insert(site.code.as_str());
                }
            }
        }
        for code in catalog.keys() {
            if !emitted.contains(code) {
                problems.push(format!(
                    "{code} is in CATALOG but nothing reports it any more; remove the entry"
                ));
            }
        }
        assert!(
            problems.is_empty(),
            "diagnostic catalog out of sync with its reporters ({} problems):\n  {}\n\nThen \
             regenerate docs with `SPECFORGE_BLESS=1 cargo test -p specforge-diagnostics \
             explain_docs_sync`.",
            problems.len(),
            problems.join("\n  ")
        );
    }
}
