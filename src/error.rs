use std::io;
use std::sync::Arc;
use thiserror::Error;

/// Custom error type for ZIP-Revelio operations
#[derive(Error, Debug, Clone)]
pub enum ZipError {
    #[error("IO error at offset {offset}: {source}")]
    Io {
        #[source]
        source: Arc<io::Error>,
        offset: u64,
    },

    #[error("Invalid ZIP signature: {0:#x}")]
    InvalidSignature(u32),

    #[error("Buffer overflow: size {size} exceeds maximum {max}")]
    BufferOverflow {
        size: usize,
        max: usize,
    },

    #[error("Unsupported compression method: {0}")]
    UnsupportedMethod(u16),

    #[error("Invalid ZIP structure: {0}")]
    InvalidStructure(String),

    #[error("CRC32 mismatch: expected {expected:#x}, got {actual:#x}")]
    Crc32Mismatch {
        expected: u32,
        actual: u32,
    },

    #[error("Memory error: {0}")]
    Memory(String),

    #[error("Thread pool error: {0}")]
    ThreadPool(String),

    #[error("Processing error: {0}")]
    Processing(String),
}

impl From<io::Error> for ZipError {
    fn from(error: io::Error) -> Self {
        ZipError::Io {
            source: Arc::new(error),
            offset: 0,
        }
    }
}

/// Helper function to create an IO error with offset
pub fn io_error_with_offset(error: io::Error, offset: u64) -> ZipError {
    ZipError::Io {
        source: Arc::new(error),
        offset,
    }
}
