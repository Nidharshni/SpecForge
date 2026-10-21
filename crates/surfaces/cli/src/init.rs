use crate::notes::Notes;
use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal, Written};
use crate::terminal::{Terminal, outln};
use specforge_ops::authoring::init;
use std::path::{Path, PathBuf};

// `specforge init`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Project name (defaults to directory name)
    #[arg(long)]
    name: Option<String>,

    /// Project version
    #[arg(long, default_value = specforge_ops::authoring::init::DEFAULT_VERSION)]
    version: String,

    /// Extensions to install: builtins and local .wasm files, several to a flag separated by commas
    #[arg(long, value_delimiter = ',')]
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

/// `specforge init`: the shared scaffold (`specforge_ops::authoring::init`), validated
/// and judged before anything is written, then presented.
fn init_at(
    path: &Path,
    name: Option<&str>,
    version: &str,
    extensions: &[String],
    format: OutputFormat,
    terminal: &Terminal,
) -> Exit {
    let request = init::Request {
        dir: path,
        name,
        version,
        extensions,
        caller: crate::caller::of(false, format, terminal),
    };
    let notes = Notes::new(terminal, format);
    let runtime = specforge_component::ComponentRuntime::with_user_cache();
    let outcome = match init::init(&request, &runtime) {
        Ok(outcome) => outcome,
        Err(error) => return Refusal::of(format).report(terminal, &error),
    };
    if let Some(enclosing) = &outcome.enclosing {
        notes.person(format_args!(
            "note: {} is inside the project at {}; the new project is separate",
            path.display(),
            enclosing.display()
        ));
    }

    // Every file init wrote: .gitignore when it lacked an entry, the
    // config, the starter, and each installed module and lock.
    let written = outcome.writes.named(Some(path));
    match format {
        OutputFormat::Json => {
            terminal.json(&Written::ran(outcome.document(), &outcome.writes, path));
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
