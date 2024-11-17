use crate::{Directory, Entry, Error, Result, ZipReader};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tokio::fs::File;
use zip::ZipArchive;

pub struct FileZipReader {
    path: PathBuf,
}

impl FileZipReader {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_owned(),
        }
    }
}

impl ZipReader for FileZipReader {
    async fn validate_size(&self) -> Result<()> {
        let metadata = tokio::fs::metadata(&self.path).await?;
        if metadata.len() > crate::MAX_SIZE {
            return Err(Error::SizeLimit {
                size: metadata.len(),
            });
        }
        Ok(())
    }

    async fn read_directory(&self) -> Result<Directory> {
        let file = File::open(&self.path).await?;
        let std_file = file.into_std().await;
        
        let archive = ZipArchive::new(std_file)
            .map_err(|e| Error::Format(e.to_string()))?;
        let archive = Mutex::new(archive);
        
        let entries: Vec<Entry> = (0..archive.lock().unwrap().len())
            .into_par_iter()
            .filter_map(|i| {
                archive.lock().unwrap().by_index(i).ok().map(|entry| Entry {
                    name: entry.name().to_owned(),
                    size: entry.size(),
                    compressed_size: entry.compressed_size(),
                    crc32: entry.crc32(),
                })
            })
            .collect();

        Ok(Directory { entries })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;
    use zip::write::FileOptions;

    fn create_test_zip() -> Result<NamedTempFile> {
        let file = NamedTempFile::new()?;
        let mut zip = zip::ZipWriter::new(std::fs::File::create(file.path())?);

        let options = FileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        zip.start_file("test.txt", options)
            .map_err(|e| Error::Format(e.to_string()))?;
        zip.write_all(b"Hello, World!")?;
        zip.finish()
            .map_err(|e| Error::Format(e.to_string()))?;

        Ok(file)
    }

    #[tokio::test]
    async fn test_valid_zip() -> Result<()> {
        let file = create_test_zip()?;
        let reader = FileZipReader::new(file.path());
        
        reader.validate_size().await?;
        let dir = reader.read_directory().await?;
        
        assert_eq!(dir.entries.len(), 1);
        assert_eq!(dir.entries[0].name, "test.txt");
        assert_eq!(dir.entries[0].size, 13);
        
        Ok(())
    }
}
