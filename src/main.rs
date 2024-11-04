use std::path::PathBuf;
use anyhow::Context;
use clap::Parser;
use tokio::{signal::ctrl_c, fs::metadata};
use crate::{
    analyzer::ParallelZipAnalyzer,
    writer::{ReportWriter, FormatConfig, ProgressTracker, ProgressConfig},
    error::Result,
};

mod analyzer;
mod error;
mod models;
mod writer;

#[derive(Parser)]
#[command(author, version, about = "Parallel ZIP file analyzer")]
struct Args {
    /// Input ZIP file path
    input: PathBuf,

    /// Output report file path
    output: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Validate input file
    let file_size = metadata(&args.input)
        .await
        .context("Failed to read input file")?
        .len();

    // Create analyzer
    let analyzer = ParallelZipAnalyzer::new(args.input.clone());
    
    // Setup progress tracking
    let progress = ProgressTracker::new(
        file_size,
        ProgressConfig::default(),
    );

    // Setup report writer
    let writer = ReportWriter::new(args.output, FormatConfig::default());

    // Handle Ctrl+C
    let progress_clone = progress.clone();
    tokio::spawn(async move {
        if let Ok(()) = ctrl_c().await {
            progress_clone.handle_interrupt().unwrap_or_else(|e| {
                eprintln!("Error during interrupt: {}", e);
            });
            std::process::exit(130); // Standard SIGINT exit code
        }
    });

    // Run analysis
    let analysis = analyzer.analyze().await?;
    
    // Write report
    writer.write(&analysis).await?;
    
    // Finish progress
    progress.finish()?;

    println!("Analysis complete! Processed {} files ({} bytes)", 
        analysis.files.len(),
        analysis.total_size
    );
    Ok(())
}
