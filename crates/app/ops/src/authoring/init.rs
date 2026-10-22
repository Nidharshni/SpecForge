//! `specforge init` and `specforge.init`: one scaffold for both surfaces.
//!
//! [`init`] validates everything, reads, claims and asks for its extension specifiers through
//! `extension::names`, as `add` does (ADR 0105, ADR 0134; two that add one extension differently are
//! refused, ADR 0098), and writes
//! one change: it enables builtins and installs local `.wasm` extensions through the lock plan
//! `add` uses (ADR 0004 D3-e, ADR 0041, ADR 0065), judged once before any file is written, so it
//! never writes an entry `specforge check` can't load. The modules, the lock, `specforge.json`,
//! `.gitignore` and the starter are put back together when any step fails. A registry extension
//! needs a configured registry, which a project that doesn't exist yet can't have, so it is added
//! afterwards with `specforge add`.

use crate::authoring::format::{Configs, Forms, Place as FormatPlace};
use crate::change;
use crate::extension::{self, Also, LockPlan, Names, Place};
use crate::{Caller, Containment, OpError, OpErrorKind, Refusal, Writes};
use serde::Serialize;
use serde_json::{Value, json};
use specforge_common::shape::Shape;
use specforge_common::{PROJECT_FILE, ProjectRoot, validate_project_name};
use specforge_graph::FieldForms;
use specforge_graph::field_form::spell;
use specforge_installed::Installed;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry::Disk;
use specforge_wasm::WasmRuntime;
use std::path::{Path, PathBuf};

mod enclosing;

/// The code an init refused with because the directory is already a project.
pub const PROJECT_EXISTS: &str = "project_exists";
/// The code an init refused with because the file it would write as the
/// starter is already there.
pub const STARTER_EXISTS: &str = "starter_exists";
/// The code an init refused with because the laid-out starter does not parse.
pub const STARTER_UNPARSED: &str = "starter_unparsed";
/// The code an init refused with because the project its directory lies inside would read other
/// sources once it is a project (ADR 0197).
pub const INSIDE_PROJECT_SOURCES: &str = "inside_project_sources";
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
/// The starter when no extension contributes one: structural syntax only, a template filled as an extension's is.
const STRUCTURAL_STARTER: &str = r#"// {project} — starter spec file
//
// This file uses only structural syntax that the core compiler
// understands without any extensions. Install extensions to unlock
// domain-specific entity types.
//
// Try: specforge check

spec "{project}" {
  version "{version}"
}
"#;
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
    /// Extension specifiers, one per string (nothing splits a string); a relative local path is
    /// read from `dir`.
    pub extensions: &'a [String],
    /// What the init may reach: a local extension is read, every file written, through
    /// `caller.containment()` (ADR 0104). Init asks nobody.
    pub caller: Caller<'a>,
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
    /// The project the new one was created inside (its root, canonical), when there is one (ADR 0197).
    pub enclosing: Option<PathBuf>,
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

