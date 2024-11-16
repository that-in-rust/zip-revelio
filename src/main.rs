use clap::Parser;
use std::path::PathBuf;
use zip_revelio::{Analyzer, Config};

/// ZIP file analyzer with parallel processing capabilities
#[derive(Parser)]
#[clap(version = "0.1.0", author = "Zip-Revelio Team")]
struct Opts {
    /// Input ZIP file to analyze
    #[clap(parse(from_os_str))]
    input: PathBuf,

    /// Output report file
    #[clap(parse(from_os_str))]
    output: PathBuf,

    /// Number of threads (default: auto)
    #[clap(short, long)]
    threads: Option<usize>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse command line arguments
    let opts = Opts::parse();

    // Configure signal handling
    ctrlc::set_handler(move || {
        println!("\nReceived Ctrl+C, cleaning up...");
        std::process::exit(0);
    })?;

    // Create analyzer configuration
    let config = Config {
        thread_count: opts.threads,
        ..Config::default()
    };

    // Create and run analyzer
    let analyzer = Analyzer::new(config);
    analyzer.analyze(opts.input, opts.output).await?;

    Ok(())
}
