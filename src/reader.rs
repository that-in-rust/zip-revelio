use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    sync::Arc,
};
use bytes::{Buf, BytesMut};
use tokio::sync::mpsc;

use crate::{
    buffer::{BufferPool, PooledBuffer},
    error::ZipError,
    Result, ZipEntry, CompressionMethod,
    constants::*,
    async_runtime::TaskMessage,
};

/// ZIP file reader with async support
pub struct ZipReader {
    /// File handle
    file: File,
    /// Current position in file
    position: u64,
    /// Buffer pool for efficient memory use
    buffer_pool: Arc<BufferPool>,
    /// Task message sender
    task_tx: mpsc::Sender<TaskMessage>,
}

impl ZipReader {
    /// Creates a new ZIP reader
    pub fn new(file: File, buffer_pool: Arc<BufferPool>, task_tx: mpsc::Sender<TaskMessage>) -> Self {
        Self {
            file,
            position: 0,
            buffer_pool,
            task_tx,
        }
    }

    /// Reads ZIP file entries
    pub async fn read_entries(&mut self) -> Result<Vec<ZipEntry>> {
        // Find end of central directory
        let eocd_pos = self.find_end_of_central_directory()?;
        self.position = eocd_pos;

        // Read central directory
        let cd_offset = self.read_central_directory_offset()?;
        self.position = cd_offset;

        // Read entries
        let mut entries = Vec::new();
        loop {
            match self.read_central_directory_entry()? {
                Some(entry) => entries.push(entry),
                None => break,
            }
        }

        // Update progress
        self.task_tx.send(TaskMessage::Progress {
            current: entries.len(),
            total: entries.len(),
        }).await.map_err(|e| ZipError::TaskChannel(e.to_string()))?;

        Ok(entries)
    }

    /// Reads entry data
    pub async fn read_entry_data(&mut self, entry: &ZipEntry) -> Result<PooledBuffer> {
        // Seek to local header
        self.position = entry.header_offset;
        self.file.seek(SeekFrom::Start(entry.header_offset))?;

        // Verify local header
        let mut signature = [0u8; 4];
        self.file.read_exact(&mut signature)?;
        let sig = u32::from_le_bytes(signature);
        if sig != LOCAL_FILE_HEADER_SIGNATURE {
            return Err(ZipError::InvalidSignature(sig));
        }

        // Skip local header
        self.file.seek(SeekFrom::Current(26))?;  // Skip to filename length
        let name_length = self.read_u16()?;
        let extra_length = self.read_u16()?;
        self.file.seek(SeekFrom::Current((name_length + extra_length) as i64))?;

        // Read compressed data
        let mut buffer = self.buffer_pool.get_buffer(entry.compressed_size as usize)?;
        self.file.read_exact(&mut buffer[..entry.compressed_size as usize])?;

        Ok(PooledBuffer::new(buffer, Arc::clone(&self.buffer_pool)))
    }

