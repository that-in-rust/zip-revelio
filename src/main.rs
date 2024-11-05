use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::fs::File;
use std::io::Write;
use std::time::{Instant, Duration};

use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle, HumanBytes};
use parking_lot::RwLock;
use futures::StreamExt;

mod types;
mod reader;
mod processor;

use crate::types::{Result, Error, ZipAnalysis, STORED, DEFLATED};
use crate::reader::ZipReader;
use crate::processor::Processor;

/// ZIP file analyzer with parallel processing
#[derive(Parser, Debug)]
#[command(
    author = "ZIP Revelio Team",
    version,
    about = "Analyzes ZIP files using parallel processing",
    long_about = None
)]
struct Args {
    /// Input ZIP file to analyze
    #[arg(help = "Path to input ZIP file")]
    input: PathBuf,
    
    /// Output file for analysis results
    #[arg(help = "Path to output report file")]
    output: PathBuf,

    /// Number of threads (defaults to number of CPU cores)
    #[arg(short, long, help = "Number of processing threads")]
    threads: Option<usize>,
}

/// Atomic write to file to prevent partial writes
fn atomic_write(path: &PathBuf, contents: String) -> Result<()> {
    let temp_path = path.with_extension("tmp");
    
    // Write to temporary file first
    let mut file = File::create(&temp_path)
        .map_err(|e| Error::Io(e.to_string()))?;
    
    file.write_all(contents.as_bytes())
        .map_err(|e| Error::Io(e.to_string()))?;
    
    // Ensure all data is written to disk
    file.sync_all()
        .map_err(|e| Error::Io(e.to_string()))?;
    
    // Atomically rename temp file to target file
    std::fs::rename(&temp_path, path)
        .map_err(|e| Error::Io(e.to_string()))?;
    
    Ok(())
}

/// Setup progress bar with enhanced style
fn setup_progress_bar(total_size: u64) -> ProgressBar {
    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) [{msg}]")
            .unwrap()
            .progress_chars("=>-")
    );
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

/// Format analysis results for output
fn format_results(analysis: &ZipAnalysis, elapsed: std::time::Duration) -> String {
    let mut output = String::new();
    
    output.push_str("=== ZIP Analysis Report ===\n\n");
    
    // Basic statistics
    output.push_str(&format!("Total size: {}\n", HumanBytes(analysis.total_size() as u64)));
    output.push_str(&format!("Files analyzed: {}\n", analysis.file_count()));
    output.push_str(&format!("Analysis time: {:.2}s\n\n", elapsed.as_secs_f64()));
    
    // Compression statistics
    output.push_str(&format!("Overall compression ratio: {:.1}%\n", 
        analysis.get_compression_ratio() * 100.0));
    output.push_str(&format!("Total compressed: {}\n", 
        HumanBytes(analysis.total_compressed())));
    output.push_str(&format!("Total uncompressed: {}\n\n", 
        HumanBytes(analysis.total_uncompressed())));
    
    // File type distribution
    output.push_str("File types:\n");
    for (type_name, count) in analysis.get_file_types() {
        output.push_str(&format!("  {}: {}\n", type_name, count));
    }
    output.push_str("\n");
    
    // Compression methods
    output.push_str("Compression methods:\n");
    for (method, count) in analysis.get_compression_methods() {
        let method_name = match *method {
            STORED => "Store",
            DEFLATED => "Deflate",
            _ => "Unknown",
        };
        output.push_str(&format!("  {}: {}\n", method_name, count));
    }
    
    output
}

/// Process ZIP file with enhanced progress tracking and cancellation support
async fn process_zip(
    path: PathBuf,
    pb: ProgressBar,
    running: Arc<AtomicBool>,
    _threads: Option<usize>,
) -> Result<ZipAnalysis> {
    // Validate input
    if !path.exists() {
        return Err(Error::Io("Input file does not exist".into()));
    }

    let _start_time = Instant::now();
    
    // Initialize with proper error handling
    let mut reader = ZipReader::new(path).await?;
    let processor = Processor::new_with_threads(_threads)
        .map_err(|e| Error::Processing(format!("Failed to create processor: {}", e)))?;
    
    let results = Arc::new(RwLock::new(ZipAnalysis::new()));
    
    // Scope the progress tracker
    {
        let progress_tracker = Arc::clone(&results);
        let mut stream = reader.stream_chunks();
        
        while let Some(chunk_result) = stream.next().await {
            if !running.load(Ordering::SeqCst) {
                pb.finish_with_message("Analysis cancelled");
                return Err(Error::Processing("Operation cancelled".into()));
            }

            let chunk = chunk_result?;
            processor.process_chunk(chunk, &mut *results.write())
                .map_err(|e| Error::Processing(format!("Chunk processing failed: {}", e)))?;

            // Progress updates using progress_tracker
            let current = progress_tracker.read();
            pb.set_message(format!(
                "{} files, {:.1}% compressed",
                current.file_count(),
                current.get_compression_ratio() * 100.0
            ));
            pb.set_position(current.total_size() as u64);
        }
    } // progress_tracker is dropped here

    // Now we can safely unwrap results
    Ok(Arc::try_unwrap(results)
        .map_err(|_| Error::Processing("Failed to collect results - active references remain".into()))?
        .into_inner())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();
    
    // Validate paths
    if !args.input.exists() {
        return Err(Error::Io("Input file does not exist".into()));
    }
    
    if let Some(parent) = args.output.parent() {
        if !parent.exists() {
            return Err(Error::Io("Output directory does not exist".into()));
        }
    }
    
    // Setup cancellation handler
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
        println!("\nCancelling...");
    })?;
    
    // Get file size for progress bar
    let file_size = tokio::fs::metadata(&args.input)
        .await?
        .len();
    
    // Setup progress tracking
    let pb = setup_progress_bar(file_size);
    
    // Process ZIP file
    let start_time = Instant::now();
    let result = process_zip(
        args.input,
        pb.clone(),
        running,
        args.threads,
    ).await?;
    
    // Format and write results
    let output = format_results(&result, start_time.elapsed());
    atomic_write(&args.output, output)?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_atomic_write() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.txt");
        
        atomic_write(&path, "test content".into()).unwrap();
        
        assert!(path.exists());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "test content"
        );
    }

    #[tokio::test]
    async fn test_process_zip() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("test.zip");
        let output = dir.path().join("report.txt");
        
        // Create test ZIP file
        std::fs::write(&input, b"PK\x03\x04test data").unwrap();
        
        let running = Arc::new(AtomicBool::new(true));
        let pb = setup_progress_bar(0);
        
        let result = process_zip(
            input,
            pb,
            running,
            None
        ).await;
        
        assert!(result.is_ok());
    }
}