/// Scaffold the project `req` describes: validated first, its extension specifiers read, claimed and
/// asked for through `extension::names`, as `add` does (E054, R-RES-003, E040; a registry package is E063 and a git source E064, both
/// before anything is written; the same way twice is enabled
/// once, another way is `extension_conflict`, naming both), its lock plan judged once, then written as one change: the
/// installed modules, `specforge.lock`, `specforge.json`, the `.gitignore` entries and the
/// starter. Init only adds: the starter is created, never overwritten, and a failed init puts
/// back what was there before it ran and removes what it created. A directory whose files the project
/// it lies inside reads as its sources, or that is where that project reads them from, is refused
/// (`inside_project_sources`). Its error reports nothing
/// written. A name whose starter spec ID `check` would refuse (E014, or E013 over the kinds of the
/// extensions it enables) is refused once those are read. A starter that does not parse is refused
/// (`starter_unparsed`), naming the extension whose template it is.
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
    let enclosing = enclosing::judged(&crate::containment::landing(req.dir))
        .map_err(|e| e.about(InitField::Dir))?;

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
    let spec_id = sanitize_entity_id(&name);
    let version = req.version.to_string();

    // The extensions it names, as `add` reads them: a relative local path is read from the
    // directory init creates.
    let about = |error: OpError| error.about(InitField::Extensions);
    let names = Names::read(
        req.extensions,
        Place {
            root: req.dir,
            base: req.dir,
            containment: req.caller.containment(),
        },
    )
    .map_err(about)?;

    // The lock plan the installs will be, `add`'s; `names` refuses a lock it cannot write before it reads a file
    // (E033), and the project's peer set is judged before anything is written.
    let installed = Installed::at(req.dir);
    let document = json!({
        "$schema": "https://specforge.dev/schema/specforge.json",
        "name": name,
        "version": version,
        "spec_root": SPEC_ROOT,
    });
    let mut plan = LockPlan::initializing(&installed, runtime, req.caller, document);
    // Test obligations (`verify`) on software kinds come from @specforge/testing (ADR 0002), so
    // enabling software enables it too; the project's test runners get the extensions that
    // collect their results. The plan reads its companions when it is checked.
    plan.accompany("@specforge/software", "@specforge/testing");
    for runner in detected_runners(req.dir, &req.caller.containment().narrowed(req.dir)) {
        plan.accompany("@specforge/testing", runner);
    }
    // A registry package needs a registry, which a project that doesn't exist yet can't have.
    let (checked, answers) = names
        .ask(
            plan,
            runtime,
            &mut |specifier, _| Err(no_registry(specifier)),
            &mut |install| install.name().to_string(),
        )
        .map_err(about)?;
    // What each local file declares, by the name it declares.
    let locals: Vec<(String, Box<ExtensionDeclaration>)> = answers
        .into_iter()
        .filter_map(|answer| match answer {
            extension::Answer::Installed {
                recorded,
                declaration: Some(declaration),
                ..
            } => Some((recorded, declaration)),
            extension::Answer::Installed { .. }
            | extension::Answer::Duplicate { .. }
            | extension::Answer::Builtin { .. } => None,
        })
        .collect();
    let extensions = checked.entries();

    // The registry build its first `check` loads, made before the project exists: it decides whether the
    // starter's spec ID is one `check` refuses (E014, E013), whatever is enabled (ADR 0189).
    let declared = declarations(&extensions, locals, runtime)?;
    let template = declared.iter().find_map(|declaration| {
        let text = declaration.handshake.starter_template.clone()?;
        Some((declaration.handshake.name.clone(), text))
    });
    let build = specforge_registry::build_registries(declared);
    match specforge_registry::refused_id(&spec_id, build.kinds()) {
        None => {}
        Some(specforge_registry::IdRefusal::Length) => {
            return Err(invalid_name(
                &name,
                "it must be 2-60 characters, as the starter's spec ID must",
            )
            .about(InitField::Name));
        }
        Some(specforge_registry::IdRefusal::Reserved) => {
            return Err(invalid_name(
                &name,
                &format!(
                    "the starter's spec ID '{spec_id}' is a reserved word (E013): a structural keyword or the \
                     keyword of a kind an extension it enables declares"
                ),
            )
            .with_suggestion(format!(
                "name the project otherwise, e.g. `{spec_id}_spec`"
            ))
            .about(InitField::Name));
        }
    }

    // Each placeholder stands for the inside of a `.spec` string: `{project}` first, so a version that
    // writes `{project}` stays literal (ADR 0226).
    let (origin, text) = match &template {
        Some((name, text)) => (Some(name.as_str()), text.as_str()),
        None => (None, STRUCTURAL_STARTER),
    };
    let starter = text
        .replace("{project}", &spell::escaped(&spec_id))
        .replace("{version}", &spell::escaped(&version));
    // Whatever the extensions contribute, the file is written as the
    // formatter writes it, so `specforge format --check` accepts it, with the
    // configuration init's bound reads (ADR 0169) and the field forms of the build above.
    let starter = canonical(
        &starter,
        req.dir,
        &req.caller.containment().narrowed(req.dir).disk(),
        build.field_forms(),
    );
    // What `check` starts with, asked before anything is written (ADR 0226): a template's text is the
    // extension's, so a starter that does not parse is refused about the extensions.
    let parsed = specforge_parser::parse(&starter, STARTER_FILE);
    if let Some(first) = parsed.errors.first() {
        return Err(
            starter_unparsed(origin, first, parsed.errors.len()).about(InitField::Extensions)
        );
    }
    let dir = req.dir;
    let files = vec![
        Also {
            path: dir.join(".gitignore"),
            stage: Box::new(move |file, change| {
                let before = file
                    .read_if_there()
                    .map_err(|e| change::unreadable(Path::new(".gitignore"), &e))?;
                if let Some(after) = gitignore_with(before.as_deref().unwrap_or_default()) {
                    change.stage(file, before, after);
                }
                Ok(())
            }),
        },
        Also {
            path: dir.join(STARTER_FILE),
            stage: Box::new(move |file, change| {
                let there = file
                    .read_if_there()
                    .map_err(|e| change::unreadable(Path::new(STARTER_FILE), &e))?;
                if there.is_some() {
                    return Err(starter_exists(dir));
                }
                change.stage(file, None, starter.into_bytes());
                Ok(())
            }),
        },
    ];
    let done = checked.writing(files).finish()?;

    Ok(Outcome {
        root: dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf()),
        name,
        version,
        extensions,
        writes: done.writes,
        enclosing,
    })
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

