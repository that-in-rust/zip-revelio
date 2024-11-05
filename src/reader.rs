use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};
use std::path::PathBuf;
use indicatif::ProgressBar;

use crate::types::{Error, Result, ZipAnalysis, ZIP_END_CENTRAL_DIR_SIGNATURE, ZIP_CENTRAL_DIR_SIGNATURE, MIN_EOCD_SIZE, MAX_COMMENT_SIZE};

pub struct ZipReader {
    file: File,
    total_size: u64,
    pb: ProgressBar,
}

impl ZipReader {
    pub async fn new(path: PathBuf, pb: ProgressBar) -> Result<Self> {
        let metadata = tokio::fs::metadata(&path).await
            .map_err(|e| Error::Io(format!("Failed to read metadata for {}: {}", path.display(), e)))?;

        if metadata.len() == 0 {
            return Err(Error::Zip(format!("File {} is empty", path.display())));
        }

        let file = File::open(&path).await
            .map_err(|e| Error::Io(format!("Failed to open {}: {}", path.display(), e)))?;

        Ok(Self {
            file,
            total_size: metadata.len(),
            pb,
        })
    }

    async fn validate_cd_offset(&self, offset: u64, size: u64) -> Result<()> {
        if offset >= self.total_size {
            return Err(Error::Zip(format!(
                "Invalid CD offset {} (file size: {})", 
                offset, self.total_size
            )));
        }
        if offset + size > self.total_size {
            return Err(Error::Zip(format!(
                "CD extends beyond file end (offset: {}, size: {}, file size: {})",
                offset, size, self.total_size
            )));
        }
        Ok(())
    }

    pub async fn analyze(&mut self, results: &mut ZipAnalysis) -> Result<()> {
        // Phase 1: End Scan
        self.pb.set_message("Scanning for End of Central Directory...");
        let read_size = std::cmp::min(MAX_COMMENT_SIZE as u64 + MIN_EOCD_SIZE as u64, self.total_size);
        let start_pos = self.total_size.saturating_sub(read_size);
        
        self.file.seek(SeekFrom::Start(start_pos)).await?;
        let mut buffer = vec![0; read_size as usize];
        self.file.read_exact(&mut buffer).await?;

        // Find EOCD
        let mut eocd_pos = None;
        for i in (0..buffer.len().saturating_sub(MIN_EOCD_SIZE as usize)).rev() {
            if &buffer[i..i+4] == &ZIP_END_CENTRAL_DIR_SIGNATURE.to_le_bytes() {
                eocd_pos = Some(i);
                break;
            }
        }

        let eocd_pos = eocd_pos.ok_or_else(|| Error::Zip(format!(
            "End of central directory not found in last {} bytes", read_size
        )))?;

        // Phase 2: Get CD Location
        self.pb.set_message("Reading Central Directory location...");
        let cd_offset = u32::from_le_bytes([
            buffer[eocd_pos+16], buffer[eocd_pos+17], 
            buffer[eocd_pos+18], buffer[eocd_pos+19]
        ]) as u64;

        let cd_size = u32::from_le_bytes([
            buffer[eocd_pos+12], buffer[eocd_pos+13],
            buffer[eocd_pos+14], buffer[eocd_pos+15]
        ]) as u64;

        // Validate CD location
        self.validate_cd_offset(cd_offset, cd_size).await?;

        // Phase 3: Read CD
        self.pb.set_message("Reading Central Directory...");
        self.file.seek(SeekFrom::Start(cd_offset)).await?;
        let mut cd_data = vec![0; cd_size as usize];
        self.file.read_exact(&mut cd_data).await?;

        // Phase 4: Process Entries
        self.pb.set_message("Processing entries...");
        let mut pos = 0;
        while pos + 46 <= cd_data.len() {
            // Verify CD entry signature
            if &cd_data[pos..pos+4] != &ZIP_CENTRAL_DIR_SIGNATURE.to_le_bytes() {
                return Err(Error::Zip(format!(
                    "Invalid Central Directory signature at offset {}", pos
                )));
            }

            let name_length = u16::from_le_bytes([cd_data[pos+28], cd_data[pos+29]]) as usize;
            let extra_length = u16::from_le_bytes([cd_data[pos+30], cd_data[pos+31]]) as usize;
            let comment_length = u16::from_le_bytes([cd_data[pos+32], cd_data[pos+33]]) as usize;

            // Validate entry size
            if pos + 46 + name_length + extra_length + comment_length > cd_data.len() {
                return Err(Error::Zip(format!(
                    "Corrupt CD entry at offset {} (entry extends beyond CD)", pos
                )));
            }

            let compressed_size = u32::from_le_bytes([
                cd_data[pos+20], cd_data[pos+21], cd_data[pos+22], cd_data[pos+23]
            ]) as u64;

            let uncompressed_size = u32::from_le_bytes([
                cd_data[pos+24], cd_data[pos+25], cd_data[pos+26], cd_data[pos+27]
            ]) as u64;

            let compression_method = u16::from_le_bytes([cd_data[pos+10], cd_data[pos+11]]);

            let file_name = String::from_utf8_lossy(&cd_data[pos+46..pos+46+name_length]).to_string();
            
            // Update results
            if !file_name.ends_with('/') {
                results.update_sizes(compressed_size, uncompressed_size);
                results.update_compression_method(compression_method);
                results.add_file_path(file_name);
                
                // Update progress
                self.pb.set_message(format!(
                    "Files: {}, Ratio: {:.1}%",
                    results.file_count(),
                    results.get_compression_ratio() * 100.0
                ));
            }

            pos += 46 + name_length + extra_length + comment_length;
        }

        Ok(())
    }
}
