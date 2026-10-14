//! What @specforge/product declares: its kinds, edges, shared fields, enhancements, validation rules, passes, feature flags, with the SDK builders.
//! The host loads exactly this (`ContributionsBuilder::declaration`);
//! `crates/runtime/component/tests/declarations/` pins its wire form.

use crate::queries::PRODUCT_KINDS;
use crate::vocabulary::*;
use specforge_extension_sdk::prelude::*;

/// A tag: lowercase letters, digits and hyphens, 2-50 characters, not starting or ending with a hyphen.
const TAG_FORMAT: &str =
    "^(?:[a-z0-9][a-z0-9-]{0,48}[a-z0-9])?(?:, (?:[a-z0-9][a-z0-9-]{0,48}[a-z0-9])?)*$";

/// Declare everything this module holds on `c`.
pub(crate) fn declare(c: &mut ContributionsBuilder) {
    kinds(c);
    edges(c);
    shared_fields(c);
    rules(c);
}

fn kinds(c: &mut ContributionsBuilder) {
    c.kind("Feature", |k| {
        k.keyword("feature")
            .description("A user-facing capability that solves a specific problem")
            .semantic_token("class")
            .lsp_icon("Class")
            .dot_shape("box")
            .dot_color("#2196F3")
            .dot_fillcolor("#E3F2FD")
            .inference_guide("Look for user-facing capabilities described in product documents, issue trackers, or feature flag configurations. Signals: feature flag definitions; product requirement documents; user stories in README or docs/; GitHub issues labeled 'feature' or 'enhancement'; CHANGELOG entries describing new capabilities; marketing copy describing what users can do. Extract the problem (user pain point) and solution (how the feature addresses it). Fill acceptance with testable completion criteria (e.g., 'User can export CSV in <2s'). Set priority and status from issue labels or project boards. Link depends_on for prerequisite features. Features are user-facing capabilities; for internal technical units of work, use behavior instead. Skip: internal technical improvements, bug fixes, refactoring work.")
            .lifecycle_field("status");
        k.field("description", |f| {
            f.field_type(FieldType::String)
                .description("Human-readable summary of the feature");
        });
        k.field("problem", |f| {
            f.field_type(FieldType::String)
                .required()
                .description("The user problem this feature addresses")
                .normative();
        });
        k.field("solution", |f| {
            f.field_type(FieldType::String)
                .description("How this feature solves the stated problem")
                .normative();
        });
        k.field("priority", |f| {
            f.vocabulary(&PRIORITY)
                .description("Importance level");
        });
        k.field("status", |f| {
            f.vocabulary(&FEATURE_STATUS)
                .description("Lifecycle state")
                .headline();
        });
        k.field("acceptance", |f| {
            f.field_type(FieldType::StringList)
                .description("Criteria that must be met for the feature to be considered complete");
        });
        k.field("depends_on", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Other features that must be completed before this one")
                .edge("FeatureDependsOn")
                .target_kind("feature");
        });
        k.field("features", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Related features referenced by this feature")
                .edge("FeatureRelatesTo")
                .target_kind("feature");
        });
        k.field("refs", |f| {
            f.field_type(FieldType::StringList)
                .description("External references such as issues, URLs, or documents");
        });
        k.field("reason", |f| {
            f.field_type(FieldType::String)
                .description("Justification for the current status or a status change");
        });
        k.field("owner", |f| {
            f.field_type(FieldType::String)
                .description("Person or team responsible for this feature");
        });
        k.field("contributors", |f| {
            f.field_type(FieldType::StringList)
                .description("Additional people or teams contributing to this feature");
        });
        k.field("effort", |f| {
            f.vocabulary(&EFFORT)
                .description("T-shirt size estimate");
        });
        k.field("tests", |f| {
            f.field_type(FieldType::StringList)
                .description("Executable test files that verify this feature, relative to the project root (RES-15 linkage)");
        });
    });
    c.kind("Journey", |k| {
        k.keyword("journey")
            .description("A user journey through the product experience")
            .semantic_token("event")
            .lsp_icon("Event")
            .dot_shape("ellipse")
            .dot_color("#FF9800")
            .dot_fillcolor("#FFF3E0")
            .inference_guide("Look for user workflows, onboarding flows, tutorial sequences, and end-to-end scenarios described in docs or UX artifacts. Signals: user flow diagrams; onboarding guides; tutorial/walkthrough documentation; e2e test scenarios that follow a user path; README 'Getting Started' sections describing multi-step workflows. Each journey should reference a persona and list flow steps in order. Link features exercised during the journey. Link channels used (web, CLI, API). Skip: single-action interactions (those are behaviors), internal system workflows, developer-only processes.");
        k.field("persona", |f| {
            f.field_type(FieldType::Reference)
                .description("The user archetype who undertakes this journey")
                .edge("JourneyTargetsPersona")
                .target_kind("persona");
        });
        k.field("description", |f| {
            f.field_type(FieldType::String)
                .description("Human-readable summary of the journey");
        });
        k.field("channels", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Communication or distribution channels used in this journey")
                .edge("JourneyUsesChannel")
                .target_kind("channel");
        });
        k.field("features", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Features exercised during this journey")
                .edge("JourneyExercisesFeature")
                .target_kind("feature");
        });
        k.field("flow", |f| {
            f.field_type(FieldType::StringList)
                .required()
                .description("Ordered steps the user takes through the journey");
        });
        k.field("priority", |f| {
            f.vocabulary(&PRIORITY)
                .description("Importance level");
        });
    });
    c.kind("Deliverable", |k| {
        k.keyword("deliverable")
            .description("A shippable unit of work with dependencies and milestones")
            .semantic_token("struct")
            .lsp_icon("Package")
            .dot_shape("box3d")
            .dot_color("#4CAF50")
            .dot_fillcolor("#E8F5E9")
            .inference_guide("Look for independently shippable artifacts: binaries, packages, services, libraries, or documentation bundles. Signals: Cargo.toml/package.json with distinct package names; Dockerfile definitions; CI/CD deployment targets; published npm/crate packages; separate apps in a monorepo (apps/ directory); versioned API specifications. Set artifact_type to the kind of artifact. Link modules it contains and milestones it's tracked under. Link depends_on for build or runtime dependencies between deliverables. Skip: internal modules (those are modules), test utilities, development-only tooling.")
            .lifecycle_field("status");
        k.field("description", |f| {
            f.field_type(FieldType::String)
                .description("Human-readable summary of the deliverable");
        });
        k.field("artifact_type", |f| {
            f.vocabulary(&ARTIFACT_TYPE)
                .required()
                .description("Kind of artifact produced");
        });
        k.field("status", |f| {
            f.vocabulary(&DELIVERABLE_STATUS)
                .description("Lifecycle state")
                .headline();
        });
        k.field("journeys", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("User journeys this deliverable supports")
                .edge("DeliverableSupportsJourney")
                .target_kind("journey");
        });
        k.field("modules", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Modules included in this deliverable")
                .edge("DeliverableContainsModule")
                .target_kind("module");
        });
        k.field("version", |f| {
            f.field_type(FieldType::String)
                .description("Semantic version of this deliverable");
        });
        k.field("milestones", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Milestones this deliverable is tracked under")
                .edge("DeliverableTrackedByMilestone")
                .target_kind("milestone");
        });
        k.field("depends_on", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Other deliverables that must ship before this one")
                .edge("DeliverableDependsOn")
                .target_kind("deliverable");
        });
        k.field("reason", |f| {
            f.field_type(FieldType::String)
                .description("Justification for the current status or a status change");
        });
        k.field("owner", |f| {
            f.field_type(FieldType::String)
                .description("Person or team responsible for this deliverable");
        });
        k.field("contributors", |f| {
            f.field_type(FieldType::StringList)
                .description("Additional people or teams contributing to this deliverable");
        });
    });
    c.kind("Milestone", |k| {
        k.keyword("milestone")
            .description("A significant project checkpoint with tracked progress")
            .semantic_token("namespace")
            .lsp_icon("Folder")
            .dot_shape("hexagon")
            .dot_color("#9C27B0")
            .dot_fillcolor("#F3E5F5")
            .inference_guide("Look for project phases, release planning checkpoints, and grouped sets of work with target dates. Signals: GitHub milestones; project board columns representing phases; roadmap documents with dated targets; sprint/iteration boundaries; 'Phase 1/2/3' labels; release planning documents. Extract target_date, exit_criteria, and status. Link features that must be delivered and modules scoped to this milestone. Link depends_on for sequential milestone ordering. Skip: individual tasks (those are features or behaviors), recurring ceremonies, ongoing maintenance work.")
            .lifecycle_field("status");
        k.field("description", |f| {
            f.field_type(FieldType::String)
                .description("Human-readable summary of the milestone");
        });
        k.field("status", |f| {
            f.vocabulary(&MILESTONE_STATUS)
                .description("Lifecycle state")
                .headline();
        });
        k.field("features", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Features that must be delivered to complete this milestone")
                .edge("MilestoneDeliversFeature")
                .target_kind("feature");
        });
        k.field("exit_criteria", |f| {
            f.field_type(FieldType::StringList)
                .description("Conditions that must be met to consider the milestone complete");
        });
        k.field("target_date", |f| {
            f.field_type(FieldType::String)
                .description("Planned completion date for this milestone");
        });
        k.field("start_date", |f| {
            f.field_type(FieldType::String)
                .description("Date when work on this milestone began or is planned to begin");
        });
        k.field("modules", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Modules scoped to this milestone")
                .edge("MilestoneScopesModule")
                .target_kind("module");
        });
        k.field("depends_on", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Other milestones that must be completed before this one")
                .edge("MilestoneDependsOn")
                .target_kind("milestone");
        });
        k.field("blockers", |f| {
            f.field_type(FieldType::StringList)
                .description("Outstanding issues preventing progress on this milestone");
        });
        k.field("priority", |f| {
            f.vocabulary(&PRIORITY)
                .description("Importance level");
        });
        k.field("reason", |f| {
            f.field_type(FieldType::String)
                .description("Justification for the current status or a status change");
        });
        k.field("owner", |f| {
            f.field_type(FieldType::String)
                .description("Person or team responsible for this milestone");
        });
        k.field("contributors", |f| {
            f.field_type(FieldType::StringList)
                .description("Additional people or teams contributing to this milestone");
        });
        k.field("refs", |f| {
            f.field_type(FieldType::StringList)
                .description("External references such as issues, URLs, or documents");
        });
    });
    c.kind("Module", |k| {
        k.keyword("module")
            .description("A logical grouping of related features and behaviors")
            .semantic_token("namespace")
            .lsp_icon("Module")
            .dot_shape("component")
            .dot_color("#607D8B")
            .dot_fillcolor("#ECEFF1")
            .inference_guide("Look for logical boundaries in the codebase: bounded contexts, packages, crates, or top-level directories that group related functionality. Signals: top-level directories in src/ (e.g., src/auth/, src/billing/); Cargo workspace members; Go packages; separate npm workspaces; DDD bounded contexts; module declarations in architecture docs. Set family for higher-level grouping (e.g., 'core', 'platform', 'integration'). Link features contained in this module and depends_on for inter-module dependencies. Modules are logical groupings of features; for shippable artifacts, use deliverable instead. Skip: pure utility directories with no domain cohesion, test directories, build tooling.");
        k.field("family", |f| {
            f.field_type(FieldType::String).description(&format!(
                "Category or family this module belongs to; the standard ones are {}",
                MODULE_FAMILY.values().join(", ")
            ));
        });
        k.field("description", |f| {
            f.field_type(FieldType::String)
                .description("Human-readable summary of the module");
        });
        k.field("features", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Features contained within this module")
                .edge("ModuleContainsFeature")
                .target_kind("feature");
        });
        k.field("depends_on", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Other modules this module depends on")
                .edge("ModuleDependsOn")
                .target_kind("module");
        });
        k.field("reason", |f| {
            f.field_type(FieldType::String)
                .description("Justification for this module's existence or grouping");
        });
    });
    c.kind("Term", |k| {
        k.keyword("term")
            .description("A glossary entry defining domain-specific vocabulary")
            .semantic_token("string")
            .lsp_icon("Text")
            .dot_shape("note")
            .dot_color("#795548")
            .dot_fillcolor("#EFEBE9")
            .inference_guide("Look for domain-specific vocabulary that team members need to agree on. Signals: glossary sections in docs; comments explaining domain jargon; type/struct names that embed domain concepts; README sections defining terminology; onboarding docs explaining project vocabulary; code comments like 'in our context, X means...'. The definition field should be precise and unambiguous. Add aliases for abbreviations or alternative names. Link see_also for related terms and module for the bounded context that owns the term. Skip: standard programming terms, framework concepts, widely-known acronyms.");
        k.field("definition", |f| {
            f.field_type(FieldType::String)
                .required()
                .description("The precise meaning of this term in the project's domain")
                .normative();
        });
        k.field("context", |f| {
            f.field_type(FieldType::String)
                .description("Where or how this term is typically used");
        });
        k.field("aliases", |f| {
            f.field_type(FieldType::StringList)
                .description("Alternative names or abbreviations for this term");
        });
        k.field("see_also", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Related terms for cross-referencing")
                .edge("TermReferencesRelatedTerm")
                .target_kind("term");
        });
        k.field("module", |f| {
            f.field_type(FieldType::Reference)
                .description("The module (bounded context) that owns this term's definition")
                .edge("TermBelongsToModule")
                .target_kind("module");
        });
    });
    c.kind("Persona", |k| {
        k.keyword("persona")
            .description("A user archetype representing a target audience segment")
            .semantic_token("variable")
            .lsp_icon("Variable")
            .dot_shape("ellipse")
            .dot_color("#E91E63")
            .dot_fillcolor("#FCE4EC")
            .inference_guide("Look for distinct user archetypes referenced in product docs, user stories, or access control systems. Signals: role-based access control definitions (admin, editor, viewer); user story formats ('As a <persona>...'); docs/personas/ directory; marketing segments; README describing who the product is for; distinct CLI/API user types. Extract technical_level, goals, and pain_points. Link key_features this persona cares about. One persona per distinct user archetype with meaningfully different needs. Skip: system/service accounts, test users, internal roles that don't represent real users.")
            .lifecycle_field("status");
        k.field("description", |f| {
            f.field_type(FieldType::String)
                .required()
                .description("Human-readable summary of this persona");
        });
        k.field("technical_level", |f| {
            f.vocabulary(&TECHNICAL_LEVEL)
                .description("Technical proficiency of this persona");
        });
        k.field("goals", |f| {
            f.field_type(FieldType::StringList)
                .description("What this persona wants to achieve with the product");
        });
        k.field("pain_points", |f| {
            f.field_type(FieldType::StringList)
                .description("Frustrations or problems this persona currently faces");
        });
        k.field("status", |f| {
            f.vocabulary(&PERSONA_STATUS)
                .description("Lifecycle state")
                .headline();
        });
        k.field("reason", |f| {
            f.field_type(FieldType::String)
                .description("Justification for the current status or a status change");
        });
        k.field("key_features", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Features this persona cares about most")
                .edge("PersonaPrioritizesFeature")
                .target_kind("feature");
        });
    });
    c.kind("Channel", |k| {
        k.keyword("channel")
            .description("A communication or distribution channel for the product")
            .semantic_token("interface")
            .lsp_icon("Interface")
            .dot_shape("rectangle")
            .dot_color("#00BCD4")
            .dot_fillcolor("#E0F7FA")
            .inference_guide("Look for distinct surfaces through which users interact with the product. Signals: separate frontend applications (web app, mobile app, CLI); API endpoints exposed to external consumers; notification channels (email, push, SMS); documentation sites; IDE extensions; webhook integrations; Slack/Discord bots. Set interaction_model to how users interact through it. Set url if applicable. One channel per distinct interaction surface. Skip: internal communication between services (those are ports), monitoring/alerting channels, CI/CD pipelines.")
            .lifecycle_field("status");
        k.field("description", |f| {
            f.field_type(FieldType::String)
                .required()
                .description("Human-readable summary of this channel");
        });
        k.field("interaction_model", |f| {
            f.vocabulary(&INTERACTION_MODEL)
                .description("How users interact through this channel");
        });
        k.field("url", |f| {
            f.field_type(FieldType::String)
                .description("URL or endpoint for this channel");
        });
        k.field("status", |f| {
            f.vocabulary(&CHANNEL_STATUS)
                .description("Lifecycle state")
                .headline();
        });
        k.field("reason", |f| {
            f.field_type(FieldType::String)
                .description("Justification for the current status or a status change");
        });
    });
    c.kind("Release", |k| {
        k.keyword("release")
            .description("A versioned product release bundling deliverables")
            .semantic_token("constant")
            .lsp_icon("Constant")
            .dot_shape("doubleoctagon")
            .dot_color("#FF5722")
            .dot_fillcolor("#FBE9E7")
            .inference_guide("Look for versioned release artifacts, changelogs, and release planning documents. Signals: CHANGELOG.md entries; GitHub releases; git tags following semver; release branches (release/v*); Cargo.toml/package.json version fields; release notes documents; deployment manifests with version numbers. Set version (semver), target_date, and status. Link deliverables included and milestones completed by this release. Skip: pre-release/nightly builds (unless formally tracked), hotfix patches (unless they warrant a release entity), internal development versions.")
            .lifecycle_field("status");
        k.field("description", |f| {
            f.field_type(FieldType::String)
                .description("Human-readable summary of this release");
        });
        k.field("version", |f| {
            f.field_type(FieldType::String)
                .required()
                .description("Semantic version identifier for this release");
        });
        k.field("status", |f| {
            f.vocabulary(&RELEASE_STATUS)
                .description("Lifecycle state of the release")
                .headline();
        });
        k.field("deliverables", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Deliverables included in this release")
                .edge("ReleaseIncludesDeliverable")
                .target_kind("deliverable");
        });
        k.field("milestones", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Milestones completed by this release")
                .edge("ReleaseCompletesMilestone")
                .target_kind("milestone");
        });
        k.field("target_date", |f| {
            f.field_type(FieldType::String)
                .description("Planned release date");
        });
        k.field("release_date", |f| {
            f.field_type(FieldType::String)
                .description("Actual date the release shipped");
        });
        k.field("changelog", |f| {
            f.field_type(FieldType::String)
                .description("Summary of changes included in this release");
        });
        k.field("depends_on", |f| {
            f.field_type(FieldType::ReferenceList)
                .description("Prior releases that must ship before this one")
                .edge("ReleaseDependsOn")
                .target_kind("release");
        });
        k.field("owner", |f| {
            f.field_type(FieldType::String)
                .description("Person or team responsible for this release");
        });
        k.field("contributors", |f| {
            f.field_type(FieldType::StringList)
                .description("Additional people or teams contributing to this release");
        });
        k.field("reason", |f| {
            f.field_type(FieldType::String)
                .description("Justification for the current status or a status change");
        });
        k.field("refs", |f| {
            f.field_type(FieldType::StringList)
                .description("External references such as issues, URLs, or documents");
        });
    });
}

