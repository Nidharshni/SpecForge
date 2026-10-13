//! Builds every committed component (`specforge_component::committed::COMMITTED`: the builtins and the fixtures,
//! wasm32-wasip2) and keeps the vendored copies under `<crate_dir>/wasm/` — for the builtins, the bytes
//! `specforge-component` embeds — in step with their sources.
//!
//! ```text
//! build-builtins             build any blob missing from target/builtin-stage/
//! build-builtins --force     rebuild every blob into target/builtin-stage/
//! build-builtins --install   rebuild, vendor into <crate_dir>/wasm/, record inputs.json
//! build-builtins --check     fail if a vendored blob or any recorded input changed (no build)
//! build-builtins --verify    rebuild every blob and fail if one differs from its vendored bytes
//!                            (a blob vendored by another rustc is built, not compared)
//! ```
//!
//! A blob is built in a staged workspace ([`Stage`]), so its bytes depend only on
//! its sources and the toolchain, not on where the repository is checked out.

use gate::blob_inputs::{self, BlobRecord, WORKSPACE_INHERITED};
use serde_json::Value;
use specforge_component::committed::{COMMITTED, CommittedBlob};
use specforge_installed::hex_sha256;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

const TARGET: &str = "wasm32-wasip2";

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Missing,
    Force,
    Install,
    Check,
    Verify,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] => Mode::Missing,
        ["--force"] => Mode::Force,
        ["--install"] => Mode::Install,
        ["--check"] => Mode::Check,
        ["--verify"] => Mode::Verify,
        other => {
            eprintln!(
                "usage: build-builtins [--force | --install | --check | --verify] (got {other:?})"
            );
            std::process::exit(2);
        }
    };
    let root = std::env::var("CARGO_MANIFEST_DIR")
        .map(|m| {
            Path::new(&m)
                .join("..")
                .canonicalize()
                .expect("repo root resolves")
        })
        .expect("CARGO_MANIFEST_DIR is set when run via cargo xtask");

    let mut failed = false;
    for blob in COMMITTED {
        let ok = match mode {
            Mode::Check => check(&root, blob),
            Mode::Verify => verify(&root, blob),
            Mode::Missing | Mode::Force => build(&root, blob, mode == Mode::Force).is_some(),
            Mode::Install => {
                build(&root, blob, true).is_some_and(|built| install(&root, blob, &built))
            }
        };
        failed |= !ok;
    }
    if failed && mode == Mode::Check {
        eprintln!(
            "\nVendored blobs are stale. Re-vendor with\n  \
             cargo run -p xtask --bin revendor\n\
             (docs/agents/revendor.md) and commit what it prints."
        );
    }
    std::process::exit(i32::from(failed));
}

/// The directory under `target/builtin-stage/` that holds `blob`'s stage: the last segment of its crate directory
/// (unique over [`COMMITTED`]; a test asserts it).
fn stage_name(blob: &CommittedBlob) -> &'static str {
    blob.crate_dir.rsplit('/').next().unwrap_or(blob.crate_dir)
}

/// Where `blob` is built.
fn staged_blob(root: &Path, blob: &CommittedBlob) -> PathBuf {
    root.join(format!(
        "target/builtin-stage/{}/target/{TARGET}/release/{}",
        stage_name(blob),
        blob.artifact
    ))
}

fn build(root: &Path, committed: &CommittedBlob, force: bool) -> Option<PathBuf> {
    let name = committed.crate_dir;
    let artifact = committed.artifact;
    let blob = staged_blob(root, committed);
    if blob.exists() && !force {
        println!("[skip]   {name}: {} exists", blob.display());
        return Some(blob);
    }
    println!(
        "[build]  {name} for {TARGET}{}",
        if force { " (forced)" } else { "" }
    );
    match Stage::prepare(root, committed).and_then(|stage| stage.build(artifact)) {
        Ok(built) => {
            println!("[ok]     {name}");
            Some(built)
        }
        Err(e) => {
            eprintln!(
                "[fail]   {name}: {e}\nIf the target is missing, run: rustup target add {TARGET}"
            );
            None
        }
    }
}

