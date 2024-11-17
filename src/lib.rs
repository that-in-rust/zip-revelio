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

#[derive(Debug)]
pub struct Entry {
    pub name: String,
    pub size: u64,
    pub compressed_size: u64,
    pub crc32: u32,
}

#[derive(Debug)]
pub struct Directory {
    pub entries: Vec<Entry>,
}

pub trait ZipReader: Send + Sync {
    fn validate_size(&self) -> impl std::future::Future<Output = Result<()>> + Send;
    fn read_directory(&self) -> impl std::future::Future<Output = Result<Directory>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use tokio;

    #[tokio::test]
    async fn test_size_limit() -> Result<()> {
        let file = NamedTempFile::new()?;
        tokio::fs::File::create(&file)
            .await?
            .set_len(MAX_SIZE + 1)
            .await?;
        
        let reader = FileZipReader::new(file.path());
        let result = reader.validate_size().await;
        
        assert!(matches!(result, Err(Error::SizeLimit { size }) if size > MAX_SIZE));
        Ok(())
    }
}
