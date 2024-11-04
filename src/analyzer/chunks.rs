use std::{
    path::PathBuf,
    sync::{Arc, atomic::{AtomicUsize, AtomicU64, Ordering}},
    io::Cursor,
};
use tokio::sync::Semaphore;
use crate::{
    error::{Result, AnalysisError},
    models::{FileInfo, ZipAnalysis, AnalysisStats, CompressionMethod},
};

pub struct ChunkConfig {
    pub chunk_size: usize,
    pub buffer_count: usize,
    pub buffer_size: usize,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            chunk_size: 16 * 1024 * 1024,  // 16MB chunks
            buffer_count: 4,
            buffer_size: 8 * 1024 * 1024,  // 8MB buffers
        }
    }
}

pub struct ChunkResult {
    pub offset: u64,
    pub files: Vec<FileInfo>,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub error: Option<crate::error::AnalysisError>,
}

#[derive(Debug)]
pub struct ChunkStats {
    pub processed_chunks: AtomicUsize,
    pub total_bytes: AtomicU64,
    pub active_threads: AtomicUsize,
}

impl ChunkStats {
    pub fn new() -> Self {
        Self {
            processed_chunks: AtomicUsize::new(0),
            total_bytes: AtomicU64::new(0),
            active_threads: AtomicUsize::new(0),
        }
    }

    pub fn clone(&self) -> Self {
        Self {
            processed_chunks: AtomicUsize::new(self.processed_chunks.load(Ordering::SeqCst)),
            total_bytes: AtomicU64::new(self.total_bytes.load(Ordering::SeqCst)),
            active_threads: AtomicUsize::new(self.active_threads.load(Ordering::SeqCst)),
        }
    }
}

pub struct BufferPool {
    buffers: Vec<Vec<u8>>,
    available: Arc<Semaphore>,
    buffer_size: usize,
}

impl BufferPool {
    pub fn new(config: &ChunkConfig) -> Self {
        let buffers = (0..config.buffer_count)
            .map(|_| vec![0; config.buffer_size])
            .collect();
        
        Self {
            buffers,
            available: Arc::new(Semaphore::new(config.buffer_count)),
            buffer_size: config.buffer_size,
        }
    }

    pub async fn acquire(&self) -> Result<Vec<u8>> {
        self.available.acquire().await.map_err(|e| {
            crate::error::AnalysisError::Progress { 
                msg: format!("Failed to acquire buffer: {}", e) 
            }
        })?;
        
        Ok(vec![0; self.buffer_size])
    }

    pub fn release(&self, _buffer: Vec<u8>) {
        self.available.add_permits(1);
    }
}

pub struct ChunkProcessor {
    config: ChunkConfig,
    buffer_pool: BufferPool,
    stats: Arc<ChunkStats>,
}

impl ChunkProcessor {
    pub fn new(config: ChunkConfig) -> Self {
        Self {
            buffer_pool: BufferPool::new(&config),
            stats: Arc::new(ChunkStats::new()),
            config,
        }
    }

    pub async fn process_chunk(&self, chunk: &[u8], offset: u64) -> Result<ChunkResult> {
        self.stats.active_threads.fetch_add(1, Ordering::SeqCst);
        let buffer = self.buffer_pool.acquire().await?;
        
        let result = rayon::scope(|s| -> Result<ChunkResult> {
            s.spawn(|_| -> Result<ChunkResult> {
                let mut cursor = Cursor::new(chunk);
                let mut zip = zip::ZipArchive::new(cursor)
                    .map_err(|e| AnalysisError::Zip { source: e })?;
                
                let mut files = Vec::new();
                let mut compressed_size = 0;
                let mut uncompressed_size = 0;

                for i in 0..zip.len() {
                    let file = zip.by_index(i)
                        .map_err(|e| AnalysisError::Zip { source: e })?;
                    
                    files.push(FileInfo {
                        path: PathBuf::from(file.name()),
                        size: file.size(),
                        compressed_size: file.compressed_size(),
                        compression_method: file.compression().into(),
                        crc32: file.crc32(),
                        modified: chrono::DateTime::from(file.last_modified().to_time().unwrap()).into(),
                    });

                    compressed_size += file.compressed_size();
                    uncompressed_size += file.size();
                }

                Ok(ChunkResult {
                    offset,
                    files,
                    compressed_size,
                    uncompressed_size,
                    error: None,
                })
            }).join().unwrap()
        });

        self.buffer_pool.release(buffer);
        self.stats.active_threads.fetch_sub(1, Ordering::SeqCst);
        self.stats.processed_chunks.fetch_add(1, Ordering::SeqCst);
        self.stats.total_bytes.fetch_add(chunk.len() as u64, Ordering::SeqCst);

        result
    }

    pub fn merge_results(results: &[ChunkResult]) -> Result<ZipAnalysis> {
        let mut all_files = Vec::new();
        let mut total_compressed = 0;
        let mut total_uncompressed = 0;
        let mut error_count = 0;

        for result in results {
            if let Some(error) = &result.error {
                error_count += 1;
                if !error.is_recoverable() {
                    return Err(error.clone());
                }
            }
            all_files.extend(result.files.clone());
            total_compressed += result.compressed_size;
            total_uncompressed += result.uncompressed_size;
        }

        let stats = AnalysisStats {
            duration_ms: 0,
            chunks_processed: results.len(),
            error_count,
            peak_memory_mb: 0,
        };

        Ok(ZipAnalysis::new(all_files, stats))
    }
}
