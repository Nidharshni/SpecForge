//! The root of this SpecForge checkout: the nearest directory above this crate whose
//! `Cargo.toml` declares the workspace.
//!
//! Tests and build tools read files outside their own crate (`spec/`, `examples/`, `docs/`,
//! `schema/`, the committed blobs under `extensions/` and `fixtures/`, another crate's
//! fixtures) by a path from this root. The root is found by walking up from this crate's own
//! directory, so no caller depends on how deep it or any other crate sits under `crates/`.
//! Nothing a user runs reads it: the shipped binaries never depend on this crate.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The checkout's root, canonical (absolute, no `..`, symlinks resolved).
///
/// # Panics
///
/// When no directory above this crate has a `Cargo.toml` with a `[workspace]` table: the crate
/// was built outside a SpecForge checkout.
pub fn repo_root() -> &'static Path {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = workspace_above(here).unwrap_or_else(|| {
            panic!(
                "no Cargo.toml with a [workspace] table above {}: specforge-repo-root reads a \
                 SpecForge checkout and must be built inside one",
                here.display()
            )
        });
        root.canonicalize()
            .unwrap_or_else(|e| panic!("{}: {e}", root.display()))
    })
}

/// The nearest ancestor of `dir`, `dir` excluded, whose `Cargo.toml` has a `[workspace]` table.
fn workspace_above(dir: &Path) -> Option<&Path> {
    dir.ancestors()
        .skip(1)
        .find(|ancestor| declares_workspace(&ancestor.join("Cargo.toml")))
}

/// Whether the manifest has a line that is exactly `[workspace]` (a `[workspace.*]` table alone
/// is not one).
fn declares_workspace(manifest: &Path) -> bool {
    std::fs::read_to_string(manifest)
        .is_ok_and(|text| text.lines().any(|line| line.trim_end() == "[workspace]"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_root_holds_the_workspace_manifest_and_the_crates() {
        let root = repo_root();
        assert!(
            declares_workspace(&root.join("Cargo.toml")),
            "{}",
            root.display()
        );
        assert!(root.join("crates").is_dir(), "{}", root.display());
        assert!(root.is_absolute());
        assert_eq!(root, root.canonicalize().unwrap());
    }

    #[test]
    fn a_crate_at_any_depth_finds_the_same_root() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path();
        std::fs::write(root.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
        for crate_dir in ["crates/flat", "crates/layer/nested", "crates/a/b/deeper"] {
            let crate_dir = root.join(crate_dir);
            std::fs::create_dir_all(&crate_dir).unwrap();
            std::fs::write(crate_dir.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
            assert_eq!(
                workspace_above(&crate_dir),
                Some(root),
                "{}",
                crate_dir.display()
            );
        }
    }

    #[test]
    fn only_a_workspace_table_marks_the_root() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path();
        std::fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
        // A manifest that inherits from the workspace is not one.
        let layer = root.join("crates/layer");
        std::fs::create_dir_all(layer.join("x")).unwrap();
        std::fs::write(
            layer.join("Cargo.toml"),
            "[workspace.package]\nedition = \"2024\"\n",
        )
        .unwrap();
        assert_eq!(workspace_above(&layer.join("x")), Some(root));
    }
}