    /// Finds end of central directory
    fn find_end_of_central_directory(&mut self) -> Result<u64> {
        let file_size = self.file.seek(SeekFrom::End(0))?;
        let mut buffer = [0u8; 22];  // Minimum EOCD size
        
        // Search backwards for EOCD signature
        for i in 0..MAX_COMMENT_LENGTH {
            let pos = file_size - 22 - i as u64;
            if pos < 0 {
                break;
            }
            
            self.file.seek(SeekFrom::Start(pos))?;
            self.file.read_exact(&mut buffer)?;
            
            let signature = u32::from_le_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]);
            if signature == END_OF_CENTRAL_DIR_SIGNATURE {
                return Ok(pos);
            }
        }
        
        Err(ZipError::EndOfCentralDirectoryNotFound)
    }

    /// Reads central directory offset
    fn read_central_directory_offset(&mut self) -> Result<u64> {
        self.file.seek(SeekFrom::Start(self.position + 16))?;
        let mut buffer = [0u8; 4];
        self.file.read_exact(&mut buffer)?;
        Ok(u32::from_le_bytes(buffer) as u64)
    }

    /// Reads a central directory entry
    fn read_central_directory_entry(&mut self) -> Result<Option<ZipEntry>> {
        // Read signature
        let mut signature = [0u8; 4];
        if let Err(e) = self.file.read_exact(&mut signature) {
            if e.kind() == io::ErrorKind::UnexpectedEof {
                return Ok(None);
            }
            return Err(e.into());
        }

        let sig = u32::from_le_bytes(signature);
        if sig != CENTRAL_DIR_ENTRY_SIGNATURE {
            return Ok(None);
        }

        // Skip version and flags
        self.file.seek(SeekFrom::Current(4))?;

        // Read compression method
        let method = self.read_u16()?;
        let method = CompressionMethod::try_from(method)?;

        // Skip time and date
        self.file.seek(SeekFrom::Current(4))?;

        // Read CRC32
        let mut crc = [0u8; 4];
        self.file.read_exact(&mut crc)?;
        let crc32 = u32::from_le_bytes(crc);

        // Read sizes
        let compressed_size = self.read_u32()? as u64;
        let size = self.read_u32()? as u64;

        // Read name length
        let name_length = self.read_u16()?;
        let extra_length = self.read_u16()?;
        let comment_length = self.read_u16()?;

        // Skip disk number start
        self.file.seek(SeekFrom::Current(4))?;

        // Skip internal/external attributes
        self.file.seek(SeekFrom::Current(6))?;

        // Read local header offset
        let header_offset = self.read_u32()? as u64;

        // Read filename
        let mut name = vec![0; name_length as usize];
        self.file.read_exact(&mut name)?;
        let name = String::from_utf8_lossy(&name).into_owned();

        // Skip extra and comment
        self.file.seek(SeekFrom::Current((extra_length + comment_length) as i64))?;

        Ok(Some(ZipEntry {
            name,
            size,
            compressed_size,
            method,
            crc32,
            header_offset,
        }))
    }

    /// Reads a u16 in little-endian format
    fn read_u16(&mut self) -> Result<u16> {
        let mut buffer = [0u8; 2];
        self.file.read_exact(&mut buffer)?;
        Ok(u16::from_le_bytes(buffer))
    }

    /// Reads a u32 in little-endian format
    fn read_u32(&mut self) -> Result<u32> {
        let mut buffer = [0u8; 4];
        self.file.read_exact(&mut buffer)?;
        Ok(u32::from_le_bytes(buffer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempfile;
    use tokio::runtime::Runtime;

    #[test]
    fn test_zip_reader() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            // Create test ZIP file
            let mut file = tempfile().unwrap();
            
            // Write local file header
            file.write_all(&LOCAL_FILE_HEADER_SIGNATURE.to_le_bytes()).unwrap();
            file.write_all(&[0u8; 26]).unwrap();  // Header fields
            file.write_all(b"test.txt").unwrap();  // Filename
            
            // Write file data
            file.write_all(b"Hello, World!").unwrap();
            
            // Write central directory
            let cd_offset = file.seek(SeekFrom::Current(0)).unwrap();
            file.write_all(&CENTRAL_DIR_ENTRY_SIGNATURE.to_le_bytes()).unwrap();
            file.write_all(&[0u8; 10]).unwrap();  // Version, flags, method, time, date
            file.write_all(&[0u8; 12]).unwrap();  // CRC32, sizes
            file.write_all(&8u16.to_le_bytes()).unwrap();  // Filename length
            file.write_all(&0u16.to_le_bytes()).unwrap();  // Extra length
            file.write_all(&0u16.to_le_bytes()).unwrap();  // Comment length
            file.write_all(&[0u8; 10]).unwrap();  // Disk number, attributes
            file.write_all(&0u32.to_le_bytes()).unwrap();  // Local header offset
            file.write_all(b"test.txt").unwrap();
            
            // Write end of central directory
            file.write_all(&END_OF_CENTRAL_DIR_SIGNATURE.to_le_bytes()).unwrap();
            file.write_all(&[0u8; 16]).unwrap();  // EOCD fields
            file.write_all(&(cd_offset as u32).to_le_bytes()).unwrap();
            file.write_all(&[0u8; 2]).unwrap();  // Comment length
            
            // Create reader
            let buffer_pool = Arc::new(BufferPool::new(1024 * 1024));
            let (tx, _rx) = mpsc::channel(1024);
            let mut reader = ZipReader::new(file, buffer_pool, tx);
            
            // Read entries
            let entries = reader.read_entries().await.unwrap();
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].name, "test.txt");
        });
    }

    #[test]
    fn test_invalid_signature() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let mut file = tempfile().unwrap();
            file.write_all(&0xFFFFFFFFu32.to_le_bytes()).unwrap();
            
            let buffer_pool = Arc::new(BufferPool::new(1024 * 1024));
            let (tx, _rx) = mpsc::channel(1024);
            let mut reader = ZipReader::new(file, buffer_pool, tx);
            
            assert!(reader.read_entries().await.is_err());
        });
    }
}
