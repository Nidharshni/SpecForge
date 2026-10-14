use specforge_extension_sdk::prelude::*;

pub(super) fn edges(c: &mut ContributionsBuilder) {
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
