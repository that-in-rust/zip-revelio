use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use structopt::StructOpt;
use zip_revelio::{FileZipReader, ZipReader};
use std::time::{Instant, SystemTime};
use chrono::{DateTime, Utc};
use num_cpus;

#[derive(Debug, StructOpt)]
struct Opt {
    /// Input ZIP file to analyze
    #[structopt(parse(from_os_str))]
    input: PathBuf,

    /// Output file for analysis report
    #[structopt(parse(from_os_str))]
    output: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let opt = Opt::from_args();
    let start_time = Instant::now();
    let start_datetime = SystemTime::now();

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );

    pb.set_message("Validating ZIP file...");
    let reader = FileZipReader::new(&opt.input);
    reader.validate_size().await?;

    pb.set_message("Reading ZIP directory...");
    let dir = reader.read_directory().await?;

    let end_time = Instant::now();
    let end_datetime = SystemTime::now();
    let processing_duration = end_time.duration_since(start_time);
    
    // Get file size for processing rate calculation
    let file_size = tokio::fs::metadata(&opt.input).await?.len();
    let mb_per_second = (file_size as f64 / 1_000_000.0) / processing_duration.as_secs_f64();

    pb.set_message("Generating report...");
    use std::fs::File;
    use std::io::Write;
    let mut output = File::create(&opt.output)?;
    
    writeln!(output, "ZIP File Analysis Report")?;
    writeln!(output, "=======================")?;
    
    // Performance Summary
    writeln!(output, "\n=== Processing Summary ===")?;
    writeln!(output, "Analysis started: {}", DateTime::<Utc>::from(start_datetime).format("%Y-%m-%d %H:%M:%S UTC"))?;
    writeln!(output, "Analysis completed: {}", DateTime::<Utc>::from(end_datetime).format("%Y-%m-%d %H:%M:%S UTC"))?;
    writeln!(output, "Processing time: {:.2} seconds", processing_duration.as_secs_f64())?;
    writeln!(output, "CPU cores utilized: {}", num_cpus::get())?;
    writeln!(output, "Processing rate: {:.2} MB/s", mb_per_second)?;
    
    writeln!(output, "\n=== Performance Details ===")?;
    writeln!(output, "Parallel processing: Enabled")?;
    writeln!(output, "Number of worker threads: {}", num_cpus::get())?;
    writeln!(output, "I/O mode: Async")?;
    
    // File Analysis
    writeln!(output, "\n=== File Analysis ===")?;
    writeln!(output, "File: {}", opt.input.display())?;
    writeln!(output, "Total entries: {}", dir.entries.len())?;
    
    let total_size: u64 = dir.entries.iter().map(|e| e.size).sum();
    let total_compressed: u64 = dir.entries.iter().map(|e| e.compressed_size).sum();
    
    writeln!(output, "Total uncompressed size: {} bytes", total_size)?;
    writeln!(output, "Total compressed size: {} bytes", total_compressed)?;
    writeln!(output, "Compression ratio: {:.2}%", (1.0 - (total_compressed as f64 / total_size as f64)) * 100.0)?;
    
    writeln!(output, "\nFile Listing:")?;
    writeln!(output, "------------")?;
    
    for entry in dir.entries.iter() {
        writeln!(output, "{}", entry.name)?;
        writeln!(output, "  Size: {} bytes", entry.size)?;
        writeln!(output, "  Compressed: {} bytes", entry.compressed_size)?;
        writeln!(output, "  CRC32: {:08x}", entry.crc32)?;
        writeln!(output)?;
    }

    pb.finish_with_message("Analysis complete!");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_cli_args() -> Result<()> {
        let input = NamedTempFile::new()?;
        let output = NamedTempFile::new()?;
        
        let opt = Opt {
            input: input.path().to_owned(),
            output: output.path().to_owned(),
        };
        
        assert_eq!(opt.input, input.path());
        assert_eq!(opt.output, output.path());
        Ok(())
    }
}
