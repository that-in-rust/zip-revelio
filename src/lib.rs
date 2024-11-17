use thiserror::Error;

pub const MAX_SIZE: u64 = 4 * 1024 * 1024 * 1024; // 4GB

mod zip;
pub use zip::FileZipReader;

#[derive(Error, Debug)]
pub enum Error {
    #[error("ZIP file too large: {size} bytes (max: 4GB)")]
    SizeLimit { size: u64 },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid ZIP format: {0}")]
    Format(String),
    #[error("Memory limit exceeded: required {required}MB, limit {limit}MB")]
    MemoryLimit { required: usize, limit: usize },
    #[error("Processing error: {0}")]
    Processing(String),
}

pub type Result<T> = std::result::Result<T, Error>;

pub trait ZipReader: Send + Sync {
    fn validate_size(&self) -> impl std::future::Future<Output = Result<()>> + Send;
    fn read_directory(&self) -> impl std::future::Future<Output = Result<Directory>> + Send;
}

#[derive(Debug)]
pub struct Directory {
    pub entries: Vec<Entry>,
}

impl Directory {
    pub fn total_size(&self) -> u64 {
        self.entries.iter().map(|e| e.size).sum()
    }

    pub fn total_compressed_size(&self) -> u64 {
        self.entries.iter().map(|e| e.compressed_size).sum()
    }

    pub fn compression_ratio(&self) -> f64 {
        let total = self.total_size();
        if total == 0 {
            return 0.0;
        }
        let compressed = self.total_compressed_size();
        1.0 - (compressed as f64 / total as f64)
    }
}

#[derive(Debug)]
pub struct Entry {
    pub name: String,
    pub size: u64,
    pub compressed_size: u64,
    pub crc32: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use tokio::fs::File;

    struct TestZipReader {
        path: std::path::PathBuf,
    }

    impl TestZipReader {
        fn new<P: AsRef<std::path::Path>>(path: P) -> Self {
            Self {
                path: path.as_ref().to_owned(),
            }
        }
    }

    impl ZipReader for TestZipReader {
        async fn validate_size(&self) -> Result<()> {
            let metadata = tokio::fs::metadata(&self.path).await?;
            if metadata.len() > MAX_SIZE {
                return Err(Error::SizeLimit {
                    size: metadata.len(),
                });
            }
            Ok(())
        }

        async fn read_directory(&self) -> Result<Directory> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn test_size_limit() -> Result<()> {
        let file = NamedTempFile::new()?;
        let f = File::create(file.path()).await?;
        f.set_len(MAX_SIZE + 1).await?;

        let reader = TestZipReader::new(file.path());
        let result = reader.validate_size().await;

        assert!(matches!(
            result,
            Err(Error::SizeLimit { size }) if size > MAX_SIZE
        ));
        Ok(())
    }

    #[test]
    fn test_directory_stats() {
        let entries = vec![
            Entry {
                name: "test1.txt".to_string(),
                size: 100,
                compressed_size: 50,
                crc32: 0,
            },
            Entry {
                name: "test2.txt".to_string(),
                size: 200,
                compressed_size: 100,
                crc32: 0,
            },
        ];

        let dir = Directory { entries };
        assert_eq!(dir.total_size(), 300);
        assert_eq!(dir.total_compressed_size(), 150);
        assert_eq!(dir.compression_ratio(), 0.5);
    }

    #[test]
    fn test_empty_directory_stats() {
        let dir = Directory { entries: vec![] };
        assert_eq!(dir.total_size(), 0);
        assert_eq!(dir.total_compressed_size(), 0);
        assert_eq!(dir.compression_ratio(), 0.0);
    }
}
