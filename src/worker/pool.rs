use super::task::Task;
use crate::buffer::{pool::BufferConfig, BufferPool};
use crate::utils::Progress;
use crate::zip::{ZipEntry, ZipError};
use parking_lot::Mutex;
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
use std::sync::Arc;

// Maximum number of threads to prevent resource exhaustion
const MAX_THREADS: usize = 16;

#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub thread_count: Option<usize>,
    pub stack_size: usize,
    pub buffer_config: BufferConfig,
    pub total_size: u64,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            thread_count: None,
            stack_size: 8 * 1024 * 1024, // 8MB stack
            buffer_config: BufferConfig::default(),
            total_size: 0,
        }
    }
}

pub struct WorkerPool {
    pool: rayon::ThreadPool,
    progress: Arc<Progress>,
}

impl WorkerPool {
    pub fn new(config: WorkerConfig) -> Result<Self, ZipError> {
        // Calculate optimal thread count
        let thread_count = config.thread_count.unwrap_or_else(|| {
            std::cmp::min(
                num_cpus::get(),
                MAX_THREADS, // Maximum threads
            )
        });

        // Create thread pool with safety limits
        let pool = ThreadPoolBuilder::new()
            .num_threads(thread_count)
            .stack_size(config.stack_size)
            .thread_name(|i| format!("worker-{}", i))
            .panic_handler(|p| {
                eprintln!("Thread panic: {:?}", p);
            })
            .build()
            .map_err(|e| ZipError::Memory(e.to_string()))?;

        // Initialize progress tracking
        let progress =
            Progress::new(config.total_size).map_err(|e| ZipError::Memory(e.to_string()))?;

        Ok(Self {
            pool,
            progress: Arc::new(progress),
        })
    }

    pub async fn process_entries(
        &self,
        entries: Vec<Arc<ZipEntry>>,
        buffer_pool: Arc<BufferPool>,
    ) -> Result<Vec<Task>, ZipError> {
        // Initialize thread-safe collections
        let results = Arc::new(Mutex::new(Vec::with_capacity(entries.len())));
        let errors = Arc::new(Mutex::new(Vec::new()));

        // Process entries in parallel with work stealing
        self.pool.install(|| {
            entries.par_iter().try_for_each(|entry| {
                // Acquire buffer for processing
                let buffer = buffer_pool
                    .acquire(entry.header.uncompressed_size as usize)
                    .map_err(|e| ZipError::Memory(e.to_string()))?;

                // Create and process task
                let mut task = Task::new(entry.clone(), buffer, buffer_pool.clone());
                match futures::executor::block_on(task.process()) {
                    Ok(processed_task) => {
                        // Store results safely
                        results.lock().push(processed_task.clone());
                        self.progress
                            .update(entry.header.compressed_size)
                            .map_err(|e| ZipError::Memory(e.to_string()))?;
                        Ok::<(), ZipError>(())
                    }
                    Err(e) => {
                        // Track errors safely
                        errors.lock().push((entry.clone(), e));
                        Ok::<(), ZipError>(())
                    }
                }
            })
        })?;

        // Check for errors
        let error_count = errors.lock().len();
        if error_count > 0 {
            return Err(ZipError::Format(format!(
                "Failed to process {} entries",
                error_count
            )));
        }

        // Return processed tasks
        Ok(Arc::try_unwrap(results)
            .map_err(|_| ZipError::Memory("Results lock still has multiple owners".into()))?
            .into_inner()
            .into_iter()
            .map(|task| task.to_owned())
            .collect())
    }

    /// Returns the current thread count
    pub fn thread_count(&self) -> usize {
        self.pool.current_num_threads()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_thread_pool_limits() {
        // Try to create pool with excessive threads
        let config = WorkerConfig {
            thread_count: Some(100),
            ..Default::default()
        };
        let pool = WorkerPool::new(config).unwrap();

        // Should be limited to MAX_THREADS
        assert!(pool.thread_count() <= MAX_THREADS);
    }

    #[test]
    fn test_parallel_processing() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        // Create pool with default config
        let pool = WorkerPool::new(Default::default()).unwrap();

        // Track parallel execution
        let counter = Arc::new(AtomicUsize::new(0));
        let max_parallel = Arc::new(AtomicUsize::new(0));

        // Run parallel tasks
        pool.pool.install(|| {
            (0..100).into_par_iter().for_each(|_| {
                let current = counter.fetch_add(1, Ordering::SeqCst);
                let mut max = max_parallel.load(Ordering::Relaxed);
                while current > max {
                    max_parallel
                        .compare_exchange(max, current, Ordering::SeqCst, Ordering::Relaxed)
                        .unwrap_or(max);
                    max = max_parallel.load(Ordering::Relaxed);
                }
                std::thread::sleep(Duration::from_millis(10));
                counter.fetch_sub(1, Ordering::SeqCst);
            });
        });

        // Should have executed in parallel
        assert!(max_parallel.load(Ordering::SeqCst) > 1);
        assert!(max_parallel.load(Ordering::SeqCst) <= MAX_THREADS);
    }

    #[test]
    fn test_stack_size() {
        // Create pool with custom stack size
        let config = WorkerConfig {
            stack_size: 16 * 1024 * 1024, // 16MB
            ..Default::default()
        };
        let pool = WorkerPool::new(config).unwrap();

        // Run recursive function to test stack
        pool.pool.install(|| {
            fn recursive(n: usize) -> usize {
                if n == 0 {
                    return 0;
                }
                let mut buf = [0u8; 1024]; // Use some stack
                buf[0] = n as u8;
                1 + recursive(n - 1)
            }

            // Should not stack overflow
            assert_eq!(recursive(1000), 1000);
        });
    }
}