/// A committed blob built so that nothing about the checkout reaches its bytes.
///
/// Each crate is its own workspace, so cargo hashes the absolute path of every path
/// dependency outside it (the SDK, the protocol types, ...) into `-C metadata`. That renames
/// the symbols and reorders generic code, and no rustc flag reaches it. The stage is a workspace
/// under `<repo>/target/builtin-stage/<last segment of the crate dir>/` holding the crate and every path package its
/// `wasm32-wasip2` build compiles, at their repo-relative paths: cargo then hashes their paths
/// relative to the stage, and passes them to rustc relative. The cargo home is remapped to
/// `/cargo` and the stage to `/specforge`.
struct Stage {
    /// Canonical.
    root: PathBuf,
    /// The crate directory from the repository root: `extensions/product`, `fixtures/greet-extension`.
    crate_dir: &'static str,
    /// The crate's package name, from `cargo metadata`.
    package: String,
    repo: PathBuf,
}

impl Stage {
    /// Lay out `blob`'s stage, after emptying it:
    /// - the path packages reached from the extension over normal and build dependencies, each
    ///   copied without `target/` (and the extension without `wasm/`);
    /// - every staged `Cargo.toml` without its dev-dependency tables: a staged path package is a
    ///   workspace member, and its dev-dependencies would join the resolution;
    /// - a `Cargo.toml` naming the extension as the member, with the root manifest's inherited
    ///   tables;
    /// - the extension's `Cargo.lock` and the toolchain file.
    fn prepare(repo: &Path, blob: &CommittedBlob) -> Result<Stage, String> {
        let crate_dir = blob.crate_dir;
        let dir = repo.join(format!("target/builtin-stage/{}", stage_name(blob)));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|e| format!("empty {}: {e}", dir.display()))?;
        }
        std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
        let root = std::fs::canonicalize(&dir).map_err(|e| e.to_string())?;

        let manifest = repo.join(crate_dir).join("Cargo.toml");
        let output = Command::new("cargo")
            .args([
                "metadata",
                "--format-version",
                "1",
                "--locked",
                "--filter-platform",
                TARGET,
                "--manifest-path",
            ])
            .arg(&manifest)
            .output()
            .map_err(|e| format!("spawn cargo metadata: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "cargo metadata failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let metadata: Value =
            serde_json::from_slice(&output.stdout).map_err(|e| format!("cargo metadata: {e}"))?;
        let (package, dirs) = path_packages(&metadata)?;

        for package_dir in &dirs {
            let rel = package_dir.strip_prefix(repo).map_err(|_| {
                format!(
                    "path package {} lies outside the repository",
                    package_dir.display()
                )
            })?;
            copy_package(package_dir, &root.join(rel), rel == Path::new(crate_dir))?;
        }

        let root_manifest = std::fs::read_to_string(repo.join("Cargo.toml"))
            .map_err(|e| format!("read Cargo.toml: {e}"))?;
        let workspace = format!(
            "[workspace]\nresolver = \"2\"\nmembers = [\"{crate_dir}\"]\n\n{}",
            blob_inputs::inherited_tables(&root_manifest)
        );
        std::fs::write(root.join("Cargo.toml"), workspace).map_err(|e| e.to_string())?;
        std::fs::copy(
            repo.join(crate_dir).join("Cargo.lock"),
            root.join("Cargo.lock"),
        )
        .map_err(|e| format!("copy Cargo.lock: {e}"))?;
        std::fs::copy(
            repo.join("rust-toolchain.toml"),
            root.join("rust-toolchain.toml"),
        )
        .map_err(|e| format!("copy rust-toolchain.toml: {e}"))?;
        Ok(Stage {
            root,
            crate_dir,
            package,
            repo: repo.to_path_buf(),
        })
    }

    /// `cargo build --release --target wasm32-wasip2 -p <package>` in the stage, with the
    /// rustflags this function sets and no `RUSTFLAGS`, `build.rustflags` or `CARGO_TARGET_DIR`
    /// of the user's reaching the blob. Then every package of the stage's lock must be in the
    /// extension's: cargo only prunes the dev-dependencies' entries. Returns the blob's path.
    fn build(&self, artifact: &str) -> Result<PathBuf, String> {
        let cargo_home = cargo_home()?;
        let flags = [
            format!("--remap-path-prefix={}=/cargo", cargo_home.display()),
            format!("--remap-path-prefix={}=/specforge", self.root.display()),
        ];
        let mut command = Command::new("cargo");
        command
            .args(["build", "--release", "--target", TARGET, "-p"])
            .arg(&self.package)
            .arg("--target-dir")
            .arg(self.root.join("target"))
            .current_dir(&self.root)
            .env("CARGO_ENCODED_RUSTFLAGS", flags.join("\x1f"))
            .env_remove("RUSTFLAGS")
            .env_remove("CARGO_TARGET_DIR")
            .env_remove("CARGO_BUILD_TARGET_DIR")
            .env_remove("CARGO_INCREMENTAL");
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("CARGO_PROFILE_") {
                command.env_remove(key);
            }
        }
        let status = command
            .status()
            .map_err(|e| format!("spawn cargo while building '{}': {e}", self.crate_dir))?;
        let blob = self
            .root
            .join(format!("target/{TARGET}/release/{artifact}"));
        if !status.success() || !blob.exists() {
            return Err(format!("build did not produce {}", blob.display()));
        }
        let staged = std::fs::read_to_string(self.root.join("Cargo.lock"))
            .map_err(|e| format!("read the stage's Cargo.lock: {e}"))?;
        let extension = std::fs::read_to_string(self.repo.join(self.crate_dir).join("Cargo.lock"))
            .map_err(|e| format!("read Cargo.lock: {e}"))?;
        lock_subset(&staged, &extension).map_err(|package| {
            format!(
                "the stage resolved {package} differently from {}/Cargo.lock",
                self.crate_dir
            )
        })?;
        Ok(blob)
    }
}

