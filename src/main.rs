use std::sync::Arc;
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use tokio;
use tracing::info;
use tracing_subscriber;

use zip_revelio::{
    cli::Cli,
    reader::AsyncZipReader,
    processor::ParallelProcessor,
    reporter::Reporter,
    stats::Stats,
    Result,
};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    info!("Starting ZIP-Revelio");

    // Parse command line arguments
    let config = Cli::parse().into_config();
    info!("Processing {:?}", config.input_path);

    // Initialize components
    let stats = Arc::new(Stats::new());
    let reader = AsyncZipReader::new(&config.input_path).await?;
    let processor = ParallelProcessor::new(config.thread_count, Arc::clone(&stats))?;
    let reporter = Reporter::new(Arc::clone(&stats), config.output_path.to_str().unwrap());

    // Set up progress bar
    let progress = if config.show_progress {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} [{elapsed_precise}] {msg}")
                .unwrap()
        );
        Some(pb)
    } else {
        None
    };

    // Process ZIP file
    process_zip(reader, processor, &progress).await?;

    // Write report
    if let Some(pb) = &progress {
        pb.set_message("Writing report...");
    }
    reporter.write_report().await?;

    // Show completion
    if let Some(pb) = progress {
        pb.finish_with_message("Analysis complete");
    }
    info!("Analysis complete. Report written to {:?}", config.output_path);

    Ok(())
}

async fn process_zip(
    mut reader: AsyncZipReader,
    processor: ParallelProcessor,
    progress: &Option<ProgressBar>,
) -> Result<()> {
    // Find end of central directory
    if let Some(pb) = progress {
        pb.set_message("Locating central directory...");
    }
    reader.seek_end_directory().await?;

    // Process entries
    if let Some(pb) = progress {
        pb.set_message("Processing entries...");
    }

    while let Some(entry) = reader.read_entry().await? {
        if let Some(pb) = progress {
            pb.set_message(format!("Processing {}...", entry.name));
        }
        processor.process_entry(entry, reader.buffer().clone()).await?;
        // Wait for entry to be processed
        tokio::task::yield_now().await;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_end_to_end() {
        let input = PathBuf::from("test_data/1mb.zip");
        let output = NamedTempFile::new().unwrap();
        
        let config = zip_revelio::Config {
            input_path: input,
            output_path: output.path().to_path_buf(),
            thread_count: 2,
            buffer_size: 64 * 1024,
            show_progress: false,
        };

        let stats = Arc::new(Stats::new());
        let reader = AsyncZipReader::new(&config.input_path).await.unwrap();
        let processor = ParallelProcessor::new(config.thread_count, Arc::clone(&stats)).unwrap();
        let reporter = Reporter::new(Arc::clone(&stats), config.output_path.to_str().unwrap());

        process_zip(reader, processor, &None).await.unwrap();
        // Wait for all tasks to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        reporter.write_report().await.unwrap();

        assert!(stats.total_files() > 0);
        assert!(stats.total_size() > 0);
        assert!(stats.errors().is_empty());
    }
}
