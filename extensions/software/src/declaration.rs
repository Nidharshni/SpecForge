//! What @specforge/software declares: its kinds, edges, shared fields, enhancements, validation rules, passes, feature flags, with the SDK builders.
//! The host loads exactly this (`ContributionsBuilder::declaration`);
//! `crates/runtime/component/tests/declarations/` pins its wire form.

use specforge_extension_sdk::prelude::*;

/// `RiskLevel`, the consequence of an invariant's violation, worst first.
const RISK: Vocabulary = Vocabulary::new(&["critical", "high", "medium", "low"]);
/// `PortDirection`.
const PORT_DIRECTION: Vocabulary = Vocabulary::new(&["inbound", "outbound", "bidirectional"]);

/// Declare everything this module holds on `c`.
pub(crate) fn declare(c: &mut ContributionsBuilder) {
    kinds(c);
    edges(c);
    enhancements(c);
    rules(c);
}

fn kinds(c: &mut ContributionsBuilder) {
    c.kind("Behavior", |k| {
        k.keyword("behavior")
            .description("A testable unit of system functionality with a defined contract")
            .semantic_token("function")
            .lsp_icon("Method")
            .dot_shape("box")
            .dot_color("#1565C0")
            .dot_fillcolor("#E3F2FD")
            .inference_guide("Look for public functions in service, handler, or controller layers that represent user-visible operations or business logic. Route handlers (HTTP, gRPC, CLI commands), use-case functions, and domain service methods are strong signals. Map function name to entity ID (snake_case). Extract doc comments or the function's purpose as the contract. If the function validates rules or enforces constraints, those are invariants. If it emits events or messages, those are produces. If it has dedicated test files or test functions, add verify statements. Skip: private helpers, utility functions, test fixtures, generated code, framework boilerplate.");
        k.field("contract", |f| {
            f.field_type(FieldType::String)
                .required()
                .description("The behavioral contract this behavior guarantees")
                .normative()
                .headline();
        });
        k.field("invariants", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Invariants this behavior enforces")
                .edge("BehaviorEnforcesInvariant")
                .target_kind("invariant")
                .inverse_of("enforced_by");
        });
        k.field("types", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Type definitions used by this behavior")
                .edge("BehaviorReferencesType")
                .target_kind("type");
        });
        k.field("ports", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Port interfaces this behavior interacts with")
                .edge("BehaviorUsesPort")
                .target_kind("port");
        });
        k.field("produces", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Events produced as a result of this behavior")
                .edge("BehaviorProducesEvent")
                .target_kind("event");
        });
        k.field("consumes", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Events this behavior reacts to")
                .edge("BehaviorConsumesEvent")
                .target_kind("event");
        });
        k.field("category", |f| {
            f.field_type(FieldType::Identifier)
                .description("Classification tag for agent task routing");
        });
        k.field("features", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Product features this behavior implements")
                .edge("BehaviorImplementsFeature")
                .target_kind("feature")
                .inverse_of("behaviors");
        });
        k.field("description", |f| {
            f.field_type(FieldType::String)
                .description("Human-readable summary of this behavior");
        });
        k.field("status", |f| {
            f.field_type(FieldType::Identifier)
                .description("Current lifecycle status of this behavior")
                .headline();
        });
        k.field("refs", |f| {
            f.field_type(FieldType::StringList)
                .description("External references such as issue or document URIs");
        });
        k.field("severity", |f| {
            f.field_type(FieldType::Identifier)
                .description("Impact level if this behavior fails");
        });
        k.field("diagnostic", |f| {
            f.field_type(FieldType::Identifier)
                .description("Diagnostic message for validation tooling");
        });
    });
    c.kind("Invariant", |k| {
        k.keyword("invariant")
            .description("A system-wide constraint that must always hold true")
            .semantic_token("property")
            .lsp_icon("Property")
            .dot_shape("diamond")
            .dot_color("#C62828")
            .dot_fillcolor("#FFEBEE")
            .inference_guide("Look for assertions, validation logic, and defensive checks that enforce system-wide rules. Signals: assert!() / assert_eq!() statements, guard clauses that panic or return errors, database constraints (UNIQUE, CHECK, NOT NULL), middleware that rejects invalid state, config validation at startup, and comments like 'must always', 'never allow', 'invariant'. The guarantee field should state what must hold (e.g., 'User email must be unique across all accounts'). The risk field describes consequences of violation. Invariants are architectural rules about state; for quantified limits (latency <200ms, max connections), use constraint instead. Skip: local variable checks, input validation that's behavior-specific, temporary debug assertions.")
            .contract_target();
        k.field("guarantee", |f| {
            f.field_type(FieldType::String)
                .required()
                .description("The constraint this invariant guarantees holds at all times")
                .normative();
        });
        k.field("risk", |f| {
            f.vocabulary(&RISK)
                .description("Impact level if this invariant is violated");
        });
        k.field("description", |f| {
            f.field_type(FieldType::String)
                .description("Human-readable summary of this invariant");
        });
        k.field("refs", |f| {
            f.field_type(FieldType::StringList)
                .description("External references such as issue or document URIs");
        });
    });
    c.kind("Event", |k| {
        k.keyword("event")
            .description("A significant occurrence in the system that triggers reactions")
            .semantic_token("event")
            .lsp_icon("Event")
            .dot_shape("ellipse")
            .dot_color("#E65100")
            .dot_fillcolor("#FFF3E0")
            .inference_guide("Look for message types, event classes, pub/sub topics, webhook payloads, and signal/notification patterns. Signals: structs/classes named *Event, *Message, *Notification; message queue topic/channel definitions; emit()/publish()/dispatch()/notify() calls; event handler registrations (on_*, handle_*); webhook payload schemas. Map the event name to entity ID. If it carries structured data, reference the payload type. Identify which behaviors produce and consume each event. Skip: internal method calls, logging statements, framework lifecycle callbacks (unless domain-meaningful).");
        k.field("channel", |f| {
            f.field_type(FieldType::String)
                .description("Communication channel this event is published on");
        });
        k.field("channel_type", |f| {
            f.field_type(FieldType::Identifier)
                .description("Transport mechanism for the channel (e.g. queue, topic, stream)");
        });
        k.field("category", |f| {
            f.field_type(FieldType::Identifier)
                .description("Classification of this event (e.g. domain, integration, system)");
        });
        k.field("payload", |f| {
            f.field_type(FieldType::Reference)
                .description("Type definition describing this event's data shape")
                .edge("EventCarriesPayloadType")
                .target_kind("type");
        });
        k.field("description", |f| {
            f.field_type(FieldType::String)
                .description("Human-readable summary of this event");
        });
        k.field("refs", |f| {
            f.field_type(FieldType::StringList)
                .description("External references such as issue or document URIs");
        });
        k.field("contract", |f| {
            f.field_type(FieldType::String)
                .description("Delivery or ordering guarantees for this event")
                .normative()
                .headline();
        });
    });
    c.kind("Type", |k| {
        k.keyword("type")
            .description("A data structure or domain model definition")
            .has_body_parser()
            .open_fields(true)
            .semantic_token("type")
            .lsp_icon("Struct")
            .dot_shape("rectangle")
            .dot_color("#2E7D32")
            .dot_fillcolor("#E8F5E9")
            .inference_guide("Look for domain model structs, data transfer objects, API request/response shapes, database entities, and enum definitions that carry business meaning. Signals: struct/class definitions in models/ or domain/ directories; TypeScript interfaces/types for API contracts; protobuf/GraphQL/JSON Schema type definitions; ORM model classes; enum types with business variants. List the type's members in its body, one `name Type` per line (with `@optional` where it may be absent). Reference composed_types for nested or referenced types. Skip: internal implementation structs, builder patterns, framework-generated types, test fixtures.")
            .declares_types();
        k.field("composed_types", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Types composed or referenced by this type (derived from its field types)")
                .edge("TypeComposesType")
                .target_kind("type")
                .derived_from("type_expressions");
        });
        k.field("extends", |f| {
            f.field_type(FieldType::Reference)
                .description("Parent type this type extends or inherits from")
                .edge("TypeExtendsType")
                .target_kind("type");
        });
        k.field("description", |f| {
            f.field_type(FieldType::String)
                .description("Human-readable summary of this type");
        });
        k.field("refs", |f| {
            f.field_type(FieldType::StringList)
                .description("External references such as issue or document URIs");
        });
    });
    c.kind("Port", |k| {
        k.keyword("port")
            .description("An interface boundary between system components")
            .has_body_parser()
            .open_fields(true)
            .semantic_token("interface")
            .lsp_icon("Interface")
            .dot_shape("trapezium")
            .dot_color("#00695C")
            .dot_fillcolor("#E0F2F1")
            .inference_guide("Look for trait definitions, abstract classes, interface declarations, and adapter patterns that define boundaries between system layers. Signals: Rust traits in ports/ or interfaces/ directories; TypeScript/Java interfaces for repositories, gateways, or external services; abstract base classes; dependency injection interfaces; API client contracts. Set direction to 'inbound' (receives requests), 'outbound' (calls external systems), or 'bidirectional'. Use open_fields/methods to list the port's operations. Set category to 'http', 'grpc', 'database', 'queue', 'filesystem', etc. Skip: internal module boundaries, utility traits (Display, Debug), marker traits, framework-imposed interfaces.");
        k.field("direction", |f| {
            f.vocabulary(&PORT_DIRECTION)
                .required()
                .description("Whether this port is inbound, outbound, or bidirectional");
        });
        k.field("category", |f| {
            f.field_type(FieldType::String)
                .description("Classification of this port, a word or a path (e.g. http, io/filesystem)");
        });
        k.field("methods", |f| {
            f.field_type(FieldType::Block)
                .description("Method signatures exposed by this port interface");
        });
        k.field("types", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Types named in this port's method signatures (derived from its methods)")
                .edge("PortReferencesType")
                .target_kind("type")
                .derived_from("method_signatures");
        });
        k.field("description", |f| {
            f.field_type(FieldType::String)
                .description("Human-readable summary of this port");
        });
        k.field("refs", |f| {
            f.field_type(FieldType::StringList)
                .description("External references such as issue or document URIs");
        });
    });
}

