//! `specforge init` and `specforge.init`: one scaffold for both surfaces.
//!
//! [`init`] validates everything, reads its extension specifiers as `add` reads them and claims
//! them as `add` does (two that add one extension differently are refused, ADR 0098), and writes
//! one change: it enables builtins and installs local `.wasm` extensions through the lock plan
//! `add` uses (ADR 0004 D3-e, ADR 0041, ADR 0065), judged once before any file is written, so it
//! never writes an entry `specforge check` can't load. The modules, the lock, `specforge.json`,
//! `.gitignore` and the starter are put back together when any step fails. A registry extension
//! needs a configured registry, which a project that doesn't exist yet can't have, so it is added
//! afterwards with `specforge add`.

use crate::extension::{
    self, AddField, Also, Ask, Candidate, Claimed, Claims, Install, LocalFile, LockPlan, Source,
};
use crate::{OpError, OpErrorKind, Refusal, Writes};
use serde::Serialize;
use serde_json::{Value, json};
use specforge_common::shape::Shape;
use specforge_common::{PROJECT_FILE, ProjectRoot, validate_project_name};
use specforge_installed::Installed;
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

/// The field of an init's request a refusal can be about (ADR 0072).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitField {
    /// `Request::dir`: the directory to scaffold.
    Dir,
    /// `Request::name`: the project name.
    Name,
    /// `Request::extensions`: the extension specifiers.
    Extensions,
}

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