fn edges(c: &mut ContributionsBuilder) {
    c.edge("FeatureDependsOn", |e| {
        e.source_kind("feature")
            .target_kind("feature")
            .edge_style("dashed")
            .edge_color("#2196F3");
    });
    c.edge("FeatureRelatesTo", |e| {
        e.description("Feature has a non-dependency relationship to another feature")
            .source_kind("feature")
            .target_kind("feature")
            .edge_style("dotted")
            .edge_color("#2196F3");
    });
    c.edge("JourneyExercisesFeature", |e| {
        e.source_kind("journey")
            .target_kind("feature")
            .edge_style("solid")
            .edge_color("#FF9800");
    });
    c.edge("JourneyTargetsPersona", |e| {
        e.source_kind("journey")
            .target_kind("persona")
            .edge_style("solid")
            .edge_color("#E91E63");
    });
    c.edge("JourneyUsesChannel", |e| {
        e.source_kind("journey")
            .target_kind("channel")
            .edge_style("solid")
            .edge_color("#00BCD4");
    });
    c.edge("DeliverableSupportsJourney", |e| {
        e.source_kind("deliverable")
            .target_kind("journey")
            .edge_style("solid")
            .edge_color("#4CAF50");
    });
    c.edge("DeliverableContainsModule", |e| {
        e.source_kind("deliverable")
            .target_kind("module")
            .edge_style("solid")
            .edge_color("#607D8B");
    });
    c.edge("DeliverableTrackedByMilestone", |e| {
        e.source_kind("deliverable")
            .target_kind("milestone")
            .edge_style("solid")
            .edge_color("#9C27B0");
    });
    c.edge("DeliverableDependsOn", |e| {
        e.source_kind("deliverable")
            .target_kind("deliverable")
            .edge_style("dashed")
            .edge_color("#4CAF50");
    });
    c.edge("MilestoneDeliversFeature", |e| {
        e.source_kind("milestone")
            .target_kind("feature")
            .edge_style("solid")
            .edge_color("#9C27B0");
    });
    c.edge("MilestoneScopesModule", |e| {
        e.source_kind("milestone")
            .target_kind("module")
            .edge_style("solid")
            .edge_color("#607D8B");
    });
    c.edge("MilestoneDependsOn", |e| {
        e.source_kind("milestone")
            .target_kind("milestone")
            .edge_style("dashed")
            .edge_color("#9C27B0");
    });
    c.edge("ModuleContainsFeature", |e| {
        e.source_kind("module")
            .target_kind("feature")
            .edge_style("solid")
            .edge_color("#607D8B");
    });
    c.edge("ModuleDependsOn", |e| {
        e.source_kind("module")
            .target_kind("module")
            .edge_style("dashed")
            .edge_color("#607D8B");
    });
    c.edge("TermReferencesRelatedTerm", |e| {
        e.source_kind("term")
            .target_kind("term")
            .edge_style("dotted")
            .edge_color("#795548");
    });
    c.edge("TermBelongsToModule", |e| {
        e.description("Term is defined within a module's bounded context")
            .source_kind("term")
            .target_kind("module")
            .edge_style("dotted")
            .edge_color("#795548");
    });
    c.edge("ReleaseIncludesDeliverable", |e| {
        e.source_kind("release")
            .target_kind("deliverable")
            .edge_style("solid")
            .edge_color("#FF5722");
    });
    c.edge("ReleaseCompletesMilestone", |e| {
        e.source_kind("release")
            .target_kind("milestone")
            .edge_style("solid")
            .edge_color("#FF5722");
    });
    c.edge("ReleaseDependsOn", |e| {
        e.source_kind("release")
            .target_kind("release")
            .edge_style("dashed")
            .edge_color("#FF5722");
    });
    c.edge("PersonaPrioritizesFeature", |e| {
        e.description("Persona prioritizes a feature")
            .source_kind("persona")
            .target_kind("feature")
            .edge_style("solid")
            .edge_color("#E91E63");
    });
}

