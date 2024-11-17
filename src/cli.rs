use std::path::PathBuf;
use clap::Parser;
use crate::Config;

/// High-performance ZIP file analyzer
#[derive(Parser, Debug)]
#[clap(
    name = "zip-revelio",
    version,
    author,
    about = "Analyze ZIP files with parallel processing power"
)]
pub struct Cli {
    /// Input ZIP file to analyze
    #[clap(parse(from_os_str))]
    pub input: PathBuf,

    /// Output report file
    #[clap(parse(from_os_str), short, long, default_value = "report.txt")]
    pub output: PathBuf,

    /// Number of processing threads (default: number of CPU cores)
    #[clap(short, long)]
    pub threads: Option<usize>,

    /// Buffer size in KB (default: 64)
    #[clap(short, long)]
    pub buffer_size: Option<usize>,

    /// Disable progress bar
    #[clap(short = 'P', long)]
    pub no_progress: bool,

    /// Verbose output
    #[clap(short, long)]
    pub verbose: bool,

    /// Maximum memory usage in MB
    #[clap(short = 'M', long, default_value = "1024")]
    pub max_memory: usize,

    /// Compression methods to analyze (comma-separated)
    #[clap(short, long, use_delimiter = true)]
    pub methods: Option<Vec<String>>,

    /// Generate detailed report
    #[clap(short, long)]
    pub detailed: bool,
}

impl Cli {
    /// Converts CLI arguments into a Config
    pub fn into_config(self) -> Config {
        Config {
            input_path: self.input,
            output_path: self.output,
            thread_count: self.threads.unwrap_or_else(num_cpus::get),
            buffer_size: self.buffer_size.unwrap_or(64) * 1024,
            show_progress: !self.no_progress,
            verbose: self.verbose,
            max_memory: self.max_memory * 1024 * 1024,
            methods: self.methods,
            detailed: self.detailed,
        }
    }

    /// Validates the configuration
    pub fn validate(&self) -> Result<(), String> {
        // Check input file
        if !self.input.exists() {
            return Err(format!("Input file does not exist: {:?}", self.input));
        }
        if !self.input.is_file() {
            return Err(format!("Input path is not a file: {:?}", self.input));
        }

        // Check thread count
        if let Some(threads) = self.threads {
            if threads == 0 {
                return Err("Thread count must be greater than 0".to_string());
            }
            if threads > 256 {
                return Err("Thread count must not exceed 256".to_string());
            }
        }

        // Check buffer size
        if let Some(size) = self.buffer_size {
            if size == 0 {
                return Err("Buffer size must be greater than 0".to_string());
            }
            if size > 1024 * 1024 {  // 1GB
                return Err("Buffer size must not exceed 1GB".to_string());
            }
        }

        // Check memory limit
        if self.max_memory == 0 {
            return Err("Maximum memory must be greater than 0".to_string());
        }
        if self.max_memory > 1024 * 1024 {  // 1TB
            return Err("Maximum memory must not exceed 1TB".to_string());
        }

        // Validate compression methods
        if let Some(ref methods) = self.methods {
            for method in methods {
                match method.to_lowercase().as_str() {
                    "store" | "deflate" => continue,
                    _ => return Err(format!("Unsupported compression method: {}", method)),
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::tempdir;

    #[test]
    fn test_cli_parsing() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("test.zip");
        File::create(&input_path).unwrap();

        let args = vec![
            "zip-revelio",
            input_path.to_str().unwrap(),
            "--threads",
            "4",
            "--buffer-size",
            "128",
            "--max-memory",
            "2048",
            "--methods",
            "store,deflate",
            "--detailed",
        ];

        let cli = Cli::try_parse_from(args).unwrap();
        assert_eq!(cli.threads, Some(4));
        assert_eq!(cli.buffer_size, Some(128));
        assert_eq!(cli.max_memory, 2048);
        assert_eq!(cli.methods, Some(vec!["store".to_string(), "deflate".to_string()]));
        assert!(cli.detailed);
    }

    #[test]
    fn test_config_conversion() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("test.zip");
        File::create(&input_path).unwrap();

        let cli = Cli {
            input: input_path,
            output: PathBuf::from("report.txt"),
            threads: Some(4),
            buffer_size: Some(128),
            no_progress: false,
            verbose: true,
            max_memory: 2048,
            methods: Some(vec!["store".to_string()]),
            detailed: true,
        };

        let config = cli.into_config();
        assert_eq!(config.thread_count, 4);
        assert_eq!(config.buffer_size, 128 * 1024);
        assert!(config.show_progress);
        assert!(config.verbose);
        assert_eq!(config.max_memory, 2048 * 1024 * 1024);
        assert_eq!(config.methods, Some(vec!["store".to_string()]));
        assert!(config.detailed);
    }

    #[test]
    fn test_validation() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("test.zip");
        File::create(&input_path).unwrap();

        // Valid configuration
        let cli = Cli {
            input: input_path.clone(),
            output: PathBuf::from("report.txt"),
            threads: Some(4),
            buffer_size: Some(128),
            no_progress: false,
            verbose: false,
            max_memory: 2048,
            methods: Some(vec!["store".to_string()]),
            detailed: false,
        };
        assert!(cli.validate().is_ok());

        // Invalid thread count
        let mut invalid = cli.clone();
        invalid.threads = Some(0);
        assert!(invalid.validate().is_err());

        // Invalid buffer size
        let mut invalid = cli.clone();
        invalid.buffer_size = Some(0);
        assert!(invalid.validate().is_err());

        // Invalid memory limit
        let mut invalid = cli.clone();
        invalid.max_memory = 0;
        assert!(invalid.validate().is_err());

        // Invalid compression method
        let mut invalid = cli;
        invalid.methods = Some(vec!["invalid".to_string()]);
        assert!(invalid.validate().is_err());
    }
}