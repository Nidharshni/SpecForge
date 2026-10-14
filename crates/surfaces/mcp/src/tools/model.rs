use crate::target::View;
use crate::tool::{CoreTool, Effect, ToolGroup};
use specforge_ops::read::model::{
    self, FieldLevel, GroupBy, ModelField, ModelFormat, ModelOptions, ModelRoot,
};

use crate::args::Arguments;
use crate::refusal::ArgumentOf;
use crate::reply::{Answer, Answered, Text};
use crate::tool::McpError;
use specforge_ops::view::ProjectView;

/// `specforge.model`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Output format
    #[arg(choice = specforge_ops::read::model::MODEL_FORMAT)]
    format: ModelFormat,
    /// How to group entities
    #[arg(choice = specforge_ops::read::model::GROUP_BY)]
    group_by: GroupBy,
    /// Field detail level
    #[arg(choice = specforge_ops::read::model::MODEL_FIELDS)]
    fields: FieldLevel,
    /// Filter to a single extension
    extension: Option<String>,
    /// Filter to specific entity kinds (empty: every kind)
    kinds: Vec<String>,
    /// Root entity kind for depth-scoped output
    root: Option<String>,
    /// Maximum depth from root kind (requires root)
    depth: Option<usize>,
}

/// `specforge.model`: the model operation over the served project. A kind
/// of `kinds` the project does not know rides in `_meta.diagnostics`
/// (I020). An unknown `root` or `extension` is refused on that argument,
/// and so is a `depth` without a `root`.
pub(crate) struct Tool;

impl ArgumentOf<ModelField> for Tool {
    fn argument(field: ModelField) -> Option<&'static str> {
        match field {
            ModelField::Extension => Some("extension"),
            ModelField::Root => Some("root"),
        }
    }
}

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.model";
    const DESCRIPTION: &'static str =
        "Render the logical data model (entity kinds, fields, relationships)";
    const EFFECT: Effect = Effect::Reads {
        group: ToolGroup::Core,
    };
    type Args = Args;
    type Input = View;
    type Returns = Answered<Text>;

    fn call(view: ProjectView<'_>, args: Args) -> Answered<Text> {
        let options = options(args)?;
        let outcome = model::model(&view, &options).map_err(Self::refused)?;
        Ok(Answer::new(Text(outcome.document)).with_diagnostics(outcome.notices))
    }
}

/// The model options the arguments name; an absent enumerated one is its
/// table's default, as `specforge model` reads it (ADR 0027). A `depth`
/// needs a `root`.
fn options(args: Args) -> Result<ModelOptions, Box<McpError>> {
    let root = match (args.root, args.depth) {
        (Some(kind), depth) => Some(ModelRoot { kind, depth }),
        (None, None) => None,
        (None, Some(_)) => {
            return Err(Box::new(McpError::invalid_input(
                "root",
                "'depth' needs 'root': the kind the depth counts from",
            )));
        }
    };
    Ok(ModelOptions {
        format: args.format,
        group_by: args.group_by,
        fields: args.fields,
        extension: args.extension,
        kinds: args.kinds,
        root,
    })
}
