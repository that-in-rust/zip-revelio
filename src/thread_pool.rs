use std::{
    sync::Arc,
    thread::{self, JoinHandle},
    time::Duration,
};
use crossbeam_channel::{bounded, Receiver, Sender};
use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};

use crate::{
    error::ZipError,
    Result,
};

/// Thread pool state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ThreadState {
    /// Thread is idle
    Idle,
    /// Thread is processing
    Processing,
    /// Thread is shutting down
    ShuttingDown,
}

/// Thread pool metrics
#[derive(Debug, Default)]
pub struct ThreadMetrics {
    /// Tasks completed
    tasks_completed: usize,
    /// Tasks failed
    tasks_failed: usize,
    /// Processing time in milliseconds
    processing_time_ms: u64,
    /// Peak memory usage in bytes
    peak_memory_bytes: u64,
}

/// Thread pool configuration
#[derive(Debug, Clone)]
pub struct ThreadPoolConfig {
    /// Number of threads
    pub thread_count: usize,
    /// Stack size in bytes
    pub stack_size: usize,
    /// Thread name prefix
    pub thread_prefix: String,
    /// Maximum tasks per thread
    pub max_tasks_per_thread: usize,
    /// Task timeout in seconds
    pub task_timeout_secs: u64,
}

impl Default for ThreadPoolConfig {
    fn default() -> Self {
        Self {
            thread_count: num_cpus::get(),
            stack_size: 5 * 1024 * 1024, // 5MB
            thread_prefix: "zip-revelio-worker".to_string(),
            max_tasks_per_thread: 1000,
            task_timeout_secs: 60,
        }
    }
}

/// Message types for thread communication
enum Message {
    /// Task to execute
    Task(Box<dyn FnOnce() -> Result<()> + Send + 'static>),
    /// Shutdown signal
    Shutdown,
}

/// Thread pool for parallel processing
pub struct ThreadPool {
    /// Thread handles
    handles: Vec<JoinHandle<()>>,
    /// Task sender
    task_sender: Sender<Message>,
    /// Thread states
    states: Arc<DashMap<thread::ThreadId, ThreadState>>,
    /// Thread metrics
    metrics: Arc<DashMap<thread::ThreadId, ThreadMetrics>>,
    /// Global metrics
    global_metrics: Arc<Mutex<ThreadMetrics>>,
    /// Pool configuration
    config: ThreadPoolConfig,
    /// Pool state
    running: Arc<RwLock<bool>>,
}

impl ThreadPool {
    /// Creates a new thread pool
    pub fn new(config: ThreadPoolConfig) -> Result<Self> {
        let (task_sender, task_receiver) = bounded(config.thread_count * 2);
        let states = Arc::new(DashMap::new());
        let metrics = Arc::new(DashMap::new());
        let global_metrics = Arc::new(Mutex::new(ThreadMetrics::default()));
        let running = Arc::new(RwLock::new(true));
        let mut handles = Vec::with_capacity(config.thread_count);

        for id in 0..config.thread_count {
            let receiver = task_receiver.clone();
            let states = Arc::clone(&states);
            let metrics = Arc::clone(&metrics);
            let global_metrics = Arc::clone(&global_metrics);
            let running = Arc::clone(&running);
            let thread_prefix = config.thread_prefix.clone();
            let timeout = Duration::from_secs(config.task_timeout_secs);

            let builder = thread::Builder::new()
                .name(format!("{}-{}", thread_prefix, id))
                .stack_size(config.stack_size);

            let handle = builder.spawn(move || {
                let thread_id = thread::current().id();
                states.insert(thread_id, ThreadState::Idle);
                metrics.insert(thread_id, ThreadMetrics::default());

                while *running.read() {
                    match receiver.recv_timeout(timeout) {
                        Ok(Message::Task(task)) => {
                            states.insert(thread_id, ThreadState::Processing);
                            let start = std::time::Instant::now();
                            let result = task();
                            let duration = start.elapsed();

                            // Update thread metrics
                            if let Some(mut metric) = metrics.get_mut(&thread_id) {
                                metric.processing_time_ms += duration.as_millis() as u64;
                                if let Ok(_) = result {
                                    metric.tasks_completed += 1;
                                } else {
                                    metric.tasks_failed += 1;
                                }
                            }

                            // Update global metrics
                            let mut global = global_metrics.lock();
                            global.processing_time_ms += duration.as_millis() as u64;
                            if let Ok(_) = result {
                                global.tasks_completed += 1;
                            } else {
                                global.tasks_failed += 1;
                            }

                            states.insert(thread_id, ThreadState::Idle);
                        }
                        Ok(Message::Shutdown) => {
                            states.insert(thread_id, ThreadState::ShuttingDown);
                            break;
                        }
                        Err(_) => continue,
                    }
                }

                states.remove(&thread_id);
                metrics.remove(&thread_id);
            })?;

            handles.push(handle);
        }

        Ok(Self {
            handles,
            task_sender,
            states,
            metrics,
            global_metrics,
            config,
            running,
        })
    }

