//! What a mutation tool's call produced, and the one place that completes it
//! (ADR 0022).
//!
//! A mutation handler is given its input and its typed arguments and returns
//! a [`Mutation`]: a run ([`Mutated::new`]: its reply and what it
//! [`Written`]) or a failure ([`Failed`]: its McpError and the files it left
//! written). Whether the call only previews is its arguments'
//! ([`MutationArgs::previews`]), read before the handler runs; what the reply
//! carries after the write is its type's ([`MutationReply::After`]). The
//! tools adapter then calls [`complete`]: it brings the target up to date,
//! sends the reply once as a [`WrittenReply`], checks it against the tool's
//! outputSchema and records the events. Nothing else refreshes a target after
//! a write, records a mutation's events, or adds a key to a reply.
//!
//! A mutation handler (format, rename, init, add_extension,
//! remove_extension, migrate) performs its real function against the same
//! library backends the CLI uses (canned placeholder responses are
//! forbidden: a tool either does real work or refuses with an explicit
//! error). It never refreshes the target or records an event itself: this
//! module does.

use std::collections::BTreeSet;
use std::path::Path;

use serde::Serialize;
use serde_json::{Value, json};
use specforge_common::shape::{Object, Shape};
use specforge_common::{Diagnostic, DiagnosticList};
use specforge_ops::{OpError, Writes};

use crate::reply::Answer;
use crate::surface_call::Event;
use crate::target::Call;
use crate::tool::{McpError, Payload, ToolOutcome};

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

/// A mutation tool's reply (its module's `Reply`, or the ops document it
/// answers).
pub trait MutationReply: Object + Serialize + 'static {
    /// What its reply carries after a successful write, beside
    /// `files_written`: [`NoDiagnostics`], or [`FreshDiagnostics`] (rename's
    /// contract).
    type After: AfterWrite;
}

/// What the pipeline adds to a mutation's reply after the call, beside
/// `files_written`. Two adapters: [`NoDiagnostics`], [`FreshDiagnostics`].
pub trait AfterWrite: Object + Serialize + 'static {
    /// It reads the call target's diagnostics. After a successful call, the
    /// pipeline then brings the target up to date even when the call wrote
    /// no file.
    const READS_DIAGNOSTICS: bool;
    /// The report. `diagnostics` are the target's, once brought up to date,
    /// after a successful call when [`Self::READS_DIAGNOSTICS`]; `None`
    /// otherwise (a preview included).
    fn of(diagnostics: Option<Vec<Diagnostic>>) -> Self;
}

/// The reply carries nothing after the write.
#[derive(Debug, Serialize, Shape)]
pub struct NoDiagnostics {}

impl AfterWrite for NoDiagnostics {
    const READS_DIAGNOSTICS: bool = false;

    fn of(_: Option<Vec<Diagnostic>>) -> Self {
        NoDiagnostics {}
    }
}

/// The reply carries what `specforge check` reports for the call target once
/// a successful call brought it up to date, as `diagnostics` (absent from a
/// preview). A reply that has its own `diagnostics` key (format's W141/W142)
/// cannot take it; [`output_schema`] refuses the pair.
#[derive(Debug, Serialize, Shape)]
pub struct FreshDiagnostics {
    #[serde(skip_serializing_if = "Option::is_none")]
    diagnostics: Option<DiagnosticList>,
}

impl AfterWrite for FreshDiagnostics {
    const READS_DIAGNOSTICS: bool = true;

    fn of(diagnostics: Option<Vec<Diagnostic>>) -> Self {
        FreshDiagnostics {
            diagnostics: diagnostics.map(DiagnosticList),
        }
    }
}

/// A mutation's reply as sent: the tool's reply `R`, the files the call wrote
/// ([`FILES_WRITTEN`], absent from a preview) and its after-write report `A`
/// (`<R as MutationReply>::After`). Built only by [`complete`]. Its derived
/// schema is the tool's outputSchema ([`output_schema`]).
#[derive(Serialize, Shape)]
pub struct WrittenReply<R, A> {
    #[serde(flatten)]
    reply: R,
    #[serde(skip_serializing_if = "Option::is_none")]
    files_written: Option<Vec<String>>,
    #[serde(flatten)]
    after: A,
}

