use std::{
    alloc::{self, Layout},
    mem,
    ops::{Deref, DerefMut},
    ptr::NonNull,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::{
    error::ZipError,
    Result,
};

/// Memory allocation tracker
#[derive(Debug)]
pub struct MemoryTracker {
    /// Current allocation size
    current_size: AtomicUsize,
    /// Peak allocation size
    peak_size: AtomicUsize,
    /// Maximum allowed size
    max_size: usize,
}

impl MemoryTracker {
    /// Creates a new memory tracker
    pub fn new(max_size: usize) -> Self {
        Self {
            current_size: AtomicUsize::new(0),
            peak_size: AtomicUsize::new(0),
            max_size,
        }
    }

    /// Tracks memory allocation
    pub fn track_alloc(&self, size: usize) -> Result<()> {
        let current = self.current_size.fetch_add(size, Ordering::SeqCst);
        if current + size > self.max_size {
            self.current_size.fetch_sub(size, Ordering::SeqCst);
            return Err(ZipError::MemoryLimit(format!(
                "Memory limit exceeded: {} + {} > {}",
                current, size, self.max_size
            )));
        }

        let peak = self.peak_size.load(Ordering::Relaxed);
        if current + size > peak {
            self.peak_size.store(current + size, Ordering::Relaxed);
        }

        Ok(())
    }

    /// Tracks memory deallocation
    pub fn track_dealloc(&self, size: usize) {
        self.current_size.fetch_sub(size, Ordering::SeqCst);
    }

    /// Gets current allocation size
    pub fn current_size(&self) -> usize {
        self.current_size.load(Ordering::Relaxed)
    }

    /// Gets peak allocation size
    pub fn peak_size(&self) -> usize {
        self.peak_size.load(Ordering::Relaxed)
    }
}

/// Memory guard for safe allocation
#[derive(Debug)]
pub struct MemoryGuard<T> {
    /// Pointer to allocated memory
    ptr: NonNull<T>,
    /// Layout of allocation
    layout: Layout,
    /// Memory tracker
    tracker: Option<&'static MemoryTracker>,
}

impl<T> MemoryGuard<T> {
    /// Creates a new memory guard
    pub fn new(value: T, tracker: Option<&'static MemoryTracker>) -> Result<Self> {
        let layout = Layout::new::<T>();
        
        if let Some(tracker) = tracker {
            tracker.track_alloc(layout.size())?;
        }

        let ptr = NonNull::new(unsafe {
            let ptr = alloc::alloc(layout) as *mut T;
            ptr.write(value);
            ptr
        })
        .ok_or_else(|| ZipError::Memory("Failed to allocate memory".into()))?;

        Ok(Self {
            ptr,
            layout,
            tracker,
        })
    }

    /// Gets the underlying pointer
    pub fn as_ptr(&self) -> *const T {
        self.ptr.as_ptr()
    }

    /// Gets the underlying mutable pointer
    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.ptr.as_ptr()
    }
}

impl<T> Deref for MemoryGuard<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.ptr.as_ref() }
    }
}

impl<T> DerefMut for MemoryGuard<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.ptr.as_mut() }
    }
}

impl<T> Drop for MemoryGuard<T> {
    fn drop(&mut self) {
        unsafe {
            let _ = Box::from_raw(self.ptr.as_ptr());
        }
        
        if let Some(tracker) = self.tracker {
            tracker.track_dealloc(self.layout.size());
        }
    }
}

/// Safe slice for memory-safe operations
#[derive(Debug)]
pub struct SafeSlice<T> {
    /// Pointer to data
    ptr: NonNull<T>,
    /// Length of slice
    len: usize,
    /// Memory tracker
    tracker: Option<&'static MemoryTracker>,
}

impl<T> SafeSlice<T> {
    /// Creates a new safe slice
    pub fn new(data: Vec<T>, tracker: Option<&'static MemoryTracker>) -> Result<Self> {
        let len = data.len();
        let layout = Layout::array::<T>(len)
            .map_err(|e| ZipError::Memory(format!("Invalid layout: {}", e)))?;

        if let Some(tracker) = tracker {
            tracker.track_alloc(layout.size())?;
        }

        let ptr = NonNull::new(Box::into_raw(data.into_boxed_slice()) as *mut T)
            .ok_or_else(|| ZipError::Memory("Failed to allocate slice".into()))?;

        Ok(Self {
            ptr,
            len,
            tracker,
        })
    }

