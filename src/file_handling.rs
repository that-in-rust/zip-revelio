use std::{
    fs::File,
    io::{self, BufReader, BufWriter, Read, Write},
    path::Path,
    sync::Arc,
};
use parking_lot::Mutex;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    error::ZipError,
    Result,
};

/// File handle pool for reusing file descriptors
#[derive(Debug)]
pub struct HandlePool {
    /// Available handles
    handles: Mutex<Vec<File>>,
    /// Maximum pool size
    max_size: usize,
}

impl HandlePool {
    /// Creates a new handle pool
    pub fn new(max_size: usize) -> Self {
        Self {
            handles: Mutex::new(Vec::with_capacity(max_size)),
            max_size,
        }
    }

    /// Acquires a file handle
    pub fn acquire(&self, path: &Path) -> io::Result<File> {
        let mut handles = self.handles.lock();
        if let Some(handle) = handles.pop() {
            Ok(handle)
        } else if handles.len() < self.max_size {
            File::open(path)
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "Handle pool exhausted",
            ))
        }
    }

    /// Releases a file handle back to the pool
    pub fn release(&self, handle: File) {
        let mut handles = self.handles.lock();
        if handles.len() < self.max_size {
            handles.push(handle);
        }
    }
}

/// File resource manager
#[derive(Debug)]
pub struct FileManager {
    /// Handle pool
    pool: Arc<HandlePool>,
    /// Resource limits
    max_open_files: usize,
    current_open: Arc<Mutex<usize>>,
}

impl FileManager {
    /// Creates a new file manager
    pub fn new(max_open_files: usize) -> Self {
        Self {
            pool: Arc::new(HandlePool::new(max_open_files)),
            max_open_files,
            current_open: Arc::new(Mutex::new(0)),
        }
    }

    /// Opens a file with resource limits
    pub fn open<P: AsRef<Path>>(&self, path: P) -> Result<ManagedFile> {
        let mut current = self.current_open.lock();
        if *current >= self.max_open_files {
            return Err(ZipError::ResourceLimit("Too many open files".into()));
        }
        
        let file = self.pool.acquire(path.as_ref())?;
        *current += 1;
        
        Ok(ManagedFile {
            file: Some(file),
            manager: self,
        })
    }

    /// Gets current open file count
    pub fn open_files(&self) -> usize {
        *self.current_open.lock()
    }
}

/// Managed file handle with automatic cleanup
#[derive(Debug)]
pub struct ManagedFile<'a> {
    /// Inner file handle
    file: Option<File>,
    /// File manager reference
    manager: &'a FileManager,
}

impl<'a> ManagedFile<'a> {
    /// Creates a buffered reader
    pub fn buffered_reader(self) -> io::BufReader<Self> {
        io::BufReader::new(self)
    }

    /// Creates a buffered writer
    pub fn buffered_writer(self) -> io::BufWriter<Self> {
        io::BufWriter::new(self)
    }
}

impl<'a> Read for ManagedFile<'a> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.file.as_mut().unwrap().read(buf)
    }
}

impl<'a> Write for ManagedFile<'a> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file.as_mut().unwrap().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.as_mut().unwrap().flush()
    }
}

impl<'a> Drop for ManagedFile<'a> {
    fn drop(&mut self) {
        if let Some(file) = self.file.take() {
            self.manager.pool.release(file);
            *self.manager.current_open.lock() -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_handle_pool() {
        let pool = HandlePool::new(2);
        let temp_file = NamedTempFile::new().unwrap();
        
        // Acquire handles
        let handle1 = pool.acquire(temp_file.path()).unwrap();
        let handle2 = pool.acquire(temp_file.path()).unwrap();
        
        // Pool should be empty
        assert!(pool.acquire(temp_file.path()).is_err());
        
        // Release handle
        pool.release(handle1);
        
        // Should be able to acquire again
        let handle3 = pool.acquire(temp_file.path()).unwrap();
        assert!(pool.acquire(temp_file.path()).is_err());
        
        // Cleanup
        pool.release(handle2);
        pool.release(handle3);
    }

    #[test]
    fn test_file_manager() {
        let manager = FileManager::new(2);
        let temp_file = NamedTempFile::new().unwrap();
        
        // Open files
        let file1 = manager.open(temp_file.path()).unwrap();
        let file2 = manager.open(temp_file.path()).unwrap();
        
        assert_eq!(manager.open_files(), 2);
        
        // Should fail to open more files
        assert!(manager.open(temp_file.path()).is_err());
        
        // Drop one file
        drop(file1);
        assert_eq!(manager.open_files(), 1);
        
        // Should be able to open another file
        let file3 = manager.open(temp_file.path()).unwrap();
        assert_eq!(manager.open_files(), 2);
        
        // Cleanup
        drop(file2);
        drop(file3);
        assert_eq!(manager.open_files(), 0);
    }

    #[test]
    fn test_managed_file_io() {
        let manager = FileManager::new(1);
        let mut temp_file = NamedTempFile::new().unwrap();
        
        // Write data
        {
            let mut file = manager.open(temp_file.path()).unwrap();
            file.write_all(b"test data").unwrap();
            file.flush().unwrap();
        }
        
        // Read data
        {
            let mut file = manager.open(temp_file.path()).unwrap();
            let mut buf = String::new();
            file.read_to_string(&mut buf).unwrap();
            assert_eq!(buf, "test data");
        }
        
        assert_eq!(manager.open_files(), 0);
    }
}
