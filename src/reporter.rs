use std::path::Path;
use std::time::Duration;
use std::collections::HashMap;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use chrono::Local;
use indicatif::{ProgressBar, ProgressStyle};
use serde_json::json;

use crate::{stats::Stats, Result};

/// Report format options
#[derive(Debug, Clone, Copy)]
pub enum ReportFormat {
    Text,
    Json,
    Markdown,
}

/// Report configuration
#[derive(Debug, Clone)]
pub struct ReportConfig {
    /// Output format
    pub format: ReportFormat,
    /// Whether to include detailed stats
    pub detailed: bool,
    /// Whether to show progress bar
    pub show_progress: bool,
    /// Whether to include performance metrics
    pub include_performance: bool,
    /// Whether to include error details
    pub include_errors: bool,
}

impl Default for ReportConfig {
    fn default() -> Self {
        Self {
            format: ReportFormat::Text,
            detailed: false,
            show_progress: true,
            include_performance: true,
            include_errors: true,
        }
    }
}

/// ZIP analysis reporter
pub struct Reporter {
    /// Statistics collector
    stats: Arc<Stats>,
    /// Report configuration
    config: ReportConfig,
    /// Progress bar
    progress: Option<ProgressBar>,
}

impl Reporter {
    /// Creates a new reporter
    pub fn new(stats: Arc<Stats>, config: ReportConfig) -> Self {
        let progress = if config.show_progress {
            let pb = ProgressBar::new(100);
            pb.set_style(ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
                .unwrap()
                .progress_chars("#>-"));
            Some(pb)
        } else {
            None
        };

        Self {
            stats,
            config,
            progress,
        }
    }

    /// Updates progress display
    pub fn update_progress(&self) {
        if let Some(pb) = &self.progress {
            let percentage = self.stats.progress_percentage() as u64;
            pb.set_position(percentage);
        }
    }

