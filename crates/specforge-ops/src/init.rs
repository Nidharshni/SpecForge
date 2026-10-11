//! `specforge init` and `specforge.init`: one scaffold for both surfaces.
//!
//! [`plan`] validates everything and builds the files in memory; only
//! [`apply`] writes. `init` enables builtins, and installs local `.wasm`
//! extensions through the lock plan `add` uses, as one change (ADR 0004
//! D3-e, ADR 0041): it never writes an entry `specforge check` can't load.
//! A registry extension needs a configured registry, which a project that
//! doesn't exist yet can't have, so it is added afterwards with `specforge
//! add`.

use crate::extension::{self, Candidate, LocalFile, Source};
use crate::{OpError, OpErrorKind, Writes};
use serde::Serialize;
use serde_json::{Value, json};
use specforge_common::shape::Shape;
use specforge_common::validate_project_name;
use specforge_wasm::WasmRuntime;
use std::path::{Path, PathBuf};

/// The code an init refused with because the directory is already a project.
pub const PROJECT_EXISTS: &str = "project_exists";
/// The code an init refused with because the file it would write as the
/// starter is already there.
pub const STARTER_EXISTS: &str = "starter_exists";
/// The code for a project name init can't use.
pub const INVALID_NAME: &str = "invalid_name";

/// The version a new project gets when none is given: what `specforge init
/// --version` and `specforge.init`'s `version` default to, and what the
/// starter spec's `spec` block states.
pub const DEFAULT_VERSION: &str = "0.1.0";

/// Where the spec files go, relative to the project root.
pub const SPEC_ROOT: &str = "spec";
/// The starter file, relative to the project root.
pub const STARTER_FILE: &str = "spec/hello.spec";
/// The files `.gitignore` gets: the inference cache, the report `collect`
/// writes, and its working directory (installed extensions too).
pub const GITIGNORE: [&str; 3] = [
    "specforge-infer.json",
    "specforge-report.json",
    ".specforge/",
];

/// What to scaffold.
#[derive(Debug, Clone)]
pub struct Request<'a> {
    /// The new project's root.
    pub dir: &'a Path,
    /// The project name; the directory's name when absent.
    pub name: Option<&'a str>,
    /// The project version (`specforge init --version`; [`DEFAULT_VERSION`]
    /// unless given).
    pub version: &'a str,
    /// Extension specifiers; an entry may hold several, comma-separated.
    pub extensions: &'a [String],
}

/// Everything init will write, validated.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub name: String,
    pub version: String,
    /// The extensions enabled, in order: builtins by name, local installs by
    /// the name they declare.
    pub extensions: Vec<String>,
    pub config: Value,
    pub starter: String,
    /// Local `.wasm` files installed through the lock plan, as one change,
    /// each read once by the plan.
    pub installs: Vec<LocalFile>,
}

/// What init wrote.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub root: PathBuf,
    pub name: String,
    pub version: String,
    pub extensions: Vec<String>,
    /// The files init wrote: `specforge.json`, the starter, `.gitignore`
    /// when it lacked an entry, and each local install's module and lock.
    pub writes: Writes,
}

impl Outcome {
    /// The document both surfaces print.
    pub fn document(&self) -> InitDocument {
        InitDocument {
            project_root: self.root.display().to_string(),
            name: self.name.clone(),
            version: self.version.clone(),
            config_file: crate::config::CONFIG_FILE.to_string(),
            starter_file: STARTER_FILE.to_string(),
            extensions_installed: self.extensions.clone(),
        }
    }
}

/// What `specforge init --format json` and `specforge.init` print
/// (`McpInitResult`).
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct InitDocument {
    /// The new project's root, absolute and canonical.
    pub project_root: String,
    pub name: String,
    pub version: String,
    /// `specforge.json`, relative to `project_root`.
    pub config_file: String,
    /// [`STARTER_FILE`], relative to `project_root`.
    pub starter_file: String,
    /// The extensions enabled: builtins by name, local installs by the name
    /// they declare.
    pub extensions_installed: Vec<String>,
}

