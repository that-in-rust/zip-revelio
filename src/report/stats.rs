use dashmap::DashMap;
use parking_lot::RwLock;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

pub struct Stats {
    // Size Information
    pub(crate) total_size: AtomicU64,
    pub(crate) compressed_size: AtomicU64,

    // File Counts
    pub(crate) file_count: AtomicUsize,

    // Timing
    pub(crate) start_time: Instant,
    pub(crate) duration: AtomicU64,

    // Categorization
    pub(crate) methods: DashMap<u16, usize>,
    pub(crate) file_types: DashMap<String, usize>,

    // File List
    pub(crate) files: RwLock<BTreeSet<String>>,
}

impl Stats {
    pub fn new() -> Self {
        Self {
            total_size: AtomicU64::new(0),
            compressed_size: AtomicU64::new(0),
            file_count: AtomicUsize::new(0),
            start_time: Instant::now(),
            duration: AtomicU64::new(0),
            methods: DashMap::new(),
            file_types: DashMap::new(),
            files: RwLock::new(BTreeSet::new()),
        }
    }

    pub fn update(&self, name: String, original_size: u64, compressed_size: u64, method: u16) {
        // Update sizes
        self.total_size.fetch_add(original_size, Ordering::Relaxed);
        self.compressed_size
            .fetch_add(compressed_size, Ordering::Relaxed);
        self.file_count.fetch_add(1, Ordering::Relaxed);

        // Update compression method count
        self.methods
            .entry(method)
            .and_modify(|e| *e += 1)
            .or_insert(1);

        // Update file type count
        let ext = Path::new(&name)
            .extension()
            .and_then(|os| os.to_str())
            .unwrap_or("unknown")
            .to_string();

        self.file_types
            .entry(ext)
            .and_modify(|e| *e += 1)
            .or_insert(1);

        // Add to file list
        self.files.write().insert(name);
    }

    pub fn compression_ratio(&self) -> f64 {
        let total = self.total_size.load(Ordering::Relaxed);
        let compressed = self.compressed_size.load(Ordering::Relaxed);

        if total == 0 {
            0.0
        } else {
            compressed as f64 / total as f64 * 100.0
        }
    }

    pub fn finish(&self) {
        self.duration.store(
            self.start_time.elapsed().as_millis() as u64,
            Ordering::Relaxed,
        );
    }
}
