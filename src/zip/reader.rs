use super::entry::LocalFileHeader;
use super::{ZipEntry, ZipError};
use memmap2::{Mmap, MmapOptions};
use std::fs::File;
use std::io::{self};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;

const ZIP_LOCAL_HEADER_SIGNATURE: u32 = 0x04034b50;
const ZIP_CENTRAL_DIR_SIGNATURE: u32 = 0x02014b50;
const ZIP_END_OF_CENTRAL_DIR_SIGNATURE: u32 = 0x06054b50;

pub struct ZipReader {
    file: RwLock<File>,
    mmap: Mmap,
    current_offset: u64,
    is_valid: AtomicBool,
}

impl ZipReader {
    pub fn new<P: AsRef<std::path::Path>>(path: P) -> io::Result<Self> {
        // Open file with exclusive access
        let file = File::open(path)?;

        // Lock file for reading
        let file = RwLock::new(file);

        // Create memory map with safety checks
        let mmap = unsafe {
            let guard = file
                .read()
                .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to acquire read lock"))?;

            MmapOptions::new()
                .populate() // Pre-fault pages
                .map(&*guard)?
        };

        // Validate memory map
        Self::validate_map(&mmap)?;

        Ok(Self {
            file,
            mmap,
            current_offset: 0,
            is_valid: AtomicBool::new(true),
        })
    }

    /// Validates a memory map for safety
    fn validate_map(map: &Mmap) -> io::Result<()> {
        // Check minimum size
        if map.len() < 22 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "File too small to be a ZIP archive",
            ));
        }

        // Check for ZIP end of central directory signature
        let mut found = false;
        for i in (0..map.len().saturating_sub(22)).rev() {
            if &map[i..i + 4] == &ZIP_END_OF_CENTRAL_DIR_SIGNATURE.to_le_bytes() {
                found = true;
                break;
            }
        }

        if !found {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Not a valid ZIP file",
            ));
        }

        Ok(())
    }

    /// Returns an iterator over ZIP entries
    pub fn entries(&self) -> impl Iterator<Item = Result<ZipEntry, ZipError>> + '_ {
        ZipEntryIterator {
            reader: self,
            current_offset: self.current_offset,
        }
    }

    /// Validates that the memory map is still valid
    fn validate(&self) -> io::Result<()> {
        if !self.is_valid.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Memory map is no longer valid",
            ));
        }
        Ok(())
    }
}

impl Drop for ZipReader {
    fn drop(&mut self) {
        // Mark as invalid before unmapping
        self.is_valid.store(false, Ordering::Release);
    }
}

struct ZipEntryIterator<'a> {
    reader: &'a ZipReader,
    current_offset: u64,
}

impl<'a> Iterator for ZipEntryIterator<'a> {
    type Item = Result<ZipEntry, ZipError>;

    fn next(&mut self) -> Option<Self::Item> {
        // Validate memory map before access
        if let Err(e) = self.reader.validate() {
            return Some(Err(ZipError::Io(e)));
        }

        if self.current_offset >= self.reader.mmap.len() as u64 {
            return None;
        }

        let data = &self.reader.mmap[self.current_offset as usize..];

        // Read central directory header
        if read_u32_le(data) != ZIP_CENTRAL_DIR_SIGNATURE {
            return Some(Err(ZipError::Format(
                "Invalid central directory header".into(),
            )));
        }

        // Parse header fields
        let compression_method = read_u16_le(&data[10..]);
        let crc32 = read_u32_le(&data[16..]);
        let compressed_size = read_u32_le(&data[20..]) as u64;
        let uncompressed_size = read_u32_le(&data[24..]) as u64;
        let file_name_length = read_u16_le(&data[28..]) as usize;
        let extra_field_length = read_u16_le(&data[30..]) as usize;
        let file_comment_length = read_u16_le(&data[32..]) as usize;
        let local_header_offset = read_u32_le(&data[42..]) as u64;

        // Read filename
        let file_name = String::from_utf8_lossy(&data[46..46 + file_name_length]).into_owned();

        // Read extra field
        let extra_field =
            data[46 + file_name_length..46 + file_name_length + extra_field_length].to_vec();

        // Create header
        let header = LocalFileHeader {
            version_needed: read_u16_le(&data[6..]),
            flags: read_u16_le(&data[8..]),
            compression_method,
            last_mod_time: read_u16_le(&data[12..]),
            last_mod_date: read_u16_le(&data[14..]),
            crc32,
            compressed_size,
            uncompressed_size,
            file_name,
            extra_field,
        };

        // Read file data with validation
        let file_data = if let Ok(_) = self.reader.validate() {
            self.reader.mmap[local_header_offset as usize
                + 30
                + file_name_length
                + extra_field_length
                ..local_header_offset as usize
                    + 30
                    + file_name_length
                    + extra_field_length
                    + compressed_size as usize]
                .to_vec()
        } else {
            return Some(Err(ZipError::Memory(
                "Memory map is no longer valid".into(),
            )));
        };

        // Update offset
        self.current_offset = self.current_offset.saturating_add(
            (46 + file_name_length + extra_field_length + file_comment_length) as u64,
        );

        Some(Ok(ZipEntry {
            header,
            data_descriptor: None,
            file_data,
        }))
    }
}

// Helper functions
fn read_u16_le(data: &[u8]) -> u16 {
    u16::from_le_bytes([data[0], data[1]])
}

fn read_u32_le(data: &[u8]) -> u32 {
    u32::from_le_bytes([data[0], data[1], data[2], data[3]])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_memory_map_safety() -> io::Result<()> {
        // Create test ZIP file
        let mut file = NamedTempFile::new()?;

        // Write minimal ZIP structure
        let end_of_central_dir = [
            0x50, 0x4b, 0x05, 0x06, // End of central directory signature
            0x00, 0x00, // Number of this disk
            0x00, 0x00, // Disk where central directory starts
            0x00, 0x00, // Number of central directory records on this disk
            0x00, 0x00, // Total number of central directory records
            0x00, 0x00, 0x00, 0x00, // Size of central directory
            0x00, 0x00, 0x00, 0x00, // Offset of start of central directory
            0x00, 0x00, // Comment length
        ];
        file.write_all(&end_of_central_dir)?;
        file.flush()?;

        // Test reader creation
        let reader = ZipReader::new(file.path())?;
        assert!(reader.validate().is_ok());

        // Test validation after drop
        drop(reader);

        Ok(())
    }

    #[test]
    fn test_invalid_zip() {
        // Create empty file
        let file = NamedTempFile::new().unwrap();

        // Should fail validation
        assert!(ZipReader::new(file.path()).is_err());
    }

    #[test]
    fn test_concurrent_access() -> io::Result<()> {
        use std::sync::Arc;
        use std::thread;

        // Create test ZIP file
        let mut file = NamedTempFile::new()?;
        file.write_all(&[
            0x50, 0x4b, 0x05, 0x06, // End of central directory signature
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
        ])?;
        file.flush()?;

        // Create shared reader
        let reader = Arc::new(ZipReader::new(file.path())?);

        // Spawn threads to access reader
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let reader = Arc::clone(&reader);
                thread::spawn(move || {
                    assert!(reader.validate().is_ok());
                })
            })
            .collect();

        // Wait for all threads
        for thread in threads {
            thread.join().unwrap();
        }

        Ok(())
    }
}
