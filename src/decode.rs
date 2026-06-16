use criterion::{black_box, criterion_group, criterion_main, Criterion};
use pct::{decode_form, encode_form};

fn bench_encode_form(c: &mut Criterion) {
    let input = "name=hello world&value=a+b";

    let mut group = c.benchmark_group("form_encode");
    group.bench_function("pct::encode_form", |b| {
        b.iter(|| encode_form(black_box(input)))
    });
    group.bench_function("urlencoding", |b| {
        b.iter(|| urlencoding::encode(black_box(input)))
    });
    group.finish();
}

fn bench_decode_form(c: &mut Criterion) {
    let input = "name=hello+world&value=a%2Bb";

    let mut group = c.benchmark_group("form_decode");
    group.bench_function("pct::decode_form", |b| {
        b.iter(|| decode_form(black_box(input)))
    });
    group.bench_function("urlencoding", |b| {
        b.iter(|| urlencoding::decode(black_box(input)))
    });
    group.finish();
}

criterion_group!(benches, bench_encode_form, bench_decode_form,);
criterion_main!(benches);
