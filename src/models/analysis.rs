use super::FileInfo;

#[derive(Debug)]
pub struct ZipAnalysis {
    pub files: Vec<FileInfo>,
    pub total_size: u64,
    pub compressed_size: u64,
    pub compression_ratio: f64,
    pub stats: AnalysisStats,
}

#[derive(Debug, Default)]
pub struct AnalysisStats {
    pub duration_ms: u64,
    pub chunks_processed: usize,
    pub error_count: usize,
    pub peak_memory_mb: usize,
}

impl ZipAnalysis {
    pub fn new(files: Vec<FileInfo>, stats: AnalysisStats) -> Self {
        let total_size: u64 = files.iter().map(|f| f.size).sum();
        let compressed_size: u64 = files.iter().map(|f| f.compressed_size).sum();
        let compression_ratio = if total_size > 0 {
            compressed_size as f64 / total_size as f64
        } else {
            1.0
        };

        Self {
            files,
            total_size,
            compressed_size,
            compression_ratio,
            stats,
        }
    }
}
