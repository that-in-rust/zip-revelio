use std::{
    alloc::{self, Layout},
    ptr::{self, NonNull},
    sync::{atomic::{AtomicUsize, Ordering}, Arc},
};
use nix::sys::mman::{MapFlags, ProtFlags};

use crate::{
    error::ZipError,
    Result,
};

/// Memory alignment requirements
#[derive(Debug, Clone, Copy)]
pub enum Alignment {
    /// Cache line alignment (typically 64 bytes)
    CacheLine,
    /// Page alignment (typically 4KB)
    Page,
    /// Custom alignment in bytes
    Custom(usize),
}

impl Alignment {
    fn value(&self) -> usize {
        match self {
            Self::CacheLine => 64,
            Self::Page => 4096,
            Self::Custom(n) => *n,
        }
    }
}

/// Memory allocation strategy
#[derive(Debug)]
pub struct AllocStrategy {
    /// Memory alignment
    alignment: Alignment,
    /// Fragmentation threshold
    frag_threshold: f32,
    /// Memory mapping threshold
    mmap_threshold: usize,
}

impl AllocStrategy {
    /// Creates a new allocation strategy
    pub fn new(alignment: Alignment, frag_threshold: f32, mmap_threshold: usize) -> Self {
        Self {
            alignment,
            frag_threshold,
            mmap_threshold,
        }
    }

    /// Checks if memory mapping should be used
    fn should_use_mmap(&self, size: usize) -> bool {
        size >= self.mmap_threshold
    }

    /// Calculates aligned size
    fn align_size(&self, size: usize) -> usize {
        let align = self.alignment.value();
        (size + align - 1) & !(align - 1)
    }
}

/// Memory allocator with fragmentation control
#[derive(Debug)]
pub struct MemoryAllocator {
    /// Allocation strategy
    strategy: AllocStrategy,
    /// Memory tracker
    tracker: Option<&'static MemoryTracker>,
    /// Fragmentation metrics
    fragmentation: Arc<AtomicUsize>,
}

impl MemoryAllocator {
    /// Creates a new memory allocator
    pub fn new(strategy: AllocStrategy, tracker: Option<&'static MemoryTracker>) -> Self {
        Self {
            strategy,
            tracker,
            fragmentation: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Allocates memory with alignment
    pub fn allocate(&self, size: usize) -> Result<NonNull<u8>> {
        let aligned_size = self.strategy.align_size(size);
        
        if let Some(tracker) = self.tracker {
            tracker.track_alloc(aligned_size)?;
        }

        let layout = Layout::from_size_align(aligned_size, self.strategy.alignment.value())
            .map_err(|e| ZipError::Memory(format!("Invalid layout: {}", e)))?;

        if self.strategy.should_use_mmap(aligned_size) {
            // Use memory mapping for large allocations
            self.mmap_alloc(aligned_size)
        } else {
            // Use standard allocation for smaller sizes
            self.standard_alloc(layout)
        }
    }

    /// Allocates using memory mapping
    fn mmap_alloc(&self, size: usize) -> Result<NonNull<u8>> {
        use nix::sys::mman::mmap;
        use std::num::NonZeroUsize;

        unsafe {
            let addr = mmap(
                None,
                NonZeroUsize::new(size).ok_or_else(|| ZipError::Memory("Invalid size".into()))?,
                ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
                MapFlags::MAP_PRIVATE | MapFlags::MAP_ANONYMOUS,
                None,
                0,
            )
            .map_err(|e| ZipError::Memory(format!("mmap failed: {}", e)))?;

            Ok(NonNull::new(addr as *mut u8).unwrap())
        }
    }

    /// Standard memory allocation
    fn standard_alloc(&self, layout: Layout) -> Result<NonNull<u8>> {
        unsafe {
            NonNull::new(alloc::alloc(layout))
                .ok_or_else(|| ZipError::Memory("Allocation failed".into()))
        }
    }

    /// Deallocates memory
    pub unsafe fn deallocate(&self, ptr: NonNull<u8>, size: usize) {
        let aligned_size = self.strategy.align_size(size);
        
        if self.strategy.should_use_mmap(aligned_size) {
            // Unmap memory-mapped allocation
            use nix::sys::mman::munmap;
            let _ = munmap(ptr.as_ptr() as *mut std::ffi::c_void, aligned_size);
        } else {
            // Standard deallocation
            let layout = Layout::from_size_align_unchecked(
                aligned_size,
                self.strategy.alignment.value(),
            );
            alloc::dealloc(ptr.as_ptr(), layout);
        }

        if let Some(tracker) = self.tracker {
            tracker.track_dealloc(aligned_size);
        }
    }

    /// Gets current fragmentation ratio
    pub fn fragmentation_ratio(&self) -> f32 {
        self.fragmentation.load(Ordering::Relaxed) as f32 / 100.0
    }
}

/// Memory guard for safe allocation
#[derive(Debug)]
pub struct MemoryGuard<T> {
    /// Pointer to allocated memory
    ptr: NonNull<T>,
    /// Memory layout
    layout: Layout,
    /// Memory tracker
    tracker: Option<&'static MemoryTracker>,
    /// Memory allocator
    allocator: &'static MemoryAllocator,
}

impl<T> MemoryGuard<T> {
    /// Creates a new memory guard
    pub fn new(value: T, allocator: &'static MemoryAllocator) -> Result<Self> {
        let layout = Layout::new::<T>();
        
        if let Some(tracker) = allocator.tracker {
            tracker.track_alloc(layout.size())?;
        }

        let ptr = unsafe {
            let ptr = allocator.allocate(layout.size())?;
            let typed_ptr = ptr.cast::<T>();
            typed_ptr.as_ptr().write(value);
            typed_ptr
        };

        Ok(Self {
            ptr,
            layout,
            tracker: allocator.tracker,
            allocator,
        })
    }

    /// Gets the underlying pointer
    pub fn as_ptr(&self) -> *const T {
        self.ptr.as_ptr()
    }

    /// Gets a mutable pointer
    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.ptr.as_ptr()
    }
}

impl<T> Drop for MemoryGuard<T> {
    fn drop(&mut self) {
        unsafe {
            ptr::drop_in_place(self.ptr.as_ptr());
            self.allocator.deallocate(self.ptr.cast(), self.layout.size());
        }
    }
}

impl<T> std::ops::Deref for MemoryGuard<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.ptr.as_ref() }
    }
}

