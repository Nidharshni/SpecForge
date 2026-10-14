use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// The checkout a run judges: its root, the directory cargo builds into (`CARGO_TARGET_DIR` honoured, from
/// `cargo metadata`), and the workspace members with their directories.
#[derive(Clone, Debug)]
pub struct Repo {
    pub root: PathBuf,
    pub target_dir: PathBuf,
    pub members: Vec<Member>,
}

/// The target kinds whose source is a crate root: a lib (any crate type), a proc macro or a binary. Tests, benches,
/// examples and build scripts are not.
const ROOT_KINDS: &[&str] = &[
    "lib",
    "rlib",
    "dylib",
    "cdylib",
    "staticlib",
    "proc-macro",
    "bin",
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Member {
    pub name: String,
    /// Repo-relative, `/`-separated, no trailing slash (`crates/leaf/coverage`).
    pub dir: String,
    /// Its path dependencies inside the root, in `cargo metadata`'s order: one per manifest entry, so a crate named
    /// under both `[dependencies]` and `[dev-dependencies]` is here twice.
    pub dependencies: Vec<Dependency>,
    /// The manifest's `description`, when it has one: the member's line in the crate map (ADR 0091).
    pub description: Option<String>,
    /// Its crate roots, repo-relative: the source of each lib, proc-macro and bin target, in manifest order.
    pub roots: Vec<String>,
}

/// A path dependency of a member on a package inside the root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dependency {
    /// The package's name (a `package = …` rename gives the real name, not the alias).
    pub name: String,
    /// Repo-relative like [`Member::dir`].
    pub dir: String,
    pub kind: DependencyKind,
}

/// The manifest table a dependency is declared in; a `[target.'cfg(…)'.dependencies]` table counts as `Normal`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DependencyKind {
    Normal,
    Build,
    Dev,
}

/// The one list of projects the spec checks judge (was `ci.yml:62`, `gate.sh:36` and `:37`).
pub const PROJECTS: &[&str] = &[
    ".",
    "integrations/rust/spec",
    "examples/todo-app",
    "examples/shop",
];

/// The crates outside the workspace, each its own manifest (was `ci.yml:54,75,92`, `gate.sh:184`).
pub const OUTSIDE_WORKSPACE: &[&str] = &["extensions/*/Cargo.toml", "fixtures/*/Cargo.toml"];

/// A project the spec checks judge: its root (where `specforge.json` is) and the directory its `.spec` files are in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    pub root: String,
    pub spec_dir: String,
}

