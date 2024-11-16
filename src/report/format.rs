use super::Stats;
use byte_unit::{Byte, UnitType};
use std::fmt::Write;
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub struct ReportFormatter {
    stats: Arc<Stats>,
}

impl ReportFormatter {
    pub fn new(stats: Arc<Stats>) -> Self {
        Self { stats }
    }

    pub fn generate(&self) -> String {
        let mut report = String::new();

        // Header
        writeln!(report, "=== ZIP Analysis Report ===\n").unwrap();

        // Size Information
        let total = Byte::from_u128(self.stats.total_size.load(Ordering::Relaxed) as u128)
            .unwrap()
            .get_appropriate_unit(UnitType::Binary);

        writeln!(report, "Total size: {}", total).unwrap();
        writeln!(
            report,
            "Files analyzed: {}",
            self.stats.file_count.load(Ordering::Relaxed)
        )
        .unwrap();
        writeln!(
            report,
            "Analysis time: {:.2}s\n",
            self.stats.duration.load(Ordering::Relaxed) as f64 / 1000.0
        )
        .unwrap();

        // Compression Information
        writeln!(
            report,
            "Overall compression ratio: {:.1}%",
            self.stats.compression_ratio()
        )
        .unwrap();

        let compressed =
            Byte::from_u128(self.stats.compressed_size.load(Ordering::Relaxed) as u128)
                .unwrap()
                .get_appropriate_unit(UnitType::Binary);

        writeln!(report, "Total compressed: {}", compressed).unwrap();
        writeln!(report, "Total uncompressed: {}\n", total).unwrap();

        // File Types
        report.push_str("File types:\n");
        for entry in self.stats.file_types.iter() {
            writeln!(report, "  {}: {}", entry.key(), entry.value()).unwrap();
        }
        report.push('\n');

        // Compression Methods
        report.push_str("Compression methods:\n");
        for entry in self.stats.methods.iter() {
            let method_name = match *entry.key() {
                0 => "Stored",
                8 => "Deflated",
                _ => "Unknown",
            };
            writeln!(report, "  {}: {}", method_name, entry.value()).unwrap();
        }
        report.push('\n');

        // File List
        report.push_str("Files (sorted alphabetically):\n");
        for file in self.stats.files.read().iter() {
            writeln!(report, "  {}", file).unwrap();
        }

        report
    }
}
