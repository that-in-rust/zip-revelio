use std::sync::Arc;
use rayon::prelude::*;
use tokio::sync::mpsc;
use flate2::read::DeflateDecoder;
use std::io::Read;

use crate::{
    buffer::PooledBuffer,
    error::ZipError,
    stats::Stats,
    Result, ZipEntry, CompressionMethod,
    async_runtime::TaskMessage,
};

/// Parallel processor for ZIP entries
pub struct Processor {
    /// Thread pool for parallel processing
    pool: rayon::ThreadPool,
    /// Shared statistics
    stats: Arc<Stats>,
    /// Task message sender
    task_tx: mpsc::Sender<TaskMessage>,
    /// Maximum memory usage
    max_memory: usize,
}

impl Processor {
    /// Creates a new processor
    pub fn new(
        thread_count: usize,
        stats: Arc<Stats>,
        task_tx: mpsc::Sender<TaskMessage>,
        max_memory: usize,
    ) -> Result<Self> {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(thread_count)
            .thread_name(|i| format!("zip-revelio-worker-{}", i))
            .build()
            .map_err(|e| ZipError::ThreadPool(e.to_string()))?;

        Ok(Self {
            pool,
            stats,
            task_tx,
            max_memory,
        })
    }

    /// Processes ZIP entries in parallel
    pub async fn process_entries(&self, entries: Vec<ZipEntry>, data: Vec<PooledBuffer>) -> Result<()> {
        // Ensure we have matching entries and data
        if entries.len() != data.len() {
            return Err(ZipError::Processing("Mismatched entries and data".into()));
        }

        // Calculate batch size based on memory limit
        let batch_size = self.calculate_batch_size(&entries);
        let total_entries = entries.len();
        let mut processed = 0;

        // Process in batches
        for (batch_entries, batch_data) in entries.chunks(batch_size)
            .zip(data.chunks(batch_size))
        {
            // Process batch in parallel
            let results: Vec<Result<()>> = self.pool.install(|| {
                batch_entries.par_iter()
                    .zip(batch_data.par_iter())
                    .map(|(entry, data)| {
                        self.process_entry(entry, data)
                    })
                    .collect()
            });

            // Check for errors
            for result in results {
                if let Err(e) = result {
                    self.stats.record_error(e);
                }
            }

            // Update progress
            processed += batch_entries.len();
            self.task_tx.send(TaskMessage::Progress {
                current: processed,
                total: total_entries,
            }).await.map_err(|e| ZipError::TaskChannel(e.to_string()))?;
        }

        Ok(())
    }

    /// Processes a single ZIP entry
    fn process_entry(&self, entry: &ZipEntry, data: &PooledBuffer) -> Result<()> {
        // Update stats
        self.stats.increment_files();
        self.stats.add_size(entry.size);
        self.stats.add_compressed_size(entry.compressed_size);
        self.stats.record_method(entry.method);

        // Process based on compression method
        match entry.method {
            CompressionMethod::Store => {
                // Verify size
                if data.as_bytes().len() as u64 != entry.size {
                    return Err(ZipError::Processing(format!(
                        "Size mismatch for {}: expected {}, got {}",
                        entry.name, entry.size, data.as_bytes().len()
                    )));
                }

                // Verify CRC32
                let crc = crc32fast::hash(data.as_bytes());
                if crc != entry.crc32 {
                    return Err(ZipError::Processing(format!(
                        "CRC mismatch for {}: expected {:x}, got {:x}",
                        entry.name, entry.crc32, crc
                    )));
                }
            }
            CompressionMethod::Deflate => {
                // Create deflate decoder
                let mut decoder = DeflateDecoder::new(data.as_bytes());
                let mut decompressed = Vec::with_capacity(entry.size as usize);

                // Decompress data
                decoder.read_to_end(&mut decompressed).map_err(|e| {
                    ZipError::Processing(format!("Decompression failed for {}: {}", entry.name, e))
                })?;

                // Verify size
                if decompressed.len() as u64 != entry.size {
                    return Err(ZipError::Processing(format!(
                        "Decompressed size mismatch for {}: expected {}, got {}",
                        entry.name, entry.size, decompressed.len()
                    )));
                }

                // Verify CRC32
                let crc = crc32fast::hash(&decompressed);
                if crc != entry.crc32 {
                    return Err(ZipError::Processing(format!(
                        "CRC mismatch for {}: expected {:x}, got {:x}",
                        entry.name, entry.crc32, crc
                    )));
                }
            }
        }

        Ok(())
    }

