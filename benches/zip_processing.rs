use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::sync::Arc;
use tokio::runtime::Runtime;
use zip_revelio::{
    reader::AsyncZipReader,
    processor::ParallelProcessor,
    stats::Stats,
};

fn benchmark_zip_processing(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("zip_processing");
    group.sample_size(10);
    group.measurement_time(std::time::Duration::from_secs(30));

    // Benchmark 1MB ZIP processing
    group.bench_function("process_1mb", |b| {
        b.iter(|| {
            rt.block_on(async {
                let stats = Arc::new(Stats::new());
                let reader = AsyncZipReader::new("test_data/1mb.zip").await.unwrap();
                let processor = ParallelProcessor::new(num_cpus::get(), Arc::clone(&stats)).unwrap();
                
                black_box(process_zip(reader, processor).await.unwrap());
            });
        });
    });

    // Benchmark 10MB ZIP processing
    group.bench_function("process_10mb", |b| {
        b.iter(|| {
            rt.block_on(async {
                let stats = Arc::new(Stats::new());
                let reader = AsyncZipReader::new("test_data/10mb.zip").await.unwrap();
                let processor = ParallelProcessor::new(num_cpus::get(), Arc::clone(&stats)).unwrap();
                
                black_box(process_zip(reader, processor).await.unwrap());
            });
        });
    });

    group.finish();
}

async fn process_zip(mut reader: AsyncZipReader, processor: ParallelProcessor) -> zip_revelio::Result<()> {
    reader.seek_end_directory().await?;
    while let Some(entry) = reader.read_entry().await? {
        processor.process_entry(entry, Default::default()).await?;
    }
    Ok(())
}

criterion_group!(benches, benchmark_zip_processing);
criterion_main!(benches);
