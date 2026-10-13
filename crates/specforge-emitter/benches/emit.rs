use criterion::{Criterion, black_box, criterion_group, criterion_main};
use specforge_emitter::{EmitFormat, EmitOptions, emit};
use specforge_graph::build_graph;
use specforge_parser::parse;

fn generate_spec(entity_count: usize) -> String {
    let mut out = String::new();
    for i in 0..entity_count / 5 {
        out.push_str(&format!(
            "type t_{i} \"Type {i}\" {{\n    fields {{\n        name String\n    }}\n}}\n\n"
        ));
    }
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

fn build_test_graph(entity_count: usize) -> specforge_graph::Graph {
    let source = generate_spec(entity_count);
    let spec_file = parse(&source, "bench.spec");
    let (graph, _) = build_graph(&[spec_file]);
    graph
}

fn bench_emit_json(c: &mut Criterion) {
    let graph = build_test_graph(200);
    c.bench_function("emit_json_200_entities", |b| {
        b.iter(|| emit(black_box(&graph), &EmitOptions::default()).unwrap())
    });
}

fn bench_emit_context(c: &mut Criterion) {
    let graph = build_test_graph(200);
    let context = EmitOptions {
        format: EmitFormat::Context,
        ..EmitOptions::default()
    };
    c.bench_function("emit_context_200_entities", |b| {
        b.iter(|| emit(black_box(&graph), &context).unwrap())
    });
}

fn bench_emit_brief(c: &mut Criterion) {
    let graph = build_test_graph(200);
    let brief = EmitOptions {
        format: EmitFormat::Brief,
        ..EmitOptions::default()
    };
    c.bench_function("emit_brief_200_entities", |b| {
        b.iter(|| emit(black_box(&graph), &brief).unwrap())
    });
}

fn bench_emit_json_large(c: &mut Criterion) {
    let graph = build_test_graph(1000);
    c.bench_function("emit_json_1000_entities", |b| {
        b.iter(|| emit(black_box(&graph), &EmitOptions::default()).unwrap())
    });
}

criterion_group!(
    benches,
    bench_emit_json,
    bench_emit_context,
    bench_emit_brief,
    bench_emit_json_large
);
criterion_main!(benches);
