use super::*;
use specforge_repo_root::repo_root;
use specforge_test_macros::test as specforge_test;
use std::collections::{BTreeMap, BTreeSet};

mod scan;

use scan::{
    cited_codes, code_literals, code_references, guest_sites, host_references, host_source,
    is_literal_consumer, opens_table, walk,
};

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
