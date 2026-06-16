use criterion::{black_box, criterion_group, criterion_main, Criterion};
use pct::{decode, decode_passthrough, decode_strict};

/// Benchmark the zero-allocation fast path: an input with no `%`
/// characters should be returned as `Cow::Borrowed` without any
/// allocation. This is where the `simd` feature shines — `find_first_byte`
/// scans 32 bytes per cycle looking for `%`.
fn bench_decode_noop_long(c: &mut Criterion) {
    // 256 bytes with no '%' — exercises the SIMD fast path.
    let input = "the quick brown fox jumps over the lazy dog \
                 the quick brown fox jumps over the lazy dog \
                 the quick brown fox jumps over the lazy dog \
                 the quick brown fox jumps over the lazy dog";

    let mut group = c.benchmark_group("decode_noop_long_fast_path");
    group.bench_function("pct::decode (no-op)", |b| {
        b.iter(|| decode(black_box(input)))
    });
    group.bench_function("percent-encoding", |b| {
        use percent_encoding::percent_decode_str;
        b.iter(|| percent_decode_str(black_box(input)).decode_utf8_lossy())
    });
    group.finish();
}

fn bench_decode_short(c: &mut Criterion) {
    let input = "hello%20world";

    let mut group = c.benchmark_group("decode_short");
    group.bench_function("pct::decode", |b| b.iter(|| decode(black_box(input))));
    group.bench_function("percent-encoding", |b| {
        use percent_encoding::percent_decode_str;
        b.iter(|| percent_decode_str(black_box(input)).decode_utf8_lossy())
    });
    group.bench_function("urlencoding", |b| {
        b.iter(|| urlencoding::decode(black_box(input)))
    });
    group.finish();
}

fn bench_decode_long(c: &mut Criterion) {
    let input =
        "the%20quick%20brown%20fox%20jumps%20over%20the%20lazy%20dog%20%2Fpath%3Fquery%3Dvalue";

    let mut group = c.benchmark_group("decode_long");
    group.bench_function("pct::decode", |b| b.iter(|| decode(black_box(input))));
    group.bench_function("percent-encoding", |b| {
        use percent_encoding::percent_decode_str;
        b.iter(|| percent_decode_str(black_box(input)).decode_utf8_lossy())
    });
    group.bench_function("urlencoding", |b| {
        b.iter(|| urlencoding::decode(black_box(input)))
    });
    group.finish();
}

fn bench_decode_modes(c: &mut Criterion) {
    let input = "hello%20world%21";

    let mut group = c.benchmark_group("decode_modes");
    group.bench_function("lossy", |b| b.iter(|| decode(black_box(input))));
    group.bench_function("strict", |b| b.iter(|| decode_strict(black_box(input))));
    group.bench_function("passthrough", |b| {
        b.iter(|| decode_passthrough(black_box(input)))
    });
    group.finish();
}

fn bench_decode_noop(c: &mut Criterion) {
    let input = "helloworld";

    let mut group = c.benchmark_group("decode_noop");
    group.bench_function("pct::decode", |b| b.iter(|| decode(black_box(input))));
    group.bench_function("percent-encoding", |b| {
        use percent_encoding::percent_decode_str;
        b.iter(|| percent_decode_str(black_box(input)).decode_utf8_lossy())
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_decode_noop_long,
    bench_decode_short,
    bench_decode_long,
    bench_decode_modes,
    bench_decode_noop,
);
criterion_main!(benches);
