//! Which source files inference covers: those the loaded analyzers would
//! scan under the manifest's source roots.

use std::collections::HashSet;
use std::path::Path;

use specforge_protocol_types::ExtensionDeclaration;

use super::manifest::InferenceManifest;

/// The source files the enabled analyzers would scan under `manifest`'s
/// source roots.
pub(super) fn source_files(
    root: &Path,
    declarations: &[ExtensionDeclaration],
    manifest: &InferenceManifest,
) -> Vec<String> {
    let analyzers: Vec<AnalyzerConfig> = declarations
        .iter()
        .flat_map(|d| d.analyzers.iter())
        .map(|ac| AnalyzerConfig {
            file_extensions: ac.file_extensions.clone(),
            excluded_dirs: ac.excluded_dirs.clone(),
        })
        .collect();
    let discovery = SourceDiscoveryConfig::from_analyzer_configs(&analyzers);
    discover_source_files(root, &manifest.source_roots, &discovery)
}

#[derive(Debug, Clone)]
pub struct AnalyzerConfig {
    pub file_extensions: Vec<String>,
    pub excluded_dirs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SourceDiscoveryConfig {
    pub source_extensions: Vec<String>,
    pub excluded_dirs: HashSet<String>,
}

impl Default for SourceDiscoveryConfig {
    fn default() -> Self {
        Self {
            source_extensions: Vec::new(),
            excluded_dirs: Self::default_excluded_dirs(),
        }
    }
}

impl SourceDiscoveryConfig {
    pub fn from_analyzer_configs(analyzers: &[AnalyzerConfig]) -> Self {
        let mut source_extensions = Vec::new();
        let mut excluded_dirs = HashSet::new();

        for a in analyzers {
            for ext in &a.file_extensions {
                let ext = if ext.starts_with('.') {
                    ext.clone()
                } else {
                    format!(".{}", ext)
                };
                if !source_extensions.contains(&ext) {
                    source_extensions.push(ext);
                }
            }
            for dir in &a.excluded_dirs {
                excluded_dirs.insert(dir.clone());
            }
        }

        if excluded_dirs.is_empty() {
            excluded_dirs = Self::default_excluded_dirs();
        }

        Self {
            source_extensions,
            excluded_dirs,
        }
    }

    fn default_excluded_dirs() -> HashSet<String> {
        [
            "target",
            "node_modules",
            "dist",
            "build",
            ".git",
            "__pycache__",
            ".venv",
            "vendor",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect()
    }

    fn is_source_file(&self, name: &str) -> bool {
        self.source_extensions
            .iter()
            .any(|ext| name.ends_with(ext.as_str()))
    }

    fn is_excluded_dir(&self, name: &str) -> bool {
        self.excluded_dirs.contains(name)
    }
}

pub fn discover_source_files(
    project_root: &Path,
    source_roots: &[String],
    config: &SourceDiscoveryConfig,
) -> Vec<String> {
    let mut files = Vec::new();
    let roots: Vec<&str> = if source_roots.is_empty() {
        vec!["."]
    } else {
        source_roots.iter().map(|s| s.as_str()).collect()
    };

    for root in roots {
        let abs_root = project_root.join(root);
        if !abs_root.is_dir() {
            continue;
        }
        walk_source_dir(&abs_root, project_root, config, &mut files);
    }
    files.sort();
    files
}

fn walk_source_dir(
    dir: &Path,
    project_root: &Path,
    config: &SourceDiscoveryConfig,
    files: &mut Vec<String>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if name.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            if config.is_excluded_dir(&name) {
                continue;
            }
            walk_source_dir(&path, project_root, config, files);
        } else if config.is_source_file(&name)
            && let Ok(rel) = path.strip_prefix(project_root)
        {
            // `/`-separated on every platform, as the manifest records it.
            let parts: Vec<_> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect();
            files.push(parts.join("/"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn default_has_no_source_extensions() {
        let config = SourceDiscoveryConfig::default();
        assert!(config.source_extensions.is_empty());
    }

    #[test]
    fn default_has_8_excluded_dirs() {
        let config = SourceDiscoveryConfig::default();
        assert_eq!(config.excluded_dirs.len(), 8);
        assert!(config.excluded_dirs.contains("target"));
        assert!(config.excluded_dirs.contains("node_modules"));
    }

    #[test]
    fn is_source_file_matches_analyzer_extensions() {
        let configs = vec![AnalyzerConfig {
            file_extensions: vec![".rs".to_string()],
            excluded_dirs: vec![],
        }];
        let config = SourceDiscoveryConfig::from_analyzer_configs(&configs);
        assert!(config.is_source_file("main.rs"));
        assert!(!config.is_source_file("readme.md"));
        assert!(!config.is_source_file("app.tsx"));
    }

    #[test]
    fn is_excluded_dir_matches() {
        let config = SourceDiscoveryConfig::default();
        assert!(config.is_excluded_dir("target"));
        assert!(config.is_excluded_dir("node_modules"));
        assert!(!config.is_excluded_dir("src"));
    }

    #[test]
    fn discover_finds_source_files() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("main.rs"), "fn main() {}").unwrap();
        std::fs::write(src.join("readme.md"), "# Hello").unwrap();

        let config = SourceDiscoveryConfig::from_analyzer_configs(&[AnalyzerConfig {
            file_extensions: vec![".rs".to_string()],
            excluded_dirs: vec![],
        }]);
        let files = discover_source_files(dir.path(), &["src".to_string()], &config);
        assert_eq!(files, vec!["src/main.rs"]);
    }

    #[test]
    fn discover_skips_excluded_dirs() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src");
        let target = dir.path().join("target");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(src.join("lib.rs"), "").unwrap();
        std::fs::write(target.join("built.rs"), "").unwrap();

        let config = SourceDiscoveryConfig::from_analyzer_configs(&[AnalyzerConfig {
            file_extensions: vec![".rs".to_string()],
            excluded_dirs: vec!["target".to_string()],
        }]);
        let files = discover_source_files(dir.path(), &[], &config);
        assert!(files.contains(&"src/lib.rs".to_string()));
        assert!(!files.iter().any(|f| f.contains("target")));
    }