/// The outputSchema of a mutation whose reply is `R`: `WrittenReply<R,
/// R::After>`'s document, built once per process. Debug builds assert that
/// `R` declares neither `files_written` nor a key its report adds.
pub fn output_schema<R: MutationReply>() -> &'static Value {
    #[cfg(debug_assertions)]
    {
        let own = property_names(specforge_common::shape::document::<R>());
        let added = property_names(specforge_common::shape::document::<R::After>());
        assert!(
            !own.contains(FILES_WRITTEN) && own.is_disjoint(&added),
            "a mutation reply declares a key the mutation pipeline adds: \
             {own:?} against {FILES_WRITTEN} and {added:?}"
        );
    }
    specforge_common::shape::document::<WrittenReply<R, R::After>>()
}

/// The property names of `schema`, or of each branch of its `oneOf`/`anyOf`.
#[cfg(debug_assertions)]
fn property_names(schema: &Value) -> std::collections::BTreeSet<String> {
    let mut names = std::collections::BTreeSet::new();
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        names.extend(properties.keys().cloned());
    }
    for key in ["oneOf", "anyOf"] {
        for branch in schema
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            names.extend(property_names(branch));
        }
    }
    names
}

/// Complete a mutation call whose handler ran, in this order:
/// - a preview is sent as it is (`WrittenReply` without `files_written`, its
///   report empty) and records nothing;
/// - otherwise the call target is brought up to date when the call wrote a
///   file, or its report reads the diagnostics of a successful call (ADR 0014
///   call-target rules; init's directory is served when nothing is);
/// - the reply is sent once: a run as `WrittenReply { reply, files_written,
///   after }`, checked against `schema` (a reply it refuses is
///   `schema_mismatch`, still listing the files in `data`); a failure as its
///   McpError with `data.files_written`;
/// - the domain event is recorded when the reply sent succeeded, then
///   `mcp_mutation_completed` (`files_changed`, `entities_affected`,
///   `success` = what the client got).
///
/// Returns the reply and the events to record, in order.
pub(crate) fn complete<R: MutationReply>(
    call: &mut Call<'_>,
    tool: &str,
    schema: &Value,
    preview: bool,
    mutation: Mutation<R>,
) -> (ToolOutcome, Vec<Event>) {
    if preview {
        return (previewed(tool, schema, mutation), Vec::new());
    }
    let wrote = match &mutation {
        Ok(mutated) => !mutated.written.files.is_empty(),
        Err(failed) => !failed.files.is_empty(),
    };
    let reads = mutation.is_ok() && <R::After as AfterWrite>::READS_DIAGNOSTICS;
    let diagnostics = (wrote || reads).then(|| call.bring_up_to_date());
    let root = call.written_root().map(Path::to_path_buf);
    finished(
        tool,
        schema,
        root.as_deref(),
        mutation,
        diagnostics.filter(|_| reads),
    )
}

/// A mutation refused before its handler ran (arguments that do not parse, a
/// call target with no project): a failed mutation that wrote nothing
/// (`data.files_written: []`, `mcp_mutation_completed` with `success:
/// false`). Arguments that do not parse cannot say they asked for a preview.
pub(crate) fn refused(tool: &str, error: impl Into<Box<McpError>>) -> (ToolOutcome, Vec<Event>) {
    failure(tool, None, Failed::from(error.into()))
}

/// A preview's reply, checked against `schema`.
fn previewed<R: MutationReply>(tool: &str, schema: &Value, mutation: Mutation<R>) -> ToolOutcome {
    match mutation {
        Ok(Mutated { answer, .. }) => crate::reply::conforming(
            tool,
            crate::reply::Source::Core,
            schema,
            sent(answer, None, <R::After as AfterWrite>::of(None)),
        ),
        Err(Failed { error, .. }) => ToolOutcome::Refused(error),
    }
}

/// What a call that was no preview sends and records. `root` names its
/// files, and `diagnostics` are the target's when its report reads them.
fn finished<R: MutationReply>(
    tool: &str,
    schema: &Value,
    root: Option<&Path>,
    mutation: Mutation<R>,
    diagnostics: Option<Vec<Diagnostic>>,
) -> (ToolOutcome, Vec<Event>) {
    match mutation {
        Ok(Mutated { answer, written }) => {
            let files = named(root, &written.files);
            let after = <R::After as AfterWrite>::of(diagnostics);
            let sent = sent(answer, Some(files.clone()), after);
            let outcome =
                match crate::reply::conforming(tool, crate::reply::Source::Core, schema, sent) {
                    // A reply its schema refuses still tells the client what the
                    // call wrote.
                    ToolOutcome::Refused(mut error) => {
                        error.set_data_field(FILES_WRITTEN, Value::from(files));
                        ToolOutcome::Refused(error)
                    }
                    sent => sent,
                };
            let events = events(tool, outcome.succeeded(), &written);
            (outcome, events)
        }
        Err(failed) => failure(tool, root, failed),
    }
}

