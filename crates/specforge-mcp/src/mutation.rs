//! What a mutation tool wrote, and the one place that acts on it (ADR 0022).
//!
//! A mutation handler returns [`Replied`]: its reply and, unless it only
//! previewed, a [`Written`] built from its operation's typed outcome. The
//! tools adapter of the request pipeline then calls [`refresh`] (inside the
//! call, while it holds the target) and [`report`]: nothing else in the
//! crate brings a target up to date after a write, names a mutation's
//! events or tells the client which files the call wrote.
//!
//! A mutation handler (format, rename, init, add_extension,
//! remove_extension, migrate) performs its real function against the same
//! library backends the CLI uses (canned placeholder responses are
//! forbidden: a tool either does real work or refuses with an explicit
//! error). It returns its reply and what its operation wrote, typed: the
//! files from the operation's [`specforge_ops::Writes`], the entities it
//! changed and its domain event; a preview says it only previewed. It never
//! refreshes the target or records an event itself: this module does.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Value, json};
use specforge_common::Diagnostic;
use specforge_common::shape::{Object, Shape};
use specforge_ops::{OpError, Writes};

use crate::reply::Answer;
use crate::surface_call::Event;
use crate::target::Call;
use crate::tool::{McpError, ToolOutcome};

/// The reply key naming the files a mutation wrote, relative to the call
/// target's root (absolute outside it), sorted.
pub const FILES_WRITTEN: &str = "files_written";

/// What one mutation call wrote.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Written {
    /// Every file the call created, rewrote or removed: its operation's
    /// [`Writes`], whether the call then succeeded or failed.
    pub files: Writes,
    /// The IDs of the entities the call changed: the renamed entity (by its
    /// new ID), the entities a removal strands, the entities an inference
    /// step produced.
    pub entities: BTreeSet<String>,
    /// The domain event the call produces; recorded only when it succeeded.
    pub event: Option<MutationEvent>,
    /// Put what `specforge check` reports for the target, once brought up
    /// to date, in the reply's `diagnostics` (rename's contract).
    pub fresh_diagnostics: bool,
}

impl Written {
    /// The call wrote `files`.
    pub fn files(files: Writes) -> Self {
        Written {
            files,
            ..Written::default()
        }
    }

    /// The call wrote nothing (a run that found nothing to do, or failed
    /// before writing).
    pub fn nothing() -> Self {
        Written::default()
    }

    /// The call changed the entities `ids`.
    pub fn with_entities<I, S>(mut self, ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.entities.extend(ids.into_iter().map(Into::into));
        self
    }

    /// The call produces `event` when it succeeds.
    pub fn with_event(mut self, event: MutationEvent) -> Self {
        self.event = Some(event);
        self
    }

    /// The reply carries the target's diagnostics once brought up to date.
    pub fn with_fresh_diagnostics(mut self) -> Self {
        self.fresh_diagnostics = true;
        self
    }
}

/// A domain event a mutation produces (`spec/behaviors/mcp-operations.spec`:
/// init produces `project_initialized`, add_extension `extension_added`),
/// its payload the one `spec/events/compilation.spec` declares.
#[derive(Debug, Clone, PartialEq)]
pub enum MutationEvent {
    /// `extension_added`: an add that succeeded, a duplicate included.
    ExtensionAdded {
        /// The specifier as the call gave it.
        specifier: String,
        /// How many extensions `specforge.json` enables after the add.
        total_extensions: usize,
        /// The extension was already installed (or enabled): nothing
        /// changed.
        was_duplicate: bool,
    },
    /// `project_initialized`: the scaffold init wrote.
    ProjectInitialized {
        project_name: String,
        extension_count: usize,
        /// The starter entity spec file, relative to the new root (not
        /// `specforge.json`).
        spec_file_path: String,
    },
}

impl MutationEvent {
    /// The event's name, as `spec/events` declares it.
    pub fn name(&self) -> &'static str {
        match self {
            MutationEvent::ExtensionAdded { .. } => "extension_added",
            MutationEvent::ProjectInitialized { .. } => "project_initialized",
        }
    }

    /// Its payload, camelCase as declared (`timestamp` is added when
    /// recorded).
    pub fn params(&self) -> Value {
        match self {
            MutationEvent::ExtensionAdded {
                specifier,
                total_extensions,
                was_duplicate,
            } => json!({
                "extensionSpecifier": specifier,
                "totalExtensions": total_extensions,
                "wasDuplicate": was_duplicate,
            }),
            MutationEvent::ProjectInitialized {
                project_name,
                extension_count,
                spec_file_path,
            } => json!({
                "projectName": project_name,
                "extensionCount": extension_count,
                "specFilePath": spec_file_path,
            }),
        }
    }
}

