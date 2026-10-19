//! `specforge set|unset|link|unlink|delete`: one typed edit of an entity each (ADR 0151); `specforge new` is in
//! `new.rs`. Each builds the `Edit` its words name and plans it over the compiled project; nothing is written when the
//! edited project would report an error it does not (E080). `--dry-run` prints the diff and writes nothing.

use crate::notes::Notes;
use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal, Written};
use crate::project::ProjectArg;
use crate::terminal::{Terminal, outln};
use specforge_ops::authoring::edit::{self, Edit, EditReport, Input, Obligation};
use specforge_ops::view::ProjectView;
use specforge_ops::{OpError, OpErrorKind};
use specforge_project::CompiledProject;

/// What a command prints under `--format json`: the plan's report, and whether it is a preview.
#[derive(serde::Serialize)]
struct Document<'r> {
    #[serde(flatten)]
    report: &'r EditReport,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    dry_run: bool,
}

/// The refusal of a command line that names no edit.
fn usage(message: &str) -> OpError {
    OpError::new(OpErrorKind::InvalidInput, "invalid_edit", message)
}

/// Plan `edits` over the project at `project`, write them unless `dry_run`, and print what they did.
pub(crate) fn execute(
    project: &ProjectArg,
    edits: Result<Vec<Edit>, OpError>,
    dry_run: bool,
    format: OutputFormat,
    terminal: &Terminal,
) -> Exit {
    let edits = match edits {
        Ok(edits) => edits,
        Err(error) => return Refusal::of(format).report(terminal, &error),
    };
    let compiled = match project.compile() {
        Ok(compiled) => compiled,
        Err(error) => return Refusal::of(format).report(terminal, &error),
    };
    execute_on(&compiled, edits, dry_run, format, terminal)
}

/// Plan `edits` over `compiled`, write them unless `dry_run`, and print what they did. It never compiles: the
/// caller's project is the one planned, validated and written.
pub(crate) fn execute_on(
    compiled: &CompiledProject,
    edits: Vec<Edit>,
    dry_run: bool,
    format: OutputFormat,
    terminal: &Terminal,
) -> Exit {
    let view = ProjectView::of(compiled);
    let root = match view.project_root() {
        Ok(root) => root,
        Err(error) => return Refusal::of(format).report(terminal, &error),
    };
    let planned = match edit::plan(&view, &edits) {
        Ok(planned) => planned,
        Err(refusal) => return Refusal::of(format).at(root).report(terminal, &refusal),
    };
    let written = if dry_run || planned.is_empty() {
        None
    } else {
        match edit::apply(&planned, crate::caller::of(false, format, terminal)) {
            Ok(writes) => Some(writes),
            Err(error) => return Refusal::of(format).at(root).report(terminal, &error),
        }
    };
    let report = planned.report();
    match format {
        OutputFormat::Json => terminal.json(&Written {
            document: Document { report, dry_run },
            files_written: written.map(|w| w.named(Some(root))),
        }),
        OutputFormat::Human => {
            if !report.diff.is_empty() {
                outln!(terminal, "{}", report.diff.trim_end());
            }
            let files = report.affected_files.len();
            match (dry_run, files) {
                (_, 0) => outln!(terminal, "nothing to write"),
                (true, n) => outln!(
                    terminal,
                    "dry run: {n} file(s) would change; nothing was written"
                ),
                (false, n) => outln!(terminal, "wrote {n} file(s)"),
            }
            Notes::new(terminal, format).carried(&report.added.0);
        }
    }
    Exit::Passed
}

/// `specforge set <entity> <field> <value>…` / `specforge set <entity> --title <title>`.
#[derive(clap::Args)]
pub(crate) struct SetArgs {
    /// The entity to change
    entity: String,

    /// The field, or `field.entry` for one entry of a block field
    #[arg(conflicts_with = "title")]
    field: Option<String>,

    /// The value; a list field takes several (a reference list also one separated by commas)
    #[arg(num_args = 0.., conflicts_with = "title")]
    values: Vec<String>,

    /// Change the header title instead of a field
    #[arg(long, value_name = "TITLE")]
    title: Option<String>,

    /// An annotation to write after the value (`readonly`, `since=1.2`); replaces the field's
    #[arg(long = "annotation", value_name = "NAME[=VALUE]")]
    annotations: Vec<String>,

    /// Print the diff and write nothing
    #[arg(long)]
    dry_run: bool,

