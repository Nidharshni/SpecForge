//! What @specforge/formal declares: its kinds, edges, shared fields, enhancements, validation rules, feature flags, with the SDK builders.
//! The host loads exactly this (`ContributionsBuilder::declaration`);
//! `crates/runtime/component/tests/declarations/` pins its wire form.

use specforge_extension_sdk::prelude::*;
use specforge_extension_sdk::{ValidatorContext, ValidatorVerdict};

/// `PropertyType`.
const PROPERTY_TYPE: Vocabulary = Vocabulary::new(&["safety", "liveness", "fairness"]);

/// Declare everything this module holds on `c`.
pub(crate) fn declare(c: &mut ContributionsBuilder) {
    kinds(c);
    edges(c);
    enhancements(c);
    rules(c);
}

fn kinds(c: &mut ContributionsBuilder) {
    c.kind("Property", |k| {
        k.keyword("property")
            .description("A temporal property assertion (safety, liveness, fairness)")
            .supports_verify(true)
            .semantic_token("property")
            .lsp_icon("Property")
            .dot_shape("hexagon")
            .dot_color("#0D47A1")
            .dot_fillcolor("#E3F2FD")
            .inference_guide("Look for system-wide temporal guarantees that must hold across all executions. Signals: safety properties ('X never happens'), liveness properties ('Y eventually happens'), fairness properties ('if X is requested, it is eventually granted'); invariants that span multiple behaviors; global ordering guarantees; eventual consistency promises; progress guarantees. Set property_type to the property's category. The expression field should state the property formally or semi-formally. Skip: local function postconditions (those are invariants), single-behavior contracts.")
            .contract_target();
        k.field("expression", |f| {
            f.field_type(FieldType::String)
                .required()
                .description("Formal expression defining the temporal property")
                .normative()
                .proof_role("claim");
        });
        k.field("property_type", |f| {
            f.vocabulary(&PROPERTY_TYPE)
                .required()
                .description("Category of temporal property");
        });
        k.field("scope", |f| {
            f.field_type(FieldType::String)
                .description("Applicability scope of this property");
        });
        k.field("description", |f| {
            f.field_type(FieldType::String)
                .description("Free-form description of the property");
        });
        k.field("refs", |f| {
            f.field_type(FieldType::StringList)
                .description("External references (URLs, documents, issue links)");
        });
    });
    c.kind("Axiom", |k| {
        k.keyword("axiom")
            .description("An assumed-true foundational assertion")
            .supports_verify(true)
            .semantic_token("constant")
            .lsp_icon("Constant")
            .dot_shape("ellipse")
            .dot_color("#311B92")
            .dot_fillcolor("#EDE7F6")
            .inference_guide("Look for foundational assumptions the system relies on without proving them. Signals: comments stating 'we assume', 'given that', 'prerequisite'; environment assumptions (e.g., 'clock is monotonic', 'network is eventually connected'); trust boundaries ('upstream service guarantees X'); mathematical axioms in algorithm implementations; configuration assumptions that must hold for correctness. The expression field states what is assumed. Add justification explaining why the assumption is safe. Link assumes to invariants the axiom supports. Skip: derived properties (those are properties), things the system proves/enforces itself.");
        k.field("expression", |f| {
            f.field_type(FieldType::String)
                .required()
                .description("Formal expression stating the axiom")
                .normative()
                .proof_role("bound");
        });
        k.field("assumes", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Invariants this axiom assumes as foundational truths")
                .edge("AxiomAssumesInvariant")
                .target_kind("invariant");
        });
        k.field("justification", |f| {
            f.field_type(FieldType::String)
                .description("Rationale for why this axiom is assumed true");
        });
        k.field("description", |f| {
            f.field_type(FieldType::String)
                .description("Free-form description of the axiom");
        });
        k.field("refs", |f| {
            f.field_type(FieldType::StringList)
                .description("External references (URLs, documents, issue links)");
        });
    });
    c.kind("Protocol", |k| {
        k.keyword("protocol")
            .description("A shared synchronization contract between processes")
            .semantic_token("interface")
            .lsp_icon("Interface")
            .dot_shape("component")
            .dot_color("#004D40")
            .dot_fillcolor("#E0F2F1")
            .inference_guide("Look for multi-party interaction patterns with defined message sequences and state machines. Signals: handshake sequences (connect/auth/ready); transaction protocols (begin/commit/rollback); consensus algorithms; leader election protocols; retry/backoff protocols with defined states; connection lifecycle state machines; API versioning/negotiation protocols. Define alphabet (set of events), states, transitions, and initial_state. One protocol per distinct coordination pattern shared between processes. Skip: single-party state machines (those are processes), simple request-response patterns, stateless interactions.");
        k.field("alphabet", |f| {
            f.field_type(FieldType::StringList)
                .required()
                .description("Set of events that this protocol can communicate");
        });
        k.field("states", |f| {
            f.field_type(FieldType::StringList)
                .description("Possible states in the protocol state machine");
        });
        k.field("transitions", |f| {
            f.field_type(FieldType::StringList)
                .description("State transitions triggered by events (from -> event -> to)");
        });
        k.field("initial_state", |f| {
            f.field_type(FieldType::String)
                .required()
                .description("Starting state of the protocol");
        });
        k.field("description", |f| {
            f.field_type(FieldType::String)
                .description("Free-form description of the protocol");
        });
        k.field("refs", |f| {
            f.field_type(FieldType::StringList)
                .description("External references (URLs, documents, issue links)");
        });
    });
    c.kind("Refinement", |k| {
        k.keyword("refinement")
            .description("A mapping from abstract to concrete specification")
            .semantic_token("function")
            .lsp_icon("Method")
            .dot_shape("parallelogram")
            .dot_color("#1B5E20")
            .dot_fillcolor("#E8F5E9")
            .inference_guide("Look for abstract-to-concrete implementation mappings where a high-level specification is progressively detailed. Signals: abstract base classes with concrete implementations; interface + implementation pairs where the implementation adds constraints; layered architectures where higher layers define contracts and lower layers fulfill them; strategy patterns; template method patterns. Link abstract_entity to the high-level behavior and concrete_entity to the implementation. List invariant_deltas for constraints added or relaxed. Chain refinements with chains_to for multi-step refinement. Skip: simple inheritance, polymorphism without specification changes.");
        k.field("abstract_entity", |f| {
            f.field_type(FieldType::Reference)
                .required()
                .description("Reference to the abstract behavior being refined")
                .edge("RefinementRefinesAbstract")
                .target_kind("behavior");
        });
        k.field("concrete_entity", |f| {
            f.field_type(FieldType::Reference)
                .required()
                .description("Reference to the concrete behavior that implements the refinement")
                .edge("RefinementRefinesConcrete")
                .target_kind("behavior");
        });
        k.field("invariant_deltas", |f| {
            f.field_type(FieldType::StringList)
                .description("Changes to invariants introduced or relaxed by this refinement");
        });
        k.field("chains_to", |f| {
            f.field_type(FieldType::Reference)
                .description("Next refinement in the refinement chain")
                .edge("RefinementChainsToRefinement")
                .target_kind("refinement");
        });
        k.field("description", |f| {
            f.field_type(FieldType::String)
                .description("Free-form description of the refinement mapping");
        });
        k.field("refs", |f| {
            f.field_type(FieldType::StringList)
                .description("External references (URLs, documents, issue links)");
        });
    });
    c.kind("Process", |k| {
        k.keyword("process")
            .description("A CSP-style communicating process with states and transitions")
            .semantic_token("function")
            .lsp_icon("Method")
            .dot_shape("ellipse")
            .dot_color("#006064")
            .dot_fillcolor("#E0F7FA")
            .inference_guide("Look for long-running stateful components that communicate via events and have defined lifecycles. Signals: actor/agent implementations; background workers with state machines; sagas/orchestrators with defined steps; finite state machines in code; async task pipelines; microservice interaction patterns with defined communication channels. Define alphabet (events it sends/receives), states, initial_state, and composition (parallel, sequential, choice). Link sub_processes for composed processes. Skip: stateless request handlers (those are behaviors), simple event handlers, one-shot tasks.");
        k.field("alphabet", |f| {
            f.field_type(FieldType::StringList)
                .required()
                .description("Set of events that this process can communicate");
        });
        k.field("states", |f| {
            f.field_type(FieldType::StringList)
                .description("Possible states in the process state machine");
        });
        k.field("initial_state", |f| {
            f.field_type(FieldType::String)
                .required()
                .description("Starting state of the process");
        });
        k.field("composition", |f| {
            f.field_type(FieldType::String)
                .description("Composition operator and sub-processes (e.g. parallel, sequential, choice)");
        });
        k.field("sub_processes", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Sub-processes composed by this process")
                .edge("ProcessComposesProcess")
                .target_kind("process");
        });
        k.field("description", |f| {
            f.field_type(FieldType::String)
                .description("Free-form description of the process");
        });
        k.field("refs", |f| {
            f.field_type(FieldType::StringList)
                .description("External references (URLs, documents, issue links)");
        });
    });
}

