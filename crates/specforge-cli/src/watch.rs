//! `specforge watch` — renders what [`SessionWatch`] does to a
//! [`ProjectSession`].
//!
//! The session is seeded by one cold build and kept current from
//! file-watcher events; every event reports its diagnostics, the set
//! `specforge check` reports for the same sources. The watchers only say
//! which paths changed: what a path is to the project, and what its change
//! does, is the session's (`classify_project_changes`); following the
//! session's inputs and catching up is the watch crate's loop (ADR 0035).

use crate::terminal::{Terminal, errln, outln};

use std::path::PathBuf;

use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal};
use crate::project::ProjectArg;
use serde::Serialize;
use specforge_common::DiagnosticList;
use specforge_common::shape::Shape;
use specforge_ops::check::Counts;
use specforge_ops::{OpError, OpErrorKind};
use specforge_project::{GraphDelta, ProjectSession, UpdateKind};
use specforge_watch::{Applied, DEFAULT_DEBOUNCE_WINDOW, Notify, SessionWatch, WatchEvent};

// `specforge watch`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    #[command(flatten)]
    project: crate::project::ProjectArg,

    #[command(flatten)]
    output: OutputArg,

    /// Check every incremental rebuild against a cold rebuild (slower)
    #[arg(long, default_value_t = false)]
    verify_incremental: bool,
}

impl Args {
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        watch_project(
            &self.project,
            self.output.format,
            self.verify_incremental,
            terminal,
        )
    }
}

fn watch_project(
    project: &ProjectArg,
    format: OutputFormat,
    verify_incremental: bool,
    terminal: &Terminal,
) -> Exit {
    // A debug build checks every rebuild (ProjectSession); a release build
    // only when asked (the check costs a cold rebuild per change).
    let root = match project.root() {
        Ok(root) => root,
        Err(error) => return Refusal::of(format).report(terminal, &error),
    };
    let mut session = ProjectSession::open(&root);
    if verify_incremental {
        session.set_verify_incremental(true);
    }

    // Watch before announcing readiness: a client that writes on seeing
    // "ready" must never race a watcher that does not exist yet.
    let (watchers, batches) = Notify::new(DEFAULT_DEBOUNCE_WINDOW);
    let mut watch = match SessionWatch::start(session, watchers) {
        Ok(watch) => watch,
        Err(e) => {
            return Refusal::of(format).report(
                terminal,
                &OpError::new(OpErrorKind::Internal, "watch_failed", e),
            );
        }
    };

    // What was written between the open's stamps and the watchers is applied
    // before `ready` (ADR 0030): the session follows the disk it saw when
    // the watchers were armed.
    for event in watch.catch_up() {
        if let WatchEvent::Unwatched(e) = event {
            errln!(terminal, "warning: {e}");
        }
    }
    print_ready(watch.session(), format, terminal);

    // Debounced batches of changed paths. The session says what they are
    // and applies them: sources take the incremental path (tree-sitter
    // trees are retained, so unchanged subtrees are not re-parsed), an
    // environment input reloads the environment, a check input re-runs the
    // checks; anything else changes nothing.
    let debug = std::env::var("SPECFORGE_WATCH_DEBUG").is_ok();
    for batch in &batches {
        if debug {
            let changes = watch
                .session()
                .changes(batch.iter().map(std::path::PathBuf::as_path));
            errln!(terminal, "[watch] batch: {batch:?} -> {changes:?}");
        }
        for event in watch.changed(&batch) {
            match event {
                WatchEvent::Applied(applied) => {
                    let rendered = render(&applied, format, unix_seconds());
                    outln!(terminal, "{}", rendered.line);
                    if let Some(warning) = rendered.warning {
                        errln!(terminal, "{warning}");
                    }
                }
                WatchEvent::Unwatched(e) => errln!(terminal, "warning: {e}"),
            }
            // The reader has gone (`| head`): nobody sees what a watch
            // would write next.
            if terminal.closed() {
                return Exit::Passed;
            }
        }
    }
    Exit::Passed
}

/// Seconds since the epoch, for a text event's stamp.
fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The `ready` event: the session as opened, and caught up with disk.
fn print_ready(session: &ProjectSession, format: OutputFormat, terminal: &Terminal) {
    let spec_root: PathBuf = std::fs::canonicalize(session.project().environment().spec_root())
        .unwrap_or_else(|_| session.project().environment().spec_root().to_path_buf());
    let diagnostics = session.project().diagnostics();
    let Counts {
        errors, warnings, ..
    } = Counts::of(&diagnostics);
    if format == OutputFormat::Json {
        let line = Line::Ready(Ready {
            spec_root: spec_root.to_string_lossy().into_owned(),
            files: session.project().file_count(),
            nodes: session.project().graph().node_count(),
            edges: session.project().graph().edge_count(),
            errors,
            warnings,
            diagnostics: DiagnosticList(diagnostics),
        });
        outln!(terminal, "{}", line.json());
    } else {
        outln!(
            terminal,
            "specforge watch: {} ({} files, {} nodes, {} edges, {} errors, {} warnings)",
            spec_root.display(),
            session.project().file_count(),
            session.project().graph().node_count(),
            session.project().graph().edge_count(),
            errors,
            warnings
        );
        outln!(terminal, "watching for changes (Ctrl-C to stop)");
    }
}