impl<T> std::ops::DerefMut for MemoryGuard<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.ptr.as_mut() }
    }
}

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

/// Safe slice for memory-safe operations
#[derive(Debug)]
pub struct SafeSlice<T> {
    /// Pointer to data
    ptr: NonNull<T>,
    /// Length of slice
    len: usize,
    /// Memory tracker
    tracker: Option<&'static MemoryTracker>,
    /// Memory allocator
    allocator: &'static MemoryAllocator,
}

impl<T> SafeSlice<T> {
    /// Creates a new safe slice
    pub fn new(data: Vec<T>, allocator: &'static MemoryAllocator) -> Result<Self> {
        let len = data.len();
        let layout = Layout::array::<T>(len)
            .map_err(|e| ZipError::Memory(format!("Invalid layout: {}", e)))?;

        if let Some(tracker) = allocator.tracker {
            tracker.track_alloc(layout.size())?;
        }

        let ptr = NonNull::new(allocator.allocate(layout.size())?.as_ptr() as *mut T)
            .ok_or_else(|| ZipError::Memory("Failed to allocate slice".into()))?;

        Ok(Self {
            ptr,
            len,
            tracker: allocator.tracker,
            allocator,
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

impl<T> std::ops::Deref for SafeSlice<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl<T> std::ops::DerefMut for SafeSlice<T> {
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
        let allocator = MemoryAllocator::new(AllocStrategy::new(Alignment::CacheLine, 0.5, 1024), Some(tracker));
        
        // Create guarded value
        let guard = MemoryGuard::new(42, &allocator).unwrap();
        assert_eq!(*guard, 42);
        
        // Modify value
        let mut guard = MemoryGuard::new(vec![1, 2, 3], &allocator).unwrap();
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
        let allocator = MemoryAllocator::new(AllocStrategy::new(Alignment::CacheLine, 0.5, 1024), Some(tracker));
        
        // Create safe slice
        let data = vec![1, 2, 3, 4, 5];
        let slice = SafeSlice::new(data, &allocator).unwrap();
        assert_eq!(slice.len(), 5);
        assert_eq!(&*slice, &[1, 2, 3, 4, 5]);
        
        // Modify slice
        let mut slice = SafeSlice::new(vec![1, 2, 3], &allocator).unwrap();
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
        let allocator = MemoryAllocator::new(AllocStrategy::new(Alignment::CacheLine, 0.5, 1024), Some(tracker));
        
        // Try to allocate more than limit
        let result = SafeSlice::new(vec![1; 100], &allocator);
        assert!(result.is_err());
        
        // Small allocation should succeed
        let slice = SafeSlice::new(vec![1, 2], &allocator).unwrap();
        assert_eq!(slice.len(), 2);
        
        // Another large allocation should fail
        let result = SafeSlice::new(vec![1; 100], &allocator);
        assert!(result.is_err());
    }

    #[test]
    fn test_concurrent_access() {
        use std::thread;
        
        let tracker = Arc::new(MemoryTracker::new(1024));
        let tracker_ref = Box::leak(Box::new(tracker.as_ref()));
        let allocator = MemoryAllocator::new(AllocStrategy::new(Alignment::CacheLine, 0.5, 1024), Some(tracker_ref));
        let mut handles = vec![];
        
        for _ in 0..4 {
            let handle = thread::spawn(move || {
                let _guard = MemoryGuard::new(vec![1; 100], &allocator).unwrap();
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
