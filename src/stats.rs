use std::{
    sync::{atomic::{AtomicU64, AtomicUsize, Ordering}, Mutex},
    collections::HashMap,
    time::{Instant, Duration},
};
use dashmap::DashMap;

use crate::{
    CompressionMethod,
    error::ZipError,
};

/// Thread-safe statistics collection
pub struct Stats {
    /// Total number of files processed
    total_files: AtomicUsize,
    /// Total uncompressed size
    total_size: AtomicU64,
    /// Total compressed size
    compressed_size: AtomicU64,
    /// Compression methods used
    methods: DashMap<CompressionMethod, usize>,
    /// Processing errors
    errors: Mutex<Vec<ZipError>>,
    /// Current progress
    current_progress: AtomicUsize,
    /// Total items to process
    total_items: AtomicUsize,
    /// Processing start time
    start_time: Instant,
    /// Memory usage peak
    peak_memory: AtomicU64,
    /// Processing rates (bytes/sec)
    processing_rates: DashMap<u64, f64>,
    /// File size distribution
    size_distribution: DashMap<SizeRange, usize>,
}

/// File size ranges for distribution analysis
#[derive(Hash, Eq, PartialEq, Debug, Clone, Copy)]
pub enum SizeRange {
    Tiny,       // 0-1KB
    Small,      // 1KB-10KB
    Medium,     // 10KB-100KB
    Large,      // 100KB-1MB
    VeryLarge,  // 1MB-10MB
    Huge,       // >10MB
}

impl SizeRange {
    /// Gets the range for a given size
    fn from_size(size: u64) -> Self {
        match size {
            0..=1024 => Self::Tiny,
            1025..=10240 => Self::Small,
            10241..=102400 => Self::Medium,
            102401..=1048576 => Self::Large,
            1048577..=10485760 => Self::VeryLarge,
            _ => Self::Huge,
        }
    }

    /// Gets a human-readable description of the range
    pub fn description(&self) -> &'static str {
        match self {
            Self::Tiny => "0-1KB",
            Self::Small => "1KB-10KB",
            Self::Medium => "10KB-100KB",
            Self::Large => "100KB-1MB",
            Self::VeryLarge => "1MB-10MB",
            Self::Huge => ">10MB",
        }
    }
}

impl Stats {
    /// Creates new statistics collector
    pub fn new() -> Self {
        Self {
            total_files: AtomicUsize::new(0),
            total_size: AtomicU64::new(0),
            compressed_size: AtomicU64::new(0),
            methods: DashMap::new(),
            errors: Mutex::new(Vec::new()),
            current_progress: AtomicUsize::new(0),
            total_items: AtomicUsize::new(0),
            start_time: Instant::now(),
            peak_memory: AtomicU64::new(0),
            processing_rates: DashMap::new(),
            size_distribution: DashMap::new(),
        }
    }

    /// Increments the total file count
    pub fn increment_files(&self) {
        self.total_files.fetch_add(1, Ordering::Relaxed);
    }

    /// Adds to the total uncompressed size
    pub fn add_size(&self, size: u64) {
        self.total_size.fetch_add(size, Ordering::Relaxed);
        
        // Update size distribution
        let range = SizeRange::from_size(size);
        self.size_distribution.entry(range)
            .and_modify(|count| *count += 1)
            .or_insert(1);
        
        // Update processing rate
        let elapsed = self.start_time.elapsed().as_secs();
        let total = self.total_size.load(Ordering::Relaxed);
        if elapsed > 0 {
            let rate = total as f64 / elapsed as f64;
            self.processing_rates.insert(elapsed, rate);
        }
    }

    /// Adds to the total compressed size
    pub fn add_compressed_size(&self, size: u64) {
        self.compressed_size.fetch_add(size, Ordering::Relaxed);
    }

    /// Records a compression method
    pub fn record_method(&self, method: CompressionMethod) {
        self.methods.entry(method)
            .and_modify(|count| *count += 1)
            .or_insert(1);
    }

    /// Records an error
    pub fn record_error(&self, error: ZipError) {
        let mut errors = self.errors.lock().unwrap();
        errors.push(error);
    }

    /// Updates progress information
    pub fn update_progress(&self, current: usize, total: usize) {
        self.current_progress.store(current, Ordering::Relaxed);
        self.total_items.store(total, Ordering::Relaxed);
    }

    /// Updates peak memory usage
    pub fn update_memory_usage(&self, usage: u64) {
        let current = self.peak_memory.load(Ordering::Relaxed);
        if usage > current {
            self.peak_memory.store(usage, Ordering::Relaxed);
        }
    }

