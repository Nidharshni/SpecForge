//! What @specforge/governance declares: its kinds, edges and field vocabularies, with the SDK builders.
//! The host loads exactly this (`ContributionsBuilder::declaration`);
//! `crates/runtime/component/tests/declarations/` pins its wire form.

use specforge_extension_sdk::prelude::*;

/// `DecisionStatus`.
const DECISION_STATUS: Vocabulary =
    Vocabulary::new(&["proposed", "accepted", "deprecated", "superseded"]);
/// `ConstraintPriority`, most important first.
const PRIORITY: Vocabulary = Vocabulary::new(&["critical", "high", "medium", "low"]);
/// `FailureSeverity`, before or after mitigation, worst first.
const SEVERITY: Vocabulary = Vocabulary::new(&["critical", "high", "medium", "low"]);
/// `FailureOccurrence`, most likely first.
const OCCURRENCE: Vocabulary =
    Vocabulary::new(&["certain", "likely", "occasional", "unlikely", "rare"]);
/// `FailureDetection`, most detectable first.
const DETECTION: Vocabulary =
    Vocabulary::new(&["certain", "likely", "moderate", "unlikely", "undetectable"]);

/// Declare everything this module holds on `c`.
pub(crate) fn declare(c: &mut ContributionsBuilder) {
    kinds(c);
    edges(c);
}