/// One line `specforge watch --format json` prints.
#[derive(Serialize, Shape)]
#[serde(tag = "event", rename_all = "snake_case")]
enum Line<'a> {
    Ready(Ready),
    ExtensionsReloaded(Reloaded<'a>),
    Rebuilt(Rebuild<'a>),
    Rechecked(Rebuild<'a>),
}

impl Line<'_> {
    /// The line as the JSON text it prints.
    fn json(&self) -> String {
        serde_json::to_string(self).expect("a watch line serializes")
    }
}

/// The session as opened, and caught up with disk.
#[derive(Serialize, Shape)]
struct Ready {
    spec_root: String,
    files: usize,
    nodes: usize,
    edges: usize,
    errors: usize,
    warnings: usize,
    diagnostics: DiagnosticList,
}

/// The extension environment loaded again.
#[derive(Serialize, Shape)]
struct Reloaded<'a> {
    extensions: &'a [String],
    files: usize,
    nodes: usize,
    errors: usize,
    warnings: usize,
    diagnostics: DiagnosticList,
}

/// An update of the sources (`rebuilt`) or of the check inputs
/// (`rechecked`).
#[derive(Serialize, Shape)]
struct Rebuild<'a> {
    changed: &'a [String],
    rebuilt_files: &'a [String],
    /// What the update changed in the graph: the GraphDelta MCP sends as
    /// specforge/graphChanged's params.
    delta: &'a GraphDelta,
    errors: usize,
    warnings: usize,
    diagnostics: DiagnosticList,
    changed_diagnostic_files: &'a [String],
    verification_failed: bool,
    /// `passed`, or the divergence; absent when the update was not
    /// verified.
    #[serde(skip_serializing_if = "Option::is_none")]
    verification: Option<String>,
}

/// One event's output: its stdout line, and, in text mode, the stderr line a
/// divergence adds.
struct Rendered {
    line: String,
    warning: Option<String>,
}

