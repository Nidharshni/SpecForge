use crate::queries::PRODUCT_KINDS;
use crate::vocabulary::*;
use specforge_extension_sdk::prelude::*;

/// A tag: lowercase letters, digits and hyphens, 2-50 characters, not starting or ending with a hyphen.
const TAG_FORMAT: &str =
    "^(?:[a-z0-9][a-z0-9-]{0,48}[a-z0-9])?(?:, (?:[a-z0-9][a-z0-9-]{0,48}[a-z0-9])?)*$";

pub(super) fn rules(c: &mut ContributionsBuilder) {
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