/// Validate `req` and build what init writes, writing nothing.
pub fn plan(req: &Request, runtime: &dyn WasmRuntime) -> Result<Plan, OpError> {
    if let Some(marker) = ["specforge.json", "specforge.spec"]
        .into_iter()
        .find(|marker| req.dir.join(marker).exists())
    {
        return Err(OpError::new(
            OpErrorKind::Conflict,
            PROJECT_EXISTS,
            format!("project already exists at {} ({marker})", req.dir.display()),
        ));
    }

    if req.dir.join(STARTER_FILE).exists() {
        return Err(starter_exists(req.dir));
    }

    let name = match req.name {
        Some(name) => name.to_string(),
        None => req
            .dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("my-project")
            .to_string(),
    };
    validate_project_name(&name).map_err(|why| invalid_name(&name, why))?;
    // The starter declares `spec "<id>"`: an ID outside the identifier
    // contract would fail its first check (E014).
    let spec_id = sanitize_entity_id(&name);
    if !(2..=60).contains(&spec_id.chars().count()) {
        return Err(invalid_name(
            &name,
            "it must be 2-60 characters, as the starter's spec ID must",
        ));
    }
    let version = req.version.to_string();

    let (extensions, installs) = extensions_of(req.extensions, runtime)?;
    // The lock plan the installs will be: refused here, before anything is written.
    extension::check_local(req.dir, &installs)?;
    // A builtin is enabled after the builtins it requires, as `add` does.
    let mut extensions = with_required_builtins(extensions, runtime)?;
    // Test obligations (`verify`) on software kinds come from
    // @specforge/testing (ADR 0002), so enabling software enables it too;
    // the project's test runners get the extensions that collect their
    // results.
    let has = |list: &[String], name: &str| list.iter().any(|e| e == name);
    if has(&extensions, "@specforge/software") && !has(&extensions, "@specforge/testing") {
        extensions.push("@specforge/testing".to_string());
    }
    if has(&extensions, "@specforge/testing") {
        for runner in detected_runners(req.dir) {
            if !has(&extensions, runner) {
                extensions.push(runner.to_string());
            }
        }
    }

    let starter = match starter_template(&extensions, &installs, runtime)? {
        Some(template) => template
            .replace("{project}", &spec_id)
            .replace("{version}", &version),
        None => structural_starter(&spec_id, &version),
    };
    // Whatever the extensions contribute, the file is written as the
    // formatter writes it, so `specforge format --check` accepts it.
    let starter = canonical(&starter, req.dir);
    let config = json!({
        "$schema": "https://specforge.dev/schema/specforge.json",
        "name": name,
        "version": version,
        "spec_root": SPEC_ROOT,
        "extensions": extensions,
    });
    Ok(Plan {
        name,
        version,
        extensions,
        config,
        starter,
        installs,
    })
}

/// Write `plan` into `dir`: `specforge.json`, the starter file, the
/// `.gitignore` entries, and the local installs as one change, without
/// reading the files again. Init only adds: the starter is created, never overwritten, and a
/// failed init puts back what was there before it ran (the lock's bytes, a
/// pre-existing `.specforge/`) and removes only what it wrote. Its error
/// reports nothing written.
pub fn apply(dir: &Path, plan: Plan) -> Result<Outcome, OpError> {
    let write_error = |what: &str, e: std::io::Error| {
        OpError::new(
            OpErrorKind::of_io(&e),
            "init_write_failed",
            format!("cannot write {what}: {e}"),
        )
    };
    let created_dir = !dir.exists();
    let spec_dir = dir.join(SPEC_ROOT);
    let created_spec_dir = !spec_dir.exists();
    std::fs::create_dir_all(&spec_dir).map_err(|e| write_error("the spec directory", e))?;
    let gitignore_path = dir.join(".gitignore");
    let gitignore_before = std::fs::read_to_string(&gitignore_path).ok();
    let lock_path = specforge_installed::lock_path(dir);
    let lock_before = std::fs::read(&lock_path).ok();
    let specforge_dir = dir.join(".specforge");
    let specforge_dir_existed = specforge_dir.exists();

    let mut writes = Writes::none();
    let mut wrote_config = false;
    let mut wrote_starter = false;
    let written = (|| -> Result<(), OpError> {
        wrote_config = true;
        crate::config::write(dir, &plan.config)?;
        writes.record(dir.join(crate::config::CONFIG_FILE));
        let appended = append_gitignore(&gitignore_path, gitignore_before.as_deref().unwrap_or(""))
            .map_err(|e| write_error(".gitignore", e))?;
        writes.record_if(appended, &gitignore_path);
        create_starter(dir, &plan.starter, &mut wrote_starter).map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => starter_exists(dir),
            _ => write_error(STARTER_FILE, e),
        })?;
        writes.record(dir.join(STARTER_FILE));
        writes.merge(extension::install_local(dir, plan.installs)?);
        Ok(())
    })();

    if let Err(mut error) = written {
        // Leave the directory as it was: remove what this init wrote, put
        // back what it replaced.
        if wrote_config {
            let _ = std::fs::remove_file(dir.join(crate::config::CONFIG_FILE));
        }
        if wrote_starter {
            let _ = std::fs::remove_file(dir.join(STARTER_FILE));
        }
        match &lock_before {
            Some(bytes) => {
                let _ = std::fs::write(&lock_path, bytes);
            }
            None => {
                let _ = std::fs::remove_file(&lock_path);
            }
        }
        if specforge_dir_existed {
            // What the failed install placed in the directory that was
            // already there, and its now-empty parents.
            let placed: Vec<PathBuf> = error
                .writes
                .paths()
                .filter(|path| path.starts_with(&specforge_dir))
                .map(Path::to_path_buf)
                .collect();
            for path in placed {
                let _ = std::fs::remove_file(&path);
                let mut parent = path.parent();
                while let Some(dir) =
                    parent.filter(|dir| dir.starts_with(&specforge_dir) && *dir != specforge_dir)
                {
                    if std::fs::remove_dir(dir).is_err() {
                        break;
                    }
                    parent = dir.parent();
                }
            }
        } else {
            let _ = std::fs::remove_dir_all(&specforge_dir);
        }
        match &gitignore_before {
            Some(text) => {
                let _ = std::fs::write(&gitignore_path, text);
            }
            None => {
                let _ = std::fs::remove_file(&gitignore_path);
            }
        }
        if created_spec_dir {
            let _ = std::fs::remove_dir(&spec_dir);
        }
        if created_dir {
            let _ = std::fs::remove_dir(dir);
        }
        // What was written is removed again.
        error.writes = Writes::none();
        return Err(error);
    }

    Ok(Outcome {
        root: dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf()),
        name: plan.name,
        version: plan.version,
        extensions: plan.extensions,
        writes,
    })
}

