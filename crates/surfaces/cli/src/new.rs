//! `specforge new <kind> <id> "<title>" --in <file> [--<field> <value>]… [--verify kind:text]…`: create an entity
//! (ADR 0151). The command line is one clap command ([`command`]) read twice with the same builder: once to find the
//! kind, once with the kind's declared fields as flags, which the project's extensions declare at run time, so
//! `specforge new behavior --help` lists `--contract`, `--invariants`, … and a misspelt flag gets clap's "did you
//! mean". The edit, the help and every refusal come from the second reading. An open kind's members are set with
//! `specforge set` afterwards. Every option, `--path` included, may stand anywhere.

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
use std::path::PathBuf;

/// What `specforge new` does: its line in `specforge --help` and its help's first line.
pub(crate) const ABOUT: &str = "Create an entity";

const KIND: &str = "new:kind";
const ID: &str = "new:id";
const TITLE: &str = "new:title";
const FIELDS: &str = "new:fields";
const IN: &str = "in";
const VERIFY: &str = "verify";
const DRY_RUN: &str = "dry-run";
const HELP: &str = "help";

/// `specforge new`'s words, as the clap tree takes them: every one, unread, for [`command`] to read once the
/// project's kinds are known (`specforge new --help` lists the options).
#[derive(clap::Args)]
#[command(
    about = ABOUT,
    disable_help_flag = true,
    after_help = "`specforge new --help` lists its options; `specforge new <kind> --help` the kind's field flags too."
)]
pub(crate) struct Args {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, num_args = 0..)]
    args: Vec<String>,
}

/// `new`'s own options, each declared once: `--path` and `--format` as every command declares them.
fn options() -> Command {
    Command::new("new")
        .about(ABOUT)
        .disable_help_flag(true)
        .arg(
            Arg::new(IN)
                .long(IN)
                .value_name("FILE")
                .help("The file, relative to the spec root; created when missing"),
        )
        .arg(
            Arg::new(VERIFY)
                .long(VERIFY)
                .action(ArgAction::Append)
                .value_name("KIND:TEXT")
                .help("A `verify` obligation, as `kind:text` (or just `text`); repeat for several"),
        )
        .arg(
            Arg::new(DRY_RUN)
                .long(DRY_RUN)
                .action(ArgAction::SetTrue)
                .help("Print the diff and write nothing"),
        )
        .arg(
            Arg::new(HELP)
                .short('h')
                .long(HELP)
                .action(ArgAction::SetTrue)
                .help("Print help (with a kind, that kind's field flags)"),
        )
        // Declared elsewhere with their own display order: placed after the options above.
        .arg(ProjectArg::arg().display_order(4))
        .arg(OutputArg::arg().display_order(5))
}

/// The fields that are flags of `new`, and those named like one of its options, which are not.
fn split(fields: &[FieldInfo]) -> (Vec<&FieldInfo>, Vec<&FieldInfo>) {
    let options = options();
    fields.iter().partition(|f| {
        !options
            .get_arguments()
            .any(|a| a.get_id() == f.name.as_str())
    })
}

