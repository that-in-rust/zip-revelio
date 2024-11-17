# ZIP-Revelio 🔍

A high-performance, memory-safe ZIP file analyzer written in Rust.

## Features

- 🚀 Async processing with parallel computation
- 🛡️ Memory-safe by design
- 📊 Detailed analysis reports
- 📈 Progress tracking
- 🧵 Multi-threaded processing
- 🔒 Error resilient

## Installation

```bash
cargo install zip-revelio
```

## Usage

```bash
# Basic usage
zip-revelio -i input.zip -o report.txt

# Specify thread count and buffer size
zip-revelio -i input.zip -o report.txt -t 4 -b 128

# Disable progress bar
zip-revelio -i input.zip -o report.txt --no-progress
```

### Options

- `-i, --input`: Input ZIP file to analyze
- `-o, --output`: Output file for the analysis report
- `-t, --threads`: Number of threads for parallel processing (default: CPU count)
- `-b, --buffer-size`: Buffer size in KB (default: 64)
- `--no-progress`: Disable progress bar
- `-h, --help`: Show help message
- `-V, --version`: Show version

## Report Format

The analysis report includes:

```
ZIP-Revelio Analysis Report
=========================

Summary:
Total Files: 100
Total Size: 1048576 bytes
Compressed Size: 524288 bytes
Compression Ratio: 50.00%
Processing Time: 1.234s

Compression Methods:
- Store: 20 files
- Deflate: 80 files

Errors:
- Invalid signature at offset 1234
```

## Development

### Prerequisites

- Rust 1.70 or later
- Cargo

### Building

```bash
# Clone repository
git clone https://github.com/amuldotexe/zip-revelio.git
cd zip-revelio

# Build
cargo build --release
```

### Testing

```bash
# Run tests
cargo test

# Run benchmarks
cargo bench
```

### Test Data

The `test_data` directory contains:
- `1mb.zip`: Standard test file
- `10mb.zip`: Large file test
- `corrupted.zip`: Error handling test

## Performance

Benchmarks on a typical system:
- 1MB ZIP: ~50ms
- 10MB ZIP: ~200ms
- Memory usage: <10MB

## Architecture

- `AsyncZipReader`: Efficient async ZIP parsing
- `ParallelProcessor`: Multi-threaded entry processing
- `Stats`: Thread-safe statistics collection
- `Reporter`: Analysis report generation

## Error Handling

- Comprehensive error types
- Detailed error context
- Safe error recovery
- Thread-safe error collection

## Contributing

1. Fork the repository
2. Create your feature branch
3. Make your changes
4. Run tests and benchmarks
5. Submit a pull request

## License

MIT License - see LICENSE file for details