/// Scaffold the project `req` describes: validated first, its extension specifiers read as `add`
/// reads them (E054, R-RES-003, E040; a registry package is E063 and a git source E064, both
/// before anything is written) and claims them as `add` does (the same way twice is enabled
/// once, another way is `extension_conflict`, naming both), its lock plan judged once, then written as one change: the
/// installed modules, `specforge.lock`, `specforge.json`, the `.gitignore` entries and the
/// starter. Init only adds: the starter is created, never overwritten, and a failed init puts
/// back what was there before it ran and removes what it created. Its error reports nothing
/// written.
pub fn init(req: &Request, runtime: &dyn WasmRuntime) -> Result<Outcome, Refusal<InitField>> {
    if ProjectRoot::is_root(req.dir) {
        return Err(OpError::new(
            OpErrorKind::Conflict,
            PROJECT_EXISTS,
            format!(
                "project already exists at {} ({PROJECT_FILE})",
                req.dir.display()
            ),
        )
        .about(InitField::Dir));
    }

    if req.dir.join(STARTER_FILE).exists() {
        return Err(starter_exists(req.dir).about(InitField::Dir));
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
    validate_project_name(&name).map_err(|why| invalid_name(&name, why).about(InitField::Name))?;
    // The starter declares `spec "<id>"`: an ID outside the identifier
    // contract would fail its first check (E014).
    let spec_id = sanitize_entity_id(&name);
    if !(2..=60).contains(&spec_id.chars().count()) {
        return Err(invalid_name(
            &name,
            "it must be 2-60 characters, as the starter's spec ID must",
        )
        .about(InitField::Name));
    }
    let version = req.version.to_string();

    let specifiers: Vec<&str> = req
        .extensions
        .iter()
        .flat_map(|specifier| specifier.split(','))
        .map(str::trim)
        .collect();
    let wanted = if specifiers.is_empty() {
        Vec::new()
    } else {
        extension::specified(&specifiers).map_err(|refusal| {
            refusal.map_subject(|AddField::Extensions| Some(InitField::Extensions))
        })?
    };
    let several = wanted.len() > 1;
    let named = |at: usize, error: OpError| {
        let error = match wanted.get(at) {
            Some(wanted) => extension::named(several, &wanted.specifier, error),
            None => error,
        };
        error.about(InitField::Extensions)
    };

    // The lock plan the installs will be, `add`'s: an unreadable lock refuses here (E033), and the
    // project's peer set is judged before anything is written.
    let installed = Installed::at(req.dir);
    let mut plan = LockPlan::initializing(
        &installed,
        runtime,
        wanted.iter().any(|wanted| wanted.source.installs()),
    )?;
    // What each local file contributes to the starter, by the name it declares.
    let mut templates: Vec<(String, Option<String>)> = Vec::new();
    let mut claims = Claims::default();
    for (at, one) in wanted.iter().enumerate() {
        // A claim names both specifiers itself, as add's does.
        let claimed = |error: OpError| error.about(InitField::Extensions);
        if claims.repeated(one).is_some() {
            continue;
        }
        match &one.source {
            Source::Builtin(name) => {
                if claims.builtin(one, name).map_err(claimed)? == Claimed::First {
                    plan.enable(Ask(at), name);
                }
            }
            Source::Local(path) => {
                let local = LocalFile::read(runtime, path).map_err(|e| named(at, e))?;
                let candidate = local.binary.candidate();
                let declared = (
                    candidate.name().to_string(),
                    candidate.starter_template().map(str::to_string),
                );
                let install = Install::local(req.dir, local).map_err(|e| named(at, e))?;
                if claims.local(one, &install).map_err(claimed)? == Claimed::First {
                    templates.push(declared);
                    plan.install(Ask(at), install);
                }
            }
            Source::Registry(_) => return Err(named(at, no_registry(&one.specifier))),
            Source::Git { url } => return Err(named(at, extension::git_not_supported(url))),
        }
    }
    // Test obligations (`verify`) on software kinds come from @specforge/testing (ADR 0002), so
    // enabling software enables it too; the project's test runners get the extensions that
    // collect their results.
    plan.accompany("@specforge/software", "@specforge/testing");
    for runner in detected_runners(req.dir) {
        plan.accompany("@specforge/testing", runner);
    }
    let checked = plan.check().map_err(|refused| {
        let (Ask(at), error) = refused.decisive();
        named(at, error)
    })?;
    let extensions = checked.entries();

    let starter = match starter_template(&extensions, &templates, runtime)? {
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

    let dir = req.dir;
    let write_error = |what: &str, e: std::io::Error| {
        OpError::new(
            OpErrorKind::of_io(&e),
            "init_write_failed",
            format!("cannot write {what}: {e}"),
        )
    };
    let files = vec![
        Also {
            path: dir.join(crate::config::CONFIG_FILE),
            write: Box::new(move || {
                std::fs::create_dir_all(dir)
                    .map_err(|e| write_error("the project directory", e))?;
                crate::config::write(dir, &config)
            }),
        },
        Also {
            path: dir.join(".gitignore"),
            write: Box::new(move || {
                let path = dir.join(".gitignore");
                let existing = std::fs::read_to_string(&path).unwrap_or_default();
                append_gitignore(&path, &existing)
                    .map(|_| ())
                    .map_err(|e| write_error(".gitignore", e))
            }),
        },
        Also {
            path: dir.join(STARTER_FILE),
            write: Box::new(move || {
                std::fs::create_dir_all(dir.join(SPEC_ROOT))
                    .map_err(|e| write_error("the spec directory", e))?;
                create_starter(dir, &starter).map_err(|e| match e.kind() {
                    std::io::ErrorKind::AlreadyExists => starter_exists(dir),
                    _ => write_error(STARTER_FILE, e),
                })
            }),
        },
    ];
    let done = checked.finish(dir, files)?;

    Ok(Outcome {
        root: dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf()),
        name,
        version,
        extensions,
        writes: done.writes,
    })
}

/// Create the starter file with `text`.
fn create_starter(dir: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(STARTER_FILE))?;
    file.write_all(text.as_bytes())
}

