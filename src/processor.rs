use rayon;

use crate::types::{Chunk, ZipAnalysis, Error, Result, ZipHeader};

/// Processor for parallel ZIP chunk analysis
pub struct Processor {
    /// Thread pool configuration:
    /// - 8MB stack size
    /// - Number of CPU cores
    /// - Optimized for parallel processing
    thread_pool: rayon::ThreadPool,
}

impl Processor {
    /// Create a new processor with optimized thread pool
    pub fn new() -> Result<Self> {
        let thread_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_cpus::get())
            .stack_size(8 * 1024 * 1024)
            .build()
            .map_err(|e| Error::Processing(e.to_string()))?;
            
        Ok(Self { thread_pool })
    }
    
    /// Process a chunk of ZIP data
    /// 
    /// # Arguments
    /// * `chunk` - ZIP data chunk to process
    /// * `results` - Mutable reference to analysis results
    pub fn process_chunk(&self, chunk: Chunk, results: &mut ZipAnalysis) -> Result<()> {
        self.thread_pool.install(|| {
            // Basic validation
            if chunk.data().len() < 30 {
                return Err(Error::Zip("Invalid chunk size".into()));
            }
            
            // Check ZIP signature
            if &chunk.data()[0..4] != b"PK\x03\x04" {
                return Err(Error::Zip("Invalid ZIP signature".into()));
            }

            // Parse header
            if let Some(header) = self.parse_zip_header(chunk.data()) {
                // Update stats
                results.add_size(chunk.size());
                results.update_compression_method(header.compression_method);
                results.update_sizes(header.compressed_size, header.uncompressed_size);
                
                // Calculate basic compression ratio
                let ratio = if header.uncompressed_size > 0 {
                    1.0 - (header.compressed_size as f64 / header.uncompressed_size as f64)
                } else {
                    0.0
                };
                results.update_compression(ratio);
                
                // Add file type
                results.add_file_type("ZIP".into());
            }

            Ok(())
        })
    }

    fn parse_zip_header(&self, data: &[u8]) -> Option<ZipHeader> {
        if data.len() < 30 || &data[0..4] != b"PK\x03\x04" {
            return None;
        }

        // Parse ZIP local file header
        let compression_method = u16::from_le_bytes([data[8], data[9]]);
        let crc32 = u32::from_le_bytes([data[14], data[15], data[16], data[17]]);
        let compressed_size = u32::from_le_bytes([data[18], data[19], data[20], data[21]]) as u64;
        let uncompressed_size = u32::from_le_bytes([data[22], data[23], data[24], data[25]]) as u64;
        let name_length = u16::from_le_bytes([data[26], data[27]]) as usize;
        let extra_length = u16::from_le_bytes([data[28], data[29]]) as usize;

        if data.len() < 30 + name_length + extra_length {
            return None;
        }

        // Extract and validate filename
        let file_name = String::from_utf8_lossy(&data[30..30 + name_length]).to_string();
        if !file_name.is_ascii() {
            return None;
        }

        // Check encryption flag
        let is_encrypted = (data[6] & 0x1) == 0x1;

        Some(ZipHeader::new(
            compression_method,
            compressed_size,
            uncompressed_size,
            file_name,
            is_encrypted,
            crc32
        ))
    }

    /// Create a new processor with custom thread count
    pub fn new_with_threads(threads: Option<usize>) -> Result<Self> {
        let mut builder = rayon::ThreadPoolBuilder::new()
            .stack_size(8 * 1024 * 1024);
        
        if let Some(count) = threads {
            builder = builder.num_threads(count);
        }
        
        let thread_pool = builder.build()
            .map_err(|e| Error::Processing(e.to_string()))?;
        
        Ok(Self { thread_pool })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_processor_creation() {
        let processor = Processor::new().unwrap();
        assert_eq!(processor.thread_pool.num_threads(), num_cpus::get());
    }

    #[test]
    fn test_chunk_processing() {
        let processor = Processor::new().unwrap();
        let mut results = ZipAnalysis::new();
        
        let data = vec![
            b'P', b'K', 0x03, 0x04,  // Signature
            0x14, 0x00,              // Version
            0x00, 0x00,              // Flags
            0x00, 0x00,              // Compression (Store)
            0x00, 0x00, 0x00, 0x00,  // Mod time/date
            0x00, 0x00, 0x00, 0x00,  // CRC32
            0x04, 0x00, 0x00, 0x00,  // Compressed size
            0x04, 0x00, 0x00, 0x00,  // Uncompressed size
            0x04, 0x00,              // Filename length
            0x00, 0x00,              // Extra field length
            b't', b'e', b's', b't',  // Filename
            b't', b'e', b's', b't',  // File data
        ];
        
        let chunk = Chunk::new(data, 0);
        processor.process_chunk(chunk, &mut results).unwrap();
        
        assert!(results.total_size() > 0);
    }
}
