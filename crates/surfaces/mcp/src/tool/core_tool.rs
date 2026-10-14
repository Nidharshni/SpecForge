use super::{CORE_SOURCE, Category, Effect, McpError, ReplySpec, ToolOutcome};
use crate::args::{Argument, Arguments};
use crate::events::EventSink;
use crate::lifecycle::Revision;
use crate::mutation::{Mutation, MutationArgs, MutationReply};
use crate::registry::{ToolAnnotations, ToolDescriptor};
use crate::reply::{Answered, Object, Source, Text};
use crate::target::{Call, Input, TargetSpec};
use serde::Serialize;
use serde_json::Value;
use std::marker::PhantomData;

/// One core MCP tool, stated once in its own module under `tools/`
/// (CONTEXT.md "Tool spec"). Implemented on the module's unit struct
/// `Tool`; listed by [`crate::tools::CORE_TOOLS`] as [`spec`]`(&x::Tool)`.
/// Everything the server lists, dispatches, checks and reports about the
/// tool derives from it ([`ToolSpec`]).
pub trait CoreTool: Sync + 'static {
    /// Its name as `tools/list` lists it and every prompt names it.
    const NAME: &'static str;
    /// Its `tools/list` description.
    const DESCRIPTION: &'static str;
    /// What it does to its environment.
    const EFFECT: Effect;
    /// Where the spec states its reply; a text reply states none.
    const SPEC: ReplySpec = ReplySpec::Text;
    /// The paths of its outputSchema (`$.nodes[].fields`) whose content an
    /// extension defines: open values no core type can close (ADR 0048 D1).
    /// The reflection test holds the schema to exactly these.
    const OPEN_VALUES: &'static [&'static str] = &[];
    /// The typed arguments it reads (ADR 0033), as its reply kind reads
    /// them before the handler runs: a mutation's arguments say whether the
    /// call only previews (`MutationArgs`); a read's say nothing.
    type Args: Arguments + Previews<Self::Returns>;
    /// What its handler is given before its arguments, and so its call
    /// target (ADR 0024 A2): `Unscoped`, `View`, `Project` or `NewProject`
    /// ([`crate::target`]).
    type Input: Input;
    /// What its handler returns, and so its reply kind: `Answered<Reply>`,
    /// `Answered<Text>` or `Mutation<Reply>` ([`Handled`]).
    type Returns: Handled;
    /// The handler: its input, then its typed arguments.
    fn call(input: <Self::Input as Input>::Given<'_>, args: Self::Args) -> Self::Returns;
}

/// A tool's arguments as its reply kind reads them before the handler takes
/// them: whether the call only previews (ADR 0022). Two blanket
/// implementations, never written by hand: a read never previews, and a
/// mutation's arguments say ([`MutationArgs::previews`]). A mutation whose
/// `Args` are no `MutationArgs`, or whose `Reply` is no `MutationReply`,
/// does not build.
pub trait Previews<H> {
    fn previews(&self) -> bool;
}

impl<A: Arguments, R> Previews<Answered<R>> for A {
    fn previews(&self) -> bool {
        false
    }
}

impl<A: MutationArgs, R: MutationReply> Previews<Mutation<R>> for A {
    fn previews(&self) -> bool {
        MutationArgs::previews(self)
    }
}

/// What a core tool's handler returns: its reply kind. Three
/// implementations: `Answered<Text>`, `Answered<R>` (`R: Object +
/// Serialize`, ADR 0048) and `Mutation<R>` (`R: MutationReply`, ADR 0022).
pub trait Handled: Sized {
    /// Whether the handler is a mutation's: it wrote its target's project
    /// files, which the pipeline refreshes and reports (ADR 0022). The tool's
    /// effect must be `Mutates` exactly then.
    const MUTATES: bool;
    /// Whether the tool lists an outputSchema; read without building it.
    const TYPED: bool;
    /// The outputSchema `tools/list` lists and every structured reply is
    /// checked against: `None` for text. The one place a core tool's output
    /// schema is built.
    fn output_schema() -> Option<&'static Value>;
    /// A call refused before its handler ran (an undeclared name, arguments
    /// that do not read, a call target that refused): the refusal as it is
    /// for a read; for a mutation a failed mutation that wrote nothing
    /// ([`crate::mutation::refused`], recorded into `events`).
    fn refused(error: Box<McpError>, tool: &'static str, events: &mut dyn EventSink)
    -> ToolOutcome;
    /// The rest of a call whose handler ran: a read's reply checked against
    /// its schema; a mutation completed ([`crate::mutation::complete`]: the
    /// target brought up to date, the reply sent once and checked, its events
    /// recorded through the call), `preview` read from its arguments
    /// beforehand.
    fn finish(self, call: &mut Call<'_>, tool: &'static str, preview: bool) -> ToolOutcome;
}

impl Handled for Answered<Text> {
    const MUTATES: bool = false;
    const TYPED: bool = false;