/// E063: init enables builtins and installs local files only, since a project that does not
/// exist yet has no registry configured.
fn no_registry(specifier: &str) -> OpError {
    let builtins: Vec<&str> = specforge_project::builtins().names().collect();
    crate::registry::no_registry("init").with_suggestion(format!(
        "init with builtins ({}) and local .wasm files, then add a \"registries\" entry to specforge.json and run `specforge add {specifier}`",
        builtins.join(", ")
    ))
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

/// The starter template the extensions contribute: the one listed first
/// wins. A local file contributes under the name it declares, read when
/// the specifier was; a builtin is read from the binary, through `runtime`,
/// only until a template is found, and one that does not load refuses the
/// init.
fn starter_template(
    extensions: &[String],
    templates: &[(String, Option<String>)],
    runtime: &dyn WasmRuntime,
) -> Result<Option<String>, OpError> {
    for name in extensions {
        let template = match templates.iter().find(|(declared, _)| declared == name) {
            Some((_, template)) => template.clone(),
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

        let error = init(&request(&dir, &extensions), &runtime()).unwrap_err();

        assert_eq!(error.error.code, "E027", "{error:?}");
        // Named by the specifier that asked for it, as `add` names it among several.
        assert!(
            error.error.message.starts_with("adding '") && error.error.message.ends_with(
                "extension '@x/a' requires peer dependency '@x/b' ^2 but version 1.0.0 is installed"
            ),
            "{error:?}"
        );
        assert!(!dir.exists(), "nothing was written");
    }

    /// The add test of the same name holds `add` to it; init judges the same peer set (ADR 0065).
    #[test]
    fn init_refuses_a_required_builtin_peer_the_embedded_one_does_not_satisfy() {
        use crate::testing::{candidates, serving_builtin};
        // formal wants software ^2.0; the software this specforge embeds is 1.0.0. (Init also
        // enables testing beside software.)
        let runtime = serving_builtin(
            serving_builtin(
                serving_builtin(
                    candidates(),
                    "@specforge/formal",
                    declaring(
                        "@specforge/formal",
                        "1.0.0",
                        &[("@specforge/software", "^2.0", false)],
                    ),
                ),
                "@specforge/software",
                declaring("@specforge/software", "1.0.0", &[]),
            ),
            "@specforge/testing",
            declaring("@specforge/testing", "1.0.0", &[]),
        );
        let target = tempfile::tempdir().unwrap();
        let dir = target.path().join("demo");
        let extensions = vec!["@specforge/formal".to_string()];

        let error = init(&request(&dir, &extensions), &runtime).unwrap_err();

        assert_eq!(error.error.code, "E027");
        assert!(!dir.exists(), "nothing was written");
    }

    #[test]
    fn init_installs_a_crate_directory() {
        let scratch = tempfile::tempdir().unwrap();
        let release = scratch.path().join("b-ext/target/wasm32-wasip2/release");
        std::fs::create_dir_all(&release).unwrap();
        std::fs::write(
            scratch.path().join("b-ext/Cargo.toml"),
            "[package]\nname = \"b-ext\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(release.join("b_ext.wasm"), B).unwrap();
        let specifier = scratch.path().join("b-ext").display().to_string();

        let dir = scratch.path().join("demo");

        let outcome = init(&request(&dir, &[specifier]), &runtime()).unwrap();

        assert_eq!(outcome.extensions, ["@x/b"]);
        let lock = std::fs::read_to_string(dir.join("specforge.lock")).unwrap();
        assert!(lock.contains("release/b_ext.wasm"), "{lock}");
    }

    #[specforge_test_macros::test(
        invariant = "mcp_structured_error_responses",
        verify = "an operation states the request field each refusal is about, and a refusal of the project or the environment is about none"
    )]
    fn init_states_which_field_a_refusal_is_about() {
        let scratch = tempfile::tempdir().unwrap();
        let init_with = |dir: &str, name: Option<&str>, extensions: &[String]| {
            let dir = scratch.path().join(dir);
            let request = Request {
                dir: &dir,
                name,
                version: DEFAULT_VERSION,
                extensions,
            };
            let refusal = init(&request, &runtime()).unwrap_err();
            (refusal.error.code.to_string(), refusal.subject)
        };
        let specifiers = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();

        // The directory is already a project.
        let existing = scratch.path().join("existing");
        std::fs::create_dir_all(&existing).unwrap();
        std::fs::write(existing.join(PROJECT_FILE), "{}").unwrap();
        assert_eq!(
            init_with("existing", Some("demo"), &[]),
            (PROJECT_EXISTS.to_string(), Some(InitField::Dir))
        );
        // The name.
        assert_eq!(
            init_with("n1", Some("x"), &[]),
            (INVALID_NAME.to_string(), Some(InitField::Name))
        );
        // The specifiers.
        let extensions = InitField::Extensions;
        assert_eq!(
            init_with(
                "n2",
                Some("demo"),
                &specifiers(&["git+https://example.com/x"])
            ),
            ("E064".to_string(), Some(extensions))
        );
        assert_eq!(
            init_with("n3", Some("demo"), &specifiers(&["@a/b@!!"])),
            ("R-RES-003".to_string(), Some(extensions))
        );
        let not_wasm = scratch.path().join("notwasm.txt");
        std::fs::write(&not_wasm, "not wasm").unwrap();
        assert_eq!(
            init_with("n4", Some("demo"), &[not_wasm.display().to_string()]),
            ("E028".to_string(), Some(extensions))
        );
        // A registry package needs a registry no new project has: a refusal of the project.
        assert_eq!(
            init_with("n5", Some("demo"), &specifiers(&["@acme/x"])),
            ("E063".to_string(), None)
        );
    }
}
