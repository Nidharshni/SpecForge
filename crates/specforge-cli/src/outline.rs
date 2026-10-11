use crate::terminal::{Terminal, out};
use std::path::Path;

use specforge_ops::model::OutlineOptions;
use specforge_ops::view::ProjectView;

use crate::outcome::Exit;
use crate::pipeline;

/// `specforge outline`: the outline operation over the project compiled at
/// `path`.
pub fn run(path: &Path, options: &OutlineOptions, terminal: &Terminal) -> Exit {
    let project = pipeline::compile_project(path);
    out!(
        terminal,
        "{}",
        specforge_ops::model::outline(&ProjectView::of(&project), options)
    );
    Exit::Passed
}
