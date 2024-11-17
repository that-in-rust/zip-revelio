use std::error::Error;
use std::fmt;
use thiserror::Error;

/// Custom error type for ZIP-Revelio operations
#[derive(Error, Debug, Clone)]
pub enum ZipError {
    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Memory error
    #[error("Memory error: {0}")]
    Memory(String),

    /// Memory limit exceeded
    #[error("Memory limit exceeded: {0}")]
    MemoryLimit(String),

    /// Resource limit exceeded
    #[error("Resource limit exceeded: {0}")]
    ResourceLimit(String),

    /// Thread safety error
    #[error("Thread safety error: {0}")]
    ThreadSafety(String),

    /// Channel error
    #[error("Channel error: {0}")]
    Channel(String),

    /// Task channel error
    #[error("Task channel error: {0}")]
    TaskChannel(String),

    /// Semaphore error
    #[error("Semaphore error: {0}")]
    Semaphore(String),

    /// End of central directory not found
    #[error("End of central directory not found")]
    EndOfCentralDirectoryNotFound,

    /// Invalid ZIP format
    #[error("Invalid ZIP format: {0}")]
    InvalidFormat(String),

    /// Invalid signature
    #[error("Invalid signature: {0:#x}")]
    InvalidSignature(u32),

    /// Unsupported compression method
    #[error("Unsupported compression method: {0}")]
    UnsupportedMethod(u16),

    /// Processing error
    #[error("Processing error: {0}")]
    Processing(String),

    /// Thread pool error
    #[error("Thread pool error: {0}")]
    ThreadPool(String),

    /// Compression error
    #[error("Compression error: {0}")]
    Compression(String),

    /// CRC error
    #[error("CRC error: {0}")]
    Crc(String),

    /// Other error
    #[error("Other error: {0}")]
    Other(String),
}
