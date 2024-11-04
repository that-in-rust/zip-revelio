use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use humansize::{format_size, BINARY};
use chrono::SecondsFormat;
use crate::{
    error::Result,
    models::{ZipAnalysis, FileInfo},
};

#[derive(Debug, Clone, Copy)]
pub enum SizeFormat {
    Binary,
    Decimal,
    Bytes,
}

#[derive(Debug, Clone, Copy)]
pub enum SortField {
    Path,
    Size,
    CompressedSize,
    CompressionRatio,
    Modified,
}

pub struct FormatConfig {
    pub include_headers: bool,
    pub size_format: SizeFormat,
    pub sort_by: SortField,
    pub timestamp_format: String,
}

impl Default for FormatConfig {
    fn default() -> Self {
        Self {
            include_headers: true,
            size_format: SizeFormat::Binary,
            sort_by: SortField::Path,
            timestamp_format: String::from("%Y-%m-%d %H:%M:%S"),
        }
    }
}

pub struct ReportWriter {
    output_path: PathBuf,
    format_config: FormatConfig,
}

impl ReportWriter {
    pub fn new(path: PathBuf, config: FormatConfig) -> Self {
        Self {
            output_path: path,
            format_config: config,
        }
    }

    pub async fn write(&self, analysis: &ZipAnalysis) -> Result<()> {
        let mut file = File::create(&self.output_path).await?;
        
        // Write header
        if self.format_config.include_headers {
            let header = self.format_header(analysis);
            file.write_all(header.as_bytes()).await?;
        }

        // Sort and write files
        let mut files = analysis.files.clone();
        self.sort_files(&mut files);

        for file_info in files {
            let line = self.format_file_entry(&file_info);
            file.write_all(line.as_bytes()).await?;
        }

        // Write summary
        let summary = self.format_summary(analysis);
        file.write_all(summary.as_bytes()).await?;

        Ok(())
    }

    fn format_header(&self, analysis: &ZipAnalysis) -> String {
        format!(
            "ZIP File Analysis Report\n\
             ====================\n\
             Analysis Duration: {}ms\n\
             Chunks Processed: {}\n\
             Peak Memory Usage: {}MB\n\
             Error Count: {}\n\n",
            analysis.stats.duration.as_millis(),
            analysis.stats.chunks_processed.load(Ordering::Relaxed),
            analysis.stats.peak_memory_mb.load(Ordering::Relaxed),
            analysis.stats.error_count.load(Ordering::Relaxed),
        )
    }

    fn format_file_entry(&self, file_info: &FileInfo) -> String {
        let size = match self.format_config.size_format {
            SizeFormat::Binary => format_size(file_info.size, BINARY),
            SizeFormat::Decimal => format_size(file_info.size, humansize::DECIMAL),
            SizeFormat::Bytes => file_info.size.to_string(),
        };

        let modified = file_info.modified.to_rfc3339_opts(SecondsFormat::Secs, true);

        format!(
            "{}\t{}\t{}\t{:?}\t{}\n",
            file_info.path.display(),
            size,
            file_info.compression_method,
            file_info.crc32,
            modified,
        )
    }

    fn format_summary(&self, analysis: &ZipAnalysis) -> String {
        format!(
            "\nSummary\n\
             =======\n\
             Total Size: {}\n\
             Compressed Size: {}\n\
             Compression Ratio: {:.2}%\n\
             Total Files: {}\n",
            format_size(analysis.total_size, BINARY),
            format_size(analysis.compressed_size, BINARY),
            (1.0 - analysis.compression_ratio) * 100.0,
            analysis.files.len(),
        )
    }

    fn sort_files(&self, files: &mut Vec<FileInfo>) {
        match self.format_config.sort_by {
            SortField::Path => files.sort_by(|a, b| a.path.cmp(&b.path)),
            SortField::Size => files.sort_by(|a, b| b.size.cmp(&a.size)),
            SortField::CompressedSize => files.sort_by(|a, b| b.compressed_size.cmp(&a.compressed_size)),
            SortField::CompressionRatio => files.sort_by(|a, b| {
                let ratio_a = a.compressed_size as f64 / a.size as f64;
                let ratio_b = b.compressed_size as f64 / b.size as f64;
                ratio_b.partial_cmp(&ratio_a).unwrap_or(std::cmp::Ordering::Equal)
            }),
            SortField::Modified => files.sort_by(|a, b| b.modified.cmp(&a.modified)),
        }
    }
}