/// `file`'s path from the stage, `/`-separated: the stage mirrors the repository's layout.
fn stage_key(stage: &Path, file: &Path) -> Option<String> {
    file.strip_prefix(stage).ok().map(repo_key)
}

/// The cargo home: `$CARGO_HOME`, else `$HOME/.cargo`.
fn cargo_home() -> Result<PathBuf, String> {
    let home = match std::env::var_os("CARGO_HOME") {
        Some(home) => PathBuf::from(home),
        None => PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?).join(".cargo"),
    };
    std::fs::canonicalize(&home).map_err(|e| format!("cargo home {}: {e}", home.display()))
}

/// The crate's package name and the directory of each path package its build compiles
/// (normal and build dependencies, transitively), from `cargo metadata`'s output.
fn path_packages(metadata: &Value) -> Result<(String, Vec<PathBuf>), String> {
    let packages = metadata["packages"]
        .as_array()
        .ok_or("cargo metadata has no packages")?;
    let nodes = metadata["resolve"]["nodes"]
        .as_array()
        .ok_or("cargo metadata has no resolve")?;
    let root_id = metadata["resolve"]["root"]
        .as_str()
        .ok_or("cargo metadata has no root package")?;
    let find = |id: &str| packages.iter().find(|p| p["id"].as_str() == Some(id));
    let package = find(root_id)
        .and_then(|p| p["name"].as_str())
        .ok_or("the root package is not listed")?
        .to_string();

    let mut seen = BTreeSet::new();
    let mut queue = vec![root_id.to_string()];
    let mut dirs = BTreeSet::new();
    while let Some(id) = queue.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        if let Some(p) = find(&id)
            && p["source"].is_null()
            && let Some(manifest) = p["manifest_path"].as_str()
            && let Some(dir) = Path::new(manifest).parent()
        {
            dirs.insert(dir.to_path_buf());
        }
        let Some(node) = nodes.iter().find(|n| n["id"].as_str() == Some(id.as_str())) else {
            continue;
        };
        for dep in node["deps"].as_array().into_iter().flatten() {
            let compiled = dep["dep_kinds"]
                .as_array()
                .is_none_or(|kinds| kinds.iter().any(|k| k["kind"].as_str() != Some("dev")));
            if compiled && let Some(pkg) = dep["pkg"].as_str() {
                queue.push(pkg.to_string());
            }
        }
    }
    Ok((package, dirs.into_iter().collect()))
}