impl Repo {
    /// `cargo metadata --format-version 1 --no-deps` from the current directory.
    pub fn discover() -> Result<Repo, String> {
        let out = Command::new("cargo")
            .args(["metadata", "--format-version", "1", "--no-deps"])
            .output()
            .map_err(|e| format!("cargo metadata: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "cargo metadata failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        let metadata: Value =
            serde_json::from_slice(&out.stdout).map_err(|e| format!("cargo metadata: {e}"))?;
        Repo::from_metadata(&metadata)
    }

    pub(crate) fn from_metadata(metadata: &Value) -> Result<Repo, String> {
        let text = |key: &str| {
            metadata[key]
                .as_str()
                .map(PathBuf::from)
                .ok_or_else(|| format!("cargo metadata has no `{key}`"))
        };
        let root = text("workspace_root")?;
        let target_dir = text("target_directory")?;
        let mut members = Vec::new();
        for package in metadata["packages"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            let name = package["name"].as_str().ok_or("a package has no name")?;
            let manifest = Path::new(
                package["manifest_path"]
                    .as_str()
                    .ok_or("a package has no manifest_path")?,
            );
            let dir = manifest
                .parent()
                .and_then(|d| relative(&root, d))
                .ok_or("a member is outside the root")?;
            members.push(Member {
                name: name.to_string(),
                dir,
                dependencies: dependencies(&root, package)?,
                description: package["description"].as_str().map(str::to_string),
                roots: crate_roots(&root, package),
            });
        }
        members.sort_by(|a, b| a.dir.cmp(&b.dir));
        Ok(Repo {
            root,
            target_dir,
            members,
        })
    }

    /// The member whose directory holds `path` (the longest prefix), if any.
    pub fn member_of(&self, path: &str) -> Option<&Member> {
        self.members
            .iter()
            .filter(|m| {
                m.dir.is_empty() || path == m.dir || path.starts_with(&format!("{}/", m.dir))
            })
            .max_by_key(|m| m.dir.len())
    }

    /// The projects the spec checks judge, each with its spec directory read from its `specforge.json`'s
    /// `spec_root`: `.` is `spec`, `integrations/rust/spec` itself, `examples/*` their `spec`.
    pub fn projects(&self) -> Result<Vec<Project>, String> {
        PROJECTS
            .iter()
            .map(|root| {
                let manifest = self.root.join(root).join("specforge.json");
                let text = std::fs::read_to_string(&manifest)
                    .map_err(|e| format!("{}: {e}", manifest.display()))?;
                let json: Value = serde_json::from_str(&text)
                    .map_err(|e| format!("{}: {e}", manifest.display()))?;
                let spec_root = json["spec_root"].as_str().unwrap_or(".");
                Ok(Project {
                    root: root.to_string(),
                    spec_dir: join(root, spec_root),
                })
            })
            .collect()
    }
}

/// A package's crate roots inside `root`: the `src_path` of each target of a kind in [`ROOT_KINDS`].
fn crate_roots(root: &Path, package: &Value) -> Vec<String> {
    package["targets"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter(|target| {
            target["kind"].as_array().is_some_and(|kinds| {
                kinds
                    .iter()
                    .any(|kind| kind.as_str().is_some_and(|k| ROOT_KINDS.contains(&k)))
            })
        })
        .filter_map(|target| target["src_path"].as_str())
        .filter_map(|path| relative(root, Path::new(path)))
        .collect()
}

/// A package's path dependencies inside `root`; a registry or git dependency, or a path outside the root, is no
/// member and is left out.
fn dependencies(root: &Path, package: &Value) -> Result<Vec<Dependency>, String> {
    let mut found = Vec::new();
    for dependency in package["dependencies"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        let Some(dir) = dependency["path"]
            .as_str()
            .and_then(|path| relative(root, Path::new(path)))
        else {
            continue;
        };
        let name = dependency["name"]
            .as_str()
            .ok_or("a dependency has no name")?;
        let kind = match dependency["kind"].as_str() {
            None => DependencyKind::Normal,
            Some("build") => DependencyKind::Build,
            Some("dev") => DependencyKind::Dev,
            Some(other) => return Err(format!("{name}: unknown dependency kind `{other}`")),
        };
        found.push(Dependency {
            name: name.to_string(),
            dir,
            kind,
        });
    }
    Ok(found)
}

/// `path` relative to `root`, `/`-separated (`""` for the root itself); None outside it.
fn relative(root: &Path, path: &Path) -> Option<String> {
    let inside = path.strip_prefix(root).ok()?;
    Some(inside.to_string_lossy().replace('\\', "/"))
}

/// `base/rel` as a repo-relative path: `.` segments vanish, so `join(".", "spec")` is `spec` and
/// `join("a/spec", ".")` is `a/spec`.
fn join(base: &str, rel: &str) -> String {
    let parts: Vec<&str> = base
        .split('/')
        .chain(rel.split('/'))
        .filter(|s| !s.is_empty() && *s != ".")
        .collect();
    if parts.is_empty() {
        ".".into()
    } else {
        parts.join("/")
    }
}

/// This repository with a literal member list, for tests: the real root (its `specforge.json` and `inputs.json`
/// files are read), no `cargo metadata`.
#[cfg(test)]
pub(crate) fn this_repo() -> Repo {
    let root = specforge_repo_root::repo_root().to_path_buf();
    let member = |name: &str, dir: &str| Member {
        name: name.into(),
        dir: dir.into(),
        ..Default::default()
    };
    Repo {
        target_dir: root.join("target"),
        members: vec![
            member("specforge-coverage", "crates/leaf/coverage"),
            member("specforge-diagnostics", "crates/leaf/diagnostics"),
            member("specforge-extension-sdk", "crates/runtime/extension-sdk"),
            member("specforge-formatter", "crates/pipeline/formatter"),
            member("specforge-lsp", "crates/surfaces/lsp"),
            member("specforge-registry", "crates/pipeline/registry"),
            member("specforge-test", "integrations/rust/specforge-test"),
            member("gate", "gate"),
            member("xtask", "xtask"),
        ],
        root,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_longest_member_directory_holds_a_path() {
        let repo = this_repo();
        assert_eq!(
            repo.member_of("crates/surfaces/lsp/src/lib.rs")
                .unwrap()
                .name,
            "specforge-lsp"
        );
        assert_eq!(
            repo.member_of("integrations/rust/specforge-test/src/atexit.rs")
                .unwrap()
                .name,
            "specforge-test"
        );
        assert_eq!(repo.member_of("xtask").unwrap().name, "xtask");
        assert!(repo.member_of("extensions/governance/src/lib.rs").is_none());
        assert!(
            repo.member_of("crates/surfaces/lsp-extra/src/lib.rs")
                .is_none()
        );
    }

    #[test]
    fn a_projects_spec_dir_follows_its_spec_root() {
        let projects = this_repo().projects().unwrap();
        let dirs: Vec<(&str, &str)> = projects
            .iter()
            .map(|p| (p.root.as_str(), p.spec_dir.as_str()))
            .collect();
        assert_eq!(
            dirs,
            [
                (".", "spec"),
                ("integrations/rust/spec", "integrations/rust/spec"),
                ("examples/todo-app", "examples/todo-app/spec"),
                ("examples/shop", "examples/shop/spec"),
            ]
        );
    }

    #[test]
    fn cargo_metadata_gives_the_root_the_target_dir_and_the_members() {
        let metadata = serde_json::json!({
            "workspace_root": "/r",
            "target_directory": "/elsewhere/targets/p04",
            "packages": [
                {"name": "gate", "manifest_path": "/r/gate/Cargo.toml"},
                {"name": "a", "manifest_path": "/r/crates/a/Cargo.toml"},
            ],
        });
        let repo = Repo::from_metadata(&metadata).unwrap();
        assert_eq!(repo.target_dir, PathBuf::from("/elsewhere/targets/p04"));
        let dirs: Vec<&str> = repo.members.iter().map(|m| m.dir.as_str()).collect();
        assert_eq!(dirs, ["crates/a", "gate"]);
    }

    #[test]
    fn cargo_metadata_gives_each_member_its_description_and_crate_roots() {
        let metadata = serde_json::json!({
            "workspace_root": "/r",
            "target_directory": "/r/target",
            "packages": [{
                "name": "a",
                "manifest_path": "/r/crates/leaf/a/Cargo.toml",
                "description": "The a crate",
                "targets": [
                    {"kind": ["lib"], "src_path": "/r/crates/leaf/a/src/lib.rs"},
                    {"kind": ["bin"], "src_path": "/r/crates/leaf/a/src/main.rs"},
                    {"kind": ["test"], "src_path": "/r/crates/leaf/a/tests/t.rs"},
                    {"kind": ["custom-build"], "src_path": "/r/crates/leaf/a/build.rs"},
                ],
            }, {"name": "b", "manifest_path": "/r/b/Cargo.toml"}],
        });
        let repo = Repo::from_metadata(&metadata).unwrap();
        let a = repo.members.iter().find(|m| m.name == "a").unwrap();
        assert_eq!(a.description.as_deref(), Some("The a crate"));
        assert_eq!(
            a.roots,
            ["crates/leaf/a/src/lib.rs", "crates/leaf/a/src/main.rs"]
        );
        let b = repo.members.iter().find(|m| m.name == "b").unwrap();
        assert_eq!((b.description.as_deref(), b.roots.len()), (None, 0));
    }

    #[test]
    fn cargo_metadata_gives_each_members_path_dependencies_and_their_kind() {
        let metadata = serde_json::json!({
            "workspace_root": "/r",
            "target_directory": "/r/target",
            "packages": [{
                "name": "specforge-cli",
                "manifest_path": "/r/crates/surfaces/cli/Cargo.toml",
                "dependencies": [
                    {"name": "specforge-ops", "kind": null, "path": "/r/crates/app/ops"},
                    {"name": "serde", "kind": null, "source": "registry+https://github.com/rust-lang/crates.io-index"},
                    {"name": "specforge-test", "kind": "dev", "path": "/r/integrations/rust/specforge-test"},
                    {"name": "cc", "kind": "build", "path": "/r/crates/leaf/cc"},
                    {"name": "elsewhere", "kind": null, "path": "/vendor/elsewhere"},
                ],
            }],
        });
        let repo = Repo::from_metadata(&metadata).unwrap();
        let dependency = |name: &str, dir: &str, kind| Dependency {
            name: name.into(),
            dir: dir.into(),
            kind,
        };
        assert_eq!(
            repo.members[0].dependencies,
            [
                dependency("specforge-ops", "crates/app/ops", DependencyKind::Normal),
                dependency(
                    "specforge-test",
                    "integrations/rust/specforge-test",
                    DependencyKind::Dev
                ),
                dependency("cc", "crates/leaf/cc", DependencyKind::Build),
            ]
        );
        let unknown = serde_json::json!({
            "workspace_root": "/r", "target_directory": "/r/target",
            "packages": [{"name": "a", "manifest_path": "/r/a/Cargo.toml",
                          "dependencies": [{"name": "b", "kind": "weird", "path": "/r/b"}]}],
        });
        assert_eq!(
            Repo::from_metadata(&unknown).unwrap_err(),
            "b: unknown dependency kind `weird`"
        );
    }
}