    /// Gets slice length
    pub fn len(&self) -> usize {
        self.len
    }

    /// Checks if slice is empty
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Gets slice as raw pointer
    pub fn as_ptr(&self) -> *const T {
        self.ptr.as_ptr()
    }

    /// Gets slice as mutable raw pointer
    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.ptr.as_ptr()
    }
}

impl<T> Deref for SafeSlice<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl<T> DerefMut for SafeSlice<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl<T> Drop for SafeSlice<T> {
    fn drop(&mut self) {
        unsafe {
            let layout = Layout::array::<T>(self.len).unwrap();
            let _ = Vec::from_raw_parts(self.ptr.as_ptr(), self.len, self.len);
            
            if let Some(tracker) = self.tracker {
                tracker.track_dealloc(layout.size());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_memory_tracker() {
        let tracker = MemoryTracker::new(1024);
        
        // Track allocation
        tracker.track_alloc(512).unwrap();
        assert_eq!(tracker.current_size(), 512);
        assert_eq!(tracker.peak_size(), 512);
        
        // Track another allocation
        tracker.track_alloc(256).unwrap();
        assert_eq!(tracker.current_size(), 768);
        assert_eq!(tracker.peak_size(), 768);
        
        // Track deallocation
        tracker.track_dealloc(512);
        assert_eq!(tracker.current_size(), 256);
        assert_eq!(tracker.peak_size(), 768);
        
        // Try to exceed limit
        assert!(tracker.track_alloc(1024).is_err());
    }

    #[test]
    fn test_memory_guard() {
        let tracker = Box::leak(Box::new(MemoryTracker::new(1024)));
        
        // Create guarded value
        let guard = MemoryGuard::new(42, Some(tracker)).unwrap();
        assert_eq!(*guard, 42);
        
        // Modify value
        let mut guard = MemoryGuard::new(vec![1, 2, 3], Some(tracker)).unwrap();
        guard.push(4);
        assert_eq!(&*guard, &[1, 2, 3, 4]);
        
        // Check tracking
        assert!(tracker.current_size() > 0);
        
        // Drop guard
        drop(guard);
        assert_eq!(tracker.current_size(), 0);
    }

    #[test]
    fn test_safe_slice() {
        let tracker = Box::leak(Box::new(MemoryTracker::new(1024)));
        
        // Create safe slice
        let data = vec![1, 2, 3, 4, 5];
        let slice = SafeSlice::new(data, Some(tracker)).unwrap();
        assert_eq!(slice.len(), 5);
        assert_eq!(&*slice, &[1, 2, 3, 4, 5]);
        
        // Modify slice
        let mut slice = SafeSlice::new(vec![1, 2, 3], Some(tracker)).unwrap();
        slice[1] = 42;
        assert_eq!(&*slice, &[1, 42, 3]);
        
        // Check tracking
        assert!(tracker.current_size() > 0);
        
        // Drop slice
        drop(slice);
        assert_eq!(tracker.current_size(), 0);
    }

    #[test]
    fn test_memory_limits() {
        let tracker = Box::leak(Box::new(MemoryTracker::new(16)));
        
        // Try to allocate more than limit
        let result = SafeSlice::new(vec![1; 100], Some(tracker));
        assert!(result.is_err());
        
        // Small allocation should succeed
        let slice = SafeSlice::new(vec![1, 2], Some(tracker)).unwrap();
        assert_eq!(slice.len(), 2);
        
        // Another large allocation should fail
        let result = SafeSlice::new(vec![1; 100], Some(tracker));
        assert!(result.is_err());
    }

    #[test]
    fn test_concurrent_access() {
        use std::thread;
        
        let tracker = Arc::new(MemoryTracker::new(1024));
        let tracker_ref = Box::leak(Box::new(tracker.as_ref()));
        let mut handles = vec![];
        
        for _ in 0..4 {
            let handle = thread::spawn(move || {
                let _guard = MemoryGuard::new(vec![1; 100], Some(tracker_ref)).unwrap();
                thread::sleep(std::time::Duration::from_millis(10));
            });
            handles.push(handle);
        }
        
        for handle in handles {
            handle.join().unwrap();
        }
        
        assert_eq!(tracker.current_size(), 0);
        assert!(tracker.peak_size() > 0);
    }
}
