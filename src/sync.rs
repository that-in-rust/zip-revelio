use std::{
    sync::{Arc, atomic::{AtomicBool, AtomicUsize, Ordering}},
    time::{Duration, Instant},
};
use crossbeam_channel::{bounded, Receiver, Sender, TryRecvError};
use dashmap::DashMap;
use parking_lot::{Mutex, RwLock, Condvar};
use tokio::sync::Semaphore;

use crate::{
    error::ZipError,
    Result,
};

/// Barrier for synchronizing multiple threads
pub struct Barrier {
    /// Number of threads to wait for
    count: AtomicUsize,
    /// Total number of threads
    total: usize,
    /// Mutex for synchronization
    mutex: Mutex<()>,
    /// Condition variable for waiting
    condvar: Condvar,
    /// Generation counter
    generation: AtomicUsize,
}

impl Barrier {
    /// Creates a new barrier
    pub fn new(count: usize) -> Self {
        Self {
            count: AtomicUsize::new(count),
            total: count,
            mutex: Mutex::new(()),
            condvar: Condvar::new(),
            generation: AtomicUsize::new(0),
        }
    }

    /// Waits for all threads to reach the barrier
    pub fn wait(&self) {
        let mut lock = self.mutex.lock();
        let gen = self.generation.load(Ordering::Relaxed);
        
        if self.count.fetch_sub(1, Ordering::SeqCst) == 1 {
            // Last thread to arrive
            self.count.store(self.total, Ordering::SeqCst);
            self.generation.fetch_add(1, Ordering::SeqCst);
            self.condvar.notify_all();
        } else {
            // Wait for other threads
            while gen == self.generation.load(Ordering::Relaxed) {
                self.condvar.wait(&mut lock);
            }
        }
    }
}

/// Resource pool for managing shared resources
pub struct ResourcePool<T> {
    /// Available resources
    resources: Mutex<Vec<T>>,
    /// Resource availability condition
    available: Condvar,
    /// Maximum pool size
    max_size: usize,
    /// Current size
    size: AtomicUsize,
}

impl<T> ResourcePool<T> {
    /// Creates a new resource pool
    pub fn new(max_size: usize) -> Self {
        Self {
            resources: Mutex::new(Vec::with_capacity(max_size)),
            available: Condvar::new(),
            max_size,
            size: AtomicUsize::new(0),
        }
    }

    /// Acquires a resource
    pub fn acquire(&self) -> Option<T> {
        let mut resources = self.resources.lock();
        while resources.is_empty() {
            if self.size.load(Ordering::Relaxed) < self.max_size {
                return None;
            }
            self.available.wait(&mut resources);
        }
        resources.pop()
    }

    /// Releases a resource back to the pool
    pub fn release(&self, resource: T) {
        let mut resources = self.resources.lock();
        resources.push(resource);
        self.available.notify_one();
    }

    /// Creates and adds a new resource
    pub fn add(&self, resource: T) -> Result<()> {
        if self.size.load(Ordering::Relaxed) >= self.max_size {
            return Err(ZipError::ResourceLimit("Pool is at capacity".into()));
        }
        
        let mut resources = self.resources.lock();
        resources.push(resource);
        self.size.fetch_add(1, Ordering::Relaxed);
        self.available.notify_one();
        Ok(())
    }
}

/// Shared state for coordinating work
pub struct SharedState {
    /// Active workers count
    active_workers: AtomicUsize,
    /// Processing complete flag
    complete: AtomicBool,
    /// Shared data
    data: DashMap<String, Vec<u8>>,
    /// Error channel
    error_tx: Sender<ZipError>,
    error_rx: Receiver<ZipError>,
    /// Resource semaphore
    semaphore: Arc<Semaphore>,
}

impl SharedState {
    /// Creates new shared state
    pub fn new(max_concurrent: usize) -> Self {
        let (error_tx, error_rx) = bounded(1000);
        Self {
            active_workers: AtomicUsize::new(0),
            complete: AtomicBool::new(false),
            data: DashMap::new(),
            error_tx,
            error_rx,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        }
    }

    /// Increments active worker count
    pub fn increment_workers(&self) {
        self.active_workers.fetch_add(1, Ordering::SeqCst);
    }

    /// Decrements active worker count
    pub fn decrement_workers(&self) {
        self.active_workers.fetch_sub(1, Ordering::SeqCst);
    }