    /// Gets the current progress percentage
    pub fn progress_percentage(&self) -> f64 {
        let current = self.current_progress.load(Ordering::Relaxed);
        let total = self.total_items.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            (current as f64 / total as f64) * 100.0
        }
    }

    /// Gets the elapsed processing time
    pub fn elapsed_time(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Gets the average processing rate (bytes/sec)
    pub fn average_processing_rate(&self) -> f64 {
        let elapsed = self.elapsed_time().as_secs();
        if elapsed == 0 {
            0.0
        } else {
            self.total_size.load(Ordering::Relaxed) as f64 / elapsed as f64
        }
    }

    /// Gets the peak memory usage
    pub fn peak_memory_usage(&self) -> u64 {
        self.peak_memory.load(Ordering::Relaxed)
    }

    /// Gets the size distribution
    pub fn size_distribution(&self) -> HashMap<SizeRange, usize> {
        self.size_distribution.iter()
            .map(|entry| (*entry.key(), *entry.value()))
            .collect()
    }

    /// Gets the total number of files processed
    pub fn total_files(&self) -> usize {
        self.total_files.load(Ordering::Relaxed)
    }

    /// Gets the total uncompressed size
    pub fn total_size(&self) -> u64 {
        self.total_size.load(Ordering::Relaxed)
    }

    /// Gets the total compressed size
    pub fn compressed_size(&self) -> u64 {
        self.compressed_size.load(Ordering::Relaxed)
    }

    /// Gets the compression ratio
    pub fn compression_ratio(&self) -> f64 {
        let total = self.total_size.load(Ordering::Relaxed);
        let compressed = self.compressed_size.load(Ordering::Relaxed);
        if total == 0 {
            1.0
        } else {
            compressed as f64 / total as f64
        }
    }

    /// Gets the compression method statistics
    pub fn method_stats(&self) -> HashMap<CompressionMethod, usize> {
        self.methods.iter()
            .map(|entry| (*entry.key(), *entry.value()))
            .collect()
    }

    /// Gets the recorded errors
    pub fn errors(&self) -> Vec<ZipError> {
        self.errors.lock().unwrap().clone()
    }

    /// Gets processing rate history
    pub fn processing_rates(&self) -> Vec<(u64, f64)> {
        self.processing_rates.iter()
            .map(|entry| (*entry.key(), *entry.value()))
            .collect()
    }

    /// Generates a summary report
    pub fn generate_summary(&self) -> String {
        let elapsed = self.elapsed_time();
        let mut summary = String::new();

        // Basic statistics
        summary.push_str(&format!("Total Files: {}\n", self.total_files()));
        summary.push_str(&format!("Total Size: {} bytes\n", self.total_size()));
        summary.push_str(&format!("Compressed Size: {} bytes\n", self.compressed_size()));
        summary.push_str(&format!("Compression Ratio: {:.2}%\n", self.compression_ratio() * 100.0));
        summary.push_str(&format!("Processing Time: {:.2}s\n", elapsed.as_secs_f64()));
        summary.push_str(&format!("Average Rate: {:.2} MB/s\n", self.average_processing_rate() / 1_048_576.0));
        summary.push_str(&format!("Peak Memory Usage: {} MB\n", self.peak_memory_usage() / 1_048_576));

        // Size distribution
        summary.push_str("\nSize Distribution:\n");
        for (range, count) in self.size_distribution() {
            summary.push_str(&format!("  {}: {} files\n", range.description(), count));
        }

        // Compression methods
        summary.push_str("\nCompression Methods:\n");
        for (method, count) in self.method_stats() {
            summary.push_str(&format!("  {:?}: {} files\n", method, count));
        }

        // Errors
        let errors = self.errors();
        if !errors.is_empty() {
            summary.push_str("\nErrors:\n");
            for error in errors {
                summary.push_str(&format!("  {}\n", error));
            }
        }

        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_size_range() {
        assert!(matches!(SizeRange::from_size(500), SizeRange::Tiny));
        assert!(matches!(SizeRange::from_size(5000), SizeRange::Small));
        assert!(matches!(SizeRange::from_size(50000), SizeRange::Medium));
        assert!(matches!(SizeRange::from_size(500000), SizeRange::Large));
        assert!(matches!(SizeRange::from_size(5000000), SizeRange::VeryLarge));
        assert!(matches!(SizeRange::from_size(50000000), SizeRange::Huge));
    }

    #[test]
    fn test_stats_collection() {
        let stats = Stats::new();
        
        // Add some test data
        stats.increment_files();
        stats.add_size(1000);
        stats.add_compressed_size(500);
        stats.record_method(CompressionMethod::Store);
        stats.update_memory_usage(1024 * 1024);
        
        // Verify stats
        assert_eq!(stats.total_files(), 1);
        assert_eq!(stats.total_size(), 1000);
        assert_eq!(stats.compressed_size(), 500);
        assert_eq!(stats.compression_ratio(), 0.5);
        assert_eq!(stats.peak_memory_usage(), 1024 * 1024);
    }

    #[test]
    fn test_progress_tracking() {
        let stats = Stats::new();
        stats.update_progress(50, 100);
        assert_eq!(stats.progress_percentage(), 50.0);
    }

    #[test]
    fn test_size_distribution() {
        let stats = Stats::new();
        
        // Add files of different sizes
        stats.add_size(500);         // Tiny
        stats.add_size(5_000);       // Small
        stats.add_size(50_000);      // Medium
        stats.add_size(500_000);     // Large
        stats.add_size(5_000_000);   // Very Large
        stats.add_size(50_000_000);  // Huge
        
        let distribution = stats.size_distribution();
        assert_eq!(distribution.len(), 6);
        assert_eq!(*distribution.get(&SizeRange::Tiny).unwrap(), 1);
        assert_eq!(*distribution.get(&SizeRange::Small).unwrap(), 1);
        assert_eq!(*distribution.get(&SizeRange::Medium).unwrap(), 1);
        assert_eq!(*distribution.get(&SizeRange::Large).unwrap(), 1);
        assert_eq!(*distribution.get(&SizeRange::VeryLarge).unwrap(), 1);
        assert_eq!(*distribution.get(&SizeRange::Huge).unwrap(), 1);
    }

    #[test]
    fn test_summary_generation() {
        let stats = Stats::new();
        stats.increment_files();
        stats.add_size(1000);
        stats.add_compressed_size(500);
        stats.record_method(CompressionMethod::Store);
        
        let summary = stats.generate_summary();
        assert!(summary.contains("Total Files: 1"));
        assert!(summary.contains("Compression Ratio: 50.00%"));
    }
}
