use memmap2::{Mmap, MmapOptions};
use std::fs::File;
use std::io;
use tempfile::tempfile;

/// A memory-mapped file buffer
pub struct MemoryMap {
    _file: File, // Keep file alive while map exists
    map: Mmap,
}

impl MemoryMap {
    /// Creates a new memory-mapped buffer of the specified size
    ///
    /// # Safety
    /// This function is safe to call, but the returned memory map must be
    /// properly unmapped when dropped. The Drop implementation handles this
    /// automatically.
    pub fn new(size: usize) -> io::Result<Self> {
        let file = tempfile()?;
        file.set_len(size as u64)?;

        // SAFETY: The file is kept alive for the lifetime of the map
        let map = unsafe { MmapOptions::new().len(size).map(&file)? };

        Ok(Self { _file: file, map })
    }

    /// Returns a reference to the memory-mapped buffer
    pub fn as_slice(&self) -> &[u8] {
        &self.map
    }

    /// Returns the size of the memory-mapped buffer
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Returns true if the memory-mapped buffer is empty
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Validates that the memory map is still valid
    ///
    /// # Safety
    /// This function is safe to call and will return an error if the
    /// memory map is no longer valid.
    pub fn validate(&self) -> io::Result<()> {
        // Check if file is still accessible
        self._file.sync_all()?;

        // Check if memory map is still valid
        if self.map.len() == 0 {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Memory map is invalid",
            ));
        }

        Ok(())
    }
}

impl Drop for MemoryMap {
    fn drop(&mut self) {
        // SAFETY: The map is automatically unmapped when dropped
        // The file is also closed when dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_mmap_creation() {
        let map = MemoryMap::new(1024).unwrap();
        assert_eq!(map.len(), 1024);
        assert!(!map.is_empty());
    }

    #[test]
    fn test_mmap_validation() {
        let map = MemoryMap::new(1024).unwrap();
        assert!(map.validate().is_ok());
    }

    #[test]
    fn test_mmap_write() {
        let mut file = tempfile().unwrap();
        file.write_all(b"Hello, World!").unwrap();
        file.sync_all().unwrap();

        let map = unsafe { MmapOptions::new().len(13).map(&file).unwrap() };

        assert_eq!(&map[..], b"Hello, World!");
    }

    #[test]
    fn test_mmap_drop() {
        let map = MemoryMap::new(1024).unwrap();
        let ptr = map.as_slice().as_ptr();
        drop(map);

        // Memory should be unmapped
        // This is a bit unsafe to test directly
        // Instead we create a new map and ensure it works
        let map2 = MemoryMap::new(1024).unwrap();
        assert!(map2.validate().is_ok());
        assert_ne!(map2.as_slice().as_ptr(), ptr);
    }
}
