use std::{
    sync::{atomic::{AtomicBool, AtomicUsize, Ordering}, Arc},
    collections::HashMap,
    time::Duration,
};
use dashmap::DashMap;
use parking_lot::{Mutex, RwLock, RwLockUpgradableReadGuard, Condvar};
use tokio::sync::Semaphore;

use crate::{
    error::ZipError,
    Result,
};

/// Thread-safe counter with overflow protection
#[derive(Debug)]
pub struct SafeCounter {
    value: Mutex<usize>,
    max: usize,
}

impl SafeCounter {
    pub fn new(max: usize) -> Self {
        Self {
            value: Mutex::new(0),
            max,
        }
    }

    pub fn increment(&self) -> Result<usize> {
        let mut value = self.value.lock();
        if *value >= self.max {
            return Err(ZipError::ThreadSafety("Counter overflow".into()));
        }
        *value += 1;
        Ok(*value)
    }

    pub fn decrement(&self) -> Result<usize> {
        let mut value = self.value.lock();
        if *value == 0 {
            return Err(ZipError::ThreadSafety("Counter underflow".into()));
        }
        *value -= 1;
        Ok(*value)
    }

    pub fn get(&self) -> usize {
        *self.value.lock()
    }
}

/// Thread-safe map with read-write locking
#[derive(Debug)]
pub struct SafeMap<K, V> {
    inner: RwLock<HashMap<K, V>>,
    access_count: AtomicUsize,
}

impl<K: Eq + std::hash::Hash, V> SafeMap<K, V> {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
            access_count: AtomicUsize::new(0),
        }
    }

    pub fn insert(&self, key: K, value: V) -> Option<V> {
        let mut map = self.inner.write();
        self.access_count.fetch_add(1, Ordering::Relaxed);
        map.insert(key, value)
    }

    pub fn get(&self, key: &K) -> Option<V>
    where
        V: Clone,
    {
        let map = self.inner.read();
        self.access_count.fetch_add(1, Ordering::Relaxed);
        map.get(key).cloned()
    }

    pub fn remove(&self, key: &K) -> Option<V> {
        let mut map = self.inner.write();
        self.access_count.fetch_add(1, Ordering::Relaxed);
        map.remove(key)
    }

    pub fn access_count(&self) -> usize {
        self.access_count.load(Ordering::Relaxed)
    }
}

/// Thread-safe queue with capacity limit
#[derive(Debug)]
pub struct SafeQueue<T> {
    inner: Mutex<Vec<T>>,
    capacity: usize,
    semaphore: Arc<Semaphore>,
    notify: Condvar,
}

impl<T> SafeQueue<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(Vec::with_capacity(capacity)),
            capacity,
            semaphore: Arc::new(Semaphore::new(capacity)),
            notify: Condvar::new(),
        }
    }

    pub async fn push(&self, item: T) -> Result<()> {
        self.semaphore.acquire().await
            .map_err(|e| ZipError::ThreadSafety(format!("Failed to acquire semaphore: {}", e)))?;
        
        let mut queue = self.inner.lock();
        if queue.len() >= self.capacity {
            self.semaphore.add_permits(1);
            return Err(ZipError::ThreadSafety("Queue is full".into()));
        }
        
        queue.push(item);
        self.notify.notify_one();
        Ok(())
    }

    pub fn try_pop(&self) -> Option<T> {
        let mut queue = self.inner.lock();
        let item = queue.pop();
        if item.is_some() {
            self.semaphore.add_permits(1);
        }
        item
    }

    pub fn wait_pop(&self, timeout: Duration) -> Result<Option<T>> {
        let mut queue = self.inner.lock();
        let result = self.notify.wait_for(&mut queue, timeout);
        
        if !result.timed_out() {
            Ok(queue.pop())
        } else {
            Ok(None)
        }
    }

    pub fn len(&self) -> usize {
        self.inner.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.lock().is_empty()
    }
}