fn kinds(c: &mut ContributionsBuilder) {
    c.kind("Decision", |k| {
        k.keyword("decision")
            .description("An architectural or design decision with rationale and status")
            .semantic_token("string")
            .lsp_icon("Text")
            .dot_shape("note")
            .dot_color("#6A1B9A")
            .dot_fillcolor("#F3E5F5")
            .inference_guide("Look for Architecture Decision Records (ADRs), design documents, RFC files, and significant comments explaining WHY something was built a certain way. Signals: docs/adr/ or docs/decisions/ directories; files named ADR-*, DECISION-*, RFC-*; PR descriptions with 'decided to', 'chosen approach', 'trade-off'; comments starting with 'Decision:', 'Rationale:', 'Why:'. Extract status, context (what prompted it), decision (what was chosen), and consequences (trade-offs). Link to invariants the decision protects and constraints it imposes. Skip: trivial implementation choices, style preferences, auto-generated config.");
        k.field("status", |f| {
            f.vocabulary(&DECISION_STATUS)
                .required()
                .description("Current lifecycle status of the decision")
                .headline();
        });
        k.field("context", |f| {
            f.field_type(FieldType::String)
                .required()
                .description("Background and circumstances that motivated this decision");
        });
        k.field("decision", |f| {
            f.field_type(FieldType::String)
                .required()
                .description("The decision statement itself")
                .normative();
        });
        k.field("consequences", |f| {
            f.field_type(FieldType::StringList)
                .description("Expected outcomes and trade-offs resulting from this decision");
        });
        k.field("superseded_by", |f| {
            f.field_type(FieldType::Reference)
                .description("Reference to a newer decision that replaces this one")
                .edge("DecisionSupersedesDecision")
                .target_kind("decision");
        });
        k.field("invariants", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Invariants that this decision protects")
                .edge("DecisionProtectsInvariant")
                .target_kind("invariant");
        });
        k.field("refs", |f| {
            f.field_type(FieldType::StringList)
                .description("External references (URLs, documents, issue links)");
        });
        k.field("description", |f| {
            f.field_type(FieldType::String)
                .description("Free-form description of the decision");
        });
        k.field("date", |f| {
            f.field_type(FieldType::String)
                .description("Date the decision was made or last updated");
        });
        k.field("authors", |f| {
            f.field_type(FieldType::StringList)
                .description("People who authored this decision record");
        });
        k.field("tags", |f| {
            f.field_type(FieldType::StringList)
                .description("Categorization tags for filtering and grouping");
        });
        k.field("alternatives", |f| {
            f.field_type(FieldType::StringList)
                .description("Alternative options that were considered");
        });
        k.field("reason", |f| {
            f.field_type(FieldType::String)
                .description("Justification for why this option was chosen over alternatives");
        });
        k.field("deciders", |f| {
            f.field_type(FieldType::StringList)
                .description("People who approved or signed off on this decision");
        });
        k.field("affects_features", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Product features affected by this decision (requires @specforge/product)")
                .edge("DecisionAffectsFeature")
                .target_kind("feature");
        });
        k.field("constraints", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Constraints imposed by this decision")
                .edge("DecisionImposesConstraint")
                .target_kind("constraint");
        });
    });
    c.kind("Constraint", |k| {
        k.keyword("constraint")
            .description("A technical or business constraint that limits design choices")
            .semantic_token("property")
            .lsp_icon("Property")
            .dot_shape("octagon")
            .dot_color("#BF360C")
            .dot_fillcolor("#FBE9E7")
            .inference_guide("Look for non-functional requirements, SLAs, performance budgets, regulatory mandates, and technical limitations. Signals: performance benchmarks or thresholds in code/config; rate limiting configuration; compliance annotations (@HIPAA, @PCI-DSS); resource limits (max connections, memory caps, timeout values); security policies; dependency version constraints; platform limitations documented in README or config. Set category (performance, security, regulatory, compatibility). Include metric and threshold when quantifiable (e.g., metric='p99 latency', threshold='<200ms'). Link enforced_by to behaviors that enforce this constraint at runtime. Constraints are quantified limits with measurable thresholds; for architectural rules about state validity, use invariant instead. Skip: soft preferences, guidelines that aren't enforced.");
        k.field("category", |f| {
            f.field_type(FieldType::String)
                .description("Classification of the constraint (e.g. performance, security, regulatory)");
        });
        k.field("priority", |f| {
            f.vocabulary(&PRIORITY)
                .description("Relative importance of this constraint");
        });
        k.field("metric", |f| {
            f.field_type(FieldType::String)
                .description("Measurable quantity used to evaluate compliance")
                .normative()
                .proof_role("bound");
        });
        k.field("threshold", |f| {
            f.field_type(FieldType::String)
                .description("Acceptable limit or target value for the metric")
                .normative();
        });
        k.field("description", |f| {
            f.field_type(FieldType::String)
                .required()
                .description("Free-form description of the constraint");
        });
        k.field("refs", |f| {
            f.field_type(FieldType::StringList)
                .description("External references (URLs, documents, issue links)");
        });
        k.field("enforced_by", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Behaviors that enforce this constraint at runtime")
                .edge("ConstraintEnforcedByBehavior")
                .target_kind("behavior")
                .inverse_of("invariants");
        });
        k.field("scope", |f| {
            f.field_type(FieldType::String)
                .description("Applicability scope (e.g. system-wide, per-module, per-request)");
        });
        k.field("constrains", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Behaviors that are limited or governed by this constraint")
                .edge("ConstraintConstrainsBehavior")
                .target_kind("behavior");
        });
        k.field("protects", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Invariants that this constraint helps protect")
                .edge("ConstraintProtectsInvariant")
                .target_kind("invariant");
        });
        k.field("governs_features", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Product features this constraint governs (requires @specforge/product)")
                .edge("ConstraintGovernsFeature")
                .target_kind("feature");
        });
    });
    c.kind("FailureMode", |k| {
        k.keyword("failure_mode")
            .description("A potential failure scenario with severity and mitigation")
            .semantic_token("variable")
            .lsp_icon("Variable")
            .dot_shape("triangle")
            .dot_color("#D32F2F")
            .dot_fillcolor("#FFCDD2")
            .inference_guide("Look for error handling paths, retry logic, circuit breakers, fallback mechanisms, and comments describing what can go wrong. Signals: catch/rescue blocks handling specific failure scenarios; circuit breaker configurations; retry policies with backoff; fallback implementations; chaos engineering tests; error types/classes representing failure categories; timeout handling; dead letter queues. Extract cause (what triggers it), effect (what breaks), and mitigation (how the system recovers). Rate severity/occurrence/detection. Link to the invariant that gets violated and behaviors affected. Skip: generic error handling, validation errors for user input, expected business rule rejections.");
        k.field("severity", |f| {
            f.vocabulary(&SEVERITY)
                .required()
                .description("Impact severity rating before mitigation");
        });
        k.field("occurrence", |f| {
            f.vocabulary(&OCCURRENCE)
                .description("Likelihood of this failure occurring before mitigation");
        });
        k.field("detection", |f| {
            f.vocabulary(&DETECTION)
                .description("Likelihood of detecting this failure before it causes harm");
        });
        k.field("cause", |f| {
            f.field_type(FieldType::String)
                .required()
                .description("Root cause or trigger of the failure");
        });
        k.field("effect", |f| {
            f.field_type(FieldType::String)
                .required()
                .description("Consequence or impact when the failure occurs");
        });
        k.field("mitigation", |f| {
            f.field_type(FieldType::String)
                .description("Strategy or action to reduce risk of the failure")
                .normative();
        });
        k.field("post_severity", |f| {
            f.vocabulary(&SEVERITY)
                .description("Impact severity rating after mitigation is applied");
        });
        k.field("post_occurrence", |f| {
            f.vocabulary(&OCCURRENCE)
                .description("Likelihood of occurrence after mitigation is applied");
        });
        k.field("post_detection", |f| {
            f.vocabulary(&DETECTION)
                .description("Likelihood of detection after mitigation is applied");
        });
        k.field("invariant", |f| {
            f.field_type(FieldType::Reference)
                .description("The invariant that this failure mode threatens")
                .edge("FailureModeTargetsInvariant")
                .target_kind("invariant");
        });
        k.field("refs", |f| {
            f.field_type(FieldType::StringList)
                .description("External references (URLs, documents, issue links)");
        });
        k.field("description", |f| {
            f.field_type(FieldType::String)
                .description("Free-form description of the failure mode");
        });
        k.field("risk", |f| {
            f.field_type(FieldType::String)
                .description("Overall risk assessment combining severity, occurrence, and detection");
        });
        k.field("rpn", |f| {
            f.field_type(FieldType::Integer)
                .description("Risk Priority Number (severity x occurrence x detection)");
        });
        k.field("post_mitigation", |f| {
            f.field_type(FieldType::Block)
                .description("Residual risk after mitigation: severity, occurrence, detection and rpn");
        });
        k.field("threatens_features", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Product features threatened by this failure mode (requires @specforge/product)")
                .edge("FailureModeThreatensFeature")
                .target_kind("feature");
        });
        k.field("affected_behaviors", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Behaviors affected by this failure mode as failure vectors")
                .edge("FailureModeAffectsBehavior")
                .target_kind("behavior");
        });
    });
}

