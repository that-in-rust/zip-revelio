use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use structopt::StructOpt;
use zip_revelio::FileZipReader;

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
    // TODO: Generate report

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