/// A failure that is no preview: its McpError with `data.files_written`,
/// `success: false`.
fn failure(tool: &str, root: Option<&Path>, failed: Failed) -> (ToolOutcome, Vec<Event>) {
    let Failed { mut error, files } = failed;
    error.set_data_field(FILES_WRITTEN, Value::from(named(root, &files)));
    let written = Written::files(files);
    (ToolOutcome::Refused(error), events(tool, false, &written))
}

/// `answer` sent as a `WrittenReply` (structured payload,
/// `_meta.diagnostics`, `_meta`).
fn sent<R: Object + Serialize, A: AfterWrite>(
    answer: Answer<R>,
    files_written: Option<Vec<String>>,
    after: A,
) -> ToolOutcome {
    let Answer {
        reply,
        diagnostics,
        meta,
    } = answer;
    let payload = WrittenReply {
        reply,
        files_written,
        after,
    };
    ToolOutcome::Done {
        payload: Payload::Json(serde_json::to_value(&payload).expect("a reply serializes")),
        is_error: false,
        diagnostics,
        meta,
    }
}

/// The domain event when `success`, then `mcp_mutation_completed`
/// (`files_changed` = the files written, `entities_affected` = the entities
/// changed, `success`).
fn events(tool: &str, success: bool, written: &Written) -> Vec<Event> {
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
    events
}

