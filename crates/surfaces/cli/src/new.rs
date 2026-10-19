//! `specforge new <kind> <id> "<title>" --in <file> [--<field> <value>]… [--verify kind:text]…`: create an entity
//! (ADR 0151). The field flags are the kind's declared fields, read from the project's extensions at run time, so
//! `specforge new behavior --help` lists `--contract`, `--invariants`, … and a misspelt flag gets clap's "did you
//! mean". An open kind's members are set with `specforge set` afterwards. `--path` may stand anywhere.

use crate::edit::execute_on;
use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal};
use crate::project::ProjectArg;
use crate::terminal::{Terminal, err, outln};
use clap::{Arg, ArgAction, ArgMatches, Command};
use specforge_ops::authoring::edit::{self, Edit, FieldInfo, Input, Obligation};
use specforge_ops::view::ProjectView;
use specforge_ops::{OpError, OpErrorKind};
use specforge_protocol_types::FieldType;
use std::collections::BTreeMap;

/// The options `new` itself takes: a field with one of these names is set with `specforge set`.
const HOST_OPTIONS: [&str; 6] = ["in", "verify", "dry-run", "path", "format", "help"];

// `specforge new`'s command line.
#[derive(clap::Args)]
#[command(
    disable_help_flag = true,
    after_help = "The kind's fields are flags too: `specforge new <kind> --help` lists them."
)]
pub(crate) struct Args {
    /// The kind of entity (behavior, invariant, …)
    #[arg(required_unless_present = "help")]
    kind: Option<String>,

    /// The new entity's id
    #[arg(required_unless_present = "help")]
    id: Option<String>,

    /// The new entity's title
    #[arg(required_unless_present = "help")]
    title: Option<String>,

    /// The file, relative to the spec root; created when missing
    #[arg(long = "in", value_name = "FILE")]
    file: Option<String>,

    /// A `verify` obligation, as `kind:text` (or just `text`); repeat for several
    #[arg(long, value_name = "KIND:TEXT")]
    verify: Vec<String>,

    /// Print the diff and write nothing
    #[arg(long)]
    dry_run: bool,

    /// Print help (with a kind, that kind's field flags)
    #[arg(short = 'h', long = "help")]
    help: bool,

    #[command(flatten)]
    project: ProjectArg,

    #[command(flatten)]
    output: OutputArg,

    /// The kind's fields: `--<field> <value>`
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, num_args = 0.., value_name = "FIELDS")]
    rest: Vec<String>,
}

/// The kind's command line: its declared fields as flags, and the options `new` takes whatever their place.
fn command(kind: &str, fields: &[FieldInfo]) -> Command {
    let mut cmd = Command::new("new")
        .no_binary_name(true)
        .disable_help_flag(true)
        .bin_name(format!("specforge new {kind} <ID> <TITLE>"))
        .arg(
            Arg::new("in")
                .long("in")
                .value_name("FILE")
                .help("The file, relative to the spec root"),
        )
        .arg(
            Arg::new("verify")
                .long("verify")
                .action(ArgAction::Append)
                .value_name("KIND:TEXT")
                .help("A verify obligation"),
        )
        .arg(
            Arg::new("dry-run")
                .long("dry-run")
                .action(ArgAction::SetTrue)
                .help("Print the diff and write nothing"),
        )
        .arg(
            Arg::new("path")
                .long("path")
                .help("A path inside the project"),
        )
        .arg(
            Arg::new("format")
                .long("format")
                .value_parser(["human", "json"])
                .help("Output format"),
        );
    for field in fields
        .iter()
        .filter(|f| !HOST_OPTIONS.contains(&f.name.as_str()))
    {
        let mut arg = Arg::new(field.name.clone())
            .long(field.name.clone())
            .allow_hyphen_values(true)
            .action(ArgAction::Append)
            .help(field.description.clone());
        arg = match field.field_type {
            FieldType::ReferenceList => arg.value_delimiter(',').value_name("ID,…"),
            FieldType::Block => arg.value_name("NAME=TEXT"),
            _ => arg.value_name("VALUE"),
        };
        cmd = cmd.arg(arg);
    }
    cmd
}

