//! `specforge://prompts/infer`: guidance for inferring spec entities from
//! code, by scope.

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_ops::infer::{self, InferencePlanRequest, PlanDocument};
use specforge_ops::navigate::{anchors_of_file, source_anchors};

use crate::args::Arguments;
use crate::prompt::{CorePrompt, Prompted};
use crate::tool::CoreTool;
use crate::tool::{ErrorCode, McpError};
use crate::tools::find_spec_for_source::{Anchored, MatchMode};
use crate::tools::{analyze, infer_progress, infer_session, query, schema, search, validate};
use specforge_ops::view::ProjectView;

/// The core tools the workflow protocol names, in the order it lists them.
const WORKFLOW_TOOLS: [&str; 6] = [
    infer_session::Tool::NAME,
    infer_progress::Tool::NAME,
    validate::Tool::NAME,
    query::Tool::NAME,
    search::Tool::NAME,
    schema::Tool::NAME,
];

/// The infer prompt (`McpInferPromptResult`).
pub struct Prompt;

impl CorePrompt for Prompt {
    const NAME: &'static str = "specforge://prompts/infer";
    const DESCRIPTION: &'static str =
        "Get inference guidance for discovering spec entities from code";
    const SPEC: &'static str = "McpInferPromptResult";
    const OPEN_VALUES: &'static [&'static str] = &["$.existing_entities"];
    type Args = Args;
    type Payload = Payload;

    fn render(view: ProjectView<'_>, args: Args) -> Result<Prompted<Payload>, Box<McpError>> {
        respond(&view, args)
    }
}

/// `specforge://prompts/infer`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Scope: omit for overview, 'kind:{name}' for focused guide, 'file:{path}' for file deduplication
    scope: Option<String>,
    /// Directory where generated .spec files are written (scope "plan")
    target_spec_directory: Option<String>,
    /// Offset into the plan's unanalyzed/stale file lists for paging (scope "plan")
    #[arg(default = 0)]
    cursor: usize,
}

/// What the prompt is about: the `scope` argument read.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Scope {
    /// No scope, or one the prompt does not know: every kind's guide.
    Overview,
    /// `kind:<name>`, the name lowercased.
    Kind(String),
    /// `file:<path>`.
    File(String),
    /// `plan`.
    Plan,
    /// `workflow`.
    Workflow,
}

impl Scope {
    /// The scope `scope` names. An empty kind or file is invalid input.
    fn parse(scope: Option<&str>) -> Result<Scope, Box<McpError>> {
        let invalid = |message: &str| {
            Box::new(McpError::new(ErrorCode::InvalidInput, message).with_argument("scope"))
        };
        Ok(match scope {
            Some("plan") => Scope::Plan,
            Some("workflow") => Scope::Workflow,
            Some(s) if s.starts_with("kind:") => {
                let kind = s["kind:".len()..].to_string();
                if kind.is_empty() {
                    return Err(invalid("Empty kind name in scope 'kind:'"));
                }
                Scope::Kind(kind)
            }
            Some(s) if s.starts_with("file:") => {
                let file = &s["file:".len()..];
                if file.is_empty() {
                    return Err(invalid("Empty file path in scope 'file:'"));
                }
                Scope::File(file.to_string())
            }
            _ => Scope::Overview,
        })
    }
}

/// The infer prompt's payload, one per scope, tagged by it
/// (`McpInferPromptResult`).
#[derive(Debug, Serialize, Shape)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum Payload {
    Overview(Overview),
    Kind(KindScope),
    File(FileScope),
    Plan(PlanDocument),
    Workflow(Workflow),
}

/// The project's guide, as `specforge infer-guide --format json` prints it,
/// then what the prompt adds.
#[derive(Debug, Serialize, Shape)]
pub struct Overview {
    #[serde(flatten)]
    pub guide: infer::GuideDocument,
    pub output_format: String,
    pub validation: String,
}

/// One kind's guide, then what the prompt adds.
#[derive(Debug, Serialize, Shape)]
pub struct KindScope {
    #[serde(flatten)]
    pub guide: infer::KindGuideDocument,
    pub validation: String,
}

/// What a source file already has, and the guides to infer the rest.
#[derive(Debug, Serialize, Shape)]
pub struct FileScope {
    pub file: String,
    pub match_mode: MatchMode,
    pub existing_entities_referencing_file: Vec<Anchored>,
    pub kinds: Vec<FileKind>,
    pub project_conventions: String,
    pub validation: String,
}

/// One kind's guide in a file scope.
#[derive(Debug, Serialize, Shape)]
pub struct FileKind {
    pub kind: String,
    pub inference_guide: String,
}

/// The tools the workflow protocol names, and the kinds installed.
#[derive(Debug, Serialize, Shape)]
pub struct Workflow {
    pub tools: Vec<&'static str>,
    pub installed_kinds: Vec<String>,
}

/// The prompt over `project`.
fn respond(project: &ProjectView, args: Args) -> Outcome {
    match Scope::parse(args.scope.as_deref())? {
        Scope::Plan => get_plan(project, args.target_spec_directory.as_deref(), args.cursor),
        Scope::Workflow => Ok(get_workflow(project)),
        Scope::Kind(kind) => get_kind_scoped(project, &kind),
        Scope::File(file) => get_file_scoped(project, &file),
        Scope::Overview => Ok(get_overview(project)),
    }
}

/// A rendered prompt.
fn rendered(instruction: impl Into<String>, payload: Payload) -> Prompted<Payload> {
    Prompted {
        instruction: instruction.into(),
        payload,
    }
}

/// What the infer prompt answers: its instruction and its typed payload, or
/// the refusal.
type Outcome = Result<Prompted<Payload>, Box<McpError>>;