fn edges(c: &mut ContributionsBuilder) {
    c.edge("DecisionProtectsInvariant", |e| {
        e.description("Decision protects an invariant")
            .source_kind("decision")
            .target_kind("invariant")
            .edge_style("dashed")
            .edge_color("#6A1B9A");
    });
    c.edge("ConstraintEnforcedByBehavior", |e| {
        e.description("Constraint is enforced by a behavior")
            .source_kind("constraint")
            .target_kind("behavior")
            .edge_style("solid")
            .edge_color("#BF360C");
    });
    c.edge("DecisionSupersedesDecision", |e| {
        e.description("Decision supersedes a previous decision")
            .source_kind("decision")
            .target_kind("decision")
            .edge_style("dotted")
            .edge_color("#6A1B9A");
    });
    c.edge("ConstraintConstrainsBehavior", |e| {
        e.description("Constraint constrains a behavior")
            .source_kind("constraint")
            .target_kind("behavior")
            .edge_style("dashed")
            .edge_color("#BF360C");
    });
    c.edge("ConstraintProtectsInvariant", |e| {
        e.description("Constraint protects an invariant")
            .source_kind("constraint")
            .target_kind("invariant")
            .edge_style("dashed")
            .edge_color("#BF360C");
    });
    c.edge("FailureModeTargetsInvariant", |e| {
        e.description("Failure mode targets an invariant")
            .source_kind("failure_mode")
            .target_kind("invariant")
            .edge_style("dashed")
            .edge_color("#D32F2F");
    });
    c.edge("ConstraintGovernsFeature", |e| {
        e.description("Constraint governs a product feature")
            .source_kind("constraint")
            .target_kind("feature")
            .edge_style("dashed")
            .edge_color("#CC6600");
    });
    c.edge("DecisionAffectsFeature", |e| {
        e.description("Decision affects a product feature")
            .source_kind("decision")
            .target_kind("feature")
            .edge_style("dashed")
            .edge_color("#CC6600");
    });
    c.edge("FailureModeThreatensFeature", |e| {
        e.description("Failure mode threatens a product feature")
            .source_kind("failure_mode")
            .target_kind("feature")
            .edge_style("dashed")
            .edge_color("#CC0000");
    });
    c.edge("DecisionImposesConstraint", |e| {
        e.description("Decision imposes a constraint on the system")
            .source_kind("decision")
            .target_kind("constraint")
            .edge_style("solid")
            .edge_color("#6A1B9A");
    });
    c.edge("FailureModeAffectsBehavior", |e| {
        e.description("Failure mode identifies a behavior as a failure vector")
            .source_kind("failure_mode")
            .target_kind("behavior")
            .edge_style("dashed")
            .edge_color("#D32F2F");
    });
}
