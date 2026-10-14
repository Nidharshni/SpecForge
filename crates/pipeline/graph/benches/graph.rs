use criterion::{Criterion, black_box, criterion_group, criterion_main};
use specforge_graph::build_graph;
use specforge_parser::parse;

fn generate_connected_spec(entity_count: usize) -> String {
    let mut out = String::new();
    // Generate types that behaviors reference
    for i in 0..entity_count / 5 {
        out.push_str(&format!(
            "type t_{i} \"Type {i}\" {{\n    fields {{\n        name String\n    }}\n}}\n\n"
        ));
    }
    // Generate behaviors with cross-references
    for i in 0..entity_count {
        let type_ref = i % (entity_count / 5).max(1);
        out.push_str(&format!(
            r#"behavior b_{i} "Behavior {i}" {{
    contract "does something"
    status active
    types [t_{type_ref}]
}}

"#,
        ));
    }
    out
}

fn bench_build_graph_small(c: &mut Criterion) {
    let source = generate_connected_spec(50);
    let spec_file = parse(&source, "bench.spec");
    c.bench_function("build_graph_50_entities", |b| {
        b.iter(|| build_graph(black_box(std::slice::from_ref(&spec_file))))
    });
}

fn bench_build_graph_medium(c: &mut Criterion) {
    let source = generate_connected_spec(250);
    let spec_file = parse(&source, "bench.spec");
    c.bench_function("build_graph_250_entities", |b| {
        b.iter(|| build_graph(black_box(std::slice::from_ref(&spec_file))))
    });
}

fn bench_build_graph_large(c: &mut Criterion) {
    let source = generate_connected_spec(1000);
    let spec_file = parse(&source, "bench.spec");
    c.bench_function("build_graph_1000_entities", |b| {
        b.iter(|| build_graph(black_box(std::slice::from_ref(&spec_file))))
    });
}

fn bench_subgraph_depth(c: &mut Criterion) {
    let source = generate_connected_spec(500);
    let spec_file = parse(&source, "bench.spec");
    let (graph, _) = build_graph(&[spec_file]);
    c.bench_function("subgraph_depth_3", |b| {
        b.iter(|| graph.subgraph_depth(black_box("b_0"), black_box(3)))
    });
}

criterion_group!(
    benches,
    bench_build_graph_small,
    bench_build_graph_medium,
    bench_build_graph_large,
    bench_subgraph_depth
);
criterion_main!(benches);
