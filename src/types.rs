pub const ZIP_LOCAL_HEADER_SIGNATURE: u32 = 0x04034b50;
pub const ZIP_CENTRAL_DIR_SIGNATURE: u32 = 0x02014b50;
pub const MAX_FILE_SIZE: u64 = 0xFFFFFFFF;  // 4GB limit
pub const STORED: u16 = 0;
pub const DEFLATED: u16 = 8;

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use parking_lot::RwLock;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct ZipHeader {
    pub compression_method: u16,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub file_name: String,
    pub is_encrypted: bool,
    pub crc32: u32,
}

impl ZipHeader {
    pub fn new(
        compression_method: u16,
        compressed_size: u64,
        uncompressed_size: u64,
        file_name: String,
        is_encrypted: bool,
        crc32: u32,
    ) -> Self {
        Self {
            compression_method,
            compressed_size,
            uncompressed_size,
            file_name,
            is_encrypted,
            crc32,
        }
    }
}

#[derive(Debug, Default)]
pub struct ZipAnalysis {
    total_size: AtomicUsize,
    compression_ratio: RwLock<f64>,
    file_types: RwLock<HashMap<String, usize>>,
    compression_methods: HashMap<u16, usize>,
    total_compressed: u64,
    total_uncompressed: u64,
    file_count: usize,
}

impl ZipAnalysis {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_size(&self, size: usize) {
        self.total_size.fetch_add(size, Ordering::Relaxed);
    }

    pub fn update_compression(&self, ratio: f64) {
        let mut current = self.compression_ratio.write();
        *current = (*current + ratio) / 2.0;
    }

    pub fn add_file_type(&self, extension: String) {
        let mut types = self.file_types.write();
        *types.entry(extension).or_insert(0) += 1;
    }

    pub fn update_compression_method(&mut self, method: u16) {
        *self.compression_methods.entry(method).or_insert(0) += 1;
    }

    pub fn update_sizes(&mut self, compressed: u64, uncompressed: u64) {
        self.total_compressed += compressed;
        self.total_uncompressed += uncompressed;
        self.file_count += 1;
    }

    pub fn total_size(&self) -> usize {
        self.total_size.load(Ordering::Relaxed)
    }

    pub fn get_compression_ratio(&self) -> f64 {
        if self.total_uncompressed == 0 {
            return 0.0;
        }
        1.0 - (self.total_compressed as f64 / self.total_uncompressed as f64)
    }

    pub fn get_compression_methods(&self) -> &HashMap<u16, usize> {
        &self.compression_methods
    }

    pub fn get_file_types(&self) -> Vec<(String, usize)> {
        let types = self.file_types.read();
        types.iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect()
    }

    pub fn file_count(&self) -> usize {
        self.file_count
    }

    pub fn total_compressed(&self) -> u64 {
        self.total_compressed
    }

    pub fn total_uncompressed(&self) -> u64 {
        self.total_uncompressed
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(String),
    
    #[error("ZIP error: {0}")]
    Zip(String),
    
    #[error("Processing error: {0}")]
    Processing(String),
}

pub type Result<T> = std::result::Result<T, Error>;

// Cache-aligned chunk
#[repr(C, align(64))]
pub struct Chunk {
    data: Vec<u8>,
    offset: u64,
    size: usize,
}

impl Chunk {
    pub fn new(data: Vec<u8>, offset: u64) -> Self {
        let size = data.len();
        Self { data, offset, size }
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn offset(&self) -> u64 {
        self.offset
    }

    pub fn size(&self) -> usize {
        self.size
    }
}

// Add From implementations for Error
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e.to_string())
    }
}

impl From<ctrlc::Error> for Error {
    fn from(e: ctrlc::Error) -> Self {
        Error::Processing(e.to_string())
    }
}