/// The refusal of a starter that does not parse: `origin` is the extension whose template it is.
fn starter_unparsed(
    origin: Option<&str>,
    first: &specforge_parser::ParseError,
    count: usize,
) -> OpError {
    let whose = origin.map_or_else(
        || "the starter".to_string(),
        |name| format!("the starter template '{name}' declares"),
    );
    let error = OpError::new(
        OpErrorKind::InvalidInput,
        STARTER_UNPARSED,
        format!(
            "{whose} does not parse (line {}: {}; {count} error{}); nothing was written",
            first.span.start_line,
            first.message,
            if count == 1 { "" } else { "s" },
        ),
    );
    match origin {
        Some(name) => error.with_suggestion(format!(
            "list another extension first in --extensions, or init without '{name}' and add it afterwards"
        )),
        None => error,
    }
}

fn invalid_name(name: &str, why: &str) -> OpError {
    OpError::new(
        OpErrorKind::InvalidInput,
        INVALID_NAME,
        format!("invalid project name '{name}': {why}"),
    )
}

/// The declarations of the extensions init enables, in `specforge.json` order: a local install's as `names`
/// read it, a builtin's from this binary through `runtime` (E028 when one does not load).
fn declarations(
    extensions: &[String],
    mut locals: Vec<(String, Box<ExtensionDeclaration>)>,
    runtime: &dyn WasmRuntime,
) -> Result<Vec<ExtensionDeclaration>, OpError> {
    let mut declared = Vec::with_capacity(extensions.len());
    for name in extensions {
        if let Some(at) = locals.iter().position(|(declared, _)| declared == name) {
            declared.push(*locals.swap_remove(at).1);
        } else if let Some(builtin) = extension::builtin_name(name) {
            declared.push(extension::builtin_declaration(runtime, builtin)?.declaration);
        }
    }
    Ok(declared)
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

/// `starter`, formatted as the project at `dir` formats its files, its values in the field `forms` of the
/// extensions it enables. The forms are handed over, not looked up by place: `dir` is no project yet (ADR 0189).
fn canonical(starter: &str, dir: &Path, disk: &dyn Disk, forms: &FieldForms) -> String {
    let config =
        Configs::new(Forms::none(), disk).layout(FormatPlace::InProject { root: dir, dir });
    specforge_formatter::format_source(starter, &config, forms).formatted
}

/// `.gitignore` with the missing [`GITIGNORE`] entries appended to `existing`, its bytes (a file that is not
/// UTF-8 is appended to, never rewritten); `None` when it holds every entry.
fn gitignore_with(existing: &[u8]) -> Option<Vec<u8>> {
    let held = String::from_utf8_lossy(existing);
    let missing: Vec<&str> = GITIGNORE
        .into_iter()
        .filter(|entry| !held.lines().any(|l| l.trim() == *entry))
        .collect();
    if missing.is_empty() {
        return None;
    }
    let mut bytes = existing.to_vec();
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        bytes.push(b'\n');
    }
    for entry in missing {
        bytes.extend_from_slice(entry.as_bytes());
        bytes.push(b'\n');
    }
    Some(bytes)
}

/// Runner extensions for the test runners the project at `path` uses.
fn detected_runners(path: &Path, containment: &Containment) -> Vec<&'static str> {
    let mut runners = Vec::new();
    if path.join("Cargo.toml").is_file() {
        runners.push("@specforge/cargo-test");
    }
    if uses_vitest(path, containment) {
        runners.push("@specforge/vitest");
    }
    runners
}

/// A vitest config file, or vitest among package.json's dependencies (a
/// project may configure it inside vite.config.*).
fn uses_vitest(path: &Path, containment: &Containment) -> bool {
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
        || containment
            .resolve(&path.join("package.json"))
            .ok()
            .and_then(|held| held.read_to_string().ok())
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

    fn request<'a>(dir: &'a Path, extensions: &'a [String]) -> Request<'a> {
        Request {
            dir,
            name: Some("demo"),
            version: DEFAULT_VERSION,
            extensions,
            caller: crate::testing::caller(),
        }
    }

    /// add's test of the same name holds add to the same peer set (ADR 0065).
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
                caller: crate::testing::caller(),
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
