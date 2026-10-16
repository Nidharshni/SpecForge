//! The `inferred` lint profile: I200 and I202 from the inference
//! manifest.

use std::fs;
use std::path::Path;

use specforge_common::{Diagnostic, codes};

use super::manifest::{InferenceManifest, detect_stale_entries};
use crate::view::ProjectView;

/// The inference density above which I202 is reported, unless
/// `inference.density_threshold` says otherwise.
const DEFAULT_DENSITY_THRESHOLD: f64 = 0.05;

/// The `inferred` lint profile's diagnostics for the view's project: I200
/// for each indexed file whose content changed since it was analyzed, and
/// I202 for each indexed file with more entities per line than the
/// threshold of the config the view was compiled with
/// (`view.env().config().inference.density_threshold`). Nothing without a
/// root or a manifest; one E071 when the manifest cannot be used.
pub fn lint(view: &ProjectView) -> Vec<Diagnostic> {
    let Some(root) = view.root() else {
        return Vec::new();
    };
    let manifest = match InferenceManifest::read(root) {
        Ok(Some(manifest)) => manifest,
        Ok(None) => return Vec::new(),
        Err(problem) => return vec![problem.diagnostic()],
    };
    let density_threshold = view
        .env()
        .config()
        .inference
        .density_threshold
        .unwrap_or(DEFAULT_DENSITY_THRESHOLD);
    compute_inference_diagnostics(root, &manifest, density_threshold)
}

fn compute_inference_diagnostics(
    project_root: &Path,
    manifest: &InferenceManifest,
    density_threshold: f64,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    let (stale, _deleted) = detect_stale_entries(project_root, manifest);
    for path in &stale {
        diagnostics.push(
            Diagnostic::new(
                codes::I200,
                format!(
                    "Source file '{}' has changed since it was analyzed — inferred entities may be stale",
                    path
                ),
            )
            .with_suggestion("Re-analyze this file to update inferred entities".to_string()),
        );
    }

    for entry in &manifest.source_index {
        if entry.entities_produced.is_empty() {
            continue;
        }
        let abs_path = project_root.join(&entry.path);
        if !abs_path.exists() {
            continue;
        }
        let line_count = match fs::read_to_string(&abs_path) {
            Ok(content) => content.lines().count().max(1),
            Err(_) => continue,
        };
        let density = entry.entities_produced.len() as f64 / line_count as f64;
        if density > density_threshold {
            diagnostics.push(
                Diagnostic::new(
                    codes::I202,
                    format!(
                        "High inference density in '{}': {} entities from {} lines ({:.1} entities/100 lines, threshold: {:.1})",
                        entry.path,
                        entry.entities_produced.len(),
                        line_count,
                        density * 100.0,
                        density_threshold * 100.0,
                    ),
                )
                .with_suggestion(
                    "Consider whether some inferred entities should be merged or removed"
                        .to_string(),
                ),
            );
        }
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::super::manifest::SourceFileEntry;
    use super::super::sources::content_hash;
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn i200_stale_source_anchor() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("main.rs");
        fs::write(&file, "fn main() {}").unwrap();

        let hash = content_hash(&fs::read(&file).unwrap());
        let mut manifest = InferenceManifest::default();
        manifest.upsert_source_entry(SourceFileEntry {
            path: "main.rs".to_string(),
            content_hash: hash,
            entities_produced: vec!["app_main".to_string()],
            analyzed_at: "t".to_string(),
            unknown: Default::default(),
        });

        let diags = compute_inference_diagnostics(dir.path(), &manifest, 0.05);
        assert!(diags.iter().all(|d| d.code != "I200"));

        fs::write(&file, "fn main() { changed }").unwrap();
        let diags = compute_inference_diagnostics(dir.path(), &manifest, 0.05);
        assert!(diags.iter().any(|d| d.code == "I200"));
    }

    #[test]
    fn i202_high_density() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("tiny.rs");
        fs::write(&file, "pub fn a() {}\npub fn b() {}").unwrap();

        let hash = content_hash(&fs::read(&file).unwrap());
        let mut manifest = InferenceManifest::default();
        manifest.upsert_source_entry(SourceFileEntry {
            path: "tiny.rs".to_string(),
            content_hash: hash,
            entities_produced: vec![
                "e1".into(),
                "e2".into(),
                "e3".into(),
                "e4".into(),
                "e5".into(),
            ],
            analyzed_at: "t".to_string(),
            unknown: Default::default(),
        });

        let diags = compute_inference_diagnostics(dir.path(), &manifest, 0.05);
        assert!(diags.iter().any(|d| d.code == "I202"));
    }

    #[test]
    fn i202_below_threshold_no_diagnostic() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("big.rs");
        let content = (0..200)
            .map(|i| format!("pub fn func_{i}() {{}}\n"))
            .collect::<String>();
        fs::write(&file, &content).unwrap();

        let hash = content_hash(&fs::read(&file).unwrap());
        let mut manifest = InferenceManifest::default();
        manifest.upsert_source_entry(SourceFileEntry {
            path: "big.rs".to_string(),
            content_hash: hash,
            entities_produced: vec!["one".into()],
            analyzed_at: "t".to_string(),
            unknown: Default::default(),
        });

        let diags = compute_inference_diagnostics(dir.path(), &manifest, 0.05);
        assert!(diags.iter().all(|d| d.code != "I202"));
    }
}
