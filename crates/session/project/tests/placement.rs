//! Where a diagnostic about entities with no span of its own is located: at the first one's
//! block, once, for every surface (ADR 0227).
use specforge_project::testing::TestProject;
use specforge_test::prelude::*;

const CYCLE: &str = "behavior alpha \"A\" {\n  depends_on [beta]\n}\nbehavior beta \"B\" {\n  depends_on [alpha]\n}\n";

#[specforge_test(
    behavior = "present_diagnostics_as_json",
    verify = "a diagnostic about entities without a span of its own is located at the first one's block"
)]
#[ignore = "flipped by T3"]
fn a_spanless_diagnostic_about_entities_is_located_at_the_first_ones_block() {
    let dir = TestProject::new()
        .config(|c| c["name"] = "c".into())
        .enabling(&["@specforge/software"])
        .files(&[("cycle.spec", CYCLE)]);
    let session = dir.session();
    let project = session.project();
    let diagnostics = project.diagnostics();
    let w061: Vec<_> = diagnostics.iter().filter(|d| d.code == "W061").collect();
    assert_eq!(w061.len(), 1, "{diagnostics:?}");
    let alpha = &project.graph().node("alpha").unwrap().source_span;
    assert_eq!(
        (alpha.file.as_str(), alpha.start_line, alpha.end_line),
        ("cycle.spec", 1, 3)
    );
    assert_eq!(w061[0].span.as_ref(), Some(alpha), "alpha's block");
}