/// Thread-safe flag with atomic operations
#[derive(Debug)]
pub struct SafeFlag {
    flag: AtomicBool,
    notify: Condvar,
    lock: Mutex<()>,
}

impl SafeFlag {
    pub fn new(initial: bool) -> Self {
        Self {
            flag: AtomicBool::new(initial),
            notify: Condvar::new(),
            lock: Mutex::new(()),
        }
    }

    pub fn set(&self, value: bool) {
        self.flag.store(value, Ordering::SeqCst);
        self.notify.notify_all();
    }

    pub fn get(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    pub fn wait_for(&self, value: bool, timeout: Duration) -> Result<bool> {
        let mut guard = self.lock.lock();
        let deadline = std::time::Instant::now() + timeout;
        
        while self.get() != value {
            let remaining = deadline.checked_duration_since(std::time::Instant::now())
                .ok_or_else(|| ZipError::ThreadSafety("Wait timeout".into()))?;
                
            if !self.notify.wait_for(&mut guard, remaining) {
                return Ok(false);
            }
        }
        
        Ok(true)
    }
}

/// Thread-safe value wrapper
#[derive(Debug)]
pub struct SafeValue<T> {
    inner: RwLock<T>,
}

impl<T> SafeValue<T> {
    pub fn new(value: T) -> Self {
        Self {
            inner: RwLock::new(value),
        }
    }

    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.inner.read().clone()
    }

    pub fn set(&self, value: T) {
        *self.inner.write() = value;
    }
}

impl<T> Clone for SafeValue<T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: RwLock::new(self.inner.read().clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use tokio::runtime::Runtime;

    #[test]
    fn test_safe_counter() {
        let counter = SafeCounter::new(5);
        
        // Test increment
        for i in 1..=5 {
            assert_eq!(counter.increment().unwrap(), i);
        }
        assert!(counter.increment().is_err());
        
        // Test decrement
        for i in (1..=5).rev() {
            assert_eq!(counter.decrement().unwrap(), i - 1);
        }
        assert!(counter.decrement().is_err());
    }

    #[test]
    fn test_safe_map() {
        let map = SafeMap::new();
        
        // Test concurrent access
        let threads: Vec<_> = (0..10).map(|i| {
            let map = &map;
            thread::spawn(move || {
                map.insert(i, i * 2);
                assert_eq!(map.get(&i).unwrap(), i * 2);
            })
        }).collect();
        
        for thread in threads {
            thread.join().unwrap();
        }
        
        assert!(map.access_count() >= 20); // At least one insert and get per thread
    }

    #[tokio::test]
    async fn test_safe_queue() {
        let queue = SafeQueue::new(2);
        
        // Test push
        assert!(queue.push(1).await.is_ok());
        assert!(queue.push(2).await.is_ok());
        assert!(queue.push(3).await.is_err());
        
        // Test pop
        assert_eq!(queue.try_pop(), Some(2));
        assert_eq!(queue.try_pop(), Some(1));
        assert_eq!(queue.try_pop(), None);
    }

    #[test]
    fn test_safe_flag() {
        let flag = SafeFlag::new(false);
        let flag2 = flag.clone();
        
        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            flag2.set(true);
        });
        
        assert!(flag.wait_for(true, Duration::from_secs(1)).unwrap());
        handle.join().unwrap();
    }

    #[test]
    fn test_safe_value() {
        let value = SafeValue::new(42);
        
        // Test get/set
        assert_eq!(value.get(), 42);
        value.set(84);
        assert_eq!(value.get(), 84);
        
        // Test clone
        let cloned = value.clone();
        assert_eq!(cloned.get(), 84);
    }
}

impl<T> Clone for SafeQueue<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Mutex::new(self.inner.lock().clone()),
            capacity: self.capacity,
            semaphore: Arc::clone(&self.semaphore),
            notify: Condvar::new(),
        }
    }
}

impl Clone for SafeFlag {
    fn clone(&self) -> Self {
        Self {
            flag: AtomicBool::new(self.get()),
            notify: Condvar::new(),
            lock: Mutex::new(()),
        }
    }
}
