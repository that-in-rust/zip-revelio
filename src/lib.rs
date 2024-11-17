//! ZIP-Revelio: High-performance, memory-safe ZIP file analysis
//! 
//! This library provides async and parallel processing capabilities for analyzing
//! ZIP files efficiently while maintaining memory safety and providing rich error
//! context.

use std::path::PathBuf;

pub mod cli;
pub mod reader;
pub mod processor;
pub mod stats;
pub mod reporter;
pub mod error;
pub mod buffer;
pub mod async_runtime;
pub mod thread_pool;
pub mod sync;
pub mod scheduler;
pub mod memory;
pub mod thread_safety;
pub mod error_handling;

pub use error::ZipError;

/// Result type for ZIP-Revelio operations
pub type Result<T> = std::result::Result<T, ZipError>;

/// Configuration for ZIP processing
#[derive(Debug, Clone)]
pub struct Config {
    /// Input ZIP file path
    pub input_path: PathBuf,
    /// Output report file path
    pub output_path: PathBuf,
    /// Number of threads for parallel processing
    pub thread_count: usize,
    /// Buffer size for reading
    pub buffer_size: usize,
    /// Show progress bar
    pub show_progress: bool,
    /// Verbose output
    pub verbose: bool,
    /// Maximum memory usage in bytes
    pub max_memory: usize,
    /// Compression methods to analyze
    pub methods: Option<Vec<String>>,
    /// Generate detailed report
    pub detailed: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            input_path: PathBuf::new(),
            output_path: PathBuf::new(),
            thread_count: num_cpus::get(),
            buffer_size: 64 * 1024,
            show_progress: true,
            verbose: false,
            max_memory: 1024 * 1024 * 1024,  // 1GB default
            methods: None,
            detailed: false,
        }
    }
}

/// Compression methods supported by ZIP-Revelio
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompressionMethod {
    Store = 0,
    Deflate = 8,
}

impl TryFrom<u16> for CompressionMethod {
    type Error = ZipError;

    fn try_from(value: u16) -> Result<Self> {
        match value {
            0 => Ok(CompressionMethod::Store),
            8 => Ok(CompressionMethod::Deflate),
            _ => Err(ZipError::UnsupportedMethod(value)),
        }
    }
}

/// Entry in a ZIP file
#[derive(Debug, Clone)]
pub struct ZipEntry {
    /// Entry name/path
    pub name: String,
    /// Uncompressed size
    pub size: u64,
    /// Compressed size
    pub compressed_size: u64,
    /// Compression method
    pub method: CompressionMethod,
    /// CRC32 checksum
    pub crc32: u32,
    /// Offset to local header
    pub header_offset: u64,
}

/// Constants used throughout the library
pub mod constants {
    /// ZIP end of central directory signature
    pub const END_OF_CENTRAL_DIR_SIGNATURE: u32 = 0x06054b50;
    /// ZIP central directory entry signature
    pub const CENTRAL_DIR_ENTRY_SIGNATURE: u32 = 0x02014b50;
    /// ZIP local file header signature
    pub const LOCAL_FILE_HEADER_SIGNATURE: u32 = 0x04034b50;
    /// Maximum size for reading buffers
    pub const MAX_BUFFER_SIZE: usize = 10 * 1024 * 1024; // 10MB
    /// Default buffer size
    pub const DEFAULT_BUFFER_SIZE: usize = 64 * 1024; // 64KB
    /// Maximum comment length
    pub const MAX_COMMENT_LENGTH: usize = 65535;
}
