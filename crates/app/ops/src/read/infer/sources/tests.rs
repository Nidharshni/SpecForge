use super::*;
use crate::read::infer::manifest::SourceFileEntry;
use specforge_protocol_types::{AnalyzerDescriptor, ExtensionDeclaration, HandshakeResponse};
use tempfile::TempDir;

#[test]
fn the_source_path_rule() {
    for (given, recorded) in [
        ("a.rs", "a.rs"),
        ("./a.rs", "a.rs"),
        ("src\\a.rs", "src/a.rs"),
        ("src//a.rs", "src/a.rs"),
        ("src/./a.rs", "src/a.rs"),
        ("src/", "src"),
    ] {
        assert_eq!(
            SourceKey::of(given).map(|k| k.as_str().to_string()),
            Ok(recorded.to_string()),
            "{given}"
        );
    }
    for outside in [
        "/x", "\\x", "C:\\x", "c:/x", "../x", "a/../b", "", ".", "./",
    ] {
        assert_eq!(SourceKey::of(outside), Err(OutsideRoot), "{outside:?}");
    }
    // A source root may be the project root itself.
    assert_eq!(SourceKey::root(".").unwrap().as_str(), ".");
    assert_eq!(SourceKey::root("./src").unwrap().as_str(), "src");
    for outside in ["", "/x", "../x"] {
        assert_eq!(SourceKey::root(outside), Err(OutsideRoot), "{outside:?}");
    }
}

#[test]
fn content_hash_is_sha256() {
    let hash = content_hash(b"fn main() {}");
    assert_eq!(hash.len(), 64);
    assert_eq!(
        content_hash(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

fn declaration(name: &str, extensions: &[&str], excluded: &[&str]) -> ExtensionDeclaration {
    ExtensionDeclaration {
        handshake: HandshakeResponse {
            name: name.into(),
            version: "1.0.0".into(),
            ..Default::default()
        },
        analyzers: vec![AnalyzerDescriptor {
            language: "any".into(),
            file_extensions: extensions.iter().map(|e| (*e).to_string()).collect(),
            excluded_dirs: excluded.iter().map(|d| (*d).to_string()).collect(),
            scan_export: "scan".into(),
            ..Default::default()
        }],
        ..Default::default()
    }
}

fn tree(files: &[&str]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for file in files {
        let path = dir.path().join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "x").unwrap();
    }
    dir
}

fn keys(sources: &InferenceSources) -> Vec<&str> {
    sources.keys().iter().map(SourceKey::as_str).collect()
}

#[test]
fn claims_merge_extensions_in_declaration_order() {
    let analyzers = Analyzers::of(&[
        declaration("@a/one", &[".rs", "ts"], &[]),
        declaration("@a/two", &[".rs", ".js"], &[]),
    ]);
    let claim = analyzers.claim("lib.rs").unwrap();
    assert_eq!(
        (claim.analyzer.as_str(), claim.extension.as_str()),
        ("@a/one", ".rs")
    );
    assert_eq!(analyzers.claim("x.js").unwrap().analyzer, "@a/two");
    // A bare extension is claimed with its dot.
    assert_eq!(analyzers.claim("a.ts").unwrap().extension, ".ts");
    assert!(analyzers.claim("a.md").is_none());
}

#[test]
fn the_longest_extension_claims_a_file() {
    let analyzers = Analyzers::of(&[
        declaration("@a/ts", &[".ts"], &[]),
        declaration("@a/dts", &[".d.ts"], &[]),
    ]);
    assert_eq!(analyzers.claim("types.d.ts").unwrap().analyzer, "@a/dts");
    assert_eq!(analyzers.claim("app.ts").unwrap().analyzer, "@a/ts");
}

#[test]
fn no_declared_exclude_still_skips_the_defaults() {
    let dir = tree(&[
        "src/a.rs",
        "node_modules/d/x.rs",
        "vendor/c/y.rs",
        "target/z.rs",
        "__pycache__/p.rs",
        ".hidden/h.rs",
    ]);
    let sources = InferenceSources::of(
        dir.path(),
        &[declaration("@a/rs", &[".rs"], &[])],
        &InferenceManifest::default(),
    );
    assert_eq!(keys(&sources), ["src/a.rs"]);
}

#[test]
fn a_declared_exclude_adds_to_the_defaults() {
    let dir = tree(&["src/a.rs", "gen/g.rs", "node_modules/d/x.rs", "target/z.rs"]);
    let sources = InferenceSources::of(
        dir.path(),
        &[declaration("@a/rs", &[".rs"], &["gen"])],
        &InferenceManifest::default(),
    );
    assert_eq!(keys(&sources), ["src/a.rs"]);
}

#[test]
fn the_walk_finds_claimed_files_under_a_root() {
    let dir = tree(&["src/a.rs", "src/b/c.rs", "src/d.md", "other/e.rs"]);
    let manifest = InferenceManifest {
        source_roots: vec!["src".into()],
        ..Default::default()
    };
    let sources = InferenceSources::of(
        dir.path(),
        &[declaration("@a/rs", &[".rs"], &[])],
        &manifest,
    );
    assert_eq!(keys(&sources), ["src/a.rs", "src/b/c.rs"]);
}

#[test]
fn no_roots_is_the_project_root() {
    let dir = tree(&["a.rs", "src/b.rs"]);
    let sources = InferenceSources::of(
        dir.path(),
        &[declaration("@a/rs", &[".rs"], &[])],
        &InferenceManifest::default(),
    );
    assert_eq!(keys(&sources), ["a.rs", "src/b.rs"]);
}

#[test]
fn indexed_states_are_current_changed_deleted_or_unreadable() {
    let dir = tree(&["a.rs", "b.rs", "d/x.rs"]);
    std::fs::write(dir.path().join("a.rs"), "one\ntwo").unwrap();
    let mut manifest = InferenceManifest::default();
    let hash_a = content_hash(b"one\ntwo");
    for (path, hash) in [
        ("./a.rs", hash_a.as_str()),
        ("b.rs", "stale"),
        ("gone.rs", "x"),
        ("../outside.rs", "x"),
        ("/abs.rs", "x"),
        ("d", "x"),
    ] {
        manifest
            .source_index
            .push(SourceFileEntry::new(path, hash, vec![], "t"));
    }
    let states: Vec<IndexState> = ProjectFiles::at(dir.path())
        .indexed(&manifest)
        .iter()
        .map(|i| i.state)
        .collect();
    assert_eq!(
        states,
        [
            IndexState::Current { lines: Some(2) },
            IndexState::Changed { lines: Some(1) },
            IndexState::Deleted,
            IndexState::Deleted,
            IndexState::Deleted,
            IndexState::Unreadable,
        ]
    );
}