    #[test]
    fn discover_defaults_to_project_root_when_no_roots() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("lib.rs"), "").unwrap();

        let config = SourceDiscoveryConfig::from_analyzer_configs(&[AnalyzerConfig {
            file_extensions: vec![".rs".to_string()],
            excluded_dirs: vec![],
        }]);
        let files = discover_source_files(dir.path(), &[], &config);
        assert_eq!(files, vec!["lib.rs"]);
    }

    #[test]
    fn from_analyzer_configs_merges_extensions() {
        let configs = vec![
            AnalyzerConfig {
                file_extensions: vec![".rs".to_string()],
                excluded_dirs: vec!["target".to_string()],
            },
            AnalyzerConfig {
                file_extensions: vec![".ts".to_string(), ".tsx".to_string()],
                excluded_dirs: vec!["node_modules".to_string()],
            },
        ];
        let config = SourceDiscoveryConfig::from_analyzer_configs(&configs);
        assert_eq!(config.source_extensions, vec![".rs", ".ts", ".tsx"]);
        assert!(config.excluded_dirs.contains("target"));
        assert!(config.excluded_dirs.contains("node_modules"));
    }

    #[test]
    fn from_analyzer_configs_normalizes_dot_prefix() {
        let configs = vec![AnalyzerConfig {
            file_extensions: vec!["go".to_string()],
            excluded_dirs: vec![],
        }];
        let config = SourceDiscoveryConfig::from_analyzer_configs(&configs);
        assert_eq!(config.source_extensions, vec![".go"]);
    }

    #[test]
    fn from_analyzer_configs_uses_default_excludes_when_none_specified() {
        let configs = vec![AnalyzerConfig {
            file_extensions: vec![".rs".to_string()],
            excluded_dirs: vec![],
        }];
        let config = SourceDiscoveryConfig::from_analyzer_configs(&configs);
        assert_eq!(config.excluded_dirs.len(), 8);
        assert!(config.excluded_dirs.contains("target"));
    }

    #[test]
    fn from_analyzer_configs_deduplicates_extensions() {
        let configs = vec![
            AnalyzerConfig {
                file_extensions: vec![".ts".to_string()],
                excluded_dirs: vec![],
            },
            AnalyzerConfig {
                file_extensions: vec![".ts".to_string(), ".tsx".to_string()],
                excluded_dirs: vec![],
            },
        ];
        let config = SourceDiscoveryConfig::from_analyzer_configs(&configs);
        assert_eq!(config.source_extensions, vec![".ts", ".tsx"]);
    }
}