/// Create the starter file with `text`; `created` is set once the file
/// exists, so a rollback removes only a starter this init made.
fn create_starter(dir: &Path, text: &str, created: &mut bool) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(STARTER_FILE))?;
    *created = true;
    file.write_all(text.as_bytes())
}

/// The refusal of a directory whose starter file is already there.
fn starter_exists(dir: &Path) -> OpError {
    OpError::new(
        OpErrorKind::Conflict,
        STARTER_EXISTS,
        format!(
            "{STARTER_FILE} already exists in {}; init would overwrite it",
            dir.display()
        ),
    )
}

fn invalid_name(name: &str, why: &str) -> OpError {
    OpError::new(
        OpErrorKind::InvalidInput,
        INVALID_NAME,
        format!("invalid project name '{name}': {why}"),
    )
}

/// The extensions `specifiers` enable, in order, and the local files to
/// install, each read once. A builtin is enabled by name; a local `.wasm` is
/// checked by its handshake and enabled by the name it declares. Anything
/// else is refused before anything is written.
fn extensions_of(
    specifiers: &[String],
    runtime: &dyn WasmRuntime,
) -> Result<(Vec<String>, Vec<LocalFile>), OpError> {
    let mut extensions = Vec::new();
    let mut installs = Vec::new();
    for specifier in specifiers.iter().flat_map(|s| s.split(',')) {
        let specifier = specifier.trim();
        let unresolvable = |why: String| {
            OpError::new(
                OpErrorKind::ExtensionNotFound,
                extension::NOT_FOUND,
                format!("unresolvable extension '{specifier}': {why}"),
            )
        };
        let name = match extension::parse(specifier).map_err(|e| unresolvable(e.message))? {
            Source::Builtin(name) => name.to_string(),
            Source::Local(path) => {
                let local = LocalFile::read(runtime, &path).map_err(|e| unresolvable(e.message))?;
                let name = local.binary.candidate().name().to_string();
                installs.push(local);
                name
            }
            Source::Registry(_) | Source::Git { .. } => {
                let builtins: Vec<&str> = specforge_project::builtins().names().collect();
                return Err(unresolvable(
                    "init enables builtins and local .wasm files only".to_string(),
                )
                .with_suggestion(format!(
                    "init with builtins ({}), then configure a registry and run `specforge add {specifier}`",
                    builtins.join(", ")
                )));
            }
        };
        if !extensions.contains(&name) {
            extensions.push(name);
        }
    }
    Ok((extensions, installs))
}

/// `extensions`, each builtin after the builtins it requires, each once, in
/// order.
fn with_required_builtins(
    extensions: Vec<String>,
    runtime: &dyn WasmRuntime,
) -> Result<Vec<String>, OpError> {
    let mut enabled: Vec<String> = Vec::new();
    for name in extensions {
        if let Some(builtin) = extension::builtin_name(&name) {
            for peer in extension::required_builtins(runtime, builtin)? {
                if !enabled.iter().any(|e| e == peer) {
                    enabled.push(peer.to_string());
                }
            }
        }
        if !enabled.contains(&name) {
            enabled.push(name);
        }
    }
    Ok(enabled)
}