fn shared_fields(c: &mut ContributionsBuilder) {
    c.shared_field("tags", |f| {
        f.field_type(FieldType::StringList)
            .description("Freeform labels for filtering and categorization");
    });
}

fn rules(c: &mut ContributionsBuilder) {
    c.rule("W041", |r| {
        r.check(CheckKind::NoIncomingEdges)
            .severity(ValidationSeverity::Warning)
            .message_template("feature '{id}' has no incoming edges — it may be unreferenced by any journey, milestone, or module")
            .target_kind("feature");
    });
    c.rule("W042", |r| {
        r.check(CheckKind::NoIncomingEdges)
            .severity(ValidationSeverity::Warning)
            .message_template(
                "journey '{id}' has no incoming edges — it may be unreferenced by any deliverable",
            )
            .target_kind("journey");
    });
    c.rule("W044", |r| {
        r.check(CheckKind::NoIncomingEdges)
            .severity(ValidationSeverity::Warning)
            .message_template("module '{id}' has no incoming edges — it may be unreferenced by any deliverable or milestone")
            .target_kind("module");
    });
    c.rule("I010", |r| {
        r.check(CheckKind::NoEdges)
            .severity(ValidationSeverity::Info)
            .message_template("term '{id}' has no edges — it may be unreferenced")
            .target_kind("term");
    });
    c.rule("I046", |r| {
        r.check(CheckKind::NoIncomingEdges)
            .severity(ValidationSeverity::Info)
            .message_template(
                "persona '{id}' has no incoming edges — it may be unreferenced by any journey",
            )
            .target_kind("persona");
    });
    c.rule("I047", |r| {
        r.check(CheckKind::NoIncomingEdges)
            .severity(ValidationSeverity::Info)
            .message_template(
                "channel '{id}' has no incoming edges — it may be unreferenced by any journey",
            )
            .target_kind("channel");
    });
    c.rule("E007", |r| {
        r.check(CheckKind::CycleDetection)
            .severity(ValidationSeverity::Error)
            .message_template("module dependency cycle detected involving '{id}'")
            .target_kind("module")
            .edge_type("ModuleDependsOn");
    });
    c.rule("E015", |r| {
        r.check(CheckKind::CycleDetection)
            .severity(ValidationSeverity::Error)
            .message_template("milestone dependency cycle detected involving '{id}'")
            .target_kind("milestone")
            .edge_type("MilestoneDependsOn");
    });
    c.rule("E052", |r| {
        r.check(CheckKind::CycleDetection)
            .severity(ValidationSeverity::Error)
            .message_template("deliverable dependency cycle detected involving '{id}'")
            .target_kind("deliverable")
            .edge_type("DeliverableDependsOn");
    });
    c.rule("W045", |r| {
        r.check(CheckKind::CycleDetection)
            .severity(ValidationSeverity::Warning)
            .message_template("feature dependency cycle detected involving '{id}'")
            .target_kind("feature")
            .edge_type("FeatureDependsOn");
    });
    c.rule("W092", |r| {
        r.check(CheckKind::CycleDetection)
            .severity(ValidationSeverity::Warning)
            .message_template("release dependency cycle detected involving '{id}'")
            .target_kind("release")
            .edge_type("ReleaseDependsOn");
    });
    c.rule("W093", |r| {
        r.check(CheckKind::FieldValueConstraint)
            .severity(ValidationSeverity::Warning)
            .message_template(
                "release '{id}' has invalid version format — expected semver (e.g., 1.0.0)",
            )
            .target_kind("release")
            .field("version");
        r.constraint(|fc| {
            fc.kind(ConstraintKind::Matches)
                .pattern("^\\d+\\.\\d+\\.\\d+(-[a-zA-Z0-9.]+)?(\\+[a-zA-Z0-9.]+)?$");
        });
    });
    c.rule("I048", |r| {
        r.check(CheckKind::FieldValueConstraint)
            .severity(ValidationSeverity::Info)
            .message_template("feature '{id}' has no acceptance criteria")
            .target_kind("feature")
            .field("acceptance");
        r.constraint(|fc| {
            fc.kind(ConstraintKind::NonEmpty);
        });
    });
    c.rule("I062", |r| {
        r.check(CheckKind::FieldValueConstraint)
            .severity(ValidationSeverity::Info)
            .message_template(
                "module '{id}' has non-standard family '{value}' — standard families: {allowed}",
            )
            .target_kind("module")
            .field("family");
        r.constraint(|fc| {
            fc.kind(ConstraintKind::OneOf)
                .values(MODULE_FAMILY.values());
        });
    });
    c.rule("I053", |r| {
        r.check(CheckKind::FieldValueConstraint)
            .severity(ValidationSeverity::Info)
            .message_template("milestone '{id}' has target_date '{value}' — expected YYYY-MM-DD")
            .target_kind("milestone")
            .field("target_date");
        r.constraint(|fc| {
            fc.kind(ConstraintKind::Matches)
                .pattern("^\\d{4}-\\d{2}-\\d{2}$");
        });
    });
    c.rule("I061", |r| {
        r.check(CheckKind::FieldValueConstraint)
            .severity(ValidationSeverity::Info)
            .message_template("deliverable '{id}' has version '{value}' — expected semver (e.g., 1.0.0)")
            .target_kind("deliverable")
            .field("version");
        r.constraint(|fc| {
            fc.kind(ConstraintKind::Matches)
                .pattern("^\\d+\\.\\d+\\.\\d+(-[a-zA-Z0-9]+(\\.[a-zA-Z0-9]+)*)?(\\+[a-zA-Z0-9]+(\\.[a-zA-Z0-9]+)*)?$");
        });
    });
    c.rule("I050", |r| {
        r.check(CheckKind::FieldValueConstraint)
            .severity(ValidationSeverity::Info)
            .message_template("journey '{id}' has an empty flow")
            .target_kind("journey")
            .field("flow");
        r.constraint(|fc| {
            fc.kind(ConstraintKind::NonEmpty);
        });
    });
    // One declaration per product kind, the same pattern for each.
    for kind in PRODUCT_KINDS {
        c.rule("I068", |r| {
            r.check(CheckKind::FieldValueConstraint)
                .severity(ValidationSeverity::Info)
                .message_template("{kind} '{id}' has a tag that is not lowercase-hyphenated (2-50 of a-z, 0-9, -, not starting or ending with -): tags [{value}]")
                .target_kind(kind)
                .field("tags");
            r.constraint(|fc| {
                fc.kind(ConstraintKind::Matches).pattern(TAG_FORMAT);
            });
        });
    }
    c.rule("I086", |r| {
        r.check(CheckKind::FieldValueConstraint)
            .severity(ValidationSeverity::Info)
            .message_template("release '{id}' has release_date '{value}' — expected YYYY-MM-DD")
            .target_kind("release")
            .field("release_date");
        r.constraint(|fc| {
            fc.kind(ConstraintKind::Matches)
                .pattern("^\\d{4}-\\d{2}-\\d{2}$");
        });
    });
    c.rule("I087", |r| {
        r.check(CheckKind::FieldValueConstraint)
            .severity(ValidationSeverity::Info)
            .message_template("milestone '{id}' has start_date '{value}' — expected YYYY-MM-DD")
            .target_kind("milestone")
            .field("start_date");
        r.constraint(|fc| {
            fc.kind(ConstraintKind::Matches)
                .pattern("^\\d{4}-\\d{2}-\\d{2}$");
        });
    });
    c.rule("W049", |r| {
        r.check(CheckKind::MissingFieldWhenFlagSet)
            .severity(ValidationSeverity::Warning)
            .message_template("milestone '{id}' has no features — it may be empty")
            .target_kind("milestone")
            .field("features");
    });
    c.rule("I059", |r| {
        r.check(CheckKind::ConditionalFieldRequired)
            .severity(ValidationSeverity::Info)
            .message_template("feature '{id}' has status 'deferred' but no reason --- consider adding a reason field")
            .target_kind("feature")
            .field("reason");
        r.constraint(|fc| {
            fc.kind(ConstraintKind::WhenFieldEquals)
                .pattern("status")
                .values(&[FEATURE_DEFERRED]);
        });
    });
    c.rule("W057", |r| {
        r.check(CheckKind::ConditionalFieldRequired)
            .severity(ValidationSeverity::Warning)
            .message_template("milestone '{id}' has status 'completed' but no exit_criteria")
            .target_kind("milestone")
            .field("exit_criteria");
        r.constraint(|fc| {
            fc.kind(ConstraintKind::WhenFieldEquals)
                .pattern("status")
                .values(&[MILESTONE_COMPLETED]);
        });
    });
    c.rule("I060", |r| {
        r.check(CheckKind::ConditionalFieldRequired)
            .severity(ValidationSeverity::Info)
            .message_template("milestone '{id}' has status 'blocked' but no blockers --- consider listing what is blocking")
            .target_kind("milestone")
            .field("blockers");
        r.constraint(|fc| {
            fc.kind(ConstraintKind::WhenFieldEquals)
                .pattern("status")
                .values(&[MILESTONE_BLOCKED]);
        });
    });
    c.rule("I066", |r| {
        r.check(CheckKind::ConditionalFieldRequired)
            .severity(ValidationSeverity::Info)
            .message_template("deliverable '{id}' has status 'deprecated' but no reason")
            .target_kind("deliverable")
            .field("reason");
        r.constraint(|fc| {
            fc.kind(ConstraintKind::WhenFieldEquals)
                .pattern("status")
                .values(&[DELIVERABLE_DEPRECATED]);
        });
    });
    c.rule("I069", |r| {
        r.check(CheckKind::ConditionalFieldRequired)
            .severity(ValidationSeverity::Info)
            .message_template("persona '{id}' has status 'deprecated' but no reason")
            .target_kind("persona")
            .field("reason");
        r.constraint(|fc| {
            fc.kind(ConstraintKind::WhenFieldEquals)
                .pattern("status")
                .values(&[PERSONA_DEPRECATED]);
        });
    });
    c.rule("I070", |r| {
        r.check(CheckKind::ConditionalFieldRequired)
            .severity(ValidationSeverity::Info)
            .message_template("channel '{id}' has status 'deprecated' but no reason")
            .target_kind("channel")
            .field("reason");
        r.constraint(|fc| {
            fc.kind(ConstraintKind::WhenFieldEquals)
                .pattern("status")
                .values(&[CHANNEL_DEPRECATED]);
        });
    });
    c.rule("I057", |r| {
        r.check(CheckKind::ConditionalFieldRequired)
            .severity(ValidationSeverity::Info)
            .message_template("milestone '{id}' has status 'blocked' but no depends_on")
            .target_kind("milestone")
            .field("depends_on");
        r.constraint(|fc| {
            fc.kind(ConstraintKind::WhenFieldEquals)
                .pattern("status")
                .values(&[MILESTONE_BLOCKED]);
        });
    });
    c.rule("I089", |r| {
        r.check(CheckKind::ConditionalFieldRequired)
            .severity(ValidationSeverity::Info)
            .message_template("release '{id}' has status 'recalled' but no reason")
            .target_kind("release")
            .field("reason");
        r.constraint(|fc| {
            fc.kind(ConstraintKind::WhenFieldEquals)
                .pattern("status")
                .values(&[RELEASE_RECALLED]);
        });
    });
    c.rule("W043", |r| {
        r.check(CheckKind::NoOutgoingEdges)
            .severity(ValidationSeverity::Warning)
            .message_template("deliverable '{id}' supports no journeys")
            .target_kind("deliverable")
            .edge_type("DeliverableSupportsJourney");
    });
    c.rule("W046", |r| {
        r.check(CheckKind::NoOutgoingEdges)
            .severity(ValidationSeverity::Warning)
            .message_template("deliverable '{id}' contains no modules")
            .target_kind("deliverable")
            .edge_type("DeliverableContainsModule");
    });
    c.rule("I067", |r| {
        r.check(CheckKind::NoOutgoingEdges)
            .severity(ValidationSeverity::Info)
            .message_template("module '{id}' contains no features")
            .target_kind("module")
            .edge_type("ModuleContainsFeature");
    });
    c.rule("I055", |r| {
        r.check(CheckKind::NoOutgoingEdges)
            .severity(ValidationSeverity::Info)
            .message_template("journey '{id}' uses no channels")
            .target_kind("journey")
            .edge_type("JourneyUsesChannel");
    });
    c.rule("I082", |r| {
        r.check(CheckKind::NoOutgoingEdges)
            .severity(ValidationSeverity::Info)
            .message_template("release '{id}' includes no deliverables")
            .target_kind("release")
            .edge_type("ReleaseIncludesDeliverable");
    });
    c.rule("I083", |r| {
        r.check(CheckKind::NoOutgoingEdges)
            .severity(ValidationSeverity::Info)
            .message_template("release '{id}' completes no milestones")
            .target_kind("release")
            .edge_type("ReleaseCompletesMilestone");
    });
    c.rule("I048", |r| {
        r.check(CheckKind::MissingRequiredField)
            .severity(ValidationSeverity::Info)
            .message_template("feature '{id}' has no acceptance criteria")
            .target_kind("feature")
            .field("acceptance");
    });
    c.rule("I054", |r| {
        r.check(CheckKind::MissingRequiredField)
            .severity(ValidationSeverity::Info)
            .message_template("journey '{id}' has no persona")
            .target_kind("journey")
            .field("persona");
    });
    for kind in ["feature", "milestone", "deliverable", "release"] {
        c.rule("I080", |r| {
            r.check(CheckKind::MissingRequiredField)
                .severity(ValidationSeverity::Info)
                .message_template("{kind} '{id}' has no owner")
                .target_kind(kind)
                .field("owner");
        });
    }
    c.rule("I081", |r| {
        r.check(CheckKind::MissingRequiredField)
            .severity(ValidationSeverity::Info)
            .message_template("feature '{id}' has no effort estimate")
            .target_kind("feature")
            .field("effort");
    });
}