/// Copy a package into the stage without `target/` (and without `wasm/` for the blob's own crate), its
/// manifest without dev-dependencies. A `Cargo.toml` nested in the package (a fixture) is copied
/// as it is.
fn copy_package(from: &Path, to: &Path, extension: bool) -> Result<(), String> {
    copy_dir(from, to, extension, true)
}

fn copy_dir(from: &Path, to: &Path, extension: bool, top: bool) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| format!("create {}: {e}", to.display()))?;
    for entry in std::fs::read_dir(from).map_err(|e| format!("read {}: {e}", from.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let file_name = entry.file_name();
        if file_name == "target" || (top && extension && file_name == "wasm") {
            continue;
        }
        let target = to.join(&file_name);
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_dir() {
            copy_dir(&entry.path(), &target, extension, false)?;
        } else if top && file_name == "Cargo.toml" {
            let text = std::fs::read_to_string(entry.path()).map_err(|e| e.to_string())?;
            std::fs::write(&target, strip_dev_dependencies(&text)).map_err(|e| e.to_string())?;
        } else {
            std::fs::copy(entry.path(), &target)
                .map_err(|e| format!("copy {}: {e}", entry.path().display()))?;
        }
    }
    Ok(())
}

/// A manifest without its `[dev-dependencies]`, `[dev-dependencies.<x>]` and
/// `[target.<cfg>.dev-dependencies]` tables (each up to the next header). Line-based, like
/// [`blob_inputs::inherited_tables`].
fn strip_dev_dependencies(manifest: &str) -> String {
    let mut out = String::new();
    let mut skip = false;
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            let header = trimmed.trim_matches(|c| c == '[' || c == ']').trim();
            skip = header == "dev-dependencies"
                || header.starts_with("dev-dependencies.")
                || (header.starts_with("target.")
                    && (header.ends_with(".dev-dependencies")
                        || header.contains(".dev-dependencies.")));
        }
        if !skip {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// The `[[package]]` blocks of a lock file, each as its `name`, `version`, `source` and
/// `checksum` lines (not its dependency lists).
fn lock_packages(lock: &str) -> BTreeSet<String> {
    let mut blocks = BTreeSet::new();
    let mut current: Option<Vec<&str>> = None;
    for line in lock.lines() {
        if line.trim() == "[[package]]" {
            if let Some(lines) = current.take() {
                blocks.insert(lines.join("\n"));
            }
            current = Some(Vec::new());
        } else if let Some(lines) = current.as_mut() {
            let key = line.split('=').next().unwrap_or("").trim();
            if matches!(key, "name" | "version" | "source" | "checksum") {
                lines.push(line.trim());
            }
        }
    }
    if let Some(lines) = current {
        blocks.insert(lines.join("\n"));
    }
    blocks
}

/// Every package of `staged` is in `extension`; the error names the first that is not.
fn lock_subset(staged: &str, extension: &str) -> Result<(), String> {
    let extension = lock_packages(extension);
    for block in lock_packages(staged) {
        if !extension.contains(&block) {
            let field = |key: &str| {
                block
                    .lines()
                    .find_map(|l| l.strip_prefix(key))
                    .map(|v| v.trim_matches('"').to_string())
                    .unwrap_or_default()
            };
            return Err(format!("{} {}", field("name = "), field("version = ")));
        }
    }
    Ok(())
}

/// `--verify`: the vendored blob is what its sources build, byte for byte. A blob built by
/// another `rustc` is built, but not compared, and that is said.
fn verify(root: &Path, committed: &CommittedBlob) -> bool {
    let (name, artifact) = (committed.crate_dir, committed.artifact);
    let vendor_dir = root.join(name).join("wasm");
    let record = BlobRecord::read(root, name).ok();
    let this = rustc_version();
    // Always built, so a cold clone proves every blob builds; another rustc's bytes differ.
    let Some(built) = build(root, committed, true) else {
        return false;
    };
    if let Some(built_with) = record.as_ref().map(|r| r.built_with.as_str())
        && built_with != this
    {
        println!("[skip]   {name}: built with {built_with}; this is {this}");
        return true;
    }
    match (
        std::fs::read(&built),
        std::fs::read(vendor_dir.join(artifact)),
    ) {
        (Ok(a), Ok(b)) if a == b => {
            println!("[same]   {name}");
            true
        }
        (Ok(_), Ok(_)) => {
            eprintln!(
                "[differs] {name}: the vendored blob is not what its sources build; re-run --install"
            );
            false
        }
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("[fail]   {name}: {e}");
            false
        }
    }
}