    #[command(flatten)]
    project: ProjectArg,

    #[command(flatten)]
    output: OutputArg,
}

impl SetArgs {
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let edits = self.edits();
        execute(
            &self.project,
            edits,
            self.dry_run,
            self.output.format,
            terminal,
        )
    }

    fn edits(&self) -> Result<Vec<Edit>, OpError> {
        let entity_id = self.entity.clone();
        if let Some(title) = &self.title {
            return Ok(vec![Edit::Retitle {
                entity_id,
                title: title.clone(),
            }]);
        }
        let Some(field) = &self.field else {
            return Err(usage("set needs a field and a value, or --title"));
        };
        let value = match self.values.as_slice() {
            [] => return Err(usage("set needs a value (unset removes a field)")),
            [one] => Input::One(one.clone()),
            many => Input::Many(many.to_vec()),
        };
        Ok(vec![Edit::Set {
            entity_id,
            field: field.clone(),
            value,
            annotations: (!self.annotations.is_empty()).then(|| self.annotations.clone()),
        }])
    }
}

/// `specforge unset <entity> <field>`.
#[derive(clap::Args)]
pub(crate) struct UnsetArgs {
    /// The entity to change
    entity: String,

    /// The field to remove, or `field.entry` for one entry of a block field
    field: String,

    /// Print the diff and write nothing
    #[arg(long)]
    dry_run: bool,

    #[command(flatten)]
    project: ProjectArg,

    #[command(flatten)]
    output: OutputArg,
}

impl UnsetArgs {
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let edits = Ok(vec![Edit::Unset {
            entity_id: self.entity,
            field: self.field,
        }]);
        execute(
            &self.project,
            edits,
            self.dry_run,
            self.output.format,
            terminal,
        )
    }
}

/// `specforge link|unlink <entity> <field> <target>` / `… <entity> --verify <kind:text>`.
#[derive(clap::Args)]
pub(crate) struct LinkArgs {
    /// The entity to change
    entity: String,

    /// The list field
    #[arg(requires = "target", conflicts_with = "verify")]
    field: Option<String>,

    /// The entity id (or text, for a string list) to add or remove
    #[arg(conflicts_with = "verify")]
    target: Option<String>,

    /// A `verify` obligation, as `kind:text` (or just `text`)
    #[arg(long, value_name = "KIND:TEXT", required_unless_present = "field")]
    verify: Option<String>,

    /// Print the diff and write nothing
    #[arg(long)]
    dry_run: bool,

    #[command(flatten)]
    project: ProjectArg,

    #[command(flatten)]
    output: OutputArg,
}

impl LinkArgs {
    pub(crate) fn run(self, add: bool, terminal: &Terminal) -> Exit {
        let entity_id = self.entity.clone();
        let edit = match (&self.verify, &self.field, &self.target) {
            (Some(verify), _, _) => {
                let Obligation { kind, text } = Obligation::parse(verify);
                if add {
                    Edit::AddVerify {
                        entity_id,
                        kind,
                        text,
                    }
                } else {
                    Edit::RemoveVerify {
                        entity_id,
                        kind,
                        text,
                    }
                }
            }
            (None, Some(field), Some(target)) if add => Edit::Link {
                entity_id,
                field: field.clone(),
                target: target.clone(),
            },
            (None, Some(field), Some(target)) => Edit::Unlink {
                entity_id,
                field: field.clone(),
                target: target.clone(),
            },
            _ => {
                return Refusal::of(self.output.format)
                    .report(terminal, &usage("name a field and a target, or --verify"));
            }
        };
        execute(
            &self.project,
            Ok(vec![edit]),
            self.dry_run,
            self.output.format,
            terminal,
        )
    }
}

/// `specforge delete <entity> [--unlink-references]`.
#[derive(clap::Args)]
pub(crate) struct DeleteArgs {
    /// The entity to delete
    entity: String,

    /// Remove every reference to it in the same change
    #[arg(long)]
    unlink_references: bool,

    /// Print the diff and write nothing
    #[arg(long)]
    dry_run: bool,

    #[command(flatten)]
    project: ProjectArg,

    #[command(flatten)]
    output: OutputArg,
}

impl DeleteArgs {
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let edits = Ok(vec![Edit::Delete {
            entity_id: self.entity,
            unlink_references: self.unlink_references,
        }]);
        execute(
            &self.project,
            edits,
            self.dry_run,
            self.output.format,
            terminal,
        )
    }
}