/// What `applied` did: `extensions_reloaded` for the environment,
/// `rechecked` for check inputs (with the same fields as `rebuilt`),
/// `rebuilt` for sources. `stamp` (seconds since the epoch) heads a text
/// line.
fn render(applied: &Applied, format: OutputFormat, stamp: u64) -> Rendered {
    let update = &applied.update;
    let Counts {
        errors, warnings, ..
    } = Counts::of(update.diagnostics.iter());
    if update.kind == UpdateKind::Environment {
        let line = if format == OutputFormat::Json {
            Line::ExtensionsReloaded(Reloaded {
                extensions: &applied.extensions,
                files: applied.files,
                nodes: applied.nodes,
                errors,
                warnings,
                diagnostics: DiagnosticList(update.diagnostics.to_vec()),
            })
            .json()
        } else {
            format!(
                "[reload] extension environment changed: {} extension(s), {} file(s) | {} errors, {} warnings",
                applied.extensions.len(),
                applied.files,
                errors,
                warnings
            )
        };
        return Rendered {
            line,
            warning: None,
        };
    }

    let event = match update.kind {
        UpdateKind::Checks => "rechecked",
        _ => "rebuilt",
    };
    if format == OutputFormat::Json {
        let rebuild = Rebuild {
            changed: &applied.changed,
            rebuilt_files: &update.rebuilt_files,
            delta: &update.delta,
            errors,
            warnings,
            diagnostics: DiagnosticList(update.diagnostics.to_vec()),
            changed_diagnostic_files: &update.changed_diagnostic_files,
            verification_failed: update.divergence().is_some(),
            verification: match &update.verification {
                None => None,
                Some(Ok(())) => Some("passed".to_string()),
                Some(Err(msg)) => Some(msg.clone()),
            },
        };
        let line = match update.kind {
            UpdateKind::Checks => Line::Rechecked(rebuild),
            _ => Line::Rebuilt(rebuild),
        }
        .json();
        Rendered {
            line,
            warning: None,
        }
    } else {
        let line = format!(
            "[{stamp}] {event} {} file(s): +{} -{} ~{} nodes, +{} -{} edges | {} errors, {} warnings",
            update.rebuilt_files.len(),
            update.delta.added_nodes.len(),
            update.delta.removed_nodes.len(),
            update.delta.modified_nodes.len(),
            update.delta.added_edges.len(),
            update.delta.removed_edges.len(),
            errors,
            warnings
        );
        Rendered {
            line,
            warning: update
                .divergence()
                .map(|msg| format!("verification FAILED: {msg}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_project::{GraphDelta, Update};
    use std::sync::Arc;

    fn applied(kind: UpdateKind, verification: Option<Result<(), String>>) -> Applied {
        Applied {
            update: Update {
                kind,
                inputs_changed: false,
                delta: GraphDelta::default(),
                rebuilt_files: vec!["a.spec".to_string()],
                changed_diagnostic_files: Vec::new(),
                diagnostics: Arc::from(Vec::new()),
                previous_diagnostics: Arc::from(Vec::new()),
                verification,
            },
            changed: vec!["a.spec".to_string()],
            files: 3,
            nodes: 7,
            extensions: vec!["@specforge/software".to_string()],
        }
    }

    fn keys(line: &str) -> Vec<String> {
        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        keys
    }

    #[test]
    fn a_rebuild_renders_its_json_fields() {
        let rendered = render(
            &applied(UpdateKind::Sources, Some(Ok(()))),
            OutputFormat::Json,
            0,
        );
        assert_eq!(
            keys(&rendered.line),
            [
                "changed",
                "changed_diagnostic_files",
                "delta",
                "diagnostics",
                "errors",
                "event",
                "rebuilt_files",
                "verification",
                "verification_failed",
                "warnings",
            ]
        );
        let value: serde_json::Value = serde_json::from_str(&rendered.line).unwrap();
        assert_eq!(value["event"], "rebuilt");
        assert_eq!(value["verification"], "passed");
        assert_eq!(value["verification_failed"], false);
        assert_eq!(value["changed"], serde_json::json!(["a.spec"]));
    }

    #[specforge_test_macros::test(
        behavior = "notify_delta_subscribers",
        verify = "watch --format json prints each rebuild's GraphDelta, the one MCP notifies"
    )]
    fn a_rebuild_prints_the_graph_delta() {
        let mut rebuilt = applied(UpdateKind::Sources, Some(Ok(())));
        rebuilt.update.delta = GraphDelta {
            added_nodes: vec![specforge_project::NodeChange {
                id: "beta".to_string(),
                kind: "behavior".to_string(),
                file: "b.spec".to_string(),
                line: 1,
            }],
            affected_files: vec!["b.spec".to_string()],
            ..GraphDelta::default()
        };
        let rendered = render(&rebuilt, OutputFormat::Json, 0);
        let value: serde_json::Value = serde_json::from_str(&rendered.line).unwrap();
        assert_eq!(
            value["delta"],
            serde_json::to_value(&rebuilt.update.delta).unwrap()
        );
    }

    #[test]
    fn a_check_input_renders_rechecked() {
        let rendered = render(&applied(UpdateKind::Checks, None), OutputFormat::Json, 0);
        let value: serde_json::Value = serde_json::from_str(&rendered.line).unwrap();
        assert_eq!(value["event"], "rechecked");
        assert!(value.get("verification").is_none(), "{value}");
        let text = render(&applied(UpdateKind::Checks, None), OutputFormat::Human, 12);
        assert!(
            text.line.starts_with("[12] rechecked 1 file(s)"),
            "{}",
            text.line
        );
    }

    /// Every line `render` produces has no `null` and no key the line's
    /// derived schema does not declare.
    #[test]
    fn every_watch_line_is_its_derived_shape() {
        let schema = specforge_common::shape::document::<Line<'static>>();
        let mut lines = Vec::new();
        for kind in [
            UpdateKind::Sources,
            UpdateKind::Checks,
            UpdateKind::Environment,
        ] {
            for verification in [None, Some(Ok(())), Some(Err("nodes differ".to_string()))] {
                lines.push(render(&applied(kind, verification), OutputFormat::Json, 0).line);
            }
        }
        for line in lines {
            let value: serde_json::Value = serde_json::from_str(&line).unwrap();
            let violations = specforge_common::shape::violations(schema, &value);
            assert!(violations.is_empty(), "{line}: {violations:?}");
        }
    }

    #[test]
    fn a_reload_renders_extensions_reloaded() {
        let rendered = render(
            &applied(UpdateKind::Environment, None),
            OutputFormat::Json,
            0,
        );
        let value: serde_json::Value = serde_json::from_str(&rendered.line).unwrap();
        assert_eq!(value["event"], "extensions_reloaded");
        assert_eq!(
            value["extensions"],
            serde_json::json!(["@specforge/software"])
        );
        assert_eq!(value["files"], 3);
        assert_eq!(value["nodes"], 7);
        let text = render(
            &applied(UpdateKind::Environment, None),
            OutputFormat::Human,
            0,
        );
        assert_eq!(
            text.line,
            "[reload] extension environment changed: 1 extension(s), 3 file(s) | 0 errors, 0 warnings"
        );
    }

    #[test]
    fn a_divergence_adds_the_stderr_line_in_text_mode() {
        let diverged = applied(UpdateKind::Sources, Some(Err("nodes differ".to_string())));
        let text = render(&diverged, OutputFormat::Human, 0);
        assert_eq!(
            text.warning.as_deref(),
            Some("verification FAILED: nodes differ")
        );
        let json = render(&diverged, OutputFormat::Json, 0);
        assert_eq!(json.warning, None);
        let value: serde_json::Value = serde_json::from_str(&json.line).unwrap();
        assert_eq!(value["verification_failed"], true);
        assert_eq!(value["verification"], "nodes differ");
    }
}
