//! A remedy names a command or the request's words, never a bare flag (ADR 0135). Every string literal in the
//! `src/` of every crate below the surfaces is read (in code and in macro arguments; not in attributes, doc
//! comments or `#[cfg(test)]` modules), and a `--flag` in one is refused unless a program (`specforge`, `cargo`,
//! `z3`) precedes it in its clause: a whole command reads the same on every surface. The diagnostic catalog's
//! explanations are exempt: they name each surface's spelling side by side.
//!
//! Not seen: strings built at run time from pieces (`format!("--{}", name)`), text coming from extensions, and the
//! surfaces themselves (the CLI's own errors may name its flags).

use std::path::{Path, PathBuf};

use proc_macro2::{TokenStream, TokenTree};
use syn::visit::Visit;

const LAYERS: [&str; 7] = [
    "leaf", "kernel", "pipeline", "runtime", "session", "app", "adapters",
];
const PROGRAMS: [&str; 3] = ["specforge ", "cargo ", "z3 "];
/// Relative to `crates/`.
const EXEMPT: &str = "leaf/diagnostics/src/catalog.rs";

/// The bare flags in `text`. A flag is `--` then a lowercase letter, then letters, digits or `-`, not glued to a
/// word before it or a word or `{` after it (`||--o{` is Mermaid). It is bare unless one of `PROGRAMS` appears in
/// its clause: the text after the last `;`, `,`, `(` or `)` before it. A literal that is one argv token
/// (`--out={report}`, no whitespace, starting with `--`) has none.
fn bare_flags(text: &str) -> Vec<&str> {
    if text.starts_with("--") && !text.contains(char::is_whitespace) {
        return Vec::new();
    }
    let bytes = text.as_bytes();
    let mut flags = Vec::new();
    let mut at = 0;
    while let Some(found) = text[at..].find("--") {
        let start = at + found;
        at = start + 2;
        if !bytes.get(start + 2).is_some_and(u8::is_ascii_lowercase) {
            continue;
        }
        if start > 0 && bytes[start - 1].is_ascii_alphanumeric() {
            continue;
        }
        let mut end = start + 2;
        while bytes
            .get(end)
            .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
        {
            end += 1;
        }
        if bytes
            .get(end)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'{')
        {
            continue;
        }
        let clause_start = text[..start]
            .rfind([';', ',', '(', ')'])
            .map_or(0, |i| i + 1);
        let clause = &text[clause_start..start];
        if !PROGRAMS.iter().any(|program| clause.contains(program)) {
            flags.push(&text[start..end]);
        }
        at = end;
    }
    flags
}

/// The string literals outside attributes and `#[cfg(test)]` modules, macro arguments included.
#[derive(Default)]
struct Literals(Vec<String>);

impl Literals {
    fn tokens(&mut self, tokens: TokenStream) {
        for tree in tokens {
            match tree {
                TokenTree::Group(group) => self.tokens(group.stream()),
                TokenTree::Literal(literal) => {
                    if let syn::Lit::Str(text) = syn::Lit::new(literal) {
                        self.0.push(text.value());
                    }
                }
                _ => {}
            }
        }
    }
}

impl<'ast> Visit<'ast> for Literals {
    fn visit_attribute(&mut self, _: &'ast syn::Attribute) {}

    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        let cfg_test = item.attrs.iter().any(|attr| {
            attr.path().is_ident("cfg")
                && attr
                    .parse_args::<syn::Ident>()
                    .is_ok_and(|arg| arg == "test")
        });
        if !cfg_test {
            syn::visit::visit_item_mod(self, item);
        }
    }

    fn visit_lit_str(&mut self, literal: &'ast syn::LitStr) {
        self.0.push(literal.value());
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        self.tokens(mac.tokens.clone());
    }
}

fn literals(source: &str) -> Result<Vec<String>, syn::Error> {
    let file = syn::parse_file(source)?;
    let mut found = Literals::default();
    found.visit_file(&file);
    Ok(found.0)
}

fn source_files() -> Vec<PathBuf> {
    let crates = specforge_repo_root::repo_root().join("crates");
    let mut files = Vec::new();
    for layer in LAYERS {
        for entry in std::fs::read_dir(crates.join(layer)).unwrap() {
            let src = entry.unwrap().path().join("src");
            if src.is_dir() {
                crate::groups::rust_files(&src, &mut files);
            }
        }
    }
    files.retain(|path| {
        let relative = path.strip_prefix(&crates).unwrap();
        let name = relative.file_name().unwrap().to_string_lossy();
        relative != Path::new(EXEMPT)
            && name != "tests.rs"
            && !name.ends_with("_tests.rs")
            && !relative
                .components()
                .any(|part| part.as_os_str() == "tests")
    });
    files.sort();
    files
}

#[test]
fn no_message_below_the_surfaces_names_a_bare_flag() {
    let crates = specforge_repo_root::repo_root().join("crates");
    let mut offenders = Vec::new();
    for path in source_files() {
        let relative = path.strip_prefix(&crates).unwrap().display().to_string();
        let source = std::fs::read_to_string(&path).unwrap();
        let found = literals(&source).unwrap_or_else(|error| panic!("{relative}: {error}"));
        for text in &found {
            let flags = bare_flags(text);
            if !flags.is_empty() {
                let shown: String = text.chars().take(100).collect();
                offenders.push(format!("{relative}: {flags:?}: {shown}"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "{} literals name a bare flag (ADR 0135):\n{}",
        offenders.len(),
        offenders.join("\n")
    );
}

#[test]
fn a_flag_is_bare_unless_a_program_precedes_it_in_its_clause() {
    for refused in [
        "--min needs test results",
        "run without --no-run to produce one",
        "in a terminal, or with --yes; or add 'k' to trusted_keys",
        "pick one with --runner (available: a)",
        "the --report files don't exist",
    ] {
        assert!(!bare_flags(refused).is_empty(), "{refused}");
    }
    for allowed in [
        "or run `specforge collect --yes`; reading",
        "raise `specforge analyze --prove-timeout`",
        "cargo build --release --target wasm32-wasip2",
        "`z3 --version` failed: exit status: 1",
        "--out={report}",
        "||--o{",
        "specforge update --major moves them together",
        "in a terminal (`specforge add --yes` and `specforge update --yes` accept it without asking)",
    ] {
        assert!(bare_flags(allowed).is_empty(), "{allowed}");
    }
}
