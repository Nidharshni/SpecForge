//! `specforge.export` answers every export format, markdown included (ADR 0152 D8).

use crate::support::*;
use serde_json::json;
use specforge_ops::read::export::{Format, Request};
use specforge_ops::view::ProjectView;
use specforge_test_macros::test as specforge_test;

#[specforge_test(
    behavior = "export_review_document",
    verify = "specforge.export answers the same document as specforge export"
)]
fn the_export_tool_answers_the_review_document() {
    let mut server = TestProject::new()
        .enabling(&["@specforge/software"])
        .file(
            "main.spec",
            "behavior alpha \"Alpha\" {\n  contract \"The system MUST work.\"\n  verify unit \"works\"\n}\n",
        )
        .serve_components();
    let root = server.root().to_path_buf();
    let expected = {
        let runtime = specforge_project::testing::component_runtime();
        let project = specforge_project::testing::compile_at(&root, Some(runtime));
        specforge_ops::read::export::export(
            &ProjectView::of(&project),
            &Request {
                format: Some(Format::Markdown),
                ..Request::default()
            },
        )
        .unwrap()
        .into_value_and_notices()
        .0
    };
    let reply = call_tool(
        &mut server,
        "specforge.export",
        json!({"format": "markdown"}),
    );
    let text = reply["result"]["content"][0]["text"].as_str().unwrap();
    assert_eq!(text, expected);
    assert!(text.starts_with("# Specification\n\n"), "{text}");
}
