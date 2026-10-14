//! Diagnostic code registry: the single canonical catalog of every code the
//! compiler, the CLI, and the first-party extensions emit.
//!
//! `specforge explain <CODE>` and the MCP `specforge.explain` tool print an
//! entry, MCP diagnostics and the LSP carry its title and docs link, doctor
//! quotes its explanation, and `docs/diagnostics.md` is generated from
//! [`CATALOG`]. The catalog is one table (`catalog.rs`) that generates both
//! [`CATALOG`] and a typed constant for every core code ([`codes`]): the
//! host names a core code as `codes::W112`, a [`Code`] whose level is the
//! catalogued one. The tests in `tests.rs` fail when an emitted
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
mod tests;
