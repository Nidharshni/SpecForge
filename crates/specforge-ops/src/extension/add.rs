//! `specforge add` and `specforge.add_extension`.

use super::candidate::LocalFile;
use super::lock_plan::{Fetch, Install, LockPlan, Refused, Verb};
use super::{Origin, builtin_name};
use crate::registry::{Publisher, Registry};
use crate::{OpError, Writes};
use specforge_common::codes;
use specforge_installed::{Installed, LockSource};
use specforge_protocol_types::package::SpecifierError;
use specforge_protocol_types::{PackageName, PackageRef};
use specforge_wasm::WasmRuntime;
use std::path::{Path, PathBuf};

/// Where an extension to add comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A builtin, embedded in the binary.
    Builtin(&'static str),
    /// A `.wasm` file on disk.
    Local(PathBuf),
    /// A registry package and the version asked for (`latest` when none).
    Registry(PackageRef),
    /// A git repository (not supported yet: E064).
    Git { url: String },
}

/// Read an `add` argument once (ADR 0036): a builtin name, a `.wasm` path
/// (or any `./`, `../` or `/` path), a `git+<url>`, or a package reference
/// `@scope/name[@requirement]`. Anything else is E054; a requirement that
/// is none is R-RES-003. No registry is asked for either.
pub fn parse(specifier: &str) -> Result<Source, OpError> {
    let specifier = specifier.trim();
    if specifier.is_empty() {
        return Err(
            OpError::diagnostic(codes::E054, "empty extension specifier")
                .with_suggestion(SPECIFIER_FORMS),
        );
    }
    if let Some(builtin) = builtin_name(specifier) {
        return Ok(Source::Builtin(builtin));
    }
    if let Some(url) = specifier.strip_prefix("git+") {
        // A `#rev` names a revision; a git source is not installable yet.
        let url = url.rfind('#').map_or(url, |hash| &url[..hash]);
        return Ok(Source::Git {
            url: url.to_string(),
        });
    }
    if specifier.ends_with(".wasm")
        || ["./", "../", "/"]
            .iter()
            .any(|prefix| specifier.starts_with(prefix))
    {
        return Ok(Source::Local(PathBuf::from(specifier)));
    }
    Ok(Source::Registry(PackageRef::parse(specifier)?))
}

/// What an `add` argument may be, as the suggestion of E054.
const SPECIFIER_FORMS: &str =
    "use a builtin's name, './local/path.wasm', 'git+https://...', or '@scope/name[@version]'";

/// E054 for a package reference that is not one, R-RES-003 for a
/// requirement that is none.
impl From<SpecifierError> for OpError {
    fn from(error: SpecifierError) -> Self {
        match error {
            SpecifierError::Requirement(_) => {
                OpError::diagnostic(codes::R_RES_003, error.to_string()).with_suggestion(
                    "use a version such as 1.2.0, a requirement such as ^1.0, ~2.3, 1.x or \
                     >=1.0.0, <2.0.0, or latest",
                )
            }
            _ => OpError::diagnostic(codes::E054, format!("invalid extension specifier: {error}"))
                .with_suggestion(SPECIFIER_FORMS),
        }
    }
}

/// How a key change in a signed package is decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trust {
    /// Refuse it (MCP, `--format json`): nobody can be asked.
    Refuse,
    /// Accept it (`--yes`).
    AssumeYes,
    /// Ask on the terminal, refusing when there is none.
    Prompt,
}

/// What to add, and where.
#[derive(Debug, Clone)]
pub struct AddRequest<'a> {
    pub root: &'a Path,
    pub source: Source,
    /// Accept a registry package with no publisher signature.
    pub allow_unsigned: bool,
    pub trust: Trust,
    /// Resolve and report what the add would do; write and download nothing. An extension
    /// already present answers as the real add would (`AlreadyPresent`, or
    /// `Builtin { changed: false }`).
    pub dry_run: bool,
}

/// What an add did (or, on a dry run, would do).
#[derive(Debug, Clone, PartialEq)]
pub enum AddOutcome {
    /// A builtin enabled; `changed` is false when it already was.
    /// `peers_enabled` are the required builtin peers enabled first.
    Builtin {
        name: &'static str,
        changed: bool,
        peers_enabled: Vec<&'static str>,
    },
    /// Installed under `.specforge/extensions/` and locked.
    Installed {
        name: String,
        version: String,
        sha256: String,
        /// How the registry's publisher was accepted; `None` for a local file.
        publisher: Option<Publisher>,
        origin: Origin,
    },
    /// Already installed at this version (or, from a local file, with
    /// these exact bytes) and enabled: nothing changed.
    AlreadyPresent { name: String, version: String },
    /// A dry run: what would be installed or enabled.
    Planned {
        name: String,
        version: Option<String>,
        origin: Origin,
    },
}

/// What an add did, and what it changed on disk.
#[derive(Debug, Clone, PartialEq)]
pub struct Added {
    pub outcome: AddOutcome,
    /// The files the add changed: `specforge.json` when an entry was added,
    /// and for an install the module and `specforge.lock` when their bytes
    /// changed. Nothing for a dry run or an extension already present. A
    /// trust pin the registry adapter keeps for the user is not one.
    pub writes: Writes,
    /// How many extensions `specforge.json` enables after the add (`0`
    /// without one): `extension_added`'s `totalExtensions`.
    pub extensions_enabled: usize,
}