/// The starter template the extensions contribute: the one listed first
/// wins. A local file contributes under the name it declares, read by the
/// plan; a builtin is read from the binary, through `runtime`, only until a
/// template is found, and one that does not load refuses the init.
fn starter_template(
    extensions: &[String],
    installs: &[LocalFile],
    runtime: &dyn WasmRuntime,
) -> Result<Option<String>, OpError> {
    for name in extensions {
        let local = installs
            .iter()
            .find(|local| local.binary.candidate().name() == name);
        let template = match local {
            Some(local) => local
                .binary
                .candidate()
                .starter_template()
                .map(str::to_string),
            // A builtin that does not load refuses the init (E028).
            None => match extension::builtin_name(name) {
                Some(builtin) => Candidate::builtin(runtime, builtin)?
                    .starter_template()
                    .map(str::to_string),
                None => None,
            },
        };
        if template.is_some() {
            return Ok(template);
        }
    }
    Ok(None)
}

fn sanitize_entity_id(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// `starter`, formatted as the project at `dir` formats its files.
fn canonical(starter: &str, dir: &Path) -> String {
    let (config, _) = specforge_formatter::load_config(dir, dir);
    specforge_formatter::format_source(starter, &config).formatted
}

fn structural_starter(project_name: &str, version: &str) -> String {
    format!(
        r#"// {project_name} — starter spec file
//
// This file uses only structural syntax that the core compiler
// understands without any extensions. Install extensions to unlock
// domain-specific entity types.
//
// Try: specforge check

spec "{project_name}" {{
  version "{version}"
}}
"#
    )
}

/// Append the missing [`GITIGNORE`] entries to `path`, whose text is
/// `existing`.
fn append_gitignore(path: &Path, existing: &str) -> std::io::Result<bool> {
    let missing: Vec<&str> = GITIGNORE
        .into_iter()
        .filter(|entry| !existing.lines().any(|l| l.trim() == *entry))
        .collect();
    if missing.is_empty() {
        return Ok(false);
    }
    let mut text = existing.to_string();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    for entry in missing {
        text.push_str(entry);
        text.push('\n');
    }
    std::fs::write(path, text).map(|()| true)
}

/// Runner extensions for the test runners the project at `path` uses.
fn detected_runners(path: &Path) -> Vec<&'static str> {
    let mut runners = Vec::new();
    if path.join("Cargo.toml").is_file() {
        runners.push("@specforge/cargo-test");
    }
    if uses_vitest(path) {
        runners.push("@specforge/vitest");
    }
    runners
}

/// A vitest config file, or vitest among package.json's dependencies (a
/// project may configure it inside vite.config.*).
fn uses_vitest(path: &Path) -> bool {
    let config = std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| {
            e.file_name().to_str().is_some_and(|n| {
                n.starts_with("vitest.config.") || n.starts_with("vitest.workspace.")
            })
        });
    config
        || std::fs::read_to_string(path.join("package.json"))
            .ok()
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .is_some_and(|pkg| {
                ["dependencies", "devDependencies"]
                    .iter()
                    .any(|deps| pkg[deps].get("vitest").is_some())
            })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::declaring;
    use specforge_wasm::testing::InProcessRuntime;

    const A: &[u8] = b"\0asm in-process @x/a 1.0.0";
    const B: &[u8] = b"\0asm in-process @x/b 1.0.0";

    /// `@x/a` 1.0.0 wanting `@x/b` ^2, and `@x/b` 1.0.0.
    fn runtime() -> InProcessRuntime {
        InProcessRuntime::new()
            .binary(A, declaring("@x/a", "1.0.0", &[("@x/b", "^2", false)]))
            .binary(B, declaring("@x/b", "1.0.0", &[]))
    }

    /// `a.wasm` and `b.wasm` written in a scratch directory, and the directory to init.
    fn files() -> (tempfile::TempDir, Vec<String>, PathBuf) {
        let scratch = tempfile::tempdir().unwrap();
        std::fs::write(scratch.path().join("a.wasm"), A).unwrap();
        std::fs::write(scratch.path().join("b.wasm"), B).unwrap();
        let extensions = ["a.wasm", "b.wasm"]
            .map(|f| scratch.path().join(f).display().to_string())
            .to_vec();
        let dir = scratch.path().join("demo");
        (scratch, extensions, dir)
    }

    fn request<'a>(dir: &'a Path, extensions: &'a [String]) -> Request<'a> {
        Request {
            dir,
            name: Some("demo"),
            version: DEFAULT_VERSION,
            extensions,
        }
    }

    #[specforge_test_macros::test(
        behavior = "non_interactive_init",
        verify = "init refuses local extensions the lock plan refuses before it writes anything"
    )]
    fn init_refuses_local_extensions_the_lock_plan_refuses_before_writing() {
        let (_scratch, extensions, dir) = files();

        let error = plan(&request(&dir, &extensions), &runtime()).unwrap_err();

        assert_eq!(error.code, "E027", "{error:?}");
        assert_eq!(
            error.message,
            "extension '@x/a' requires peer dependency '@x/b' ^2 but version 1.0.0 is installed"
        );
        assert!(!dir.exists(), "nothing was written");
    }
}