/// `specforge new`'s command line, the one its help and both of its readings use: `<KIND> <ID> <TITLE>`, its own
/// options declared once, then either every word after the title (`kind` = `None`: what the line is read with to
/// find the kind) or that kind's declared fields as flags, leaving out a field named like an option.
pub(crate) fn command(kind: Option<(&str, &[FieldInfo])>) -> Command {
    let hidden = kind.is_some();
    let mut cmd = options()
        .arg(
            Arg::new(KIND)
                .value_name("KIND")
                .required_unless_present(HELP)
                .hide(hidden)
                .help("The kind of entity (behavior, invariant, …)"),
        )
        .arg(
            Arg::new(ID)
                .value_name("ID")
                .required_unless_present(HELP)
                .hide(hidden)
                .help("The new entity's id"),
        )
        .arg(
            Arg::new(TITLE)
                .value_name("TITLE")
                .required_unless_present(HELP)
                .hide(hidden)
                .help("The new entity's title"),
        );
    let Some((kind, fields)) = kind else {
        return cmd
            .after_help(
                "The kind's fields are flags too: `specforge new <kind> --help` lists them.",
            )
            .arg(
                Arg::new(FIELDS)
                    .value_name("FIELDS")
                    .trailing_var_arg(true)
                    .allow_hyphen_values(true)
                    .num_args(0..)
                    .help("The kind's fields: `--<field> <value>`"),
            );
    };
    cmd = cmd.override_usage(format!("specforge new {kind} <ID> <TITLE> [OPTIONS]"));
    let (flagged, unflagged) = split(fields);
    if !unflagged.is_empty() {
        let names: Vec<&str> = unflagged.iter().map(|f| f.name.as_str()).collect();
        cmd = cmd.after_help(format!(
            "{}: no flag of new (an option has the name); set with `specforge set <ID> <field> <value>` after creating.",
            names.join(", ")
        ));
    }
    for field in flagged {
        let mut arg = Arg::new(field.name.clone())
            .long(field.name.clone())
            .allow_hyphen_values(true)
            .action(ArgAction::Append)
            .help(field.description.clone());
        arg = match field.field_type {
            FieldType::ReferenceList => arg.value_name("ID,…"),
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
    for field in split(fields).0 {
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
            _ => match values.as_slice() {
                [one] => Input::One(one.clone()),
                _ => Input::Many(values),
            },
        };
        out.insert(field.name.clone(), input);
    }
    Ok(out)
}

impl Args {
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        // First reading: the kind, and the format a refusal before the fields are known is printed in.
        let first = match command(None)
            .no_binary_name(true)
            .try_get_matches_from(&self.args)
        {
            Ok(matches) => matches,
            Err(e) => {
                err!(terminal, "{}", e.render());
                return Exit::Unjudged;
            }
        };
        let format = first
            .get_one::<OutputFormat>(OutputArg::NAME)
            .copied()
            .unwrap_or(OutputFormat::Human);
        let Some(kind) = first.get_one::<String>(KIND) else {
            // Only `new --help` gets here: the other positionals are required.
            outln!(terminal, "{}", command(None).render_long_help());
            return Exit::Passed;
        };
        // The project the words name is read before the line can be (ADR 0053 D7a): the fields are its own.
        let project = ProjectArg::scan(&self.args);
        let compiled = match project.compile() {
            Ok(compiled) => compiled,
            Err(error) => return Refusal::of(format).report(terminal, &error),
        };
        let view = ProjectView::of(&compiled);
        if let Err(error) = view.kinds().declared(kind) {
            return Refusal::of(format).report(terminal, &error);
        }
        let fields = edit::fields_of(&view, kind);
        // Second reading: every word, with the kind's fields. The edit, the help and the refusals come from it.
        let mut cmd = command(Some((kind, &fields)));
        let matches = match cmd
            .clone()
            .no_binary_name(true)
            .try_get_matches_from(&self.args)
        {
            Ok(matches) => matches,
            Err(e) => {
                err!(terminal, "{}", e.render());
                return Exit::Unjudged;
            }
        };
        if matches.get_flag(HELP) {
            outln!(terminal, "{}", cmd.render_long_help());
            return Exit::Passed;
        }
        let named = matches.get_one::<PathBuf>(ProjectArg::NAME);
        if named.map(PathBuf::as_path) != Some(project.path()) {
            err!(
                terminal,
                "error: --path names '{}' read before the kind's flags and '{}' after: give --path apart from a field's value (--<field>=<value>)",
                project.path().display(),
                named.map(|p| p.display().to_string()).unwrap_or_default()
            );
            return Exit::Unjudged;
        }
        let format = matches
            .get_one::<OutputFormat>(OutputArg::NAME)
            .copied()
            .unwrap_or(format);
        match edit(kind, &fields, &matches) {
            Ok(edit) => execute_on(
                &compiled,
                vec![edit],
                matches.get_flag(DRY_RUN),
                format,
                terminal,
            ),
            Err(error) => Refusal::of(format).report(terminal, &error),
        }
    }
}

/// The edit `matches`, the second reading of the line, asks for.
fn edit(kind: &str, fields: &[FieldInfo], matches: &ArgMatches) -> Result<Edit, OpError> {
    let file = matches.get_one::<String>(IN).cloned().ok_or_else(|| {
        OpError::new(
            OpErrorKind::InvalidInput,
            "invalid_edit",
            "new needs --in <file>, relative to the spec root",
        )
    })?;
    let verify = matches
        .get_many::<String>(VERIFY)
        .into_iter()
        .flatten()
        .map(|text| Obligation::parse(text))
        .collect();
    Ok(Edit::Create {
        kind: kind.to_string(),
        id: matches.get_one::<String>(ID).cloned().unwrap_or_default(),
        title: matches
            .get_one::<String>(TITLE)
            .cloned()
            .unwrap_or_default(),
        file,
        fields: inputs(fields, matches)?,
        verify,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str) -> FieldInfo {
        FieldInfo {
            name: name.to_string(),
            field_type: FieldType::String,
            description: String::new(),
            required: false,
        }
    }

    #[specforge_test_macros::test(
        behavior = "create_entity",
        verify = "a kind's field named like one of new's options is no flag of new, and the kind's help says to set it with set"
    )]
    fn a_field_named_like_an_option_of_new_is_no_flag_of_it() {
        let fields = [field("path"), field("format"), field("body")];
        let mut cmd = command(Some(("doc", &fields)));
        let arg = |cmd: &Command, id: &str| cmd.get_arguments().find(|a| a.get_id() == id).cloned();
        assert_eq!(
            arg(&cmd, "path")
                .unwrap()
                .get_help()
                .map(ToString::to_string),
            ProjectArg::arg().get_help().map(ToString::to_string)
        );
        assert_eq!(
            arg(&cmd, "format").unwrap().get_possible_values().len(),
            OutputArg::arg().get_possible_values().len()
        );
        assert!(arg(&cmd, "body").is_some());
        let help = cmd.render_long_help().to_string();
        assert!(help.contains("path, format: no flag of new"), "{help}");
        assert!(help.contains("specforge set"), "{help}");
    }
}
