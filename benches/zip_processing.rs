use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::path::PathBuf;
use zip_revelio::{Analyzer, Config};

async fn process_zip(input: PathBuf, output: PathBuf) -> anyhow::Result<()> {
    let config = Config::default();
    let analyzer = Analyzer::new(config);
    analyzer.analyze(input, output).await
}

pub fn zip_processing_benchmark(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("process_1mb_zip", |b| {
        b.to_async(&runtime).iter(|| async {
            process_zip(
                black_box(PathBuf::from("test_data/1mb.zip")),
                black_box(PathBuf::from("test_data/report.txt")),
            )
            .await
        });
    });

    c.bench_function("process_10mb_zip", |b| {
        b.to_async(&runtime).iter(|| async {
            process_zip(
                black_box(PathBuf::from("test_data/10mb.zip")),
                black_box(PathBuf::from("test_data/report.txt")),
            )
            .await
        });
    });
}

criterion_group!(benches, zip_processing_benchmark);
criterion_main!(benches);
