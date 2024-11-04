use std::time::Duration;
use std::sync::atomic::{AtomicUsize, Ordering};
use super::FileInfo;

#[derive(Debug)]
pub struct ZipAnalysis {
    pub files: Vec<FileInfo>,
    pub total_size: u64,
    pub compressed_size: u64,
    pub compression_ratio: f64,
    pub stats: AnalysisStats,
}

#[derive(Debug, Clone)]
pub struct AnalysisStats {
    pub duration: Duration,
    pub chunks_processed: AtomicUsize,
    pub error_count: AtomicUsize,
    pub peak_memory_mb: AtomicUsize,
}

impl AnalysisStats {
    pub fn new() -> Self {
        Self {
            duration: Duration::default(),
            chunks_processed: AtomicUsize::new(0),
            error_count: AtomicUsize::new(0),
            peak_memory_mb: AtomicUsize::new(0),
        }
    }
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

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn total_savings(&self) -> u64 {
        self.total_size.saturating_sub(self.compressed_size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CompressionMethod;
    use std::path::PathBuf;
    use chrono::Utc;

    #[test]
    fn test_analysis_calculations() {
        let files = vec![
            FileInfo {
                path: PathBuf::from("test1.txt"),
                size: 1000,
                compressed_size: 500,
                compression_method: CompressionMethod::Deflated,
                crc32: 0,
                modified: Utc::now(),
            },
            FileInfo {
                path: PathBuf::from("test2.txt"),
                size: 2000,
                compressed_size: 1000,
                compression_method: CompressionMethod::Deflated,
                crc32: 0,
                modified: Utc::now(),
            },
        ];

        let analysis = ZipAnalysis::new(files, AnalysisStats::new());

        assert_eq!(analysis.total_size, 3000);
        assert_eq!(analysis.compressed_size, 1500);
        assert_eq!(analysis.compression_ratio, 0.5);
        assert_eq!(analysis.file_count(), 2);
        assert_eq!(analysis.total_savings(), 1500);
    }
}