    /// Executes a task in the thread pool
    pub fn execute<F>(&self, task: F) -> Result<()>
    where
        F: FnOnce() -> Result<()> + Send + 'static,
    {
        if !*self.running.read() {
            return Err(ZipError::ThreadPool("Thread pool is shutting down".into()));
        }

        self.task_sender
            .send(Message::Task(Box::new(task)))
            .map_err(|e| ZipError::ThreadPool(e.to_string()))?;

        Ok(())
    }

    /// Gets the current thread states
    pub fn thread_states(&self) -> Vec<(thread::ThreadId, ThreadState)> {
        self.states.iter().map(|e| (*e.key(), *e.value())).collect()
    }

    /// Gets thread metrics
    pub fn thread_metrics(&self) -> Vec<(thread::ThreadId, ThreadMetrics)> {
        self.metrics
            .iter()
            .map(|e| (*e.key(), e.value().clone()))
            .collect()
    }

    /// Gets global metrics
    pub fn global_metrics(&self) -> ThreadMetrics {
        self.global_metrics.lock().clone()
    }

    /// Gets the number of active threads
    pub fn active_threads(&self) -> usize {
        self.states
            .iter()
            .filter(|e| *e.value() == ThreadState::Processing)
            .count()
    }

    /// Gets the thread pool configuration
    pub fn config(&self) -> &ThreadPoolConfig {
        &self.config
    }

    /// Shuts down the thread pool
    pub fn shutdown(&self) -> Result<()> {
        *self.running.write() = false;

        // Send shutdown message to all threads
        for _ in 0..self.config.thread_count {
            self.task_sender
                .send(Message::Shutdown)
                .map_err(|e| ZipError::ThreadPool(e.to_string()))?;
        }

        Ok(())
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        let _ = self.shutdown();
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_thread_pool_creation() {
        let config = ThreadPoolConfig {
            thread_count: 4,
            ..Default::default()
        };
        let pool = ThreadPool::new(config).unwrap();
        assert_eq!(pool.thread_states().len(), 4);
        assert!(pool.active_threads() == 0);
    }

    #[test]
    fn test_task_execution() {
        let pool = ThreadPool::new(ThreadPoolConfig::default()).unwrap();
        let counter = Arc::new(Mutex::new(0));

        for _ in 0..10 {
            let counter = Arc::clone(&counter);
            pool.execute(move || {
                *counter.lock() += 1;
                Ok(())
            })
            .unwrap();
        }

        // Wait for tasks to complete
        thread::sleep(Duration::from_millis(100));

        assert_eq!(*counter.lock(), 10);
        let metrics = pool.global_metrics();
        assert_eq!(metrics.tasks_completed, 10);
        assert_eq!(metrics.tasks_failed, 0);
    }

    #[test]
    fn test_error_handling() {
        let pool = ThreadPool::new(ThreadPoolConfig::default()).unwrap();
        let counter = Arc::new(Mutex::new(0));

        // Execute tasks that will succeed and fail
        for i in 0..10 {
            let counter = Arc::clone(&counter);
            pool.execute(move || {
                if i % 2 == 0 {
                    *counter.lock() += 1;
                    Ok(())
                } else {
                    Err(ZipError::ThreadPool("Test error".into()))
                }
            })
            .unwrap();
        }

        // Wait for tasks to complete
        thread::sleep(Duration::from_millis(100));

        assert_eq!(*counter.lock(), 5);
        let metrics = pool.global_metrics();
        assert_eq!(metrics.tasks_completed, 5);
        assert_eq!(metrics.tasks_failed, 5);
    }

    #[test]
    fn test_shutdown() {
        let pool = ThreadPool::new(ThreadPoolConfig::default()).unwrap();
        pool.shutdown().unwrap();

        // Try to execute task after shutdown
        let result = pool.execute(|| Ok(()));
        assert!(result.is_err());
    }

    #[test]
    fn test_thread_metrics() {
        let pool = ThreadPool::new(ThreadPoolConfig::default()).unwrap();
        let counter = Arc::new(Mutex::new(0));

        // Execute some tasks
        for _ in 0..5 {
            let counter = Arc::clone(&counter);
            pool.execute(move || {
                thread::sleep(Duration::from_millis(10));
                *counter.lock() += 1;
                Ok(())
            })
            .unwrap();
        }

        // Wait for tasks to complete
        thread::sleep(Duration::from_millis(100));

        let metrics = pool.thread_metrics();
        assert!(!metrics.is_empty());
        
        // Check that all threads have metrics
        let total_tasks: usize = metrics.iter()
            .map(|(_, m)| m.tasks_completed)
            .sum();
        assert_eq!(total_tasks, 5);
    }
}
