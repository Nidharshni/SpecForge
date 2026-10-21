//! The DOT export draws what an extension declares for it: a kind's fill and an edge type's
//! style (render_extension_defined_dot_shapes, render_extension_defined_edge_styles).

use crate::view_support::{Assembled, Viewed};
use specforge_extension_sdk::prelude::*;
use specforge_ops::read::export::{Format, Request, export};
use specforge_test_macros::test as specforge_test;

/// `@t/soft`: a `behavior` filled `#E3F2FD` consumes an `event` through `BehaviorConsumesEvent`,
/// declared dashed, `#E65100`, arrowhead `vee`.
fn styled() -> Assembled {
    let mut soft = ContributionsBuilder::new(ExtensionMeta::new("@t/soft", "1.0.0"));
    soft.kind("behavior", |k| {
        k.keyword("behavior").dot_fillcolor("#E3F2FD");
        k.field("consumes", |f| {
            f.field_type(FieldType::ReferenceList)
                .edge("BehaviorConsumesEvent")
                .target_kind("event");
        });
    });
    soft.kind("event", |k| {
        k.keyword("event");
    });
    soft.edge("BehaviorConsumesEvent", |e| {
        e.source_kind("behavior")
            .target_kind("event")
            .edge_style("dashed")
            .edge_color("#E65100")
            .edge_arrowhead("vee");
    });
    Assembled::over(specforge_registry::build_registries(vec![
        soft.declaration(),
    ]))
    .source("event e \"E\" {\n}\n\nbehavior b \"B\" {\n  consumes [e]\n}\n")
}

fn dot(project: &Assembled) -> String {
    let request = Request {
        format: Some(Format::Dot),
        ..Request::default()
    };
    export(&project.view(), &request)
        .unwrap()
        .into_value_and_notices()
        .0
}

#[specforge_test(
    behavior = "render_extension_defined_dot_shapes",
    verify = "dot_fillcolor sets DOT node fillcolor and style=filled attributes"
)]
fn a_declared_fill_color_fills_the_node() {
    let text = dot(&styled());
    assert!(
        text.contains(r##""b" [label="b\nB" fillcolor="#E3F2FD" style="filled"];"##),
        "{text}"
    );
}

#[specforge_test(
    behavior = "render_extension_defined_edge_styles",
    verify = "edge uses extension-defined edge_style"
)]
fn a_declared_edge_style_reaches_the_dot_edge() {
    let text = dot(&styled());
    assert!(
        text.contains(
            r##""b" -> "e" [label="consumes" style="dashed" color="#E65100" arrowhead="vee"];"##
        ),
        "{text}"
    );
}
