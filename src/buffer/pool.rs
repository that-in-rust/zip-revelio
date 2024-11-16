use super::mmap::MemoryMap;
use crate::ZipError;
use crossbeam::queue::ArrayQueue;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, RwLock};

thread_local! {
    static BUFFER_POOL: RefCell<Option<Arc<BufferPool>>> = RefCell::new(None);
}

#[derive(Debug, Clone)]
pub enum Buffer {
    Small(Vec<u8>),
    Medium(Vec<u8>),
    Large(usize),
}

impl Buffer {
    pub fn as_slice(&self) -> &[u8] {
        match self {
            Buffer::Small(buf) | Buffer::Medium(buf) => buf.as_slice(),
            Buffer::Large(_) => unimplemented!("Memory map slice not implemented"),
        }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        match self {
            Buffer::Small(buf) | Buffer::Medium(buf) => buf.as_mut_slice(),
            Buffer::Large(_) => unimplemented!("Memory map slice not implemented"),
        }
    }

    pub fn copy_from_slice(&mut self, data: &[u8]) {
        match self {
            Buffer::Small(buf) | Buffer::Medium(buf) => {
                buf.clear();
                buf.extend_from_slice(data);
            }
            Buffer::Large(_) => unimplemented!("Memory map copy not implemented"),
        }
    }

    /// Validates that the buffer memory is properly allocated and accessible
    pub fn validate_memory(&self) -> Result<(), ZipError> {
        match self {
            Buffer::Small(buf) | Buffer::Medium(buf) => {
                if buf.capacity() == 0 {
                    return Err(ZipError::Memory("Zero capacity buffer".into()));
                }
                Ok(())
            }
            Buffer::Large(id) => BUFFER_POOL.with(|pool| {
                if let Some(pool) = &*pool.borrow() {
                    let maps = pool
                        .large_maps
                        .read()
                        .map_err(|_| ZipError::Memory("Failed to acquire read lock".into()))?;
                    if !maps.contains_key(id) {
                        return Err(ZipError::Memory("Invalid memory map ID".into()));
                    }
                    Ok(())
                } else {
                    Err(ZipError::Memory("Buffer pool not initialized".into()))
                }
            }),
        }
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        match self {
            Buffer::Large(id) => {
                BUFFER_POOL.with(|pool| {
                    if let Some(pool) = &*pool.borrow() {
                        if let Ok(mut maps) = pool.large_maps.write() {
                            maps.remove(id);
                        }
                    }
                });
            }
            _ => {}
        }
    }
}

impl Deref for Buffer {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl DerefMut for Buffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}

#[derive(Debug)]
pub struct BufferPool {
    small_pool: ArrayQueue<Vec<u8>>,
    medium_pool: ArrayQueue<Vec<u8>>,
    large_maps: RwLock<HashMap<usize, MemoryMap>>,
    config: BufferConfig,
    next_map_id: std::sync::atomic::AtomicUsize,
}

#[derive(Debug, Clone)]
pub struct BufferConfig {
    pub small_size: usize,
    pub medium_size: usize,
    pub small_count: usize,
    pub medium_count: usize,
}

impl Default for BufferConfig {
    fn default() -> Self {
        Self {
            small_size: 64 * 1024,    // 64KB
            medium_size: 1024 * 1024, // 1MB
            small_count: 32,
            medium_count: 16,
        }
    }
}

impl BufferPool {
    pub fn new(config: BufferConfig) -> Result<Self, ZipError> {
        let config = config.clone(); // Clone config before moving
        let pool = Self {
            small_pool: ArrayQueue::new(config.small_count),
            medium_pool: ArrayQueue::new(config.medium_count),
            large_maps: RwLock::new(HashMap::new()),
            config,
            next_map_id: std::sync::atomic::AtomicUsize::new(0),
        };

        // Pre-allocate buffers
        for _ in 0..pool.config.small_count {
            pool.small_pool
                .push(vec![0; pool.config.small_size])
                .map_err(|_| ZipError::Memory("Failed to initialize small buffer".into()))?;
        }

        for _ in 0..pool.config.medium_count {
            pool.medium_pool
                .push(vec![0; pool.config.medium_size])
                .map_err(|_| ZipError::Memory("Failed to initialize medium buffer".into()))?;
        }

        // Initialize thread-local pool
        BUFFER_POOL.with(|p| {
            *p.borrow_mut() = Some(Arc::new(pool.clone()));
        });

        Ok(pool)
    }

