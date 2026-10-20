//! `specforge extension init|build|validate`: author an extension with the
//! SDK (ADR 0012). `init` scaffolds an SDK crate declaring the extension,
//! `build` builds its `wasm32-wasip2` component, and `validate` loads the
//! built component and reports its declaration as the registry build sees
//! it. No manifest file is written or read: the binary declares the
//! extension.

use crate::notes::Notes;
use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal};
use crate::terminal::{Terminal, errln, outln};
use serde_json::json;
use specforge_common::{Code, codes};
use specforge_ops::OpErrorKind;
use specforge_ops::extension::{LocalComponent, WASM_TARGET, target_dir};
use specforge_package::PackageName;
use std::path::{Path, PathBuf};

/// The diagnostic for an extension project that isn't there or isn't
/// built.
const NOT_BUILT: Code = codes::E040;

// `specforge extension`'s subcommands.
#[derive(clap::Subcommand)]
pub(crate) enum Action {
    /// Scaffold an extension crate written with the SDK
    Init(InitArgs),
    /// Build the extension's component (cargo build --release --target
    /// wasm32-wasip2)
    Build(BuildArgs),
    /// Load the built component and report its declaration's diagnostics
    Validate(ValidateArgs),
}

// `specforge extension init`'s command line.
#[derive(clap::Args)]
pub(crate) struct InitArgs {
    /// Extension name
    #[arg(long)]
    name: Option<String>,

    /// Directory for the new extension
    #[arg(long, default_value = ".")]
    dir: PathBuf,

    #[command(flatten)]
    output: OutputArg,
}

// `specforge extension build`'s command line.
#[derive(clap::Args)]
pub(crate) struct BuildArgs {
    /// The extension crate's directory
    #[arg(long, default_value = ".")]
    dir: PathBuf,

    #[command(flatten)]
    output: OutputArg,
}

// `specforge extension validate`'s command line.
#[derive(clap::Args)]
pub(crate) struct ValidateArgs {
    /// The extension crate's directory
    #[arg(long, default_value = ".")]
    dir: PathBuf,

    #[command(flatten)]
    output: OutputArg,
}

impl Action {
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        match self {
            Self::Init(args) => run_init(
                &args.dir,
                args.name.as_deref(),
                args.output.format,
                terminal,
            ),
            Self::Build(args) => run_build(&args.dir, args.output.format, terminal),
            Self::Validate(args) => run_validate(&args.dir, args.output.format, terminal),
        }
    }
}

fn run_init(path: &Path, name: Option<&str>, format: OutputFormat, terminal: &Terminal) -> Exit {
    let ext_name = name.unwrap_or("my-extension");
    let ext_dir = path.join(ext_name.rsplit('/').next().unwrap_or(ext_name));
    let declared = if ext_name.starts_with('@') {
        ext_name.to_string()
    } else {
        format!("@local/{ext_name}")
    };

    if let Err(why) = PackageName::parse(&declared) {
        return Refusal::of(format).coded(
            terminal,
            OpErrorKind::InvalidInput,
            codes::E065,
            why.to_string(),
        );
    }
    if ext_dir.exists() {
        return Refusal::of(format).coded(
            terminal,
            OpErrorKind::Conflict,
            codes::E065,
            format!("directory '{}' already exists", ext_dir.display()),
        );
    }
    if let Err(message) = crate::scaffold::scaffold(&ext_dir, &declared) {
        return Refusal::of(format).coded(terminal, OpErrorKind::Internal, codes::E066, message);
    }

    match format {
        OutputFormat::Json => {
            let output = json!({
                "status": "created",
                "name": declared,
                "short": crate::scaffold::short_name(&declared),
                "path": ext_dir.display().to_string(),
                "files": crate::scaffold::SCAFFOLDED,
            });
            terminal.json(&output);
        }
        OutputFormat::Human => {
            outln!(
                terminal,
                "Created extension {} at {}",
                declared,
                ext_dir.display()
            );
            for file in crate::scaffold::SCAFFOLDED {
                outln!(terminal, "  {file}");
            }
            outln!(terminal);
            outln!(terminal, "next steps:");
            outln!(
                terminal,
                "  specforge extension build --dir {}",
                ext_dir.display()
            );
            outln!(
                terminal,
                "  specforge extension validate --dir {}",
                ext_dir.display()
            );
        }
    }
    Exit::Passed
}

fn run_build(path: &Path, format: OutputFormat, terminal: &Terminal) -> Exit {
    if !path.join("Cargo.toml").exists() {
        return Refusal::of(format).coded(
            terminal,
            OpErrorKind::PreconditionFailed,
            NOT_BUILT,
            format!("no Cargo.toml found at {}", path.display()),
        );
    }

    // The component the host loads, built where the candidate module's reader looks
    // for it (`WASM_TARGET`, `target_dir`: the crate's own `target/`, whatever CARGO_TARGET_DIR says).
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let output = match std::process::Command::new(cargo)
        .args([
            "build",
            "--release",
            "--target",
            WASM_TARGET,
            "--target-dir",
        ])
        .arg(target_dir(path))
        .current_dir(path)
        .output()
    {
        Ok(output) => output,
        Err(e) => {
            return Refusal::of(format).coded(
                terminal,
                OpErrorKind::Internal,
                NOT_BUILT,
                format!("cannot run cargo: {e}"),
            );
        }
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let first_error = stderr.lines().find(|l| l.starts_with("error"));
        let message = format!(
            "cargo build --release --target {WASM_TARGET} failed in {}{}",
            path.display(),
            first_error.map(|l| format!(": {l}")).unwrap_or_default()
        );
        Notes::new(terminal, format).person(format_args!("{}", stderr.trim_end()));
        return Refusal::of(format).coded(terminal, OpErrorKind::Internal, NOT_BUILT, message);
    }
    let binary = match LocalComponent::read(
        path,
        crate::caller::of(false, format, terminal).containment(),
    ) {
        Ok(component) => component.path,
        Err(error) => return Refusal::of(format).report(terminal, &error),
    };

    match format {
        OutputFormat::Json => terminal.json(&json!({
            "status": "built",
            "path": path.display().to_string(),
            "component": binary.display().to_string(),
        })),
        OutputFormat::Human => outln!(terminal, "Built {}", binary.display()),
    }
    Exit::Passed
}

fn run_validate(path: &Path, format: OutputFormat, terminal: &Terminal) -> Exit {
    let runtime = specforge_component::ComponentRuntime::with_user_cache();
    let validated = match specforge_ops::extension::validate(path, &runtime) {
        Ok(validated) => validated,
        Err(error) => return Refusal::of(format).report(terminal, &error),
    };
    Notes::new(terminal, format).carried(&validated.diagnostics);

    let declaration = &validated.declaration;
    match format {
        OutputFormat::Json => terminal.json(&validated.document()),
        OutputFormat::Human if validated.valid() => outln!(
            terminal,
            "{} v{} declares a valid extension ({})",
            declaration.name(),
            declaration.version(),
            validated.binary.display()
        ),
        OutputFormat::Human => errln!(
            terminal,
            "{} v{}: the declaration has errors",
            declaration.name(),
            declaration.version()
        ),
    }
    Exit::of_verdict(validated.valid())
}