fn get_overview(project: &ProjectView) -> Prompted<Payload> {
    let guide = infer::guide(project);
    let payload = Payload::Overview(Overview {
        output_format: format!(
            "Write .spec files in the {} directory. Use `keyword entity_id \"Title\" {{ fields }}` syntax. Entity IDs are snake_case identifiers (letters, digits, underscores, 2-60 chars).",
            guide.spec_directory
        ),
        validation: validation(),
        guide: guide.document(),
    });

    let instruction = format!(
        "You are inferring spec entities from this codebase. \
         Use the inference guides below to identify entities in the code, \
         write .spec files, and validate them with {}. \
         Each kind has signals describing what to look for in code. \
         Do not duplicate entities that already exist.",
        validate::Tool::NAME
    );

    rendered(instruction, payload)
}

fn get_kind_scoped(project: &ProjectView, kind_name: &str) -> Outcome {
    let guide = infer::kind_guide(project, kind_name)
        .map_err(|error| Box::new(McpError::from(error).with_argument("scope")))?;
    let payload = Payload::Kind(KindScope {
        guide: guide.document(),
        validation: validation(),
    });

    let instruction = format!(
        "You are inferring '{kind_name}' entities from this codebase. \
         Use the guide below. Do not duplicate the existing entity IDs listed."
    );

    Ok(rendered(instruction, payload))
}

/// What the agent runs after writing `.spec` files, naming the tools as the
/// tool table does.
fn validation() -> String {
    format!(
        "After writing .spec files, call {} to check for errors (and {} for coverage/contract findings). Fix any errors before proceeding.",
        validate::Tool::NAME,
        analyze::Tool::NAME,
    )
}

fn get_file_scoped(project: &ProjectView, file_path: &str) -> Outcome {
    // The entities anchored to the file: the one file rule
    // (specforge_ops::navigate::anchors_of_file) over the anchors manifest,
    // the answer specforge.find_spec_for_source gives (C9-09). With no
    // project there is no manifest.
    let manifest = source_anchors(project).map_err(McpError::from)?;
    let found = anchors_of_file(&manifest, file_path);
    let referencing_entities: Vec<Anchored> = found
        .anchors
        .iter()
        .map(|anchor| Anchored::of(anchor, project.graph()))
        .collect();
    let match_mode = MatchMode::from(found.mode);

    let guide = infer::guide(project);
    let kinds = guide
        .kinds
        .iter()
        .map(|kind| FileKind {
            kind: kind.keyword.to_string(),
            inference_guide: kind.guide.clone(),
        })
        .collect();

    let result = Payload::File(FileScope {
        file: file_path.to_string(),
        match_mode,
        existing_entities_referencing_file: referencing_entities,
        kinds,
        project_conventions: guide.conventions.unwrap_or("").to_string(),
        validation: validation(),
    });

    let instruction = format!(
        "You are inferring spec entities from the file '{}'. \
         The entities listed below already reference this file — do not duplicate them. \
         Use the kind guides to identify new entities.",
        file_path
    );

    Ok(rendered(instruction, result))
}

fn get_plan(project: &ProjectView, target_spec_directory: Option<&str>, cursor: usize) -> Outcome {
    let request = InferencePlanRequest {
        target_spec_directory,
        cursor,
    };
    let plan = infer::inference_plan(project, &request).map_err(McpError::from)?;
    let result = Payload::Plan(plan.document());

    let instruction = format!(
        "Create a prioritized inference plan. There are {} unanalyzed files and {} stale files. \
         Write .spec files to '{}'. Process files with the most entity signals first. \
         Use {} to track progress (start → mark_analyzed per file → end). \
         After each file, call {} to check for errors.",
        plan.unanalyzed.total,
        plan.stale.total,
        plan.target_spec_directory,
        infer_session::Tool::NAME,
        validate::Tool::NAME,
    );

    Ok(rendered(instruction, result))
}

fn get_workflow(project: &ProjectView) -> Prompted<Payload> {
    let tool_names = WORKFLOW_TOOLS;

    let installed_kinds: Vec<String> = infer::guide(project)
        .kinds
        .iter()
        .map(|kind| kind.keyword.to_string())
        .collect();

    let [
        infer_session,
        infer_progress,
        validate,
        query,
        search,
        _schema,
    ] = tool_names;
    let result = Payload::Workflow(Workflow {
        tools: tool_names.to_vec(),
        installed_kinds,
    });

    let workflow = format!(
        "\
## Inference Workflow Protocol

### Step 1: Start Session
Call `{infer_session}` with `action: \"start\"` and `agent: \"<your-id>\"`.
Optionally set `source_roots` to limit scanning scope.

### Step 2: Check Progress
Call `{infer_progress}` to see unanalyzed files and current project.

### Step 3: For Each Source File
1. Read the source file
2. Identify entities (behaviors, types, events, etc.) using entity kind guides
3. Write a `.spec` file with the discovered entities
4. Call `{validate}` to check for errors — fix any before proceeding
5. Call `{infer_session}` with `action: \"mark_analyzed\"`, `source_file`, and `entities_produced`

### Step 4: Validate Continuously
After every 3-5 files, call `{validate}` to catch cross-file issues.
Use `{search}` to find existing entities and avoid duplicates.
Use `{query}` to check how new entities connect to the graph.

### Step 5: End Session
Call `{infer_session}` with `action: \"end\"` and the `session_id` from Step 1.
Use `status: \"completed\"` when done, or `status: \"paused\"` to resume later.

### Retry Pattern
If validation fails, fix the .spec file and re-validate. Do not skip errors.
If a file has no identifiable entities, still mark it as analyzed with an empty `entities_produced`.
",
    );

    rendered(workflow, result)
}
