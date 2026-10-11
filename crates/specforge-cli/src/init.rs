use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal, Written};
use crate::terminal::{Terminal, errln, outln};
use specforge_common::find_project_root;
use specforge_ops::init;
use std::path::{Path, PathBuf};

// `specforge init`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Project name (defaults to directory name)
    #[arg(long)]
    name: Option<String>,

    /// Project version
    #[arg(long, default_value = specforge_ops::init::DEFAULT_VERSION)]
    version: String,

    /// Extensions to install
    #[arg(long)]
    extensions: Vec<String>,

    #[command(flatten)]
    output: OutputArg,
}

impl Args {
    /// `specforge init` in the current directory.
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        init_at(
            &path,
            self.name.as_deref(),
            &self.version,
            &self.extensions,
            self.output.format,
            terminal,
        )
    }
}

/// `specforge init`: the shared scaffold (`specforge_ops::init`), planned
/// and validated before anything is written, then presented.
fn init_at(
    path: &Path,
    name: Option<&str>,
    version: &str,
    extensions: &[String],
    format: OutputFormat,
    terminal: &Terminal,
) -> Exit {
    // A project further up doesn't block init: the new one is separate, and
    // commands run inside it resolve to it (the nearest wins).
    let request = init::Request {
        dir: path,
        name,
        version,
        extensions,
    };
    let runtime = specforge_component::ComponentRuntime::with_user_cache();
    let plan = match init::plan(&request, &runtime) {
        Ok(plan) => plan,
        Err(error) => return Refusal::of(format).report(terminal, &error),
    };
    if let Some(enclosing) = path.parent().and_then(find_project_root)
        && format != OutputFormat::Json
    {
        errln!(
            terminal,
            "note: {} is inside the project at {}; the new project is separate",
            path.display(),
            enclosing.display()
        );
    }
    let outcome = match init::apply(path, plan) {
        Ok(outcome) => outcome,
        Err(error) => return Refusal::of(format).report(terminal, &error),
    };

    // Every file init wrote: .gitignore when it lacked an entry, the
    // config, the starter, and each installed module and lock.
    let written = outcome.writes.names_under(path);
    match format {
        OutputFormat::Json => {
            terminal.json(&Written::of(outcome.document(), written));
        }
        OutputFormat::Human => {
            outln!(
                terminal,
                "Initialized project '{}' at {}",
                outcome.name,
                path.display()
            );
            for file in &written {
                outln!(terminal, "  {file}");
            }
            if outcome.extensions.is_empty() {
                outln!(
                    terminal,
                    "\nNo extensions installed. Add one with: specforge add <extension>"
                );
            }
            outln!(terminal, "\nNext steps:");
            outln!(terminal, "  specforge check    # validate your spec files");
            outln!(terminal, "  specforge export   # export the graph");
            if outcome
                .extensions
                .iter()
                .any(|e| e == "@specforge/cargo-test" || e == "@specforge/vitest")
            {
                outln!(
                    terminal,
                    "  specforge collect  # run the tests and record what they prove"
                );
            }
        }
    }
    Exit::Passed
}