fn edges(c: &mut ContributionsBuilder) {
    c.edge("AxiomAssumesInvariant", |e| {
        e.description("Axiom assumes an invariant as foundational truth")
            .source_kind("axiom")
            .target_kind("invariant")
            .edge_style("dotted")
            .edge_color("#311B92");
    });
    c.edge("BehaviorSatisfiesProperty", |e| {
        e.description("Behavior satisfies a temporal property")
            .source_kind("behavior")
            .target_kind("property")
            .edge_style("solid")
            .edge_color("#0D47A1");
    });
    c.edge("EventFollowsProtocol", |e| {
        e.description("Event follows a synchronization protocol")
            .source_kind("event")
            .target_kind("protocol")
            .edge_style("solid")
            .edge_color("#004D40");
    });
    c.edge("PropertyDependsOnInvariant", |e| {
        e.description("Property depends on an invariant")
            .source_kind("property")
            .target_kind("invariant")
            .edge_style("dashed")
            .edge_color("#0D47A1");
    });
    c.edge("BehaviorRefinesBehavior", |e| {
        e.description("Concrete behavior refines an abstract behavior (specification layering)")
            .source_kind("behavior")
            .target_kind("behavior")
            .edge_style("dashed")
            .edge_color("#4A148C");
    });
    c.edge("RefinementRefinesAbstract", |e| {
        e.description("Refinement maps from this abstract behavior")
            .source_kind("refinement")
            .target_kind("behavior")
            .edge_style("dashed")
            .edge_color("#1B5E20");
    });
    c.edge("RefinementRefinesConcrete", |e| {
        e.description("Refinement maps to this concrete behavior")
            .source_kind("refinement")
            .target_kind("behavior")
            .edge_style("solid")
            .edge_color("#1B5E20");
    });
    c.edge("RefinementChainsToRefinement", |e| {
        e.description("Chain of refinements")
            .source_kind("refinement")
            .target_kind("refinement")
            .edge_style("dashed")
            .edge_color("#1B5E20");
    });
    c.edge("EventParticipatesInProcess", |e| {
        e.description("Event participates in a process")
            .source_kind("event")
            .target_kind("process")
            .edge_style("solid")
            .edge_color("#006064");
    });
    c.edge("ProcessComposesProcess", |e| {
        e.description("Process composes sub-processes")
            .source_kind("process")
            .target_kind("process")
            .edge_style("solid")
            .edge_color("#006064");
    });
}