/// Add an extension to the project at `req.root`.
///
/// - A builtin is enabled in `specforge.json`, after the builtins it
///   requires as non-optional peers.
/// - A local `.wasm` is validated by its handshake, copied under
///   `.specforge/extensions/`, locked with the version it declares and
///   `source: "local:<path>"`, and enabled.
/// - A registry package is resolved, downloaded, integrity- and
///   signature-checked, validated by its handshake, checked against the
///   ADR-0001 diamond gate, installed, locked and enabled.
///
/// An installed extension is enabled by its bare name, which the runtime
/// loads from the lock (ADR 0004 D3-b).
///
/// An install is all or nothing: the module, its lock entry and the
/// `specforge.json` entry are written as one change, and a failure at any
/// step puts every file back (the error's [`OpError::writes`] names what the
/// rollback could not, normally nothing).
pub fn add(
    req: &AddRequest,
    registry: &dyn Registry,
    runtime: &dyn WasmRuntime,
) -> Result<Added, OpError> {
    // The project must exist, with a config the writer can edit, before
    // anything is installed into it: the refusal `update` and `remove`
    // give an unusable specforge.json too (`config_invalid`).
    let config = crate::config::required(req.root)?.config;
    match &req.source {
        Source::Builtin(name) => add_builtin(req, &config, name, runtime),
        Source::Local(path) => {
            // A specforge.lock that can't be read refuses before anything is read.
            let installed = Installed::at(req.root);
            let plan = LockPlan::new(&installed, Verb::Install)?;
            let local = LocalFile::read(runtime, path)?;
            add_local(req, &config, &installed, plan, local)
        }
        Source::Registry(package) => add_from_registry(req, &config, registry, runtime, package),
        Source::Git { url } => Err(OpError::diagnostic(
            codes::E064,
            format!("git source '{url}' not yet supported"),
        )),
    }
}

/// `outcome`, with nothing written: the extensions `config` enables are
/// still the ones it enabled.
fn unchanged(outcome: AddOutcome, config: &specforge_common::ProjectConfig) -> Added {
    Added {
        outcome,
        writes: Writes::none(),
        extensions_enabled: config.extensions.len(),
    }
}

fn add_builtin(
    req: &AddRequest,
    config: &specforge_common::ProjectConfig,
    name: &'static str,
    runtime: &dyn WasmRuntime,
) -> Result<Added, OpError> {
    // An already-enabled extension is left exactly as it is, a dry run included.
    if super::enabled_builtins(config).contains(&name) {
        return Ok(unchanged(
            AddOutcome::Builtin {
                name,
                changed: false,
                peers_enabled: Vec::new(),
            },
            config,
        ));
    }
    if req.dry_run {
        return Ok(unchanged(
            AddOutcome::Planned {
                name: name.to_string(),
                version: None,
                origin: Origin::Builtin,
            },
            config,
        ));
    }
    // Required builtin peers come first, so they're enabled before the
    // extension that builds on them, in the one edit that enables it.
    let peers = super::required_builtins(runtime, name)?;
    let wanted: Vec<&str> = peers.iter().copied().chain([name]).collect();
    let enabled = crate::config::enable(req.root, &wanted)?;
    let mut writes = Writes::none();
    writes.record_if(
        !enabled.appended.is_empty(),
        req.root.join(crate::config::CONFIG_FILE),
    );
    Ok(Added {
        outcome: AddOutcome::Builtin {
            name,
            changed: enabled.appended.iter().any(|appended| appended == name),
            peers_enabled: peers
                .into_iter()
                .filter(|peer| enabled.appended.iter().any(|appended| appended == peer))
                .collect(),
        },
        writes,
        extensions_enabled: enabled.total,
    })
}

fn add_local(
    req: &AddRequest,
    config: &specforge_common::ProjectConfig,
    installed: &Installed,
    plan: LockPlan<'_>,
    local: LocalFile,
) -> Result<Added, OpError> {
    let install = Install::local(req.root, local)?;
    if let Some(present) = already_present(installed, config, install.name(), |e| {
        e.wasm_hash == install.sha256() && e.source.local_path().is_some()
    }) {
        return Ok(unchanged(present, config));
    }
    if req.dry_run {
        return Ok(unchanged(
            AddOutcome::Planned {
                name: install.name().to_string(),
                version: Some(install.version().to_string()),
                origin: Origin::Installed {
                    source: install.source().clone(),
                },
            },
            config,
        ));
    }
    commit(req.root, plan, install)
}

fn add_from_registry(
    req: &AddRequest,
    config: &specforge_common::ProjectConfig,
    registry: &dyn Registry,
    runtime: &dyn WasmRuntime,
    package: &PackageRef,
) -> Result<Added, OpError> {
    // A specforge.lock that can't be read refuses before a registry is asked.
    let installed = Installed::at(req.root);
    let plan = LockPlan::new(&installed, Verb::Install)?;
    let name = package.name.as_str();
    let version = super::resolve(registry, package)?;
    if let Some(present) = already_present(&installed, config, &package.name, |e| {
        e.version == version.to_string() && e.source.is_registry()
    }) {
        return Ok(unchanged(present, config));
    }
    if req.dry_run {
        return Ok(unchanged(
            AddOutcome::Planned {
                name: name.to_string(),
                version: Some(version.to_string()),
                origin: Origin::Installed {
                    source: LockSource::Registry,
                },
            },
            config,
        ));
    }
    let fetch = Fetch {
        registry,
        runtime,
        allow_unsigned: req.allow_unsigned,
        trust: req.trust,
    };
    commit(req.root, plan, fetch.install(&package.name, &version)?)
}

