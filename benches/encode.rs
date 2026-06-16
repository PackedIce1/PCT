use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use pct::{encode, encode_raw, encode_with, EncodeSet};

fn bench_encode_short(c: &mut Criterion) {
    let input = "hello world";

    let mut group = c.benchmark_group("encode_short");
    group.bench_function("pct::encode", |b| b.iter(|| encode(black_box(input))));
    group.bench_function("percent-encoding", |b| {
        use percent_encoding::utf8_percent_encode;
        use percent_encoding::NON_ALPHANUMERIC;
        b.iter(|| utf8_percent_encode(black_box(input), &NON_ALPHANUMERIC))
    });
    group.bench_function("urlencoding", |b| {
        b.iter(|| urlencoding::encode(black_box(input)))
    });
    group.finish();
}

fn bench_encode_long(c: &mut Criterion) {
    let input = "the quick brown fox jumps over the lazy dog /path?query=value&other=thing#frag";

    let mut group = c.benchmark_group("encode_long");
    group.bench_function("pct::encode", |b| b.iter(|| encode(black_box(input))));
    group.bench_function("percent-encoding", |b| {
        use percent_encoding::utf8_percent_encode;
        use percent_encoding::NON_ALPHANUMERIC;
        b.iter(|| utf8_percent_encode(black_box(input), &NON_ALPHANUMERIC))
    });
    group.bench_function("urlencoding", |b| {
        b.iter(|| urlencoding::encode(black_box(input)))
    });
    group.finish();
}

fn bench_encode_special_chars(c: &mut Criterion) {
    let input = "100% off! sale+deal&ref=abc#top";

    let mut group = c.benchmark_group("encode_special_chars");
    group.bench_function("pct::encode", |b| b.iter(|| encode(black_box(input))));
    group.bench_function("percent-encoding", |b| {
        use percent_encoding::utf8_percent_encode;
        use percent_encoding::NON_ALPHANUMERIC;
        b.iter(|| utf8_percent_encode(black_box(input), &NON_ALPHANUMERIC))
    });
    group.bench_function("urlencoding", |b| {
        b.iter(|| urlencoding::encode(black_box(input)))
    });
    group.finish();
}

fn bench_encode_already_encoded(c: &mut Criterion) {
    let input = "hello%20world%21";

    let mut group = c.benchmark_group("encode_already_encoded");
    group.bench_function("pct::encode (idempotent)", |b| {
        b.iter(|| encode(black_box(input)))
    });
    group.bench_function("pct::encode_raw", |b| {
        b.iter(|| encode_raw(black_box(input), &EncodeSet::COMPONENT))
    });
    group.bench_function("urlencoding", |b| {
        b.iter(|| urlencoding::encode(black_box(input)))
    });
    group.finish();
}

fn bench_encode_context_sets(c: &mut Criterion) {
    let input = "a/b c?d=e&f#g";

    let mut group = c.benchmark_group("encode_context_sets");
    group.bench_function("COMPONENT", |b| {
        b.iter(|| encode_with(black_box(input), &EncodeSet::COMPONENT))
    });
    group.bench_function("PATH", |b| {
        b.iter(|| encode_with(black_box(input), &EncodeSet::PATH))
    });
    group.bench_function("QUERY", |b| {
        b.iter(|| encode_with(black_box(input), &EncodeSet::QUERY))
    });
    group.bench_function("FRAGMENT", |b| {
        b.iter(|| encode_with(black_box(input), &EncodeSet::FRAGMENT))
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_encode_short,
    bench_encode_long,
    bench_encode_special_chars,
    bench_encode_already_encoded,
    bench_encode_context_sets,
);
criterion_main!(benches);