fn enhancements(c: &mut ContributionsBuilder) {
    c.enhance("behavior", "@specforge/formal", |e| {
        e.field("requires", |f| {
            f.field_type(FieldType::Block)
                .description("Named preconditions: each condition's name and what it requires");
        });
        e.field("ensures", |f| {
            f.field_type(FieldType::Block)
                .description("Named postconditions: each condition's name and what it guarantees");
        });
        e.field("maintains", |f| {
            f.field_type(FieldType::Block)
                .description("Named frame conditions that hold throughout execution");
        });
        e.field("satisfies", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Temporal properties this behavior satisfies")
                .edge("BehaviorSatisfiesProperty")
                .target_kind("property");
        });
        e.field("sync", |f| {
            f.field_type(FieldType::Block)
                .description("Synchronization bound: barrier, timeout or delivery");
        });
        e.field("abstract", |f| {
            f.field_type(FieldType::Bool)
                .description("Marks a specification-only behavior that concrete behaviors refine; it carries no verify obligations of its own")
                .exempts_obligations();
        });
        e.field("refines", |f| {
            f.field_type(FieldType::Reference)
                .description("Abstract behavior this concrete behavior refines; its ensures conditions must be kept (E031)")
                .edge("BehaviorRefinesBehavior")
                .target_kind("behavior");
        });
    });
    c.enhance("event", "@specforge/formal", |e| {
        e.field("follows_protocol", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Synchronization protocol this event follows")
                .edge("EventFollowsProtocol")
                .target_kind("protocol");
        });
        e.field("participates_in", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Processes this event participates in")
                .edge("EventParticipatesInProcess")
                .target_kind("process");
        });
        e.field("sync", |f| {
            f.field_type(FieldType::Block)
                .description("Synchronization bound: barrier, timeout or delivery");
        });
    });
    c.enhance("invariant", "@specforge/formal", |e| {
        e.field("expression", |f| {
            f.field_type(FieldType::String)
                .description(
                    "Machine-checkable claim the prove pass must entail from the declared bounds",
                )
                .normative()
                .proof_role("claim");
        });
    });
}

