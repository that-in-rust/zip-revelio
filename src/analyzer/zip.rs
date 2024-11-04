use std::path::PathBuf;
use tokio::{
    sync::{mpsc, watch},
    io::{AsyncReadExt, BufReader},
    time::Duration,
};
use crate::{
    error::{Result, AnalysisError},
    models::ZipAnalysis,
    writer::progress::ProgressUpdate,
};
use super::{
    chunks::{ChunkConfig, ChunkProcessor, ChunkResult},
};

#[derive(Debug, Clone)]
pub enum ControlSignal {
    Continue,
    Pause,
    Stop { graceful: bool },
}

pub struct AnalyzerChannels {
    progress_tx: mpsc::Sender<ProgressUpdate>,
    chunk_results_tx: mpsc::Sender<ChunkResult>,
    control_tx: watch::Sender<ControlSignal>,
}

pub struct ParallelZipAnalyzer {
    zip_path: PathBuf,
    chunk_size: usize,
    thread_count: usize,
    chunk_processor: ChunkProcessor,
}

impl ParallelZipAnalyzer {
    pub fn new(zip_path: PathBuf) -> Self {
        let config = ChunkConfig::default();
        Self {
            zip_path,
            chunk_size: config.chunk_size,
            thread_count: num_cpus::get() - 1,
            chunk_processor: ChunkProcessor::new(config),
        }
    }

    pub async fn analyze(&self) -> Result<ZipAnalysis> {
        let start_time = std::time::Instant::now();
        let file = tokio::fs::File::open(&self.zip_path).await
            .map_err(|e| AnalysisError::Io { source: e, offset: 0 })?;
        
        let file_size = file.metadata().await
            .map_err(|e| AnalysisError::Io { source: e, offset: 0 })?.len();
        
        let (progress_tx, _) = mpsc::channel(100);
        let (chunk_results_tx, mut chunk_results_rx) = mpsc::channel(100);
        let (control_tx, mut control_rx) = watch::channel(ControlSignal::Continue);

        let mut chunk_results = Vec::new();
        let mut current_offset = 0;
        let mut buffer = vec![0; self.chunk_size];

        let mut file = tokio::io::BufReader::new(file);

        while current_offset < file_size {
            // Check for control signals
            if let Ok(signal) = control_rx.has_changed() {
                match control_rx.borrow().clone() {
                    ControlSignal::Stop { graceful: true } => break,
                    ControlSignal::Stop { graceful: false } => {
                        return Err(AnalysisError::Progress { 
                            msg: "Analysis interrupted".to_string() 
                        });
                    },
                    ControlSignal::Pause => {
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        continue;
                    },
                    ControlSignal::Continue => {}
                }
            }

            let bytes_read = file.read(&mut buffer).await
                .map_err(|e| AnalysisError::Io { 
                    source: e, 
                    offset: current_offset 
                })?;

            if bytes_read == 0 {
                break;
            }

            let chunk = &buffer[..bytes_read];
            let result = self.chunk_processor.process_chunk(chunk, current_offset).await?;
            
            // Update progress
            progress_tx.send(ProgressUpdate {
                bytes_processed: current_offset + bytes_read as u64,
                files_processed: result.files.len(),
                current_file: result.files.last()
                    .map(|f| f.path.display().to_string())
                    .unwrap_or_default(),
                chunk_offset: current_offset,
                compression_ratio: result.compressed_size as f64 / result.uncompressed_size as f64,
                estimated_remaining_secs: ((file_size - current_offset) * start_time.elapsed().as_secs() as u64) / current_offset,
                error_count: if result.error.is_some() { 1 } else { 0 },
            }).await.map_err(|e| AnalysisError::Progress { 
                msg: e.to_string() 
            })?;

            chunk_results.push(result);
            current_offset += bytes_read as u64;
        }

        let duration = start_time.elapsed();
        let mut analysis = ChunkProcessor::merge_results(chunk_results)?;
        analysis.stats.duration_ms = duration.as_millis() as u64;
        
        Ok(analysis)
    }
}