    /// Calculates batch size based on memory limit
    fn calculate_batch_size(&self, entries: &[ZipEntry]) -> usize {
        if entries.is_empty() {
            return 0;
        }

        // Calculate average entry size
        let total_size: u64 = entries.iter()
            .map(|e| e.compressed_size)
            .sum();
        let avg_size = total_size / entries.len() as u64;

        // Calculate how many entries we can process at once
        let thread_count = self.pool.current_num_threads();
        let memory_per_thread = self.max_memory / thread_count;
        let entries_per_thread = memory_per_thread as u64 / avg_size;

        // Ensure at least one entry per thread
        entries_per_thread.max(1) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::runtime::Runtime;
    use bytes::BytesMut;
    use crate::buffer::BufferPool;

    fn create_test_entry(name: &str, size: u64, method: CompressionMethod) -> ZipEntry {
        ZipEntry {
            name: name.to_string(),
            size,
            compressed_size: size,
            method,
            crc32: 0,
            header_offset: 0,
        }
    }

    fn create_test_buffer(data: &[u8], pool: Arc<BufferPool>) -> PooledBuffer {
        let mut buffer = BytesMut::with_capacity(data.len());
        buffer.extend_from_slice(data);
        PooledBuffer::new(buffer, pool)
    }

    #[test]
    fn test_processor() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let stats = Arc::new(Stats::new());
            let (tx, _rx) = mpsc::channel(1024);
            let buffer_pool = Arc::new(BufferPool::new(1024 * 1024));

            let processor = Processor::new(2, Arc::clone(&stats), tx, 1024 * 1024).unwrap();

            let entries = vec![
                create_test_entry("test1.txt", 5, CompressionMethod::Store),
                create_test_entry("test2.txt", 5, CompressionMethod::Store),
            ];

            let data = vec![
                create_test_buffer(b"Hello", Arc::clone(&buffer_pool)),
                create_test_buffer(b"World", Arc::clone(&buffer_pool)),
            ];

            processor.process_entries(entries, data).await.unwrap();

            assert_eq!(stats.total_files(), 2);
            assert_eq!(stats.total_size(), 10);
        });
    }

    #[test]
    fn test_invalid_size() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let stats = Arc::new(Stats::new());
            let (tx, _rx) = mpsc::channel(1024);
            let buffer_pool = Arc::new(BufferPool::new(1024 * 1024));

            let processor = Processor::new(2, Arc::clone(&stats), tx, 1024 * 1024).unwrap();

            let entries = vec![
                create_test_entry("test.txt", 10, CompressionMethod::Store),
            ];

            let data = vec![
                create_test_buffer(b"Hello", Arc::clone(&buffer_pool)),
            ];

            assert!(processor.process_entries(entries, data).await.is_err());
        });
    }

    #[test]
    fn test_batch_size_calculation() {
        let stats = Arc::new(Stats::new());
        let (tx, _rx) = mpsc::channel(1024);
        
        let processor = Processor::new(
            2,
            Arc::clone(&stats),
            tx,
            1024 * 1024,  // 1MB max memory
        ).unwrap();

        let entries = vec![
            create_test_entry("test1.txt", 1024, CompressionMethod::Store),
            create_test_entry("test2.txt", 1024, CompressionMethod::Store),
        ];

        let batch_size = processor.calculate_batch_size(&entries);
        assert!(batch_size > 0);
    }
}