fn edges(c: &mut ContributionsBuilder) {
    c.edge("References", |e| {
        e.description("General cross-reference between entities");
    });
    c.edge("BehaviorImplementsFeature", |e| {
        e.description("Behavior implements a feature (cross-extension via peer_dependency @specforge/product)")
            .source_kind("behavior")
            .target_kind("feature")
            .edge_style("solid")
            .edge_color("#1565C0");
    });
    c.edge("BehaviorProducesEvent", |e| {
        e.source_kind("behavior")
            .target_kind("event")
            .edge_style("solid")
            .edge_color("#E65100");
    });
    c.edge("BehaviorConsumesEvent", |e| {
        e.source_kind("behavior")
            .target_kind("event")
            .edge_style("dashed")
            .edge_color("#E65100");
    });
    c.edge("BehaviorReferencesType", |e| {
        e.source_kind("behavior")
            .target_kind("type")
            .edge_style("solid")
            .edge_color("#2E7D32");
    });
    c.edge("EventCarriesPayloadType", |e| {
        e.source_kind("event")
            .target_kind("type")
            .edge_style("solid")
            .edge_color("#2E7D32");
    });
    c.edge("TypeComposesType", |e| {
        e.source_kind("type")
            .target_kind("type")
            .edge_style("solid")
            .edge_color("#2E7D32");
    });
    c.edge("PortReferencesType", |e| {
        e.description("Port names a type in a method parameter or return type")
            .source_kind("port")
            .target_kind("type")
            .edge_style("solid")
            .edge_color("#2E7D32");
    });
    c.edge("BehaviorUsesPort", |e| {
        e.source_kind("behavior")
            .target_kind("port")
            .edge_style("solid")
            .edge_color("#00695C");
    });
    c.edge("BehaviorEnforcesInvariant", |e| {
        e.source_kind("behavior")
            .target_kind("invariant")
            .edge_style("dashed")
            .edge_color("#C62828");
    });
    c.edge("TypeExtendsType", |e| {
        e.source_kind("type")
            .target_kind("type")
            .edge_style("solid")
            .edge_color("#2E7D32")
            .edge_arrowhead("empty");
    });
    c.edge("ExternalRef", |e| {
        e.description("Entity references an external URI or resource")
            .edge_style("dotted")
            .edge_color("#9E9E9E");
    });
    c.edge("MilestoneIncludesBehavior", |e| {
        e.description("Milestone delivers a behavior (cross-extension enhancement edge)")
            .source_kind("milestone")
            .target_kind("behavior")
            .edge_style("solid")
            .edge_color("#9C27B0");
    });
    c.edge("ModuleConsumesPort", |e| {
        e.source_kind("module")
            .target_kind("port")
            .edge_style("dashed")
            .edge_color("#00695C");
    });
    c.edge("ModuleDefinesPort", |e| {
        e.source_kind("module")
            .target_kind("port")
            .edge_style("solid")
            .edge_color("#00695C");
    });
}