    /// Gets active worker count
    pub fn active_workers(&self) -> usize {
        self.active_workers.load(Ordering::SeqCst)
    }

    /// Sets completion status
    pub fn set_complete(&self) {
        self.complete.store(true, Ordering::SeqCst);
    }

    /// Checks if processing is complete
    pub fn is_complete(&self) -> bool {
        self.complete.load(Ordering::SeqCst)
    }

    /// Stores data
    pub fn store_data(&self, key: String, value: Vec<u8>) {
        self.data.insert(key, value);
    }

    /// Retrieves data
    pub fn get_data(&self, key: &str) -> Option<Vec<u8>> {
        self.data.get(key).map(|v| v.clone())
    }

    /// Records an error
    pub fn record_error(&self, error: ZipError) -> Result<()> {
        self.error_tx
            .send(error)
            .map_err(|e| ZipError::Channel(e.to_string()))
    }

    /// Collects all recorded errors
    pub fn collect_errors(&self) -> Vec<ZipError> {
        let mut errors = Vec::new();
        while let Ok(error) = self.error_rx.try_recv() {
            errors.push(error);
        }
        errors
    }

    /// Acquires a permit from the semaphore
    pub async fn acquire_permit(&self) -> Result<()> {
        self.semaphore
            .acquire()
            .await
            .map_err(|e| ZipError::Semaphore(e.to_string()))?;
        Ok(())
    }

    /// Releases a permit back to the semaphore
    pub fn release_permit(&self) {
        self.semaphore.add_permits(1);
    }
}

/// Rate limiter for controlling operation frequency
pub struct RateLimiter {
    /// Time between operations
    interval: Duration,
    /// Last operation timestamp
    last_op: RwLock<Instant>,
}

impl RateLimiter {
    /// Creates a new rate limiter
    pub fn new(ops_per_second: u32) -> Self {
        Self {
            interval: Duration::from_secs(1) / ops_per_second,
            last_op: RwLock::new(Instant::now()),
        }
    }

    /// Waits until operation is allowed
    pub fn wait(&self) {
        let mut last_op = self.last_op.write();
        let now = Instant::now();
        let elapsed = now.duration_since(*last_op);
        
        if elapsed < self.interval {
            std::thread::sleep(self.interval - elapsed);
        }
        
        *last_op = Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_barrier() {
        let barrier = Arc::new(Barrier::new(3));
        let mut handles = Vec::new();

        for _ in 0..3 {
            let barrier = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                barrier.wait();
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_resource_pool() {
        let pool = ResourcePool::new(2);
        
        // Add resources
        pool.add(1).unwrap();
        pool.add(2).unwrap();
        
        // Acquire and release
        let resource = pool.acquire().unwrap();
        assert!(resource == 2);
        pool.release(resource);
        
        // Try to add beyond capacity
        assert!(pool.add(3).is_err());
    }

    #[test]
    fn test_shared_state() {
        let state = SharedState::new(10);
        
        // Test worker counting
        state.increment_workers();
        assert_eq!(state.active_workers(), 1);
        state.decrement_workers();
        assert_eq!(state.active_workers(), 0);
        
        // Test data storage
        state.store_data("test".into(), vec![1, 2, 3]);
        assert_eq!(state.get_data("test").unwrap(), vec![1, 2, 3]);
        
        // Test error handling
        state.record_error(ZipError::InvalidSignature(0)).unwrap();
        assert_eq!(state.collect_errors().len(), 1);
    }

    #[test]
    fn test_rate_limiter() {
        let limiter = RateLimiter::new(10); // 10 ops per second
        let start = Instant::now();
        
        // Perform 5 operations
        for _ in 0..5 {
            limiter.wait();
        }
        
        // Should take at least 400ms
        assert!(start.elapsed() >= Duration::from_millis(400));
    }

    #[tokio::test]
    async fn test_semaphore() {
        let state = SharedState::new(2);
        
        // Acquire permits
        state.acquire_permit().await.unwrap();
        state.acquire_permit().await.unwrap();
        
        // Third acquire should timeout
        let acquire_future = state.acquire_permit();
        tokio::time::timeout(Duration::from_millis(100), acquire_future)
            .await
            .unwrap_err();
        
        // Release permit
        state.release_permit();
        
        // Should be able to acquire again
        state.acquire_permit().await.unwrap();
    }
}
