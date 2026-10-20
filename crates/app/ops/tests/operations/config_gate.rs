//! The compile's config problems gate every run that rewrites the sources (`config::refuse_rewrite`).

use specforge_ops::authoring::edit::{self, Edit};
use specforge_ops::authoring::rename;
use specforge_ops::read::navigate::Navigator;
use specforge_ops::view::ProjectView;

/// `specforge.json` texts that do not say which files are the project's sources.
const HIDES_SOURCES: [&str; 5] = [
    r#"{"spec_root": "spec", "extensions": ["#,
    "[1,2]",
    r#"{"exclude": "vendor"}"#,
    r#"{"spec_root": ["spec"]}"#,
    r#"{"spec_root": "spec", "extensions": "@specforge/product"}"#,
];

#[test]
fn a_config_that_hides_the_sources_refuses_a_typed_edit_and_a_rename() {
    for config in HIDES_SOURCES {
        let compiled = specforge_project::testing::TestProject::new()
            .raw_config(config)
            .compile();
        let first = compiled
            .environment()
            .config_problems()
            .iter()
            .find(|p| p.blocks_rewrites())
            .unwrap_or_else(|| panic!("{config}"));
        let edit = Edit::Retitle {
            entity_id: "x".into(),
            title: "X".into(),
        };
        let refused = edit::plan(&ProjectView::of(&compiled), &[edit]).unwrap_err();
        assert_eq!(refused.error.code, "config_invalid", "{config}");
        assert_eq!(refused.error.message, first.to_string(), "{config}");
        let nav = Navigator::new(ProjectView::of(&compiled));
        let refused = rename::plan(&nav, "x", "y").unwrap_err();
        assert_eq!(refused.error.code, "config_invalid", "{config}");
        assert_eq!(refused.error.message, first.to_string(), "{config}");
    }
}