fn rules(c: &mut ContributionsBuilder) {
    c.rule("W123", |r| {
        r.check(CheckKind::NoIncomingEdges)
            .severity(ValidationSeverity::Warning)
            .message_template(
                "property '{id}' is not referenced by any behavior — it may be unused",
            )
            .target_kind("property");
    });
    c.rule("W126", |r| {
        r.check(CheckKind::NoIncomingEdges)
            .severity(ValidationSeverity::Warning)
            .message_template("axiom '{id}' is not referenced by any entity — it may be unused")
            .target_kind("axiom");
    });
    c.rule("W128", |r| {
        r.check(CheckKind::NoIncomingEdges)
            .severity(ValidationSeverity::Warning)
            .message_template("protocol '{id}' is not referenced by any event — it may be unused")
            .target_kind("protocol");
    });
    c.rule("W131", |r| {
        r.check(CheckKind::NoIncomingEdges)
            .severity(ValidationSeverity::Warning)
            .message_template(
                "refinement '{id}' is not referenced by any entity — it may be unused",
            )
            .target_kind("refinement");
    });
    c.rule("W134", |r| {
        r.check(CheckKind::NoIncomingEdges)
            .severity(ValidationSeverity::Warning)
            .message_template("process '{id}' is not referenced by any entity — it may be unused")
            .target_kind("process");
    });
    // A written description that is empty or only whitespace (an absent
    // one is not reported): W124, W127, W129, W132, W135.
    for (code, kind) in [
        ("W124", "property"),
        ("W127", "axiom"),
        ("W129", "protocol"),
        ("W132", "refinement"),
        ("W135", "process"),
    ] {
        c.rule(code, |r| {
            r.check(CheckKind::FieldValueConstraint)
                .severity(ValidationSeverity::Warning)
                .message_template(&format!("{kind} '{{id}}' has empty description"))
                .target_kind(kind)
                .field("description");
            r.constraint(|fc| {
                fc.kind(ConstraintKind::Matches).pattern(NOT_BLANK);
            });
        });
    }
    // `alphabet` is required (E006 when absent); W136 is the written but
    // empty one.
    c.rule("W136", |r| {
        r.check(CheckKind::FieldValueConstraint)
            .severity(ValidationSeverity::Warning)
            .message_template("process '{id}' has no alphabet (no events declared)")
            .target_kind("process")
            .field("alphabet");
        r.constraint(|fc| {
            fc.kind(ConstraintKind::Matches).pattern(NOT_BLANK);
        });
    });
    c.rule("W133", |r| {
        r.check(CheckKind::Custom)
            .severity(ValidationSeverity::Warning)
            .message_template("refinement '{id}' declares no invariant_deltas")
            .target_kind("refinement")
            .wasm_function("validate__refinement_deltas")
            .validate(refinement_declares_deltas);
    });
}

/// A field text with at least one non-whitespace character.
const NOT_BLANK: &str = r"\S";

/// W133: a refinement must write a non-empty `invariant_deltas` (what the
/// refinement adds or relaxes); absent and `[]` both fail.
pub(crate) fn refinement_declares_deltas(context: &ValidatorContext) -> ValidatorVerdict {
    let written = context
        .entity
        .fields
        .iter()
        .filter(|f| f.key == INVARIANT_DELTAS_FIELD)
        .any(|f| f.value.as_str().is_some_and(|v| !v.trim().is_empty()));
    if written {
        ValidatorVerdict::Pass
    } else {
        ValidatorVerdict::Fail {
            field: Some(INVARIANT_DELTAS_FIELD.to_string()),
            value: None,
        }
    }
}

const INVARIANT_DELTAS_FIELD: &str = "invariant_deltas";
