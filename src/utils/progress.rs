use crate::ZipError;
use indicatif::{ProgressBar, ProgressStyle};
use parking_lot::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub struct Progress {
    bar: ProgressBar,
    processed: AtomicU64,
    total: u64,
    start: Instant,
    speed: RwLock<f64>,
    last_update: AtomicU64,
    update_interval: Duration,
}

impl Progress {
    pub fn new(total: u64) -> Result<Self, ZipError> {
        let style = ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] \
                     [{bar:40.cyan/blue}] \
                     {bytes}/{total_bytes} ({eta}) [{msg}]",
            )
            .map_err(|e| ZipError::Format(format!("Failed to create progress bar: {}", e)))?
            .progress_chars("=>-");

        let bar = ProgressBar::new(total).with_style(style);

        Ok(Self {
            bar,
            processed: AtomicU64::new(0),
            total,
            start: Instant::now(),
            speed: RwLock::new(0.0),
            last_update: AtomicU64::new(0),
            update_interval: Duration::from_millis(100),
        })
    }

    pub fn update(&self, bytes: u64) {
        let processed = self.processed.fetch_add(bytes, Ordering::Release);
        let now = self.start.elapsed().as_millis() as u64;

        if now - self.last_update.load(Ordering::Acquire) >= self.update_interval.as_millis() as u64
        {
            self.bar.set_position(processed);
            self.update_speed(processed, now);
            self.last_update.store(now, Ordering::Release);
        }
    }

    fn update_speed(&self, processed: u64, now: u64) {
        let elapsed = now as f64 / 1000.0; // Convert to seconds
        let speed = processed as f64 / elapsed;
        *self.speed.write() = speed;

        self.bar
            .set_message(format!("{:.1} MB/s", speed / (1024.0 * 1024.0)));
    }
}

impl Drop for Progress {
    fn drop(&mut self) {
        self.bar.finish_with_message("Complete");
    }
}
