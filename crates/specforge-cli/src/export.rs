use crate::terminal::{Terminal, errln, outln};
use specforge_ops::export;
use specforge_ops::schema::SchemaRequest;
use specforge_ops::view::ProjectView;
use std::path::Path;

use crate::outcome::{Exit, OutputFormat, Refusal};
use crate::pipeline;

/// Export the project compiled at `path` to stdout. Before it writes, the
/// schema the extensions produce is compared with the one the previous
/// export cached in `<path>/.specforge/schema-cache.json` (the root it
/// compiled, never an ancestor's): each breaking change is a W053 warning
/// on stderr, and the cached schema's version, bumped by what changed, is
/// the version the export carries. After a successful export that schema
/// replaces the cache, so the next export compares against it. The export
/// is written whatever the comparison finds.
pub fn run(path: &Path, request: &export::Request, terminal: &Terminal) -> Exit {
    let project = pipeline::compile_project(path);
    let view = ProjectView::of(&project);

    // The export goes to stdout: there is no output directory holding
    // earlier exports, and `.specforge/` holds extensions and the watch
    // snapshot too, so nothing here shows the project was exported before.
    let recorded = export::export_recorded(&view, request);
    for diagnostic in &recorded.breaking {
        errln!(terminal, "{}", specforge_common::render_plain(diagnostic));
    }
    let output = match recorded.export {
        Ok(output) => output,
        Err(error) => return Refusal::of(OutputFormat::Human).report(terminal, &error),
    };
    outln!(terminal, "{}", output);

    if let export::CacheWrite::WriteFailed { dir, error } = &recorded.cache {
        errln!(
            terminal,
            "warning: could not write the schema cache in {}: {error}",
            dir.display()
        );
    }
    Exit::Passed
}

/// `specforge schema`: the schema operation over the project compiled at
/// `path`, versioned as the next export would be (the cache is only read):
/// the document `specforge.schema` returns for the same request, pretty
/// printed. With `publish`, the JSON Schema an export of that format
/// conforms to. An unknown kind is refused with the closest one (exit 1).
pub fn run_schema(
    path: &Path,
    request: &SchemaRequest,
    publish: Option<export::Format>,
    terminal: &Terminal,
) -> Exit {
    let project = pipeline::compile_project(path);
    let view = ProjectView::of(&project);
    let output = match publish {
        Some(format) => specforge_ops::schema::json_schema(&view, format),
        None => specforge_ops::schema::schema(&view, request)
            .map(|outcome| serde_json::to_string_pretty(&outcome).expect("a schema serializes")),
    };
    match output {
        Ok(text) => {
            outln!(terminal, "{text}");
            Exit::Passed
        }
        Err(error) => Refusal::of(OutputFormat::Human).report(terminal, &error),
    }
}
