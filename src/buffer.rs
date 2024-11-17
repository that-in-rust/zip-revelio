use std::sync::Arc;
use bytes::{BytesMut, BufMut};
use dashmap::DashMap;
use crate::{
    error::ZipError,
    Result,
    constants::*,
};

/// Thread-safe buffer pool for efficient memory reuse
pub struct BufferPool {
    /// Available buffers
    buffers: DashMap<usize, Vec<BytesMut>>,
    /// Current allocation size
    total_size: std::sync::atomic::AtomicUsize,
    /// Maximum pool size
    max_size: usize,
}

impl BufferPool {
    /// Creates a new buffer pool with specified maximum size
    pub fn new(max_size: usize) -> Self {
        Self {
            buffers: DashMap::new(),
            total_size: std::sync::atomic::AtomicUsize::new(0),
            max_size,
        }
    }

    /// Gets a buffer of specified size from the pool or creates a new one
    pub fn get_buffer(&self, size: usize) -> Result<BytesMut> {
        // Round up to nearest power of 2 for better reuse
        let aligned_size = size.next_power_of_two();
        
        // Try to get existing buffer
        if let Some(mut buffers) = self.buffers.get_mut(&aligned_size) {
            if let Some(mut buffer) = buffers.pop() {
                buffer.clear();
                return Ok(buffer);
            }
        }

        // Check if we can allocate more memory
        let current_size = self.total_size.load(std::sync::atomic::Ordering::Relaxed);
        if current_size + aligned_size > self.max_size {
            return Err(ZipError::Memory(format!(
                "Buffer pool size limit reached: {} + {} > {}",
                current_size, aligned_size, self.max_size
            )));
        }

        // Create new buffer
        self.total_size.fetch_add(aligned_size, std::sync::atomic::Ordering::Relaxed);
        Ok(BytesMut::with_capacity(aligned_size))
    }

    /// Returns a buffer to the pool
    pub fn return_buffer(&self, mut buffer: BytesMut) {
        let size = buffer.capacity();
        buffer.clear();
        
        if let Some(mut buffers) = self.buffers.get_mut(&size) {
            buffers.push(buffer);
        } else {
            self.buffers.insert(size, vec![buffer]);
        }
    }
}

impl Drop for BufferPool {
    fn drop(&mut self) {
        // Clear all buffers
        self.buffers.clear();
        self.total_size.store(0, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Thread-safe reference to a buffer from the pool
pub struct PooledBuffer {
    buffer: BytesMut,
    pool: Arc<BufferPool>,
}

impl PooledBuffer {
    /// Creates a new pooled buffer
    pub fn new(buffer: BytesMut, pool: Arc<BufferPool>) -> Self {
        Self { buffer, pool }
    }

    /// Gets a reference to the underlying buffer
    pub fn as_bytes(&self) -> &[u8] {
        &self.buffer
    }

    /// Gets a mutable reference to the underlying buffer
    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        &mut self.buffer
    }

    /// Gets the capacity of the buffer
    pub fn capacity(&self) -> usize {
        self.buffer.capacity()
    }

    /// Clears the buffer contents
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

impl Drop for PooledBuffer {
    fn drop(&mut self) {
        // Return buffer to pool
        let buffer = std::mem::replace(&mut self.buffer, BytesMut::new());
        self.pool.return_buffer(buffer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_pool() {
        let pool = BufferPool::new(1024 * 1024);  // 1MB pool
        
        // Get buffer
        let buffer = pool.get_buffer(1000).unwrap();
        assert!(buffer.capacity() >= 1000);
        
        // Return buffer
        pool.return_buffer(buffer);
        
        // Get same size again
        let buffer2 = pool.get_buffer(1000).unwrap();
        assert!(buffer2.capacity() >= 1000);
    }

    #[test]
    fn test_pool_limit() {
        let pool = BufferPool::new(1024);  // 1KB pool
        
        // Should fail when exceeding pool size
        assert!(pool.get_buffer(2048).is_err());
    }

    #[test]
    fn test_pooled_buffer() {
        let pool = Arc::new(BufferPool::new(1024 * 1024));
        let buffer = pool.get_buffer(1000).unwrap();
        
        let mut pooled = PooledBuffer::new(buffer, Arc::clone(&pool));
        pooled.clear();
        assert_eq!(pooled.as_bytes().len(), 0);
    }
}