    fn output_schema() -> Option<&'static Value> {
        None
    }

    fn refused(error: Box<McpError>, _: &'static str, _: &mut dyn EventSink) -> ToolOutcome {
        ToolOutcome::Refused(error)
    }

    fn finish(self, _: &mut Call<'_>, _: &'static str, _: bool) -> ToolOutcome {
        crate::reply::text(self)
    }
}

impl<R: Object + Serialize + 'static> Handled for Answered<R> {
    const MUTATES: bool = false;
    const TYPED: bool = true;

    fn output_schema() -> Option<&'static Value> {
        Some(crate::reply::output_schema::<R>())
    }

    fn refused(error: Box<McpError>, _: &'static str, _: &mut dyn EventSink) -> ToolOutcome {
        ToolOutcome::Refused(error)
    }

    fn finish(self, _: &mut Call<'_>, tool: &'static str, _: bool) -> ToolOutcome {
        let schema = Self::output_schema().expect("a typed reply has an outputSchema");
        crate::reply::conforming(tool, Source::Core, schema, crate::reply::structured(self))
    }
}

impl<R: MutationReply> Handled for Mutation<R> {
    const MUTATES: bool = true;
    const TYPED: bool = true;

    /// `WrittenReply<R, R::After>`'s schema ([`crate::mutation::output_schema`]).
    fn output_schema() -> Option<&'static Value> {
        Some(crate::mutation::output_schema::<R>())
    }

    fn refused(
        error: Box<McpError>,
        tool: &'static str,
        events: &mut dyn EventSink,
    ) -> ToolOutcome {
        crate::mutation::refused(tool, error, events)
    }

    fn finish(self, call: &mut Call<'_>, tool: &'static str, preview: bool) -> ToolOutcome {
        // `complete` owns the order: refresh, send once, check against the
        // schema, record the events.
        let schema = Self::output_schema().expect("a mutation has an outputSchema");
        crate::mutation::complete::<R>(call, tool, schema, preview, self)
    }
}

/// A core tool as the table holds it: everything the server lists,
/// dispatches, checks and reports about it. Implemented once, for every
/// [`CoreTool`]; listings, the tools adapter, events and the reflection
/// tests read it.
pub trait ToolSpec: Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn effect(&self) -> Effect;
    /// Where the spec states its reply.
    fn spec_reply(&self) -> ReplySpec;
    /// The open values of its outputSchema ([`CoreTool::OPEN_VALUES`]).
    fn open_values(&self) -> &'static [&'static str];
    /// Its declared arguments, in field order.
    fn arguments(&self) -> Vec<Argument>;
    /// Its call target: its input's ([`Input::TARGET`]).
    fn target(&self) -> TargetSpec;
    /// Its outputSchema ([`Handled::output_schema`]); `None` for text.
    fn output_schema(&self) -> Option<&'static Value>;
    /// Whether it lists an outputSchema, without building it.
    fn is_typed(&self) -> bool;
    /// Run it on its resolved call: a name neither it nor its target
    /// declares is refused first, then its arguments are read, its input
    /// given and its reply finished. The request pipeline's; a test cannot
    /// build a `Call`.
    fn run(&self, call: &mut Call<'_>, arguments: &Value) -> ToolOutcome;
    /// The outcome of a call refused before it ran; a refused mutation
    /// records into `events`.
    fn refused(&self, error: McpError, events: &mut dyn EventSink) -> ToolOutcome;

    /// `Mutation` for a mutation, else its effect's group.
    fn category(&self) -> Category {
        self.effect().category()
    }

    /// `readOnlyHint` for a read, else its hints'.
    fn annotations(&self) -> ToolAnnotations {
        self.effect().annotations()
    }

    /// Whether it writes its target's project files.
    fn is_mutation(&self) -> bool {
        self.effect().is_mutation()
    }

    /// The input schema `tools/list` lists: its arguments, then its target's,
    /// and no other property ([`crate::args::input_schema`]).
    fn input_schema(&self) -> Value {
        crate::args::input_schema(&self.arguments(), self.target())
    }

    /// Every argument the call reads: its own, then its target's
    /// ([`TargetSpec::fields`]).
    fn reads(&self) -> Vec<&'static str> {
        self.arguments()
            .iter()
            .map(|argument| argument.name)
            .chain(
                self.target()
                    .fields()
                    .iter()
                    .map(|argument| argument.name()),
            )
            .collect()
    }

    /// The refusal of a call that sends a name neither the tool nor its
    /// target declares ([`crate::args::undeclared`]).
    fn undeclared(&self, arguments: &Value) -> Option<McpError> {
        crate::args::undeclared(arguments, &self.arguments(), self.target())
    }

    /// The tool as `tools/list` describes it under `revision`: it lists its
    /// outputSchema from 2025-06-18, the revision that came with
    /// structured content.
    fn descriptor(&self, revision: Revision) -> ToolDescriptor<'static> {
        ToolDescriptor {
            name: self.name().into(),
            description: self.description().into(),
            input_schema: self.input_schema(),
            output_schema: self
                .output_schema()
                .filter(|_| revision.sends_structured_content()),
            category: self.category(),
            source: CORE_SOURCE.into(),
            annotations: self.annotations(),
        }
    }
}

