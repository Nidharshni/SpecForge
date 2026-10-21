//! W061: the entities on a cycle, however the references close it and
//! however they are written (ADR 0196).

use specforge_common::DiagnosticData;
use specforge_graph::build_graph;
use specforge_parser::parse;
use specforge_test_macros::test as specforge_test;

/// The W061s of `source`.
fn w061s(source: &str) -> Vec<specforge_common::Diagnostic> {
    let (_, diagnostics) = build_graph(&[parse(source, "main.spec")]);
    diagnostics
        .into_iter()
        .filter(|d| d.code == "W061")
        .collect()
}

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "W061 names every entity of a cycle, one whose cycle closes through an entity already walked included"
)]
#[ignore = "flipped by T4"]
fn w061_names_every_entity_of_a_tangle_of_cycles() {
    let found = w061s(
        r#"
behavior alpha "A" { contract "c" depends_on [beta, delta] }
behavior beta "B" { contract "c" depends_on [gamma] }
behavior gamma "C" { contract "c" depends_on [alpha] }
behavior delta "D" { contract "c" depends_on [beta] }
"#,
    );

    assert_eq!(found.len(), 1, "{found:?}");
    let data = found[0].data.as_deref().expect("W061 carries data");
    let path: Vec<String> = [
        "alpha", "beta", "gamma", "alpha", "delta", "beta", "gamma", "alpha",
    ]
    .map(String::from)
    .into();
    assert_eq!(data, &DiagnosticData::ReferenceCycle { path });
    assert_eq!(data.entities(), ["alpha", "beta", "gamma", "delta"]);
}

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "W061 is the same whatever order the references are written in"
)]
#[ignore = "flipped by T4"]
fn w061_is_the_same_whatever_order_the_references_are_written_in() {
    let written = |first: &str, second: &str| {
        w061s(&format!(
            r#"
behavior alpha "A" {{ contract "c" depends_on [{first}, {second}] }}
behavior beta "B" {{ contract "c" depends_on [gamma] }}
behavior gamma "C" {{ contract "c" depends_on [alpha] }}
"#
        ))
    };

    let (forward, backward) = (written("beta", "gamma"), written("gamma", "beta"));

    assert_eq!(forward, backward);
    assert_eq!(forward.len(), 1);
    assert_eq!(
        forward[0].message,
        "reference cycle detected: alpha -> beta -> gamma -> alpha"
    );
}
