use std::sync::Arc;
use indicatif::{ProgressBar, ProgressStyle};
use tokio::sync::RwLock;
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct ProgressUpdate {
    pub bytes_processed: u64,
    pub files_processed: usize,
    pub current_file: String,
    pub compression_ratio: f64,
    pub estimated_remaining_secs: u64,
    pub error_count: usize,
}

pub struct ProgressConfig {
    pub update_frequency_ms: u64,
    pub style_template: String,
    pub refresh_rate: std::time::Duration,
}

impl Default for ProgressConfig {
    fn default() -> Self {
        Self {
            update_frequency_ms: 100,
            style_template: String::from("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}"),
            refresh_rate: std::time::Duration::from_millis(33),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct ProgressStats {
    pub total_bytes: u64,
    pub processed_bytes: u64,
    pub total_files: usize,
    pub processed_files: usize,
    pub error_count: usize,
}

#[derive(Clone)]
pub struct ProgressTracker {
    bar: Arc<ProgressBar>,
    stats: Arc<RwLock<ProgressStats>>,
    config: ProgressConfig,
}

impl ProgressTracker {
    pub fn new(total_size: u64, config: ProgressConfig) -> Self {
        let bar = ProgressBar::new(total_size);
        bar.set_style(
            ProgressStyle::default_bar()
                .template(&config.style_template)
                .unwrap()
                .progress_chars("=>-")
        );
        bar.enable_steady_tick(config.refresh_rate);

        Self {
            bar,
            stats: Arc::new(RwLock::new(ProgressStats::default())),
            config,
        }
    }

    pub async fn update(&self, update: ProgressUpdate) -> Result<()> {
        let mut stats = self.stats.write().await;
        stats.processed_bytes = update.bytes_processed;
        stats.processed_files = update.files_processed;
        stats.error_count = update.error_count;

        self.bar.set_position(update.bytes_processed);
        self.bar.set_message(format!(
            "Processing: {} ({:.1}% compressed)", 
            update.current_file,
            update.compression_ratio * 100.0
        ));

        Ok(())
    }

    pub fn finish(self) -> Result<()> {
        self.bar.finish_with_message("Analysis complete!");
        Ok(())
    }
}
