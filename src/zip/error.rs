use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ZipError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("Invalid ZIP format: {0}")]
    Format(String),

    #[error("Unsupported compression method: {0}")]
    UnsupportedMethod(u16),

    #[error("Non-ASCII filename")]
    NonAsciiName,

    #[error("File too large (>4GB)")]
    FileTooLarge,

    #[error("Memory error: {0}")]
    Memory(String),

    #[error("No buffer available")]
    NoBufferAvailable,

    #[error("CRC32 mismatch")]
    Crc32Mismatch,
}