fn install(root: &Path, committed: &CommittedBlob, built: &Path) -> bool {
    let (name, artifact) = (committed.crate_dir, committed.artifact);
    let vendor_dir = root.join(name).join("wasm");
    let result = (|| -> Result<usize, String> {
        let bytes = std::fs::read(built).map_err(|e| format!("read {}: {e}", built.display()))?;
        let stage = std::fs::canonicalize(
            root.join(format!("target/builtin-stage/{}", stage_name(committed))),
        )
        .map_err(|e| e.to_string())?;
        let inputs = collect_inputs(root, &stage, name, &built.with_extension("d"))?;
        let count = inputs.len();
        std::fs::create_dir_all(&vendor_dir).map_err(|e| e.to_string())?;
        std::fs::write(vendor_dir.join(artifact), &bytes).map_err(|e| e.to_string())?;
        BlobRecord {
            artifact: artifact.to_string(),
            artifact_sha256: hex_sha256(&bytes),
            built_with: rustc_version(),
            inputs,
        }
        .write(root, name)?;
        Ok(count)
    })();
    match result {
        Ok(count) => {
            println!("[vendor] {name}: {count} inputs recorded");
            true
        }
        Err(e) => {
            eprintln!("[fail]   {name}: {e}");
            false
        }
    }
}

fn check(root: &Path, committed: &CommittedBlob) -> bool {
    let (name, artifact) = (committed.crate_dir, committed.artifact);
    let vendor_dir = root.join(name).join("wasm");
    let record = match BlobRecord::read(root, name) {
        Ok(record) => record,
        Err(e) => {
            eprintln!("[stale]  {name}: {e}");
            return false;
        }
    };
    let inputs = &record.inputs;

    let mut problems = Vec::new();
    match std::fs::read(vendor_dir.join(artifact)) {
        Ok(bytes) if record.artifact_sha256 == hex_sha256(&bytes) => {}
        Ok(_) => problems.push(format!(
            "{artifact} differs from the blob --install recorded"
        )),
        Err(e) => problems.push(format!("{artifact}: {e}")),
    }
    for (key, recorded) in inputs {
        match input_hash(root, key) {
            Ok(current) if *recorded == current => {}
            Ok(_) => problems.push(format!("changed: {key}")),
            Err(_) => problems.push(format!("missing: {key}")),
        }
    }

    if problems.is_empty() {
        println!("[fresh]  {name}: blob and {} inputs match", inputs.len());
        true
    } else {
        eprintln!("[stale]  {name}:");
        for problem in problems {
            eprintln!("           {problem}");
        }
        false
    }
}

