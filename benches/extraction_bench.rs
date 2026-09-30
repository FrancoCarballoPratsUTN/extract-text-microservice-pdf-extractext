use criterion::{Criterion, black_box, criterion_group, criterion_main};
use extract::domain::{
    extract_document, extract_document_lean, parse_pdf, test_support,
    test_support::{DENSE_PDF_PATH, SAMPLE_PDF_PATH},
};
use mimalloc::MiMalloc;
use rayon::ThreadPoolBuilder;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

const PAGE_COUNT: usize = 200;
const LIMIT: usize = 64 * 1024 * 1024;

fn load_payload(path: &str) -> Vec<u8> {
    std::fs::read(path).expect("fixture file should exist")
}

fn shared_pool() -> rayon::ThreadPool {
    ThreadPoolBuilder::new()
        .num_threads(
            std::thread::available_parallelism()
                .map(|value| value.get())
                .unwrap_or(1),
        )
        .build()
        .unwrap()
}

fn bench_synthetic(c: &mut Criterion) {
    let parsed = parse_pdf(test_support::valid_pdf(PAGE_COUNT).as_bytes()).expect("fixture parses");
    let pool = shared_pool();

    let mut group = c.benchmark_group("synthetic_200p");
    group.sample_size(10);
    group.bench_function("parallel", |b| {
        b.iter(|| black_box(extract_document(&parsed, LIMIT, &pool)));
    });
    group.bench_function("lean", |b| {
        b.iter(|| black_box(extract_document_lean(&parsed, LIMIT, &pool)));
    });
    group.finish();
}

fn bench_real_sample(c: &mut Criterion) {
    let parsed = parse_pdf(&load_payload(SAMPLE_PDF_PATH)).expect("sample parses");
    let pool = shared_pool();

    let mut group = c.benchmark_group("real_sample_250p_5mb");
    group.sample_size(10);
    group.measurement_time(std::time::Duration::from_secs(10));
    group.bench_function("parallel", |b| {
        b.iter(|| black_box(extract_document(&parsed, LIMIT, &pool)));
    });
    group.bench_function("lean", |b| {
        b.iter(|| black_box(extract_document_lean(&parsed, LIMIT, &pool)));
    });
    group.finish();
}

fn bench_dense_fixture(c: &mut Criterion) {
    let parsed = parse_pdf(&load_payload(DENSE_PDF_PATH)).expect("dense fixture parses");
    let pool = shared_pool();

    let mut group = c.benchmark_group("dense_fixture_500p_12mb");
    group.sample_size(10);
    group.measurement_time(std::time::Duration::from_secs(10));
    group.bench_function("parallel", |b| {
        b.iter(|| black_box(extract_document(&parsed, LIMIT, &pool)));
    });
    group.bench_function("lean", |b| {
        b.iter(|| black_box(extract_document_lean(&parsed, LIMIT, &pool)));
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_synthetic,
    bench_real_sample,
    bench_dense_fixture
);
criterion_main!(benches);