/// `AlreadyPresent` when the lock holds `name` as `same` accepts, its
/// binary is in place and is the one the lock pins, and `specforge.json`
/// enables it. A binary that changed after install is not present: adding
/// it again reinstalls it.
fn already_present(
    installed: &Installed,
    config: &specforge_common::ProjectConfig,
    name: &PackageName,
    same: impl Fn(&specforge_installed::LockFileEntry) -> bool,
) -> Option<AddOutcome> {
    let entry = installed.verified(name.as_str()).filter(|e| same(e))?;
    let enabled = config
        .extensions
        .iter()
        .any(|e| specforge_common::extension_entry_name(e) == name.as_str());
    enabled.then(|| AddOutcome::AlreadyPresent {
        name: name.to_string(),
        version: entry.version.clone(),
    })
}

/// Stage `install`, check the plan, and commit it with its `specforge.json` entry (its bare
/// name): one change that puts everything back when a step fails. A locked extension the new
/// version leaves with an unsatisfied peer refuses the install before anything is written
/// (ADR 0041).
fn commit<'a>(root: &Path, mut plan: LockPlan<'a>, install: Install<'a>) -> Result<Added, OpError> {
    let name = install.name().to_string();
    let outcome = AddOutcome::Installed {
        name: name.clone(),
        version: install.version().to_string(),
        sha256: install.sha256().to_string(),
        publisher: install.publisher().cloned(),
        origin: Origin::Installed {
            source: install.source().clone(),
        },
    };
    plan.stage(install);
    let mut total = 0;
    let writes = plan.check().map_err(Refused::first)?.commit_with(
        &root.join(crate::config::CONFIG_FILE),
        || {
            let enabled = crate::config::enable(root, &[name.as_str()])?;
            total = enabled.total;
            Ok(!enabled.appended.is_empty())
        },
    )?;
    Ok(Added {
        outcome,
        writes,
        extensions_enabled: total,
    })
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::{declaration_of, entry, greet, installed, project};
    use super::*;
    use crate::OpErrorKind;
    use crate::registry::testing::{MemoryRegistry, Published, declaration};
    use specforge_installed::LockFileEntry;
    use specforge_protocol_types::ExtensionDeclaration;
    use specforge_registry::PeerDependency;
    use specforge_test_macros::test as specforge_test;

    use crate::testing::{self, GREET, declaring, serving_builtin};
    use specforge_wasm::testing::InProcessRuntime;

    /// What the tests serve in process: `@sdk/greet` and `@test/probe`.
    fn runtime() -> InProcessRuntime {
        testing::candidates()
    }

    #[test]
    fn parse_names_each_source() {
        assert_eq!(
            parse("@specforge/product"),
            Ok(Source::Builtin("@specforge/product"))
        );
        assert_eq!(
            parse("ext/greet.wasm"),
            Ok(Source::Local(PathBuf::from("ext/greet.wasm")))
        );
        assert_eq!(
            parse("./ext/dir"),
            Ok(Source::Local(PathBuf::from("./ext/dir")))
        );
        assert_eq!(
            parse("@acme/tool@^1.2"),
            Ok(Source::Registry(
                PackageRef::parse("@acme/tool@^1.2").unwrap()
            ))
        );
        assert_eq!(
            parse("git+https://example.com/x.git"),
            Ok(Source::Git {
                url: "https://example.com/x.git".into()
            })
        );
        assert_eq!(parse("").unwrap_err().code, "E054");
        assert_eq!(parse("not-scoped").unwrap_err().code, "E054");
        assert_eq!(parse("@acme/").unwrap_err().code, "E054");
    }

    /// What `parse` makes of every input of plan 12's table (§2.2): a
    /// package reference, or the code of the refusal. Nothing reaches a
    /// registry that is not a scoped package name with a requirement.
    #[specforge_test(
        behavior = "parse_extension_specifier",
        verify = "each add argument reads as one extension source"
    )]
    fn parse_reads_each_input() {
        fn registry(reference: &str) -> Result<Source, String> {
            Ok(Source::Registry(PackageRef::parse(reference).unwrap()))
        }
        let cases: Vec<(&str, Result<Source, String>)> = vec![
            ("@acme/tool", registry("@acme/tool")),                 // I1
            ("@acme/tool@", Err("E054".into())),                    // I2
            ("@acme/tool@1.2.0", registry("@acme/tool@1.2.0")),     // I3
            ("@acme/tool@^1.2", registry("@acme/tool@^1.2")),       // I4
            ("@acme/tool@1.x", registry("@acme/tool@1.x")),         // I5
            ("@acme/tool@1.2", registry("@acme/tool@1.2")),         // I6
            ("@acme/tool@1.0.0/x", Err("R-RES-003".into())),        // I7
            ("@acme/tool@1.0.0?x=1", Err("R-RES-003".into())),      // I8
            ("foo@/bar", Err("E054".into())),                       // I9
            ("tool@1.0.0", Err("E054".into())),                     // I10
            ("tool", Err("E054".into())),                           // I11
            ("@acme/..", Err("E054".into())),                       // I12
            ("@acme/aa/bb", Err("E054".into())),                    // I13
            ("@acme/a/b", Err("E054".into())),                      // I14
            ("@a/x", registry("@a/x")),                             // I15
            ("@acme/T ool", Err("E054".into())),                    // I16
            ("Acme@1", Err("E054".into())),                         // I17
            ("@acme/tool@latest", registry("@acme/tool")),          // I18
            ("@acme/tool@*", registry("@acme/tool")),               // I18
            ("@acme/tool@>=1, <2", registry("@acme/tool@>=1, <2")), // I19
            ("@acme/tool@^bogus", Err("R-RES-003".into())),         // I20
            (
                "@acme/tool@2.0.0+build.1",
                registry("@acme/tool@2.0.0+build.1"),
            ), // I21
            ("@scope", Err("E054".into())),                         // I22
            (
                "@specforge/software",
                Ok(Source::Builtin("@specforge/software")),
            ), // I24
            (
                "git+https://h/r#v",
                Ok(Source::Git {
                    url: "https://h/r".into(),
                }),
            ), // I25
            (" @acme/tool ", registry("@acme/tool")),               // I26
        ];
        for (input, want) in cases {
            let got = parse(input).map_err(|error| error.code.to_string());
            assert_eq!(got, want, "{input:?}");
        }
    }

    #[specforge_test(
        behavior = "management_operations_over_the_project_view",
        verify = "add, update and remove refuse an unusable specforge.json with one refusal, before they write"
    )]
    fn an_unusable_config_is_refused_before_anything_is_installed() {
        use crate::config::testing::{UNUSABLE, files_under};

        for config in UNUSABLE {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("specforge.json"), config).unwrap();
            let before = files_under(dir.path());
            let read = specforge_common::read_project_config(dir.path());
            let refused = crate::config::refusal(&read.problems[0]);

            for source in [
                Source::Builtin("@specforge/product"),
                // A file that is not there: the config is refused first.
                Source::Local(dir.path().join("missing.wasm")),
                Source::Registry(PackageRef::parse("@acme/tool").unwrap()),
            ] {
                for dry_run in [false, true] {
                    let request = AddRequest {
                        root: dir.path(),
                        source: source.clone(),
                        allow_unsigned: false,
                        trust: Trust::Refuse,
                        dry_run,
                    };
                    let error = add(&request, &crate::registry::Unconfigured("add"), &runtime())
                        .unwrap_err();

                    assert_eq!(error, refused, "{config}: {source:?}");
                    assert_eq!(files_under(dir.path()), before, "{config}: {source:?}");
                }
            }
        }
    }

    #[specforge_test(
        behavior = "install_wasm_extension",
        verify = "an install of an extension whose binary changed after install replaces it"
    )]
    fn add_reinstalls_a_binary_that_changed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("specforge.json"),
            r#"{"name": "p", "version": "0.1.0", "extensions": []}"#,
        )
        .unwrap();
        let files = tempfile::tempdir().unwrap();
        let greet = files.path().join("greet.wasm");
        std::fs::write(&greet, GREET).unwrap();
        let request = AddRequest {
            root: dir.path(),
            source: Source::Local(greet),
            allow_unsigned: false,
            trust: Trust::Refuse,
            dry_run: false,
        };
        let unconfigured = crate::registry::Unconfigured("add");
        add(&request, &unconfigured, &runtime()).unwrap();
        let module = dir
            .path()
            .join(".specforge/extensions/@sdk/greet/extension.wasm");
        let pinned = std::fs::read(&module).unwrap();
        let mut changed = pinned.clone();
        changed.extend_from_slice(b"changed after install");
        std::fs::write(&module, &changed).unwrap();

        let added = add(&request, &unconfigured, &runtime()).unwrap();

        assert!(
            matches!(added.outcome, AddOutcome::Installed { .. }),
            "{:?}",
            added.outcome
        );
        assert_eq!(std::fs::read(&module).unwrap(), pinned);
        assert_eq!(added.writes.len(), 1, "only the module differed");

        // Now that it is the pinned binary again, adding it is a no-op.
        let again = add(&request, &unconfigured, &runtime()).unwrap();
        assert!(matches!(again.outcome, AddOutcome::AlreadyPresent { .. }));
    }

    /// A project directory whose `specforge.json` holds `config` verbatim.
    fn project_with(config: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("specforge.json"), config).unwrap();
        dir
    }

    const EMPTY_PROJECT: &str = r#"{"name": "p", "version": "0.1.0", "extensions": []}"#;

    fn local_request<'a>(root: &'a Path, file: &Path) -> AddRequest<'a> {
        AddRequest {
            root,
            source: Source::Local(file.to_path_buf()),
            allow_unsigned: false,
            trust: Trust::Refuse,
            dry_run: false,
        }
    }

    fn builtin_request<'a>(root: &'a Path, name: &'static str) -> AddRequest<'a> {
        AddRequest {
            root,
            source: Source::Builtin(name),
            allow_unsigned: false,
            trust: Trust::Refuse,
            dry_run: false,
        }
    }

    #[test]
    fn add_refuses_a_binary_that_claims_a_builtins_name() {
        use crate::config::testing::files_under;
        let dir = project_with(EMPTY_PROJECT);
        let file = tempfile::tempdir().unwrap();
        let wasm = file.path().join("product.wasm");
        std::fs::write(&wasm, b"\0asm impostor").unwrap();
        let impostor = runtime().binary(
            b"\0asm impostor",
            declaring("@specforge/product", "9.9.9", &[]),
        );
        let before = files_under(dir.path());

        let error = add(
            &local_request(dir.path(), &wasm),
            &crate::registry::Unconfigured("add"),
            &impostor,
        )
        .unwrap_err();

        assert_eq!(error.code, "extension_conflict");
        assert_eq!(error.kind, OpErrorKind::Conflict);
        assert_eq!(
            error.message,
            "the extension declares the name of the builtin '@specforge/product'"
        );
        assert_eq!(
            error.suggestion.as_deref(),
            Some("enable the builtin instead: specforge add @specforge/product")
        );
        assert_eq!(files_under(dir.path()), before);
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "add unresolvable extension rejects with diagnostic"
    )]
    fn a_local_file_add_cannot_read_is_e054() {
        use crate::config::testing::files_under;
        let dir = project_with(EMPTY_PROJECT);
        let registry = crate::registry::Unconfigured("add");

        let before = files_under(dir.path());
        let missing = dir.path().join("missing.wasm");
        let error = add(&local_request(dir.path(), &missing), &registry, &runtime()).unwrap_err();
        assert_eq!(error.code, "E054");
        assert_eq!(
            error.message,
            format!("file not found: {}", missing.display())
        );
        assert_eq!(files_under(dir.path()), before);

        let scratch = tempfile::tempdir().unwrap();
        let directory = scratch.path().join("a-directory");
        std::fs::create_dir(&directory).unwrap();
        let error = add(
            &local_request(dir.path(), &directory),
            &registry,
            &runtime(),
        )
        .unwrap_err();
        assert_eq!(error.code, "E054");
        assert!(
            error
                .message
                .starts_with(&format!("cannot read {}: ", directory.display())),
            "{}",
            error.message
        );
        assert_eq!(files_under(dir.path()), before);
    }

    #[test]
    fn a_local_file_that_is_no_extension_is_e028() {
        use crate::config::testing::files_under;
        let dir = project_with(EMPTY_PROJECT);
        let file = tempfile::tempdir().unwrap();
        let wasm = file.path().join("junk.wasm");
        std::fs::write(&wasm, b"not wasm").unwrap();
        let before = files_under(dir.path());

        let error = add(
            &local_request(dir.path(), &wasm),
            &crate::registry::Unconfigured("add"),
            &InProcessRuntime::new(),
        )
        .unwrap_err();

        assert_eq!(error.code, "E028");
        assert!(
            error
                .message
                .starts_with("not a loadable SpecForge extension:"),
            "{}",
            error.message
        );
        assert!(
            error
                .suggestion
                .as_deref()
                .unwrap()
                .contains("specforge-extension-sdk")
        );
        assert_eq!(files_under(dir.path()), before);
    }

    #[specforge_test(
        behavior = "install_wasm_extension",
        verify = "an install over an unreadable specforge.lock is refused before anything is written"
    )]
    fn an_unreadable_lock_refuses_before_the_local_file_is_read() {
        let dir = project_with(EMPTY_PROJECT);
        std::fs::write(dir.path().join("specforge.lock"), "not a lock {{{").unwrap();
        let missing = dir.path().join("missing.wasm");

        let error = add(
            &local_request(dir.path(), &missing),
            &crate::registry::Unconfigured("add"),
            &runtime(),
        )
        .unwrap_err();

        assert_eq!(error.code, "E033");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("specforge.lock")).unwrap(),
            "not a lock {{{"
        );
    }

    /// `runtime()`, also serving the builtins `@specforge/cargo-test` (which
    /// requires `@specforge/testing`) and `@specforge/testing`.
    fn runtime_with_cargo_test() -> InProcessRuntime {
        serving_builtin(
            serving_builtin(
                runtime(),
                "@specforge/cargo-test",
                declaring(
                    "@specforge/cargo-test",
                    "1.0.0",
                    &[("@specforge/testing", "^1.0", false)],
                ),
            ),
            "@specforge/testing",
            declaring("@specforge/testing", "1.0.0", &[]),
        )
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "add enables a builtin's required peers but not its optional ones"
    )]
    fn a_builtin_is_enabled_after_its_required_builtin_peers() {
        let dir = project_with(EMPTY_PROJECT);

        let added = add(
            &builtin_request(dir.path(), "@specforge/cargo-test"),
            &crate::registry::Unconfigured("add"),
            &runtime_with_cargo_test(),
        )
        .unwrap();

        assert_eq!(
            added.outcome,
            AddOutcome::Builtin {
                name: "@specforge/cargo-test",
                changed: true,
                peers_enabled: vec!["@specforge/testing"],
            }
        );
        assert_eq!(added.writes.names_under(dir.path()), ["specforge.json"]);
        assert_eq!(
            specforge_common::read_project_config(dir.path())
                .config
                .extensions,
            ["@specforge/testing", "@specforge/cargo-test"]
        );
        assert_eq!(added.extensions_enabled, 2);
    }

    #[test]
    fn extensions_enabled_counts_the_string_entries_after_the_add() {
        let dir =
            project_with(r#"{"name":"p","version":"0.1.0","extensions":[1,"@specforge/rust"]}"#);
        let registry = crate::registry::Unconfigured("add");
        let request = builtin_request(dir.path(), "@specforge/product");
        let runtime = serving_builtin(
            runtime(),
            "@specforge/product",
            declaring("@specforge/product", "1.0.0", &[]),
        );

        let added = add(&request, &registry, &runtime).unwrap();
        assert_eq!(added.extensions_enabled, 2);

        let again = add(&request, &registry, &runtime).unwrap();
        assert!(matches!(
            again.outcome,
            AddOutcome::Builtin { changed: false, .. }
        ));
        assert!(again.writes.is_empty());
        assert_eq!(again.extensions_enabled, 2);
    }

    #[specforge_test(
        behavior = "install_wasm_extension",
        verify = "add, init and publish read a candidate's declaration in the runtime their surface passes"
    )]
    fn a_candidate_is_read_in_the_runtime_the_caller_passes() {
        let dir = project_with(EMPTY_PROJECT);
        let files = tempfile::tempdir().unwrap();
        let greet = files.path().join("greet.wasm");
        std::fs::write(&greet, GREET).unwrap();
        let runtime = runtime();

        add(
            &local_request(dir.path(), &greet),
            &crate::registry::Unconfigured("add"),
            &runtime,
        )
        .unwrap();

        let handshakes: Vec<_> = runtime
            .calls()
            .into_iter()
            .filter(|call| call.extension == "__candidate" && call.export == "__handshake")
            .collect();
        assert_eq!(handshakes.len(), 1, "{handshakes:?}");
        assert!(
            !specforge_wasm::WasmRuntime::unload(&runtime, "__candidate"),
            "the candidate is left unloaded"
        );
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "a builtin whose declaration cannot be read is refused before anything is written"
    )]
    fn a_builtin_whose_declaration_cannot_be_read_is_refused() {
        use crate::config::testing::files_under;
        let dir = project_with(EMPTY_PROJECT);
        let before = files_under(dir.path());

        let error = add(
            &builtin_request(dir.path(), "@specforge/cargo-test"),
            &crate::registry::Unconfigured("add"),
            &InProcessRuntime::new(),
        )
        .unwrap_err();

        assert_eq!(error.code, "E028");
        assert!(
            error
                .message
                .starts_with("the builtin '@specforge/cargo-test' does not load: "),
            "{}",
            error.message
        );
        assert_eq!(files_under(dir.path()), before);
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "add enables the builtins a required builtin peer requires, dependencies first"
    )]
    fn add_enables_what_a_required_builtin_requires_first() {
        let dir = project_with(EMPTY_PROJECT);
        let registry = crate::registry::Unconfigured("add");
        let chain = serving_builtin(
            serving_builtin(
                serving_builtin(
                    runtime(),
                    "@specforge/formal",
                    declaring(
                        "@specforge/formal",
                        "1.0.0",
                        &[("@specforge/software", "^1.0", false)],
                    ),
                ),
                "@specforge/software",
                declaring(
                    "@specforge/software",
                    "1.0.0",
                    &[("@specforge/product", "^1.0", false)],
                ),
            ),
            "@specforge/product",
            declaring("@specforge/product", "1.0.0", &[]),
        );

        let added = add(
            &builtin_request(dir.path(), "@specforge/formal"),
            &registry,
            &chain,
        )
        .unwrap();

        assert_eq!(
            added.outcome,
            AddOutcome::Builtin {
                name: "@specforge/formal",
                changed: true,
                peers_enabled: vec!["@specforge/product", "@specforge/software"],
            }
        );
        assert_eq!(
            specforge_common::read_project_config(dir.path())
                .config
                .extensions,
            [
                "@specforge/product",
                "@specforge/software",
                "@specforge/formal"
            ]
        );
        assert_eq!(added.writes.names_under(dir.path()), ["specforge.json"]);

        // A cycle among required builtins stops the walk: the registry build
        // reports it, and the add succeeds.
        let cyclic_dir = project_with(EMPTY_PROJECT);
        let cycle = serving_builtin(
            serving_builtin(
                runtime(),
                "@specforge/formal",
                declaring(
                    "@specforge/formal",
                    "1.0.0",
                    &[("@specforge/software", "^1.0", false)],
                ),
            ),
            "@specforge/software",
            declaring(
                "@specforge/software",
                "1.0.0",
                &[("@specforge/formal", "^1.0", false)],
            ),
        );
        let added = add(
            &builtin_request(cyclic_dir.path(), "@specforge/formal"),
            &registry,
            &cycle,
        )
        .unwrap();
        assert!(matches!(
            added.outcome,
            AddOutcome::Builtin { changed: true, .. }
        ));
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "a required builtin peer the embedded builtin does not satisfy is refused before anything is written"
    )]
    fn a_required_builtin_peer_the_embedded_one_does_not_satisfy_is_refused() {
        use crate::config::testing::files_under;
        let dir = project_with(EMPTY_PROJECT);
        let before = files_under(dir.path());
        // formal wants software ^2.0; the software this specforge embeds is 1.0.0.
        let runtime = serving_builtin(
            serving_builtin(
                runtime(),
                "@specforge/formal",
                declaring(
                    "@specforge/formal",
                    "1.0.0",
                    &[("@specforge/software", "^2.0", false)],
                ),
            ),
            "@specforge/software",
            declaring("@specforge/software", "1.0.0", &[]),
        );

        let error = add(
            &builtin_request(dir.path(), "@specforge/formal"),
            &crate::registry::Unconfigured("add"),
            &runtime,
        )
        .unwrap_err();

        assert_eq!(error.code, "E027");
        assert!(
            error.message.contains("'@specforge/software' ^2.0")
                && error.message.contains("embeds @specforge/software 1.0.0"),
            "{}",
            error.message
        );
        assert_eq!(files_under(dir.path()), before);

        // init refuses the same way, before it writes anything.
        let target = tempfile::tempdir().unwrap();
        let extensions = vec!["@specforge/formal".to_string()];
        let error = crate::init::plan(
            &crate::init::Request {
                dir: &target.path().join("demo"),
                name: Some("demo"),
                version: crate::init::DEFAULT_VERSION,
                extensions: &extensions,
            },
            &runtime,
        )
        .unwrap_err();
        assert_eq!(error.code, "E027");
    }

    #[test]
    fn add_reads_specforge_json_once_before_its_edit() {
        let dir = project_with(
            r#"{"name": "p", "version": "0.1.0", "sentinel": {"keep": [1, 2]}, "extensions": []}"#,
        );
        let files = tempfile::tempdir().unwrap();
        let greet = files.path().join("greet.wasm");
        std::fs::write(&greet, GREET).unwrap();

        let added = add(
            &local_request(dir.path(), &greet),
            &crate::registry::Unconfigured("add"),
            &runtime(),
        )
        .unwrap();

        assert!(matches!(added.outcome, AddOutcome::Installed { .. }));
        assert_eq!(added.extensions_enabled, 1);
        let config: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join("specforge.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(config["sentinel"], serde_json::json!({"keep": [1, 2]}));
        assert_eq!(config["extensions"], serde_json::json!(["@sdk/greet"]));
    }

    #[cfg(unix)]
    #[specforge_test(
        behavior = "install_wasm_extension",
        verify = "a failed install puts back the binary, specforge.lock and specforge.json it changed"
    )]
    fn a_failed_builtin_edit_enables_no_peer() {
        use std::os::unix::fs::PermissionsExt;
        let dir = project_with(EMPTY_PROJECT);
        let config = dir.path().join("specforge.json");
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o444)).unwrap();
        let before = std::fs::read(&config).unwrap();

        let error = add(
            &builtin_request(dir.path(), "@specforge/cargo-test"),
            &crate::registry::Unconfigured("add"),
            &runtime_with_cargo_test(),
        )
        .unwrap_err();
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o644)).unwrap();

        assert_eq!(error.code, "config_write_failed", "{error:?}");
        assert!(error.writes.is_empty());
        assert_eq!(std::fs::read(&config).unwrap(), before);
    }

    #[test]
    fn a_missing_config_is_config_not_found_with_the_hint_to_init() {
        let dir = tempfile::tempdir().unwrap();
        let request = AddRequest {
            root: dir.path(),
            source: Source::Builtin("@specforge/product"),
            allow_unsigned: false,
            trust: Trust::Refuse,
            dry_run: false,
        };

        let error = add(&request, &crate::registry::Unconfigured("add"), &runtime()).unwrap_err();

        assert_eq!(error.code, "config_not_found");
        assert!(error.suggestion.unwrap().contains("specforge init"));
        assert!(!dir.path().join("specforge.json").exists());
    }

    /// A project enabling nothing yet, with `lock` locked.
    fn add_project(lock: Vec<LockFileEntry>) -> tempfile::TempDir {
        let dir = project(lock);
        std::fs::write(
            dir.path().join("specforge.json"),
            r#"{"name": "p", "version": "0.1.0", "extensions": []}"#,
        )
        .unwrap();
        dir
    }

    fn add_greet(root: &Path, registry: &MemoryRegistry) -> Result<AddOutcome, OpError> {
        add(
            &AddRequest {
                root,
                source: Source::Registry(PackageRef::parse("@sdk/greet@0.1.0").unwrap()),
                allow_unsigned: true,
                trust: Trust::Refuse,
                dry_run: false,
            },
            registry,
            &runtime(),
        )
        .map(|added| added.outcome)
    }

    fn with_peer(
        mut declaration: ExtensionDeclaration,
        name: &str,
        range: &str,
    ) -> ExtensionDeclaration {
        declaration
            .handshake
            .peer_dependencies
            .push(PeerDependency {
                name: name.to_string(),
                version: range.to_string(),
                optional: false,
            });
        declaration
    }

    #[specforge_test(
        behavior = "check_registry_reply",
        verify = "add refuses a package whose binary declares other than its published declaration"
    )]
    fn a_binary_that_declares_other_than_its_published_declaration_is_refused() {
        // R4/R5: the published declaration differs from the binary's, in
        // its handshake (another description) or in a category (no kinds).
        // A served declaration is trusted for nothing the binary doesn't
        // declare: the package is refused before anything is installed.
        let mut description = declaration_of(&greet());
        description.handshake.description = Some("Something else".to_string());
        for published in [description, {
            let mut kinds = declaration_of(&greet());
            kinds.entities.clear();
            kinds
        }] {
            let dir = add_project(Vec::new());
            let registry = MemoryRegistry::new().serving(Published::new(greet(), published));
            let err = add_greet(dir.path(), &registry).unwrap_err();
            assert!(err.is(crate::registry::METADATA_MISMATCH), "{err:?}");
            assert!(err.message.contains("@sdk/greet@0.1.0"), "{err:?}");
            assert!(
                !installed(dir.path(), "@sdk/greet").exists(),
                "nothing is installed"
            );
        }
        // The message names the first part that differs.
        let mut kinds = declaration_of(&greet());
        kinds.entities.clear();
        let dir = add_project(Vec::new());
        let registry = MemoryRegistry::new().serving(Published::new(greet(), kinds));
        let err = add_greet(dir.path(), &registry).unwrap_err();
        assert!(err.message.contains("another entities"), "{err:?}");
        // The published declaration equal to the binary's installs.
        let dir = add_project(Vec::new());
        let registry =
            MemoryRegistry::new().serving(Published::new(greet(), declaration_of(&greet())));
        add_greet(dir.path(), &registry).unwrap();
    }

    #[specforge_test(
        behavior = "check_registry_reply",
        verify = "the diamond gate decides on the published declaration's peers"
    )]
    fn the_diamond_gate_reads_the_published_declarations_peers() {
        // R4: the published declaration names a peer @acme/x ^2 that the
        // lock holds at 1.0.0. The gate refuses before the binary (which
        // declares no peer) is loaded.
        let dir = add_project(vec![entry("@acme/x", "1.0.0", "registry", &[])]);
        let registry = MemoryRegistry::new()
            .serving(Published::new(
                greet(),
                with_peer(declaration_of(&greet()), "@acme/x", "^2"),
            ))
            .serving(Published::new(
                b"\0asm x".to_vec(),
                declaration("@acme/x", "1.0.0", &[]),
            ));
        let err = add_greet(dir.path(), &registry).unwrap_err();
        assert!(!err.is(crate::registry::METADATA_MISMATCH), "{err:?}");
        assert!(err.code.starts_with("R-RES"), "{err:?}");
        assert!(err.message.contains("@acme/x"), "{err:?}");
    }

    #[test]
    fn adding_an_installed_exact_version_asks_the_registry_nothing() {
        let dir = add_project(Vec::new());
        let registry =
            MemoryRegistry::new().serving(Published::new(greet(), declaration_of(&greet())));
        let first = add_greet(dir.path(), &registry).unwrap();
        assert!(matches!(first, AddOutcome::Installed { .. }), "{first:?}");
        let asked = registry.asked();
        assert_eq!(asked.len(), 1, "{asked:?}");

        // The same version again: installed and enabled, so nothing is asked.
        let again = add_greet(dir.path(), &registry).unwrap();
        assert!(
            matches!(again, AddOutcome::AlreadyPresent { .. }),
            "{again:?}"
        );
        assert_eq!(
            registry.asked(),
            asked,
            "the registry was asked nothing more"
        );
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "an add that upgrades a locked package judges its peers against the lock as the add leaves it"
    )]
    fn an_add_that_upgrades_a_locked_package_judges_its_peers_without_its_previous_entry() {
        let dir = add_project(vec![
            entry("@x/b", "1.0.0", "registry", &[]),
            entry("@x/a", "1.0.0", "registry", &[("@x/b", "^1")]),
        ]);
        let registry = MemoryRegistry::new()
            .serving(Published::new(
                b"\0asm b1".to_vec(),
                declaration("@x/b", "1.0.0", &[]),
            ))
            .serving(Published::new(
                b"\0asm b2".to_vec(),
                declaration("@x/b", "2.0.0", &[]),
            ))
            .serving(Published::new(
                b"\0asm a2".to_vec(),
                declaration("@x/a", "2.0.0", &[("@x/b", "^2")]),
            ));
        let lock_before = std::fs::read(dir.path().join("specforge.lock")).unwrap();

        let error = add(
            &AddRequest {
                root: dir.path(),
                source: Source::Registry(PackageRef::parse("@x/a@2.0.0").unwrap()),
                allow_unsigned: true,
                trust: Trust::Refuse,
                dry_run: false,
            },
            &registry,
            &runtime(),
        )
        .unwrap_err();

        assert_eq!(error.code, "R-RES-006", "{error:?}");
        assert_eq!(
            error.message,
            "version diamond: '@x/a' requires peer '@x/b' ^2 but 1.0.0 is locked; @x/b 2.0.0 would satisfy every requirer"
        );
        assert_eq!(
            std::fs::read(dir.path().join("specforge.lock")).unwrap(),
            lock_before
        );
        assert_eq!(
            registry.asked(),
            [
                crate::registry::testing::Asked::Fetch(
                    PackageName::parse("@x/a").unwrap(),
                    specforge_protocol_types::package::Version::new(2, 0, 0)
                ),
                crate::registry::testing::Asked::Versions(PackageName::parse("@x/b").unwrap()),
            ]
        );
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "a version diamond names each requirer once"
    )]
    fn a_version_diamond_names_each_requirer_once() {
        let dir = add_project(vec![entry(
            "@acme/app",
            "1.0.0",
            "registry",
            &[("@sdk/greet", "^2.0")],
        )]);
        let registry =
            MemoryRegistry::new().serving(Published::new(greet(), declaration_of(&greet())));

        let error = add_greet(dir.path(), &registry).unwrap_err();

        assert_eq!(error.code, "R-RES-005", "{error:?}");
        assert!(
            error
                .message
                .starts_with("installing '@sdk/greet' 0.1.0 breaks '@acme/app': "),
            "{error:?}"
        );
        assert_eq!(
            error
                .message
                .matches("@acme/app wants @sdk/greet ^2.0")
                .count(),
            1,
            "{error:?}"
        );
        assert!(
            !installed(dir.path(), "@sdk/greet").exists(),
            "nothing is installed"
        );
    }

    #[specforge_test(
        behavior = "add_extension_to_existing_project",
        verify = "a dry run of an extension already present answers as the add would, whatever its source"
    )]
    fn a_dry_run_of_a_present_extension_answers_as_the_add_would() {
        let dry = |root, source| AddRequest {
            root,
            source,
            allow_unsigned: true,
            trust: Trust::Refuse,
            dry_run: true,
        };

        // A registry install, present: already present first.
        let dir = add_project(Vec::new());
        let registry =
            MemoryRegistry::new().serving(Published::new(greet(), declaration_of(&greet())));
        add_greet(dir.path(), &registry).unwrap();
        let source = Source::Registry(PackageRef::parse("@sdk/greet@0.1.0").unwrap());
        let outcome = add(&dry(dir.path(), source), &registry, &runtime())
            .unwrap()
            .outcome;
        assert_eq!(
            outcome,
            AddOutcome::AlreadyPresent {
                name: "@sdk/greet".into(),
                version: "0.1.0".into()
            }
        );

        // A local install, present: already present too.
        let dir = project_with(EMPTY_PROJECT);
        let files = tempfile::tempdir().unwrap();
        let wasm = files.path().join("greet.wasm");
        std::fs::write(&wasm, GREET).unwrap();
        let unconfigured = crate::registry::Unconfigured("add");
        add(&local_request(dir.path(), &wasm), &unconfigured, &runtime()).unwrap();
        let outcome = add(
            &dry(dir.path(), Source::Local(wasm)),
            &unconfigured,
            &runtime(),
        )
        .unwrap()
        .outcome;
        assert_eq!(
            outcome,
            AddOutcome::AlreadyPresent {
                name: "@sdk/greet".into(),
                version: "0.1.0".into()
            }
        );

        // A builtin, enabled: the no-op the real add returns.
        let dir =
            project_with(r#"{"name":"p","version":"0.1.0","extensions":["@specforge/product"]}"#);
        let source = Source::Builtin("@specforge/product");
        let before = crate::config::testing::files_under(dir.path());
        let added = add(&dry(dir.path(), source), &unconfigured, &runtime()).unwrap();
        assert_eq!(
            added.outcome,
            AddOutcome::Builtin {
                name: "@specforge/product",
                changed: false,
                peers_enabled: Vec::new()
            }
        );
        assert!(added.writes.is_empty());
        assert_eq!(crate::config::testing::files_under(dir.path()), before);
    }
}
