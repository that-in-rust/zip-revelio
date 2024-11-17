use crate::{Directory, Entry, Error, Result, ZipReader};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use parking_lot::Mutex;
use zip::ZipArchive;
use std::fs::File;

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
        let file = File::open(&self.path)
            .map_err(|e| Error::Format(e.to_string()))?;
            
        let archive = ZipArchive::new(file)
            .map_err(|e| Error::Format(e.to_string()))?;
            
        let len = archive.len();
        let archive = Arc::new(Mutex::new(archive));
        
        let entries: Vec<_> = (0..len)
            .into_par_iter()
            .filter_map(|i| {
                let mut guard = archive.lock();
                guard.by_index(i).ok().map(|entry| {
                    Entry {
                        name: entry.name().to_owned(),
                        size: entry.size(),
                        compressed_size: entry.compressed_size(),
                        crc32: entry.crc32(),
                    }
                })
            })
            .collect();

        Ok(Directory { entries })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    fn create_test_zip() -> Result<NamedTempFile> {
        let file = NamedTempFile::new().unwrap();
        let mut zip = zip::ZipWriter::new(File::create(file.path()).unwrap());
        
        zip.start_file("test.txt", Default::default()).unwrap();
        zip.write_all(b"Hello, World!").unwrap();
        zip.finish().unwrap();
        
        Ok(file)
    }

    #[tokio::test]
    async fn test_valid_zip() -> Result<()> {
        let file = create_test_zip()?;
        let reader = FileZipReader::new(file.path());
        let dir = reader.read_directory().await?;
        
        assert_eq!(dir.entries.len(), 1);
        assert_eq!(dir.entries[0].name, "test.txt");
        assert_eq!(dir.entries[0].size, 13);
        Ok(())
    }

    #[tokio::test]
    async fn test_empty_zip() -> Result<()> {
        let file = NamedTempFile::new()?;
        let mut zip = zip::ZipWriter::new(File::create(file.path())?);
        zip.finish().map_err(|e| Error::Format(e.to_string()))?;

        let reader = FileZipReader::new(file.path());
        let dir = reader.read_directory().await?;
        
        assert_eq!(dir.entries.len(), 0);
        Ok(())
    }

    #[tokio::test]
    async fn test_nonexistent_file() -> Result<()> {
        let reader = FileZipReader::new("nonexistent.zip");
        let result = reader.read_directory().await;
        
        assert!(matches!(result, Err(Error::Format(_))));
        Ok(())
    }
}
