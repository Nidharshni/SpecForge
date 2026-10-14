use criterion::{Criterion, black_box, criterion_group, criterion_main};
use specforge_parser::parse;

fn generate_spec(entity_count: usize) -> String {
    let mut out = String::new();
    for i in 0..entity_count {
        out.push_str(&format!(
            r#"behavior b_{i} "Behavior {i}" {{
    contract "does something useful"
    status active
    priority high
    features [feature_a, feature_b]
    gherkin ["tests/b_{i}.feature"]
    verify auto "has unit tests"
    verify manual "reviewed by team"
}}

"#,
            i = i
        ));
    }
    out
}

fn bench_parse_small(c: &mut Criterion) {
    let source = generate_spec(10);
    c.bench_function("parse_10_entities", |b| {
        b.iter(|| parse(black_box(&source), "bench.spec"))
    });
}

fn bench_parse_medium(c: &mut Criterion) {
    let source = generate_spec(100);
    c.bench_function("parse_100_entities", |b| {
        b.iter(|| parse(black_box(&source), "bench.spec"))
    });
}

fn bench_parse_large(c: &mut Criterion) {
    let source = generate_spec(500);
    c.bench_function("parse_500_entities", |b| {
        b.iter(|| parse(black_box(&source), "bench.spec"))
    });
}

criterion_group!(
    benches,
    bench_parse_small,
    bench_parse_medium,
    bench_parse_large
);
criterion_main!(benches);