/// The untyped mutation result the pipeline refreshes and reports: the
/// outcome of a handler's typed [`Mutated`] ([`replied`]) and what it wrote.
#[derive(Debug)]
pub struct Replied {
    pub outcome: ToolOutcome,
    /// `None`: the call only previewed (`dry_run`, `check`, `diff`) and is
    /// no mutation: no refresh, no `mcp_mutation_completed`, no
    /// `files_written`.
    pub written: Option<Written>,
}

impl Replied {
    /// Refused before it wrote anything: a failed mutation.
    pub fn refused(error: impl Into<ToolOutcome>) -> Self {
        Replied {
            outcome: error.into(),
            written: Some(Written::nothing()),
        }
    }
}

/// A mutation that ran: its answer (the reply, the diagnostics that ride in
/// `_meta.diagnostics`, its other `_meta`) and what it wrote. A preview's
/// [`Written`] holds no file and is dropped.
#[derive(Debug)]
pub struct Mutated<R> {
    answer: Answer<R>,
    written: Written,
}

impl<R> Mutated<R> {
    /// The one constructor of a run.
    pub fn new(answer: impl Into<Answer<R>>, written: Written) -> Self {
        Mutated {
            answer: answer.into(),
            written,
        }
    }
}

/// A mutation that failed: its McpError and the files it left written (a
/// format that failed on one file, a migration rolled back), which a failure
/// that is no preview lists as `data.files_written`. A failure changes no
/// entity and produces no domain event.
#[derive(Debug)]
pub struct Failed {
    error: Box<McpError>,
    files: Writes,
}

impl Failed {
    /// `error`, having left `files` written.
    pub fn after(error: impl Into<Box<McpError>>, files: Writes) -> Self {
        Failed {
            error: error.into(),
            files,
        }
    }

    /// The same failure, its McpError mapped (naming the argument it is
    /// about).
    pub fn map_error(mut self, map: impl FnOnce(McpError) -> McpError) -> Self {
        self.error = Box::new(map(*self.error));
        self
    }

    /// The same failure, `extra` added to the diagnostics its reply carries
    /// in `_meta.diagnostics`.
    pub fn with_diagnostics(mut self, extra: Vec<Diagnostic>) -> Self {
        self.error.reported.extend(extra);
        self
    }
}

/// Failed before it wrote anything.
impl From<McpError> for Failed {
    fn from(error: McpError) -> Self {
        Failed::after(error, Writes::none())
    }
}

/// Failed before it wrote anything.
impl From<Box<McpError>> for Failed {
    fn from(error: Box<McpError>) -> Self {
        Failed::after(error, Writes::none())
    }
}

/// An operation's failure: what it left written is its [`OpError::writes`]
/// (ADR 0022 D4).
impl From<OpError> for Failed {
    fn from(mut error: OpError) -> Self {
        let files = std::mem::take(&mut error.writes);
        Failed::after(McpError::from(error), files)
    }
}

/// What a mutation handler returns. `?` on an `OpError`, an `McpError` or a
/// `Box<McpError>` is a failure, carrying what an operation left written.
pub type Mutation<R> = Result<Mutated<R>, Failed>;

/// A mutation tool's arguments (its module's `Args`).
pub trait MutationArgs: crate::args::Arguments {
    /// The call only previews (`dry_run`; format's `check` or `diff` without
    /// `write`): it writes nothing and is no mutation. No refresh, no domain
    /// event, no `mcp_mutation_completed`, no `files_written`, whatever its
    /// operation answered (ADR 0022 D2).
    fn previews(&self) -> bool;
}

/// `mutation` serialized: what the pipeline refreshes and reports. A call
/// that `preview`s is no mutation whatever its handler built: its `Written`
/// is dropped.
pub fn replied<R: Object + Serialize>(preview: bool, mutation: Mutation<R>) -> Replied {
    let (outcome, written) = match mutation {
        Ok(Mutated { answer, written }) => (crate::reply::structured(Ok(answer)), written),
        Err(Failed { error, files }) => (ToolOutcome::Refused(error), Written::files(files)),
    };
    debug_assert!(
        !preview || written.files.is_empty(),
        "a preview wrote files"
    );
    Replied {
        outcome,
        written: (!preview).then_some(written),
    }
}

/// A mutation's reply as sent: the tool's reply and, unless it only
/// previewed, the files it wrote ([`FILES_WRITTEN`], set by [`report`]).
#[derive(Serialize, Shape)]
pub struct WrittenReply<R> {
    #[serde(flatten)]
    pub reply: R,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_written: Option<Vec<String>>,
}