/// Every repo-local file rustc read for the artifact (from cargo's dep-info, taken in the
/// stage, which mirrors the repository's layout), the manifest owning each, the crate's
/// lockfile (pins registry crates), and the root manifest tables the shared crates inherit
/// from. Each is hashed as the repository holds it.
fn collect_inputs(
    root: &Path,
    stage: &Path,
    name: &str, // the crate directory
    dep_info: &Path,
) -> Result<BTreeMap<String, String>, String> {
    let text = std::fs::read_to_string(dep_info)
        .map_err(|e| format!("read dep-info {}: {e}", dep_info.display()))?;
    let mut keys = BTreeSet::new();
    for dep in parse_dep_info(&text) {
        let dep = std::fs::canonicalize(&dep).unwrap_or(dep);
        let Some(key) = stage_key(stage, &dep) else {
            continue;
        };
        // Build products are not inputs.
        if key.starts_with("target/") {
            continue;
        }
        keys.insert(key);
        if let Some(manifest) = owning_manifest(stage, &dep) {
            keys.insert(manifest);
        }
    }
    let own_lib = format!("{name}/src/lib.rs");
    if !keys.contains(&own_lib) {
        return Err(format!(
            "dep-info {} does not list {own_lib}; refusing to record an incomplete input set",
            dep_info.display()
        ));
    }
    keys.insert(format!("{name}/Cargo.lock"));
    keys.insert(WORKSPACE_INHERITED.to_string());

    keys.into_iter()
        .map(|key| {
            let hash = input_hash(root, &key).map_err(|e| format!("hash {key}: {e}"))?;
            Ok((key, hash))
        })
        .collect()
}

fn input_hash(root: &Path, key: &str) -> std::io::Result<String> {
    let path = root.join(blob_inputs::path_of(key));
    if key == WORKSPACE_INHERITED {
        let manifest = std::fs::read_to_string(path)?;
        return Ok(hex_sha256(
            blob_inputs::inherited_tables(&manifest).as_bytes(),
        ));
    }
    Ok(hex_sha256(&std::fs::read(path)?))
}

fn owning_manifest(root: &Path, file: &Path) -> Option<String> {
    let mut dir = file.parent();
    while let Some(d) = dir {
        if !d.starts_with(root) {
            return None;
        }
        let manifest = d.join("Cargo.toml");
        if manifest.is_file() {
            return manifest.strip_prefix(root).ok().map(repo_key);
        }
        dir = d.parent();
    }
    None
}

