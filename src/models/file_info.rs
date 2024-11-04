use std::path::PathBuf;
use chrono::{DateTime, Utc};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use crate::error::AnalysisError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    pub path: PathBuf,
    pub size: u64,
    pub compressed_size: u64,
    pub compression_method: CompressionMethod,
    pub crc32: u32,
    pub modified: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionMethod {
    Stored,
    Deflated,
    Other(u16),
}

#[derive(Debug)]
pub struct ProcessingStats {
    pub bytes_processed: AtomicU64,
    pub files_processed: AtomicUsize,
    pub errors_encountered: AtomicUsize,
}

impl ProcessingStats {
    pub fn new() -> Self {
        Self {
            bytes_processed: AtomicU64::new(0),
            files_processed: AtomicUsize::new(0),
            errors_encountered: AtomicUsize::new(0),
        }
    }

    pub fn increment_bytes(&self, bytes: u64) {
        self.bytes_processed.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn increment_files(&self) {
        self.files_processed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_errors(&self) {
        self.errors_encountered.fetch_add(1, Ordering::Relaxed);
    }
}

impl From<zip::CompressionMethod> for CompressionMethod {
    fn from(method: zip::CompressionMethod) -> Self {
        match method {
            zip::CompressionMethod::Stored => Self::Stored,
            zip::CompressionMethod::Deflated => Self::Deflated,
            other => Self::Other(other.to_u16()),
        }
    }
}

impl std::fmt::Display for CompressionMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stored => write!(f, "Stored"),
            Self::Deflated => write!(f, "Deflated"),
            Self::Other(n) => write!(f, "Method({})", n)
        }
    }
}

impl TryFrom<u16> for CompressionMethod {
    type Error = AnalysisError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Stored),
            8 => Ok(Self::Deflated),
            other => Ok(Self::Other(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_file_info_creation() {
        let info = FileInfo {
            path: Path::new("test.txt").to_path_buf(),
            size: 100,
            compressed_size: 50,
            compression_method: CompressionMethod::Deflated,
            crc32: 12345,
            modified: Utc::now(),
        };
        assert_eq!(info.compression_method, CompressionMethod::Deflated);
        assert_eq!(info.size, 100);
        assert_eq!(info.compressed_size, 50);
    }

    #[test]
    fn test_processing_stats_thread_safety() {
        let stats = ProcessingStats::new();
        stats.increment_bytes(100);
        stats.increment_files();
        assert_eq!(stats.bytes_processed.load(Ordering::Relaxed), 100);
        assert_eq!(stats.files_processed.load(Ordering::Relaxed), 1);
    }
}
