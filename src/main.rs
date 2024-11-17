use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use structopt::StructOpt;
use zip_revelio::{FileZipReader, ZipReader};

#[derive(StructOpt, Debug)]
#[structopt(name = "zip-revelio")]
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
    
    // Create progress bar
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );
    pb.set_message("Validating ZIP file...");

    // Create ZIP reader
    let reader = FileZipReader::new(&opt.input);
    reader.validate_size().await?;

    pb.set_message("Reading ZIP directory...");
    let directory = reader.read_directory().await?;

    pb.set_message("Generating report...");
    // Generate and write report
    use std::fs::File;
    use std::io::Write;
    let mut output = File::create(&opt.output)?;
    
    writeln!(output, "ZIP File Analysis Report")?;
    writeln!(output, "=======================")?;
    writeln!(output, "\nFile: {}", opt.input.display())?;
    writeln!(output, "Total entries: {}", directory.entries.len())?;
    
    let total_size: u64 = directory.entries.iter().map(|e| e.size).sum();
    let total_compressed: u64 = directory.entries.iter().map(|e| e.compressed_size).sum();
    
    writeln!(output, "Total uncompressed size: {} bytes", total_size)?;
    writeln!(output, "Total compressed size: {} bytes", total_compressed)?;
    writeln!(output, "Compression ratio: {:.2}%", (1.0 - (total_compressed as f64 / total_size as f64)) * 100.0)?;
    
    writeln!(output, "\nFile Listing:")?;
    writeln!(output, "------------")?;
    
    for entry in directory.entries.iter() {
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
    async fn test_cli_args() {
        let input = NamedTempFile::new().unwrap();
        let output = NamedTempFile::new().unwrap();

        let args = vec![
            "zip-revelio",
            input.path().to_str().unwrap(),
            output.path().to_str().unwrap(),
        ];

        let opt = Opt::from_iter_safe(args).unwrap();
        assert_eq!(opt.input, input.path());
        assert_eq!(opt.output, output.path());
    }
}