fn repo_key(rel: &Path) -> String {
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Cargo's dep-info is one Makefile rule, `target: dep dep ...`, with spaces
/// inside paths escaped as `\ `.
fn parse_dep_info(text: &str) -> Vec<PathBuf> {
    let Some((_, deps)) = text.lines().next().and_then(|line| line.split_once(": ")) else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    let mut current = String::new();
    let mut chars = deps.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&' ') => {
                chars.next();
                current.push(' ');
            }
            ' ' => {
                if !current.is_empty() {
                    paths.push(PathBuf::from(std::mem::take(&mut current)));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        paths.push(PathBuf::from(current));
    }
    paths
}

fn rustc_version() -> String {
    Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|v| v.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dep_info_splits_on_unescaped_spaces_only() {
        let text = "/r/out.wasm: /r/a.rs /r/dir\\ with\\ space/b.rs /r/c.json\n";
        assert_eq!(
            parse_dep_info(text),
            vec![
                PathBuf::from("/r/a.rs"),
                PathBuf::from("/r/dir with space/b.rs"),
                PathBuf::from("/r/c.json"),
            ]
        );
    }

    #[test]
    fn dev_dependency_tables_are_stripped_and_nothing_else() {
        let manifest = "[package]\nname = \"x\"\n\n[dependencies]\nserde = \"1\"\n\n\
             [dev-dependencies]\ninsta = \"1\"\n\n[dev-dependencies.foo]\npath = \"../foo\"\n\n\
             [target.'cfg(unix)'.dev-dependencies]\nlibc = \"0\"\n\n[features]\ndefault = []\n";
        assert_eq!(
            strip_dev_dependencies(manifest),
            "[package]\nname = \"x\"\n\n[dependencies]\nserde = \"1\"\n\n[features]\ndefault = []\n"
        );
        let plain = "[package]\nname = \"x\"\n\n[dependencies]\nserde = \"1\"\n";
        assert_eq!(strip_dev_dependencies(plain), plain);
    }

    #[test]
    fn a_stage_lock_must_be_a_subset_of_the_extensions() {
        let extension = "[[package]]\nname = \"a\"\nversion = \"1.0.0\"\nsource = \"registry+x\"\n\
             checksum = \"c\"\ndependencies = [\"b\"]\n\n[[package]]\nname = \"b\"\nversion = \"2.0.0\"\n";
        let pruned = "[[package]]\nname = \"b\"\nversion = \"2.0.0\"\n";
        assert_eq!(lock_subset(pruned, extension), Ok(()));
        let moved = "[[package]]\nname = \"b\"\nversion = \"2.1.0\"\n";
        assert_eq!(lock_subset(moved, extension), Err("b 2.1.0".to_string()));
    }

    #[test]
    fn a_stage_mirrors_the_repo_layout() {
        let stage = Path::new("/r/target/builtin-stage/product");
        assert_eq!(
            stage_key(stage, &stage.join("crates/x/src/lib.rs")),
            Some("crates/x/src/lib.rs".to_string())
        );
        assert_eq!(stage_key(stage, Path::new("/elsewhere/lib.rs")), None);
    }

    /// The committed blobs that are builtins. The fixtures record their inputs at the round's re-vendor; the
    /// stage builds them like the builtins, but these two lists are the builtins' (ADR 0039, 0050).
    fn builtins() -> impl Iterator<Item = &'static CommittedBlob> {
        COMMITTED
            .iter()
            .filter(|blob| blob.crate_dir.starts_with("extensions/"))
    }

    #[test]
    fn every_stage_name_is_unique() {
        let mut names = BTreeSet::new();
        for blob in COMMITTED {
            assert!(
                names.insert(stage_name(blob)),
                "{}: stage name {} is taken",
                blob.name,
                stage_name(blob)
            );
        }
    }

    /// The crates a builtin blob compiles besides its own sources: the guest wire, the SDK and its
    /// macros, and the coverage rule (ADR 0039, 0050). A host-only module in one of them, or a new
    /// path dependency of a guest, shows here as an input outside this list.
    const GUEST_CRATES: &[&str] = &[
        "crates/specforge-protocol-types/",
        "crates/specforge-extension-sdk/",
        "crates/specforge-extension-sdk-macros/",
        "crates/specforge-coverage/",
    ];

    #[test]
    fn a_builtin_compiles_only_its_own_sources_and_the_guest_crates() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        for builtin in builtins() {
            let name = builtin.crate_dir;
            let record = BlobRecord::read(&root, name).unwrap();
            let own = format!("{name}/");
            let inputs = &record.inputs;
            for key in inputs.keys() {
                assert!(
                    key == WORKSPACE_INHERITED
                        || key.starts_with(&own)
                        || GUEST_CRATES.iter().any(|guest| key.starts_with(guest)),
                    "{name} compiles {key}, outside its own sources and the guest crates"
                );
            }
            assert!(
                !inputs
                    .keys()
                    .any(|k| k.ends_with("/peers.rs") || k.ends_with("/package.rs")),
                "{name}: the package and peer rules are host-side"
            );
        }
    }

    /// A blob compiles no test code: a test module is declared out of line (`#[cfg(test)] mod
    /// tests;`), which a release build never reads, so a test-only edit re-vendors nothing; and
    /// the SDK's test helpers are behind its `testing` feature (ADR 0050).
    #[test]
    fn a_builtin_compiles_no_test_code() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        for builtin in builtins() {
            let name = builtin.crate_dir;
            let record = BlobRecord::read(&root, name).unwrap();
            for key in record.inputs.keys() {
                assert_ne!(
                    key, "crates/specforge-extension-sdk/src/testing.rs",
                    "{name} compiles the SDK's test helpers"
                );
                let Ok(text) = std::fs::read_to_string(root.join(key)) else {
                    continue;
                };
                let lines: Vec<&str> = text.lines().collect();
                for pair in lines.windows(2) {
                    let inline_module = pair[1]
                        .trim_start()
                        .strip_prefix("pub ")
                        .map_or_else(|| pair[1].trim_start(), str::trim_start);
                    assert!(
                        !(pair[0].trim() == "#[cfg(test)]"
                            && inline_module.starts_with("mod ")
                            && inline_module.trim_end().ends_with('{')),
                        "{name}: {key} holds an inline test module `{}`; declare it out of line",
                        pair[1].trim()
                    );
                }
            }
        }
    }
}
