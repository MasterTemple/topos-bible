//! `cargo bench -p topos-lib`

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use topos_bible::{
    matcher::BibleMatcher,
    segments::{Segments, autocomplete::CompleteOptions},
};

/// Prose with references, including near misses like `is` and `Mar`
const PARAGRAPH: &str = "In the beginning (Gen 1:1-3), God made all things. Paul writes in \
    Romans 8:28-30; 9:1 that all things work together, and John 1:1, 3 John 5 agrees. This is \
    1 of the reasons, as Isa 53 shows. On Mar 25, 2025 we read Matth. x, 8 and Ps 119:105, \
    then Jude 5ff and Ephe-\nsians 2:8-10 before lunch.\n";

fn corpus(bytes: usize) -> String {
    PARAGRAPH.repeat(bytes / PARAGRAPH.len() + 1)
}

fn search(c: &mut Criterion) {
    let matcher = BibleMatcher::default();
    let mut group = c.benchmark_group("search");
    for size in [10_000, 1_000_000] {
        let text = corpus(size);
        group.throughput(Throughput::Bytes(text.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &text, |b, text| {
            b.iter(|| matcher.search(black_box(text)))
        });
    }
    group.finish();
}

fn parse(c: &mut Criterion) {
    c.bench_function("parse segments", |b| {
        b.iter(|| Segments::parse(black_box("5:1-3,5,7-9,12-6:6; 7:7-8:8")))
    });
}

fn complete(c: &mut Criterion) {
    let matcher = BibleMatcher::default();
    let options = CompleteOptions::default();
    c.bench_function("complete", |b| {
        b.iter(|| matcher.complete(black_box("see Gen 1:1-"), 12, &options))
    });
}

criterion_group!(benches, search, parse, complete);
criterion_main!(benches);