/// `files` named from `root` ([`Writes::names_under`]), else displayed
/// absolute.
fn named(root: Option<&Path>, files: &Writes) -> Vec<String> {
    match root {
        Some(root) => files.names_under(root),
        None => files
            .paths()
            .map(|path| path.display().to_string())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::ErrorCode;
    use specforge_common::{Diagnostic, Severity};

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
                payload: Payload::Json(value),
                ..
            } => value.clone(),
            ToolOutcome::Refused(error) => error.to_json(),
            other => panic!("not JSON: {other:?}"),
        }
    }

    #[derive(Serialize, Shape)]
    struct Probe {
        name: String,
    }

    impl MutationReply for Probe {
        type After = NoDiagnostics;
    }

    #[derive(Serialize, Shape)]
    struct FreshProbe {
        name: String,
    }

    impl MutationReply for FreshProbe {
        type After = FreshDiagnostics;
    }

    fn probe(name: &str) -> Probe {
        Probe { name: name.into() }
    }

    fn added() -> MutationEvent {
        MutationEvent::ExtensionAdded {
            specifier: "@x/y".into(),
            total_extensions: 1,
            was_duplicate: false,
        }
    }

    #[test]
    fn a_preview_is_sent_as_it_is_and_records_nothing() {
        let schema = output_schema::<Probe>();
        let outcome = previewed(
            "specforge.rename",
            schema,
            Ok(Mutated::new(probe("x"), Written::nothing())),
        );
        assert_eq!(payload(&outcome), json!({"name": "x"}));

        let failure = McpError::new(ErrorCode::Conflict, "no");
        let outcome = previewed::<Probe>("specforge.rename", schema, Err(failure.into()));
        assert!(!outcome.succeeded());
        assert!(payload(&outcome).get("data").is_none());
    }

    #[test]
    fn a_failure_reports_its_writes_and_no_domain_event() {
        let failure = McpError::new(ErrorCode::InternalError, "failed to write /p/a.spec");
        let schema = output_schema::<Probe>();

        let (outcome, events) = finished::<Probe>(
            "specforge.format",
            schema,
            Some(Path::new("/p")),
            Err(Failed::after(failure, Writes::from_iter(["/p/b.spec"]))),
            None,
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
        let written = Written::files(Writes::from_iter(["/p/specforge.json"])).with_event(added());
        let schema = output_schema::<Probe>();

        let (outcome, events) = finished(
            "specforge.add_extension",
            schema,
            Some(Path::new("/p")),
            Ok(Mutated::new(probe("x"), written)),
            None,
        );

        let names: Vec<&str> = events.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["extension_added", "mcp_mutation_completed"]);
        assert_eq!(
            payload(&outcome),
            json!({"name": "x", "files_written": ["specforge.json"]})
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
        let schema = output_schema::<Probe>();
        let (outcome, events) = finished(
            "specforge.migrate",
            schema,
            Some(Path::new("/p")),
            Ok(Mutated::new(probe("x"), Written::nothing())),
            None,
        );
        assert_eq!(payload(&outcome), json!({"name": "x", "files_written": []}));
        assert_eq!(
            completed(&events),
            [
                json!({"toolName": "specforge.migrate", "files_changed": 0, "entities_affected": 0, "success": true})
            ]
        );
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

    #[specforge_test_macros::test(
        behavior = "list_mcp_tools",
        verify = "what the pipeline adds to a mutation's reply is declared once, beside its reply: files_written, and rename's diagnostics"
    )]
    fn fresh_diagnostics_ride_beside_files_written() {
        let fresh = output_schema::<FreshProbe>();
        assert_eq!(
            fresh["properties"]["diagnostics"],
            specforge_common::shape::build::<DiagnosticList>()
        );
        assert!(fresh["properties"][FILES_WRITTEN].is_object());
        assert_eq!(fresh["required"], json!(["name"]));
        assert!(
            output_schema::<Probe>()["properties"]
                .get("diagnostics")
                .is_none()
        );

        let w001 = Diagnostic::untyped("W001", Severity::Warning, "careful");
        let schema = output_schema::<FreshProbe>();
        let (outcome, _) = finished(
            "specforge.rename",
            schema,
            Some(Path::new("/p")),
            Ok(Mutated::new(
                FreshProbe { name: "x".into() },
                Written::files(Writes::from_iter(["/p/a.spec"])),
            )),
            Some(vec![w001.clone()]),
        );
        let sent = payload(&outcome);
        assert_eq!(
            sent["diagnostics"],
            serde_json::to_value(specforge_common::diagnostics_json(&[w001])).unwrap()
        );
        assert_eq!(sent["files_written"], json!(["a.spec"]));

        let outcome = previewed(
            "specforge.rename",
            schema,
            Ok(Mutated::new(
                FreshProbe { name: "x".into() },
                Written::nothing(),
            )),
        );
        assert_eq!(payload(&outcome), json!({"name": "x"}));
    }

    #[test]
    fn a_reply_its_schema_refuses_keeps_files_written_in_data() {
        let schema = json!({
            "type": "object",
            "properties": {"other": {"type": "string"}},
            "required": ["other"],
        });
        let (outcome, events) = finished(
            "specforge.migrate",
            &schema,
            Some(Path::new("/p")),
            Ok(Mutated::new(
                probe("x"),
                Written::files(Writes::from_iter(["/p/a.spec"])),
            )),
            None,
        );
        let error = payload(&outcome);
        assert_eq!(error["code"], "schema_mismatch");
        assert_eq!(error["data"]["files_written"], json!(["a.spec"]));
        assert!(error["data"]["violations"].is_array());
        assert_eq!(
            completed(&events),
            [
                json!({"toolName": "specforge.migrate", "files_changed": 1, "entities_affected": 0, "success": false})
            ]
        );
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
        let schema = output_schema::<Probe>();
        let (_, events) = finished::<Probe>("specforge.rename", schema, None, Err(failed), None);
        assert_eq!(
            completed(&events),
            [
                json!({"toolName": "specforge.rename", "files_changed": 1, "entities_affected": 0, "success": false})
            ]
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "declares a key the mutation pipeline adds")]
    fn a_reply_may_not_declare_what_the_pipeline_adds() {
        #[derive(Serialize, Shape)]
        struct Clashing {
            files_written: Vec<String>,
        }
        impl MutationReply for Clashing {
            type After = NoDiagnostics;
        }
        let _ = output_schema::<Clashing>();
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "declares a key the mutation pipeline adds")]
    fn a_reply_with_its_own_diagnostics_cannot_take_fresh_ones() {
        #[derive(Serialize, Shape)]
        struct Clashing2 {
            diagnostics: Vec<u8>,
        }
        impl MutationReply for Clashing2 {
            type After = FreshDiagnostics;
        }
        let _ = output_schema::<Clashing2>();
    }
}