    /// Generates and writes report to file
    pub async fn write_report<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let mut file = File::create(path).await?;
        let content = match self.config.format {
            ReportFormat::Text => self.generate_text_report(),
            ReportFormat::Json => self.generate_json_report(),
            ReportFormat::Markdown => self.generate_markdown_report(),
        };
        file.write_all(content.as_bytes()).await?;
        Ok(())
    }

    /// Formats a size in human-readable format
    fn format_size(&self, size: u64) -> String {
        const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
        let mut size = size as f64;
        let mut unit_index = 0;

        while size >= 1024.0 && unit_index < UNITS.len() - 1 {
            size /= 1024.0;
            unit_index += 1;
        }

        format!("{:.2} {}", size, UNITS[unit_index])
    }

    /// Formats a duration in human-readable format
    fn format_duration(&self, duration: Duration) -> String {
        let secs = duration.as_secs();
        let mins = secs / 60;
        let hours = mins / 60;
        let mins = mins % 60;
        let secs = secs % 60;

        if hours > 0 {
            format!("{}h {}m {}s", hours, mins, secs)
        } else if mins > 0 {
            format!("{}m {}s", mins, secs)
        } else {
            format!("{}s", secs)
        }
    }

    /// Generates a text report
    fn generate_text_report(&self) -> String {
        let mut report = String::new();
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");

        // Header
        report.push_str(&format!("ZIP-Revelio Analysis Report\n"));
        report.push_str(&format!("Generated: {}\n\n", timestamp));

        // Basic Statistics
        report.push_str("Basic Statistics:\n");
        report.push_str(&format!("Total Files: {}\n", self.stats.total_files()));
        report.push_str(&format!("Total Size: {}\n", self.format_size(self.stats.total_size())));
        report.push_str(&format!("Compressed Size: {}\n", self.format_size(self.stats.compressed_size())));
        report.push_str(&format!("Compression Ratio: {:.2}%\n", self.stats.compression_ratio() * 100.0));
        report.push_str(&format!("Processing Time: {}\n", self.format_duration(self.stats.elapsed_time())));

        if self.config.include_performance {
            report.push_str("\nPerformance Metrics:\n");
            report.push_str(&format!("Average Processing Rate: {}/s\n", 
                self.format_size((self.stats.average_processing_rate() as u64))));
            report.push_str(&format!("Peak Memory Usage: {}\n", 
                self.format_size(self.stats.peak_memory_usage())));
        }

        if self.config.detailed {
            // Size Distribution
            report.push_str("\nSize Distribution:\n");
            for (range, count) in self.stats.size_distribution() {
                report.push_str(&format!("  {}: {} files\n", range.description(), count));
            }

            // Compression Methods
            report.push_str("\nCompression Methods:\n");
            for (method, count) in self.stats.method_stats() {
                report.push_str(&format!("  {:?}: {} files\n", method, count));
            }

            // Processing Rates
            report.push_str("\nProcessing Rate History:\n");
            for (time, rate) in self.stats.processing_rates() {
                report.push_str(&format!("  {} sec: {}/s\n", 
                    time, self.format_size(rate as u64)));
            }
        }

        if self.config.include_errors {
            let errors = self.stats.errors();
            if !errors.is_empty() {
                report.push_str("\nErrors:\n");
                for error in errors {
                    report.push_str(&format!("  {}\n", error));
                }
            }
        }

        report
    }

    /// Generates a JSON report
    fn generate_json_report(&self) -> String {
        let mut data = json!({
            "timestamp": Local::now().to_rfc3339(),
            "basic_stats": {
                "total_files": self.stats.total_files(),
                "total_size": self.stats.total_size(),
                "compressed_size": self.stats.compressed_size(),
                "compression_ratio": self.stats.compression_ratio(),
                "processing_time_secs": self.stats.elapsed_time().as_secs()
            }
        });

        if self.config.include_performance {
            data.as_object_mut().unwrap().insert("performance".into(), json!({
                "average_rate": self.stats.average_processing_rate(),
                "peak_memory": self.stats.peak_memory_usage()
            }));
        }

        if self.config.detailed {
            data.as_object_mut().unwrap().insert("size_distribution".into(), 
                json!(self.stats.size_distribution()
                    .iter()
                    .map(|(range, count)| {
                        (range.description(), count)
                    })
                    .collect::<HashMap<_, _>>()));

            data.as_object_mut().unwrap().insert("compression_methods".into(),
                json!(self.stats.method_stats()));

            data.as_object_mut().unwrap().insert("processing_rates".into(),
                json!(self.stats.processing_rates()));
        }

        if self.config.include_errors {
            data.as_object_mut().unwrap().insert("errors".into(),
                json!(self.stats.errors()
                    .iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()));
        }

        serde_json::to_string_pretty(&data).unwrap_or_else(|_| "{}".to_string())
    }

    /// Generates a Markdown report
    fn generate_markdown_report(&self) -> String {
        let mut report = String::new();
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");

        // Header
        report.push_str("# ZIP-Revelio Analysis Report\n\n");
        report.push_str(&format!("Generated: {}\n\n", timestamp));

        // Basic Statistics
        report.push_str("## Basic Statistics\n\n");
        report.push_str("| Metric | Value |\n");
        report.push_str("|--------|-------|\n");
        report.push_str(&format!("| Total Files | {} |\n", self.stats.total_files()));
        report.push_str(&format!("| Total Size | {} |\n", self.format_size(self.stats.total_size())));
        report.push_str(&format!("| Compressed Size | {} |\n", self.format_size(self.stats.compressed_size())));
        report.push_str(&format!("| Compression Ratio | {:.2}% |\n", self.stats.compression_ratio() * 100.0));
        report.push_str(&format!("| Processing Time | {} |\n\n", self.format_duration(self.stats.elapsed_time())));

        if self.config.include_performance {
            report.push_str("## Performance Metrics\n\n");
            report.push_str("| Metric | Value |\n");
            report.push_str("|--------|-------|\n");
            report.push_str(&format!("| Average Processing Rate | {}/s |\n",
                self.format_size((self.stats.average_processing_rate() as u64))));
            report.push_str(&format!("| Peak Memory Usage | {} |\n\n",
                self.format_size(self.stats.peak_memory_usage())));
        }

        if self.config.detailed {
            // Size Distribution
            report.push_str("## Size Distribution\n\n");
            report.push_str("| Range | Files |\n");
            report.push_str("|-------|-------|\n");
            for (range, count) in self.stats.size_distribution() {
                report.push_str(&format!("| {} | {} |\n", range.description(), count));
            }
            report.push_str("\n");

            // Compression Methods
            report.push_str("## Compression Methods\n\n");
            report.push_str("| Method | Files |\n");
            report.push_str("|--------|-------|\n");
            for (method, count) in self.stats.method_stats() {
                report.push_str(&format!("| {:?} | {} |\n", method, count));
            }
            report.push_str("\n");
        }

        if self.config.include_errors {
            let errors = self.stats.errors();
            if !errors.is_empty() {
                report.push_str("## Errors\n\n");
                for error in errors {
                    report.push_str(&format!("- {}\n", error));
                }
            }
        }

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CompressionMethod, ZipError};
    use tempfile::NamedTempFile;
    use tokio::fs;

    #[tokio::test]
    async fn test_report_generation() {
        let stats = Arc::new(Stats::new());
        stats.increment_files();
        stats.add_size(1000);
        stats.add_compressed_size(500);
        stats.record_method(CompressionMethod::Store);
        stats.record_error(ZipError::InvalidSignature(0));

        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();
        
        let config = ReportConfig {
            format: ReportFormat::Text,
            detailed: true,
            show_progress: false,
            include_performance: true,
            include_errors: true,
        };
        let reporter = Reporter::new(stats, config);
        reporter.write_report(path.to_string()).await.unwrap();

        let content = fs::read_to_string(path).await.unwrap();
        assert!(content.contains("Total Files: 1"));
        assert!(content.contains("Total Size: 1000.00 B"));
        assert!(content.contains("Compressed Size: 500.00 B"));
        assert!(content.contains("Store: 1 files"));
        assert!(content.contains("Invalid ZIP signature"));
    }

    #[tokio::test]
    async fn test_empty_report() {
        let stats = Arc::new(Stats::new());
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();
        
        let config = ReportConfig {
            format: ReportFormat::Text,
            detailed: true,
            show_progress: false,
            include_performance: true,
            include_errors: true,
        };
        let reporter = Reporter::new(stats, config);
        reporter.write_report(path.to_string()).await.unwrap();

        let content = fs::read_to_string(path).await.unwrap();
        assert!(content.contains("Total Files: 0"));
        assert!(content.contains("Total Size: 0.00 B"));
        assert!(!content.contains("Errors:"));
    }
}
