use tokio::fs::File;
use tokio::io::{AsyncReadExt};
use std::path::PathBuf;
use futures::Stream;
use std::future::Future;
use pin_project_lite::pin_project;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::types::{Chunk, Error, Result, MAX_FILE_SIZE};

// Add at top of file
const CHUNK_SIZE: usize = 256 * 1024;  // 256KB

/// Async ZIP file reader with chunk streaming
pub struct ZipReader {
    /// File handle for async I/O
    file: File,
    /// Size of each chunk
    chunk_size: usize,
    /// Total file size
    total_size: u64,
    /// Current position in file
    position: u64,
}

impl ZipReader {
    /// Create a new ZIP reader with validation
    pub async fn new(path: PathBuf) -> Result<Self> {
        let metadata = tokio::fs::metadata(&path).await
            .map_err(|e| Error::Io(format!("Failed to read metadata: {}", e)))?;

        // Essential validations
        if metadata.len() == 0 {
            return Err(Error::Zip("Empty file".into()));
        }
        if metadata.len() > MAX_FILE_SIZE {
            return Err(Error::Zip("File too large (>4GB not supported)".into()));
        }

        let file = File::open(&path).await
            .map_err(|e| Error::Io(format!("Failed to open file: {}", e)))?;

        Ok(Self {
            file,
            chunk_size: CHUNK_SIZE,  // Use constant instead of magic number
            total_size: metadata.len(),
            position: 0,
        })
    }
    
    /// Stream chunks from the ZIP file
    pub fn stream_chunks(&mut self) -> impl Stream<Item = Result<Chunk>> + '_ {
        let chunk_size = self.chunk_size;  // Cache before borrow
        ChunkStream {
            reader: self,
            buffer: vec![0; chunk_size],  // Use cached size
        }
    }
    
    /// Get total file size
    pub fn total_size(&self) -> u64 {
        self.total_size
    }
    
    /// Get current position
    pub fn position(&self) -> u64 {
        self.position
    }
}

impl Drop for ZipReader {
    fn drop(&mut self) {
        // Ensure file handle is properly closed and synced
        let _ = futures::executor::block_on(self.file.sync_all());
    }
}

// Custom stream implementation for chunk reading
pin_project! {
    struct ChunkStream<'a> {
        #[pin]
        reader: &'a mut ZipReader,
        buffer: Vec<u8>,
    }
}

impl<'a> Stream for ChunkStream<'a> {
    type Item = Result<Chunk>;
    
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        
        if this.reader.position >= this.reader.total_size {
            return Poll::Ready(None);
        }
        
        let fut = this.reader.file.read(this.buffer);
        match Pin::new(&mut Box::pin(fut)).poll(cx) {
            Poll::Ready(Ok(0)) => Poll::Ready(None),
            Poll::Ready(Ok(bytes_read)) => {
                this.reader.position += bytes_read as u64;
                Poll::Ready(Some(Ok(Chunk::new(
                    this.buffer[..bytes_read].to_vec(),
                    this.reader.position - bytes_read as u64
                ))))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Some(Err(Error::Io(e.to_string())))),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::fs::File;
    use tokio::io::AsyncWriteExt;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_reader_creation() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.zip");
        
        // Create test file
        let mut file = File::create(&path).await.unwrap();
        file.write_all(b"PK\x03\x04test data").await.unwrap();
        
        let reader = ZipReader::new(path).await.unwrap();
        assert!(reader.total_size() > 0);
    }

    #[tokio::test]
    async fn test_chunk_streaming() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.zip");
        
        // Create larger test file
        let mut file = File::create(&path).await.unwrap();
        let data = vec![1u8; 1024 * 1024]; // 1MB
        file.write_all(&data).await.unwrap();
        
        let reader = ZipReader::new(path).await.unwrap();
        let mut stream = reader.stream_chunks();
        
        let mut total_bytes = 0;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.unwrap();
            total_bytes += chunk.size();
        }
        
        assert_eq!(total_bytes, 1024 * 1024);
    }
}