    pub fn acquire(&self, size: usize) -> Result<Buffer, ZipError> {
        match size {
            s if s <= self.config.small_size => self
                .small_pool
                .pop()
                .map(Buffer::Small)
                .ok_or(ZipError::NoBufferAvailable),
            s if s <= self.config.medium_size => self
                .medium_pool
                .pop()
                .map(Buffer::Medium)
                .ok_or(ZipError::NoBufferAvailable),
            s => {
                let id = self
                    .next_map_id
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let map = MemoryMap::new(s)?;
                self.large_maps
                    .write()
                    .map_err(|_| ZipError::Memory("Failed to acquire write lock".into()))?
                    .insert(id, map);
                Ok(Buffer::Large(id))
            }
        }
    }

    pub fn release(&self, buffer: &Buffer) {
        match buffer {
            Buffer::Small(buf) => {
                if buf.capacity() == self.config.small_size {
                    let mut new_buf = vec![0; self.config.small_size];
                    new_buf.clear();
                    let _ = self.small_pool.push(new_buf);
                }
            }
            Buffer::Medium(buf) => {
                if buf.capacity() == self.config.medium_size {
                    let mut new_buf = vec![0; self.config.medium_size];
                    new_buf.clear();
                    let _ = self.medium_pool.push(new_buf);
                }
            }
            Buffer::Large(_) => {} // Handled by Drop trait
        }
    }

    /// Releases a large buffer by ID
    pub fn release_large_buffer(&self, id: usize) {
        if let Ok(mut maps) = self.large_maps.write() {
            maps.remove(&id);
        }
    }
}

impl Clone for BufferPool {
    fn clone(&self) -> Self {
        Self {
            small_pool: ArrayQueue::new(self.config.small_count),
            medium_pool: ArrayQueue::new(self.config.medium_count),
            large_maps: RwLock::new(HashMap::new()),
            config: self.config.clone(),
            next_map_id: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_cleanup() {
        let pool = BufferPool::new(Default::default()).unwrap();
        let buffer = pool.acquire(1024 * 1024 * 10).unwrap(); // 10MB
        assert!(matches!(buffer, Buffer::Large(_)));
        drop(buffer); // Should properly clean up
        assert!(pool.large_maps.read().unwrap().is_empty());
    }

    #[test]
    fn test_buffer_validation() {
        let pool = BufferPool::new(Default::default()).unwrap();

        // Test small buffer
        let small = pool.acquire(1024).unwrap();
        assert!(small.validate_memory().is_ok());

        // Test medium buffer
        let medium = pool.acquire(100_000).unwrap();
        assert!(medium.validate_memory().is_ok());

        // Test large buffer
        let large = pool.acquire(1024 * 1024 * 10).unwrap();
        assert!(large.validate_memory().is_ok());
    }

    #[test]
    fn test_buffer_reuse() {
        let pool = BufferPool::new(Default::default()).unwrap();

        // Acquire and release small buffer
        let buf1 = pool.acquire(1024).unwrap();
        assert!(matches!(buf1, Buffer::Small(_)));
        pool.release(&buf1);

        // Should get same buffer back
        let buf2 = pool.acquire(1024).unwrap();
        assert!(matches!(buf2, Buffer::Small(_)));
    }

    #[test]
    fn test_memory_safety() {
        let pool = BufferPool::new(Default::default()).unwrap();

        // Test concurrent access
        let pool = Arc::new(pool);
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let pool = pool.clone();
                std::thread::spawn(move || {
                    for _ in 0..100 {
                        let buffer = pool.acquire(1024).unwrap();
                        assert!(buffer.validate_memory().is_ok());
                        pool.release(&buffer);
                    }
                })
            })
            .collect();

        // Wait for all threads
        for thread in threads {
            thread.join().unwrap();
        }

        // Check pool state
        assert!(pool.large_maps.read().unwrap().is_empty());
    }
}
