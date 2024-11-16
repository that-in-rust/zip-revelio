use super::error::ZipError;
use crate::buffer::Buffer;
use crc32fast::Hasher;
use flate2::read::DeflateDecoder;
use std::io::Read;

#[derive(Debug)]
pub struct ZipEntry {
    pub(crate) header: LocalFileHeader,
    pub(crate) data_descriptor: Option<DataDescriptor>,
    pub(crate) file_data: Vec<u8>,
}

#[derive(Debug)]
pub(crate) struct LocalFileHeader {
    pub version_needed: u16,
    pub flags: u16,
    pub compression_method: u16,
    pub last_mod_time: u16,
    pub last_mod_date: u16,
    pub crc32: u32,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub file_name: String,
    pub extra_field: Vec<u8>,
}

#[derive(Debug)]
pub(crate) struct DataDescriptor {
    pub crc32: u32,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
}

impl ZipEntry {
    pub async fn process(&self, buffer: &mut Buffer) -> Result<ProcessedEntry, ZipError> {
        // Validate entry
        self.validate()?;

        // Process based on method
        match self.header.compression_method {
            0 => {
                // Store
                buffer.copy_from_slice(&self.file_data);
                let mut hasher = Hasher::new();
                hasher.update(&self.file_data);
                let crc = hasher.finalize();

                if crc != self.header.crc32 {
                    return Err(ZipError::Crc32Mismatch);
                }

                Ok(ProcessedEntry {
                    name: self.header.file_name.clone(),
                    original_size: self.header.uncompressed_size,
                    compressed_size: self.header.compressed_size,
                    crc32: crc,
                    method: self.header.compression_method,
                })
            }
            8 => {
                // Deflate
                let mut decoder = DeflateDecoder::new(&self.file_data[..]);
                let mut decompressed = Vec::with_capacity(self.header.uncompressed_size as usize);
                decoder.read_to_end(&mut decompressed)?;
                buffer.copy_from_slice(&decompressed);

                let mut hasher = Hasher::new();
                hasher.update(&decompressed);
                let crc = hasher.finalize();

                if crc != self.header.crc32 {
                    return Err(ZipError::Crc32Mismatch);
                }

                Ok(ProcessedEntry {
                    name: self.header.file_name.clone(),
                    original_size: self.header.uncompressed_size,
                    compressed_size: self.header.compressed_size,
                    crc32: crc,
                    method: self.header.compression_method,
                })
            }
            _ => Err(ZipError::UnsupportedMethod(self.header.compression_method)),
        }
    }

    fn validate(&self) -> Result<(), ZipError> {
        // Check filename
        if !self.header.file_name.is_ascii() {
            return Err(ZipError::NonAsciiName);
        }

        // Check size
        if self.header.uncompressed_size > 0xFFFFFFFF {
            return Err(ZipError::FileTooLarge);
        }

        // Check compression method
        match self.header.compression_method {
            0 | 8 => Ok(()),
            _ => Err(ZipError::UnsupportedMethod(self.header.compression_method)),
        }
    }
}

#[derive(Debug)]
pub struct ProcessedEntry {
    pub name: String,
    pub original_size: u64,
    pub compressed_size: u64,
    pub crc32: u32,
    pub method: u16,
}
