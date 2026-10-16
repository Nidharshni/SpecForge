//! The documents the commands print, computed where they are computed. One function per command a test used to
//! spawn: each calls the operation the command calls (`crates/surfaces/cli/src/{check,export,query,trace,stats,analyze}.rs`)
//! with the request the command's flags build, over a project compiled as the CLI compiles it
//! ([`TestProject::compile`]). The JSON a function returns is what `--format json` prints, so an assertion on a
//! document's paths reads the same here as it did on stdout.

use serde_json::Value;
use specforge_ops::Noticed;
use specforge_ops::analysis::analyze::{AnalyzeOptions, analyze};
use specforge_ops::analysis::check::{CheckOptions, CheckOutcome, check};
use specforge_ops::read::export::{self, Format};
use specforge_ops::read::query::{QueryRequest, query};
use specforge_ops::read::schema::{self, SchemaRequest};
use specforge_ops::read::stats::{self, Stats};
use specforge_ops::read::trace::{self, Target, TraceOutcome};
use specforge_ops::view::ProjectView;
use specforge_project::CompiledProject;
pub use specforge_project::testing::{TestProject, specs};

/// A project holding `files` (paths under the root), compiled. The project is returned with it: it owns the
/// directory the export's schema cache is written in.
pub fn built(files: &[(&str, &str)]) -> (TestProject, CompiledProject) {
    let project = TestProject::new().files(files);
    let compiled = project.compile();
    (project, compiled)
}

/// `specforge check` (`cli/src/check.rs`): the check operation over what the compile reported.
pub fn checked(project: &CompiledProject, options: CheckOptions) -> CheckOutcome {
    check(&ProjectView::of(project), &options).expect("check runs on a compiled project")
}

/// `specforge check --format json`: the diagnostics the check shows, as the array it prints.
pub fn check_document(project: &CompiledProject, options: CheckOptions) -> Vec<Value> {
    let outcome = checked(project, options);
    let shown: Vec<_> = outcome.shown().into_iter().cloned().collect();
    serde_json::to_value(specforge_common::diagnostics_json(&shown))
        .expect("diagnostics serialize")
        .as_array()
        .cloned()
        .expect("diagnostics are an array")
}

/// `specforge export --format <format>` (`cli/src/export.rs`): the recorded export, its text or its refusal.
pub fn exported_text(
    project: &CompiledProject,
    request: &export::Request<'_>,
) -> Result<String, specforge_ops::Refusal<export::ExportField>> {
    export::export_recorded(&ProjectView::of(project), request)
        .into_value_and_notices()
        .0
        .export
}

/// [`exported_text`] of `format`, the schema as the command's default policy has it.
pub fn exported_as(project: &CompiledProject, format: Format) -> String {
    exported_text(
        project,
        &export::Request {
            format: Some(format),
            ..Default::default()
        },
    )
    .unwrap_or_else(|e| panic!("export refused: {e:?}"))
}

/// `specforge export --format graph`, parsed.
pub fn exported(project: &CompiledProject) -> Value {
    parse(&exported_as(project, Format::Graph))
}

/// `specforge export` with `request`, parsed.
pub fn exported_with(project: &CompiledProject, request: &export::Request<'_>) -> Value {
    parse(&exported_text(project, request).unwrap_or_else(|e| panic!("export refused: {e:?}")))
}

/// `specforge schema` (`cli/src/export.rs`): the schema operation.
pub fn schema_of(project: &CompiledProject, request: &SchemaRequest<'_>) -> Value {
    let outcome = schema::schema(&ProjectView::of(project), request)
        .unwrap_or_else(|e| panic!("schema refused: {e:?}"));
    serde_json::to_value(outcome.into_value_and_notices().0).expect("a schema serializes")
}

/// `specforge schema --publish --format <format>`.
pub fn published(project: &CompiledProject, format: Format) -> Value {
    schema::json_schema(&ProjectView::of(project), format)
        .map(|published| published.into_value_and_notices().0)
        .unwrap_or_else(|e| panic!("publish refused: {e:?}"))
}

/// `specforge query` (`cli/src/query.rs`): the operation's outcome, or its refusal.
pub fn queried(
    project: &CompiledProject,
    request: &QueryRequest<'_>,
) -> Result<Noticed<String>, specforge_ops::Refusal<specforge_ops::read::query::QueryField>> {
    query(&ProjectView::of(project), request)
}

/// `specforge query <entity> --depth <depth>` with these kinds, its document parsed.
pub fn query_document(
    project: &CompiledProject,
    entity: &str,
    depth: Option<usize>,
    kinds: &[&str],
) -> Value {
    let outcome = queried(
        project,
        &QueryRequest {
            entity_id: entity,
            depth,
            kinds: kinds.to_vec(),
            ..Default::default()
        },
    )
    .unwrap_or_else(|e| panic!("query refused: {e:?}"));
    parse(&outcome.into_value_and_notices().0)
}

/// `specforge trace` (`cli/src/trace.rs`): the outcome.
pub fn traced(project: &CompiledProject, target: Target<'_>) -> TraceOutcome {
    trace::trace(&ProjectView::of(project), target)
        .unwrap_or_else(|e| panic!("trace refused: {e:?}"))
}

/// `specforge trace <entity> --format json`, parsed.
pub fn trace_document(project: &CompiledProject, entity: &str) -> Value {
    serde_json::to_value(traced(project, Target::Entity(entity))).expect("a trace serializes")
}

/// `specforge stats` (`cli/src/stats.rs`).
pub fn stats_of(project: &CompiledProject) -> Stats {
    stats::stats(&ProjectView::of(project)).expect("stats of a compiled project")
}

/// `specforge analyze --format json` with `options`: the document it prints.
pub fn analyzed(project: &CompiledProject, options: &AnalyzeOptions<'_>) -> Value {
    let outcome = analyze(&ProjectView::of(project), options)
        .unwrap_or_else(|e| panic!("analyze refused: {e:?}"));
    serde_json::to_value(outcome.document()).expect("an analysis serializes")
}

fn parse(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("not JSON ({e}): {text}"))
}

/// `specforge <short> <id> --<arg> <value> --format json` (`cli/src/extension_command.rs`): the extension command
/// `id` of the extension routed as `short`, run over `given` (the args as the operation normalizes them). Returns
/// the exit code the command gives and its stdout, parsed.
pub fn command_document(
    project: &CompiledProject,
    short: &str,
    id: &str,
    given: Value,
) -> (i32, Value) {
    let view = ProjectView::of(project);
    let commands = specforge_ops::extension::command::ExtensionCommands::build(view.registries());
    let command = commands
        .of(short)
        .find(|command| command.id() == id)
        .unwrap_or_else(|| panic!("no command {short} {id}"));
    let output = specforge_ops::extension::command::run(
        &view,
        command,
        given.as_object().expect("args are an object"),
        specforge_ops::extension::command::CommandFormat::Json,
    )
    .unwrap_or_else(|e| panic!("{short} {id} refused: {e:?}"));
    (output.exit_code, parse(&output.stdout))
}