fn enhancements(c: &mut ContributionsBuilder) {
    c.enhance("module", |e| {
        e.field("ports", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Port interfaces this module consumes")
                .edge("ModuleConsumesPort")
                .target_kind("port");
        });
        e.field("ports_defined", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Port interfaces this module defines")
                .edge("ModuleDefinesPort")
                .target_kind("port");
        });
    });
    c.enhance("milestone", |e| {
        e.field("behaviors", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Behaviors this milestone includes in its delivery scope")
                .edge("MilestoneIncludesBehavior")
                .target_kind("behavior");
        });
    });
}

fn rules(c: &mut ContributionsBuilder) {
    c.rule("W001", |r| {
        r.check(CheckKind::NoOutgoingEdges)
            .severity(ValidationSeverity::Warning)
            .message_template("behavior '{id}' does not implement any feature")
            .target_kind("behavior")
            .edge_type("BehaviorImplementsFeature");
    });
    c.rule("W002", |r| {
        r.check(CheckKind::NoIncomingEdges)
            .severity(ValidationSeverity::Warning)
            .message_template("type '{id}' is not referenced by any behavior, port, or type")
            .target_kind("type");
    });
    c.rule("W003", |r| {
        r.check(CheckKind::NoIncomingEdges)
            .severity(ValidationSeverity::Warning)
            .message_template("invariant '{id}' is not enforced by any behavior")
            .target_kind("invariant");
    });
    c.rule("W005", |r| {
        r.check(CheckKind::NoIncomingEdges)
            .severity(ValidationSeverity::Warning)
            .message_template("port '{id}' is not referenced by any behavior")
            .target_kind("port");
    });
    c.rule("W006", |r| {
        r.check(CheckKind::MissingFieldWhenFlagSet)
            .severity(ValidationSeverity::Warning)
            .message_template(
                "behavior '{id}' has no category — agents use category for task routing",
            )
            .target_kind("behavior")
            .field("category");
    });
    c.rule("W007", |r| {
        r.check(CheckKind::NoIncomingEdges)
            .severity(ValidationSeverity::Warning)
            .message_template("event '{id}' is not produced by any behavior")
            .target_kind("event");
    });
    c.rule("W008", |r| {
        r.check(CheckKind::NoIncomingEdges)
            .severity(ValidationSeverity::Warning)
            .message_template("feature '{id}' is not implemented by any behavior")
            .target_kind("feature")
            .edge_type("BehaviorImplementsFeature");
    });
    c.rule("W010", |r| {
        r.check(CheckKind::Custom)
            .severity(ValidationSeverity::Warning)
            .message_template("type '{id}' field '{field}' has unknown annotation '{value}'")
            .target_kind("type")
            .wasm_function("validate__type_field_annotations")
            .validate(crate::validate_type_field_annotations);
    });
    c.rule("E004", |r| {
        r.check(CheckKind::Custom)
            .severity(ValidationSeverity::Error)
            .message_template("port '{id}' method '{field}' references unknown type '{value}'")
            .target_kind("port")
            .wasm_function("validate__port_methods")
            .validate(crate::validate_port_methods);
    });
    c.rule("E010", |r| {
        r.check(CheckKind::Custom)
            .severity(ValidationSeverity::Error)
            .message_template("milestone '{id}' behaviors range is invalid: {reason}")
            .target_kind("milestone")
            .wasm_function("validate__milestone_behavior_ranges")
            .validate(crate::validate_milestone_behavior_ranges);
    });
}