/// `tool` as the table holds it, its declarations checked when the table is
/// built (at compile time, `cargo check` included): its effect is `Mutates`
/// exactly when its handler answers a `Mutation`, and it names a spec type
/// exactly when its reply is typed.
pub const fn spec<T: CoreTool>(tool: &'static T) -> &'static dyn ToolSpec {
    let () = Agrees::<T>::CHECKED;
    tool
}

struct Agrees<T>(PhantomData<T>);

impl<T: CoreTool> Agrees<T> {
    const CHECKED: () = {
        assert!(
            T::EFFECT.is_mutation() == <T::Returns as Handled>::MUTATES,
            "a core tool's effect is Mutates exactly when its handler answers a Mutation"
        );
        assert!(
            T::SPEC.is_text() != <T::Returns as Handled>::TYPED,
            "a core tool names a spec type exactly when it answers a typed reply"
        );
    };
}

impl<T: CoreTool> ToolSpec for T {
    fn name(&self) -> &'static str {
        T::NAME
    }

    fn description(&self) -> &'static str {
        T::DESCRIPTION
    }

    fn effect(&self) -> Effect {
        T::EFFECT
    }

    fn spec_reply(&self) -> ReplySpec {
        T::SPEC
    }

    fn open_values(&self) -> &'static [&'static str] {
        T::OPEN_VALUES
    }

    fn arguments(&self) -> Vec<Argument> {
        <T::Args as Arguments>::declared()
    }

    fn target(&self) -> TargetSpec {
        <T::Input as Input>::TARGET
    }

    fn output_schema(&self) -> Option<&'static Value> {
        <T::Returns as Handled>::output_schema()
    }

    fn is_typed(&self) -> bool {
        <T::Returns as Handled>::TYPED
    }

    fn run(&self, call: &mut Call<'_>, arguments: &Value) -> ToolOutcome {
        // A name the tool and its target do not declare is refused before
        // anything is read (a refused mutation is a failed one).
        if let Some(error) = self.undeclared(arguments) {
            return self.refused(error, call.events());
        }
        let args = match crate::args::read::<T::Args>(arguments) {
            Ok(args) => args,
            // Arguments that do not read cannot say they asked for a preview.
            Err(refused) => {
                return <T::Returns as Handled>::refused(refused, T::NAME, call.events());
            }
        };
        // Read before the handler takes the arguments (ADR 0022).
        let preview = <T::Args as Previews<T::Returns>>::previews(&args);
        // The input borrows the call; what the handler returns does not.
        let returned = <T::Input as Input>::given(call).map(|input| T::call(input, args));
        match returned {
            Ok(returned) => returned.finish(call, T::NAME, preview),
            // The call target refused a call with no project before this
            // runs; unreachable, answered rather than panicked.
            Err(refused) => <T::Returns as Handled>::refused(refused, T::NAME, call.events()),
        }
    }

    fn refused(&self, error: McpError, events: &mut dyn EventSink) -> ToolOutcome {
        <T::Returns as Handled>::refused(Box::new(error), T::NAME, events)
    }
}