/// The outputSchema of a mutation whose reply is `R`: [`WrittenReply<R>`]'s.
pub fn output_schema<R: Object>() -> Value {
    WrittenReply::<R>::schema()
}

/// Bring the call's target up to date with what the mutation wrote, when
/// it wrote anything, succeeded or not; then, when asked, put the target's
/// diagnostics in the reply. The served project is brought up to date
/// (`ensure_fresh`); another project is compiled again; the directory init
/// created is served when nothing is (ADR 0014 D5).
///
/// Returns the root the written files are named from: the call target's
/// (the directory init created, for init).
pub(crate) fn refresh(call: &mut Call<'_>, mutated: &mut Replied) -> Option<PathBuf> {
    if let Some(written) = &mutated.written {
        let wrote = !written.files.is_empty();
        if wrote || written.fresh_diagnostics {
            let diagnostics = call.bring_up_to_date();
            if written.fresh_diagnostics {
                let diagnostics =
                    serde_json::to_value(specforge_common::diagnostics_json(&diagnostics))
                        .unwrap_or_default();
                mutated.outcome.set_field("diagnostics", diagnostics);
            }
        }
    }
    call.written_root().map(Path::to_path_buf)
}

/// What the completed mutation produced, and its reply: the events to
/// record, the domain event when it succeeded, then `mcp_mutation_completed`
/// (`files_changed` = the files written, `entities_affected` = the entities
/// changed, `success`); and the reply's `files_written` (the same files,
/// named from `root`; on a refusal, in its `data`). A preview records
/// nothing and is returned as it is.
pub(crate) fn report(
    tool: &str,
    root: Option<&Path>,
    mutated: Replied,
) -> (ToolOutcome, Vec<Event>) {
    let Replied { outcome, written } = mutated;
    let Some(written) = written else {
        return (outcome, Vec::new());
    };
    let success = outcome.succeeded();
    let mut events = Vec::new();
    if success && let Some(event) = &written.event {
        events.push((event.name().to_string(), event.params()));
    }
    events.push((
        "mcp_mutation_completed".to_string(),
        json!({
            "toolName": tool,
            "files_changed": written.files.len(),
            "entities_affected": written.entities.len(),
            "success": success,
        }),
    ));
    let files: Vec<String> = match root {
        Some(root) => written.files.names_under(root),
        None => written
            .files
            .paths()
            .map(|path| path.display().to_string())
            .collect(),
    };
    (
        outcome.with_field(FILES_WRITTEN, Value::from(files)),
        events,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::ErrorCode;

    /// The `mcp_mutation_completed` payloads among `events`.
    fn completed(events: &[Event]) -> Vec<Value> {
        events
            .iter()
            .filter(|(name, _)| name == "mcp_mutation_completed")
            .map(|(_, params)| params.clone())
            .collect()
    }

    fn payload(outcome: &ToolOutcome) -> Value {
        match outcome {
            ToolOutcome::Done {
                payload: crate::tool::Payload::Json(value),
                ..
            } => value.clone(),
            ToolOutcome::Refused(error) => error.to_json(),
            other => panic!("not JSON: {other:?}"),
        }
    }

    #[test]
    fn a_preview_reports_nothing() {
        let (outcome, events) = report(
            "specforge.rename",
            Some(Path::new("/p")),
            Replied {
                outcome: ToolOutcome::ok(json!({"dry_run": true})),
                written: None,
            },
        );
        assert!(events.is_empty());
        assert_eq!(payload(&outcome), json!({"dry_run": true}));
    }

    #[test]
    fn a_failed_call_reports_its_writes_and_no_domain_event() {
        let failure = McpError::new(ErrorCode::InternalError, "failed to write /p/a.spec");
        let written = Written::files(Writes::from_iter(["/p/b.spec"])).with_event(
            MutationEvent::ExtensionAdded {
                specifier: "@x/y".into(),
                total_extensions: 1,
                was_duplicate: false,
            },
        );

        let (outcome, events) = report(
            "specforge.format",
            Some(Path::new("/p")),
            Replied {
                outcome: failure.into(),
                written: Some(written),
            },
        );

        assert!(!events.iter().any(|(name, _)| name == "extension_added"));
        assert_eq!(
            completed(&events),
            [
                json!({"toolName": "specforge.format", "files_changed": 1, "entities_affected": 0, "success": false})
            ]
        );
        assert_eq!(
            payload(&outcome)["data"]["files_written"],
            json!(["b.spec"])
        );
    }

    #[test]
    fn the_domain_event_precedes_the_mutation_event() {
        let written = Written::files(Writes::from_iter(["/p/specforge.json"])).with_event(
            MutationEvent::ExtensionAdded {
                specifier: "@x/y".into(),
                total_extensions: 1,
                was_duplicate: false,
            },
        );

        let (outcome, events) = report(
            "specforge.add_extension",
            Some(Path::new("/p")),
            Replied {
                outcome: ToolOutcome::ok(json!({"installed": true})),
                written: Some(written),
            },
        );

        let names: Vec<&str> = events.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["extension_added", "mcp_mutation_completed"]);
        assert_eq!(
            payload(&outcome),
            json!({"installed": true, "files_written": ["specforge.json"]})
        );
    }

    #[test]
    fn event_names_are_the_specs() {
        let added = MutationEvent::ExtensionAdded {
            specifier: "@x/y".into(),
            total_extensions: 2,
            was_duplicate: true,
        };
        let initialized = MutationEvent::ProjectInitialized {
            project_name: "demo".into(),
            extension_count: 1,
            spec_file_path: "spec/hello.spec".into(),
        };
        assert_eq!(added.name(), "extension_added");
        assert_eq!(
            added.params(),
            json!({"extensionSpecifier": "@x/y", "totalExtensions": 2, "wasDuplicate": true})
        );
        assert_eq!(initialized.name(), "project_initialized");
        assert_eq!(
            initialized.params(),
            json!({"projectName": "demo", "extensionCount": 1, "specFilePath": "spec/hello.spec"})
        );
    }

    #[test]
    fn a_mutation_that_wrote_nothing_lists_no_file() {
        let (outcome, events) = report(
            "specforge.migrate",
            Some(Path::new("/p")),
            Replied {
                outcome: ToolOutcome::ok(json!({})),
                written: Some(Written::nothing()),
            },
        );
        assert_eq!(payload(&outcome), json!({"files_written": []}));
        assert_eq!(
            completed(&events),
            [
                json!({"toolName": "specforge.migrate", "files_changed": 0, "entities_affected": 0, "success": true})
            ]
        );
    }

    #[derive(Serialize, Shape)]
    struct Probe {
        name: String,
    }

    #[test]
    fn the_written_reply_states_files_written() {
        let schema = output_schema::<Probe>();
        assert_eq!(
            schema["properties"][FILES_WRITTEN],
            json!({"type": "array", "items": {"type": "string"}})
        );
        assert_eq!(schema["required"], json!(["name"]));
        assert_eq!(schema["additionalProperties"], false);
    }

    #[test]
    fn a_run_and_a_failure_are_replied_with_what_they_wrote() {
        let written = Written::files(Writes::from_iter(["/p/a.spec"]));
        let wrote = replied(false, Ok(Mutated::new(Probe { name: "x".into() }, written)));
        assert_eq!(payload(&wrote.outcome), json!({"name": "x"}));
        assert_eq!(wrote.written.expect("it wrote").files.len(), 1);

        let preview = replied(
            true,
            Ok(Mutated::new(Probe { name: "y".into() }, Written::nothing())),
        );
        assert!(preview.written.is_none());

        let failed = replied::<Probe>(
            false,
            Err(Failed::after(
                McpError::new(ErrorCode::Conflict, "no"),
                Writes::from_iter(["/p/b.spec"]),
            )),
        );
        assert!(!failed.outcome.succeeded());
        assert_eq!(failed.written.expect("it failed").files.len(), 1);

        let refused = replied::<Probe>(true, Err(McpError::new(ErrorCode::Conflict, "no").into()));
        assert!(!refused.outcome.succeeded());
        assert!(refused.written.is_none());
    }

    #[test]
    fn a_preview_records_nothing_whatever_its_handler_answered() {
        let event = MutationEvent::ExtensionAdded {
            specifier: "@x/y".into(),
            total_extensions: 1,
            was_duplicate: true,
        };
        let replied = replied(
            true,
            Ok(Mutated::new(
                Probe { name: "x".into() },
                Written::files(Writes::none()).with_event(event),
            )),
        );
        assert!(replied.written.is_none());
    }

    #[test]
    fn an_operation_failure_keeps_what_it_left_written() {
        let mut error = OpError::new(
            specforge_ops::OpErrorKind::Internal,
            "write_failed",
            "failed",
        );
        error.writes = Writes::from_iter(["/p/a"]);
        let failed = Failed::from(error);
        assert_eq!(failed.files.len(), 1);
        let written = replied::<Probe>(false, Err(failed))
            .written
            .expect("failed");
        assert_eq!(written.files.len(), 1);
    }
}
