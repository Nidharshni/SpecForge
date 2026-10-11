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

use std::path::{Path, PathBuf};

use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal};
use specforge_ops::check::Counts;
use specforge_ops::{OpError, OpErrorKind};
use specforge_project::{ProjectSession, UpdateKind};
use specforge_watch::{Applied, DEFAULT_DEBOUNCE_WINDOW, Notify, SessionWatch, WatchEvent};

// `specforge watch`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Project directory
    #[arg(long, default_value = ".")]
    path: PathBuf,

    #[command(flatten)]
    output: OutputArg,

    /// Check every incremental rebuild against a cold rebuild (slower)
    #[arg(long, default_value_t = false)]
    verify_incremental: bool,
}

impl Args {
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let json = self.output.format == OutputFormat::Json;
        watch_project(&self.path, json, self.verify_incremental, terminal)
    }
}

fn watch_project(path: &Path, json: bool, verify_incremental: bool, terminal: &Terminal) -> Exit {
    // A debug build checks every rebuild (ProjectSession); a release build
    // only when asked (the check costs a cold rebuild per change).
    let mut session = ProjectSession::open(path);
    if verify_incremental {
        session.set_verify_incremental(true);
    }

    // Watch before announcing readiness: a client that writes on seeing
    // "ready" must never race a watcher that does not exist yet.
    let (watchers, batches) = Notify::new(DEFAULT_DEBOUNCE_WINDOW);
    let mut watch = match SessionWatch::start(session, watchers) {
        Ok(watch) => watch,
        Err(e) => {
            let format = if json {
                OutputFormat::Json
            } else {
                OutputFormat::Human
            };
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
    print_ready(watch.session(), json, terminal);

    // Debounced batches of changed paths. The session says what they are
    // and applies them: sources take the incremental path (tree-sitter
    // trees are retained, so unchanged subtrees are not re-parsed), an
    // environment input reloads the environment, a check input re-runs the
    // checks; anything else changes nothing.
    let debug = std::env::var("SPECFORGE_WATCH_DEBUG").is_ok();
    for batch in &batches {
        if debug {
            let roles: Vec<_> = batch.iter().map(|path| watch.classify(path)).collect();
            errln!(terminal, "[watch] batch: {batch:?} -> {roles:?}");
        }
        for event in watch.changed(&batch) {
            match event {
                WatchEvent::Applied(applied) => {
                    let rendered = render(&applied, json, unix_seconds());
                    outln!(terminal, "{}", rendered.line);
                    if let Some(warning) = rendered.warning {
                        errln!(terminal, "{warning}");
                    }
                }
                WatchEvent::Unwatched(e) => errln!(terminal, "warning: {e}"),
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
fn print_ready(session: &ProjectSession, json: bool, terminal: &Terminal) {
    let spec_root: PathBuf = std::fs::canonicalize(session.project().environment().spec_root())
        .unwrap_or_else(|_| session.project().environment().spec_root().to_path_buf());
    let diagnostics = session.project().diagnostics();
    let Counts {
        errors, warnings, ..
    } = Counts::of(&diagnostics);
    if json {
        outln!(
            terminal,
            "{}",
            serde_json::json!({
                "event": "ready",
                "spec_root": spec_root.to_string_lossy(),
                "files": session.project().file_count(),
                "nodes": session.project().graph().node_count(),
                "edges": session.project().graph().edge_count(),
                "errors": errors,
                "warnings": warnings,
                "diagnostics": specforge_common::diagnostics_json(&diagnostics),
            })
        );
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
fn render(applied: &Applied, json: bool, stamp: u64) -> Rendered {
    let update = &applied.update;
    let Counts {
        errors, warnings, ..
    } = Counts::of(&update.diagnostics);
    if update.kind == UpdateKind::Environment {
        let line = if json {
            serde_json::json!({
                "event": "extensions_reloaded",
                "extensions": applied.extensions,
                "files": applied.files,
                "nodes": applied.nodes,
                "errors": errors,
                "warnings": warnings,
                "diagnostics": specforge_common::diagnostics_json(&update.diagnostics),
            })
            .to_string()
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
    if json {
        let line = serde_json::json!({
            "event": event,
            "changed": applied.changed,
            "rebuilt_files": update.rebuilt_files,
            "added_nodes": update.delta.added_nodes.len(),
            "removed_nodes": update.delta.removed_nodes.len(),
            "modified_nodes": update.delta.modified_nodes.len(),
            "added_edges": update.delta.added_edges.len(),
            "removed_edges": update.delta.removed_edges.len(),
            "errors": errors,
            "warnings": warnings,
            "diagnostics": specforge_common::diagnostics_json(&update.diagnostics),
            "changed_diagnostic_files": update.changed_diagnostic_files,
            "verification_failed": update.divergence().is_some(),
            "verification": match &update.verification {
                None => serde_json::Value::Null,
                Some(Ok(())) => "passed".into(),
                Some(Err(msg)) => msg.clone().into(),
            },
        })
        .to_string();
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

    fn applied(kind: UpdateKind, verification: Option<Result<(), String>>) -> Applied {
        Applied {
            update: Update {
                kind,
                inputs_changed: false,
                delta: GraphDelta::default(),
                rebuilt_files: vec!["a.spec".to_string()],
                changed_diagnostic_files: Vec::new(),
                diagnostics: Vec::new(),
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
    fn a_rebuild_renders_todays_json_fields() {
        let rendered = render(&applied(UpdateKind::Sources, Some(Ok(()))), true, 0);
        assert_eq!(
            keys(&rendered.line),
            [
                "added_edges",
                "added_nodes",
                "changed",
                "changed_diagnostic_files",
                "diagnostics",
                "errors",
                "event",
                "modified_nodes",
                "rebuilt_files",
                "removed_edges",
                "removed_nodes",
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

    #[test]
    fn a_check_input_renders_rechecked() {
        let rendered = render(&applied(UpdateKind::Checks, None), true, 0);
        let value: serde_json::Value = serde_json::from_str(&rendered.line).unwrap();
        assert_eq!(value["event"], "rechecked");
        assert_eq!(value["verification"], serde_json::Value::Null);
        let text = render(&applied(UpdateKind::Checks, None), false, 12);
        assert!(
            text.line.starts_with("[12] rechecked 1 file(s)"),
            "{}",
            text.line
        );
    }

    #[test]
    fn a_reload_renders_extensions_reloaded() {
        let rendered = render(&applied(UpdateKind::Environment, None), true, 0);
        let value: serde_json::Value = serde_json::from_str(&rendered.line).unwrap();
        assert_eq!(value["event"], "extensions_reloaded");
        assert_eq!(
            value["extensions"],
            serde_json::json!(["@specforge/software"])
        );
        assert_eq!(value["files"], 3);
        assert_eq!(value["nodes"], 7);
        let text = render(&applied(UpdateKind::Environment, None), false, 0);
        assert_eq!(
            text.line,
            "[reload] extension environment changed: 1 extension(s), 3 file(s) | 0 errors, 0 warnings"
        );
    }

    #[test]
    fn a_divergence_adds_the_stderr_line_in_text_mode() {
        let diverged = applied(UpdateKind::Sources, Some(Err("nodes differ".to_string())));
        let text = render(&diverged, false, 0);
        assert_eq!(
            text.warning.as_deref(),
            Some("verification FAILED: nodes differ")
        );
        let json = render(&diverged, true, 0);
        assert_eq!(json.warning, None);
        let value: serde_json::Value = serde_json::from_str(&json.line).unwrap();
        assert_eq!(value["verification_failed"], true);
        assert_eq!(value["verification"], "nodes differ");
    }
}
