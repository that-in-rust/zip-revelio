use crate::buffer::{Buffer, BufferPool};
use crate::zip::{ZipEntry, ZipError};
use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug)]
pub struct Task {
    entry: Arc<ZipEntry>,
    buffer: Buffer,
    buffer_pool: Arc<BufferPool>,
    processed: bool,
    is_valid: AtomicBool,
}

impl Task {
    pub fn new(entry: Arc<ZipEntry>, buffer: Buffer, buffer_pool: Arc<BufferPool>) -> Self {
        Self {
            entry,
            buffer,
            buffer_pool,
            processed: false,
            is_valid: AtomicBool::new(true),
        }
    }

    pub fn entry(&self) -> &ZipEntry {
        &self.entry
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    pub fn is_processed(&self) -> bool {
        self.processed
    }

    /// Validates that the task resources are still valid
    fn validate(&self) -> Result<(), ZipError> {
        if !self.is_valid.load(Ordering::Acquire) {
            return Err(ZipError::Memory("Task is no longer valid".into()));
        }
        self.buffer.validate_memory()
    }

    pub async fn process(&mut self) -> Result<&Self, ZipError> {
        // Validate resources before processing
        self.validate()?;

        // Process the entry
        self.entry.process(&mut self.buffer).await?;
        self.processed = true;

        Ok(self)
    }
}

impl Drop for Task {
    fn drop(&mut self) {
        // Mark as invalid first to prevent concurrent access
        self.is_valid.store(false, Ordering::Release);

        // Release buffer back to pool
        match &self.buffer {
            Buffer::Small(_) | Buffer::Medium(_) => {
                // Release buffer back to pool if not processed
                if !self.processed {
                    self.buffer_pool.release(self.buffer.clone());
                }
            }
            Buffer::Large(id) => {
                // Always release large buffers
                self.buffer_pool.release_large_buffer(*id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::pool::BufferConfig;

    fn create_test_entry(size: usize) -> ZipEntry {
        ZipEntry {
            header: LocalFileHeader {
                version_needed: 0,
                flags: 0,
                compression_method: 0,
                last_mod_time: 0,
                last_mod_date: 0,
                crc32: 0,
                compressed_size: 0,
                uncompressed_size: size as u64,
                file_name: "test.txt".into(),
                extra_field: vec![],
            },
            data_descriptor: None,
            file_data: vec![0; size],
        }
    }

    #[test]
    fn test_task_cleanup() -> Result<()> {
        let pool = Arc::new(BufferPool::new(BufferConfig {
            small_size: 1024,
            medium_size: 1024 * 1024,
            small_count: 1,
            medium_count: 1,
        })?);

        // Create large buffer that should be cleaned up
        let buffer = pool
            .acquire(1024 * 1024 * 10)
            .map_err(|e| anyhow::anyhow!("Failed to acquire buffer: {}", e))?;
        assert!(matches!(buffer, Buffer::Large(_)));

        let entry = Arc::new(create_test_entry(1024 * 1024 * 10));
        let task = Task::new(entry, buffer, pool.clone());

        // Verify task is valid
        task.validate()
            .map_err(|e| anyhow::anyhow!("Task validation failed: {}", e))?;

        // Drop should clean up
        drop(task);

        // Verify cleanup
        assert!(pool
            .large_maps
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock"))?
            .is_empty());

        Ok(())
    }

    #[test]
    fn test_task_validation() -> Result<()> {
        let pool = Arc::new(BufferPool::new(Default::default())?);
        let buffer = pool
            .acquire(1024)
            .map_err(|e| anyhow::anyhow!("Failed to acquire buffer: {}", e))?;

        let entry = Arc::new(create_test_entry(1024));
        let mut task = Task::new(entry, buffer, pool);

        // Should validate before processing
        task.validate()
            .map_err(|e| anyhow::anyhow!("Task validation failed: {}", e))?;

        // Process should succeed
        let result = futures::executor::block_on(task.process())
            .map_err(|e| anyhow::anyhow!("Task processing failed: {}", e))?;

        assert!(result.is_processed());
        Ok(())
    }

    #[test]
    fn test_task_creation() -> Result<()> {
        let pool = Arc::new(BufferPool::new(Default::default())?);
        let buffer = pool
            .acquire(1024)
            .map_err(|e| anyhow::anyhow!("Failed to acquire buffer: {}", e))?;

        let entry = Arc::new(create_test_entry(1024));
        let task = Task::new(entry, buffer, pool);

        assert!(!task.is_processed());
        task.validate()
            .map_err(|e| anyhow::anyhow!("Task validation failed: {}", e))?;

        Ok(())
    }

    #[test]
    fn test_concurrent_access() -> Result<()> {
        use std::thread;

        let pool = Arc::new(BufferPool::new(Default::default())?);
        let buffer = pool
            .acquire(1024)
            .map_err(|e| anyhow::anyhow!("Failed to acquire buffer: {}", e))?;

        let entry = Arc::new(create_test_entry(1024));
        let task = Arc::new(Task::new(entry, buffer, pool));

        // Spawn threads to access task
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let task = Arc::clone(&task);
                thread::spawn(move || {
                    task.validate()
                        .map_err(|e| anyhow::anyhow!("Task validation failed: {}", e))
                })
            })
            .collect();

        // Wait for all threads
        for thread in threads {
            thread
                .join()
                .map_err(|_| anyhow::anyhow!("Thread panicked"))?
                .map_err(|e| anyhow::anyhow!("Thread error: {}", e))?;
        }

        Ok(())
    }
}
