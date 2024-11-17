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
    use std::os::unix::fs::FileExt;

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
        // Create a sparse file by setting the length without writing data
        let f = std::fs::OpenOptions::new()
            .write(true)
            .open(file.path())?;
        // Just write a single byte at MAX_SIZE + 1 to create a sparse file
        f.set_len(MAX_SIZE + 1)?;
        
        let reader = TestZipReader::new(file.path());
        let result = reader.validate_size().await;

        assert!(matches!(
            result,
            Err(Error::SizeLimit { size }) if size > MAX_SIZE
        ));
        Ok(())
    }
}