/// The fields `matches` sets, as the inputs `Create` takes.
fn inputs(fields: &[FieldInfo], matches: &ArgMatches) -> Result<BTreeMap<String, Input>, OpError> {
    let mut out = BTreeMap::new();
    for field in fields
        .iter()
        .filter(|f| !HOST_OPTIONS.contains(&f.name.as_str()))
    {
        let Some(values) = matches.get_many::<String>(&field.name) else {
            continue;
        };
        let values: Vec<String> = values.cloned().collect();
        let input = match field.field_type {
            FieldType::ReferenceList | FieldType::StringList => Input::Many(values),
            FieldType::Block => {
                let mut entries = BTreeMap::new();
                for value in &values {
                    let Some((name, text)) = value.split_once('=') else {
                        return Err(OpError::new(
                            OpErrorKind::InvalidInput,
                            "invalid_edit",
                            format!("--{} takes NAME=TEXT, got '{value}'", field.name),
                        ));
                    };
                    entries.insert(name.trim().to_string(), text.to_string());
                }
                Input::Entries(entries)
            }
            _ => Input::One(values.last().cloned().unwrap_or_default()),
        };
        out.insert(field.name.clone(), input);
    }
    Ok(out)
}

impl Args {
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let format = self.output.format;
        let project = self.project.overridden_by(&self.rest);
        let compiled = match project.compile() {
            Ok(compiled) => compiled,
            Err(error) => return Refusal::of(format).report(terminal, &error),
        };
        let view = ProjectView::of(&compiled);
        let kind = self.kind.clone().unwrap_or_default();
        let fields = edit::fields_of(&view, &kind);
        if self.help {
            return help(&kind, &fields, terminal);
        }
        let matches = match command(&kind, &fields).try_get_matches_from(&self.rest) {
            Ok(matches) => matches,
            Err(e) => {
                err!(terminal, "{}", e.render());
                return Exit::Unjudged;
            }
        };
        let format = matches.get_one::<String>("format").map_or(format, |f| {
            if f == "json" {
                OutputFormat::Json
            } else {
                OutputFormat::Human
            }
        });
        let dry_run = self.dry_run || matches.get_flag("dry-run");
        match self.edit(&kind, &fields, &matches) {
            Ok(edit) => execute_on(&compiled, vec![edit], dry_run, format, terminal),
            Err(error) => Refusal::of(format).report(terminal, &error),
        }
    }

    fn edit(
        &self,
        kind: &str,
        fields: &[FieldInfo],
        matches: &ArgMatches,
    ) -> Result<Edit, OpError> {
        let file = self
            .file
            .clone()
            .or_else(|| matches.get_one::<String>("in").cloned())
            .ok_or_else(|| {
                OpError::new(
                    OpErrorKind::InvalidInput,
                    "invalid_edit",
                    "new needs --in <file>, relative to the spec root",
                )
            })?;
        let verify = self
            .verify
            .iter()
            .chain(matches.get_many::<String>("verify").into_iter().flatten())
            .map(|text| Obligation::parse(text))
            .collect();
        Ok(Edit::Create {
            kind: kind.to_string(),
            id: self.id.clone().unwrap_or_default(),
            title: self.title.clone().unwrap_or_default(),
            file,
            fields: inputs(fields, matches)?,
            verify,
        })
    }
}

/// `specforge new --help`, or the field flags of `kind`.
fn help(kind: &str, fields: &[FieldInfo], terminal: &Terminal) -> Exit {
    use clap::Args as _;
    let mut cmd = if fields.is_empty() {
        Args::augment_args(Command::new("specforge new").about("Create an entity"))
    } else {
        command(kind, fields)
    };
    outln!(terminal, "{}", cmd.render_long_help());
    Exit::Passed
}
