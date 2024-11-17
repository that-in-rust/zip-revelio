use std::sync::Arc;
use tokio::{
    runtime::{Builder, Runtime},
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use crate::{
    error::ZipError,
    Result,
    stats::Stats,
};

/// Message types for task coordination
#[derive(Debug)]
pub enum TaskMessage {
    /// Process a chunk of data
    Process {
        data: Vec<u8>,
        response: oneshot::Sender<Result<()>>,
    },
    /// Update progress
    Progress {
        current: usize,
        total: usize,
    },
    /// Task completed
    Complete,
}

/// Async runtime manager for ZIP processing
pub struct AsyncRuntime {
    /// Tokio runtime instance
    runtime: Runtime,
    /// Task message sender
    tx: mpsc::Sender<TaskMessage>,
    /// Task message receiver
    rx: mpsc::Receiver<TaskMessage>,
    /// Shared statistics
    stats: Arc<Stats>,
}

impl AsyncRuntime {
    /// Creates a new async runtime with specified thread count
    pub fn new(thread_count: usize, stats: Arc<Stats>) -> Result<Self> {
        // Create runtime with specified threads
        let runtime = Builder::new_multi_thread()
            .worker_threads(thread_count)
            .thread_name("zip-revelio-worker")
            .enable_all()
            .build()
            .map_err(|e| ZipError::ThreadPool(e.to_string()))?;

        // Create message channels
        let (tx, rx) = mpsc::channel(1024);

        Ok(Self {
            runtime,
            tx,
            rx,
            stats,
        })
    }

    /// Spawns a new async task
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: std::future::Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.runtime.spawn(future)
    }

    /// Gets a sender for task messages
    pub fn sender(&self) -> mpsc::Sender<TaskMessage> {
        self.tx.clone()
    }

    /// Starts the task processing loop
    pub async fn run(&mut self) -> Result<()> {
        while let Some(msg) = self.rx.recv().await {
            match msg {
                TaskMessage::Process { data, response } => {
                    let stats = Arc::clone(&self.stats);
                    let handle = self.spawn(async move {
                        // Process data chunk
                        let result = process_data(&data, &stats).await;
                        // Send response
                        let _ = response.send(result);
                    });

                    // Ensure task completes
                    handle.await.map_err(|e| ZipError::ThreadPool(e.to_string()))?;
                }
                TaskMessage::Progress { current, total } => {
                    // Update progress stats
                    self.stats.update_progress(current, total);
                }
                TaskMessage::Complete => {
                    break;
                }
            }
        }
        Ok(())
    }
}

/// Processes a chunk of data asynchronously
async fn process_data(data: &[u8], stats: &Stats) -> Result<()> {
    // Simulate some async processing
    tokio::task::yield_now().await;
    
    // Update stats
    stats.add_size(data.len() as u64);
    stats.increment_files();
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;

    #[test]
    fn test_async_runtime() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let stats = Arc::new(Stats::new());
            let mut runtime = AsyncRuntime::new(2, Arc::clone(&stats)).unwrap();
            
            // Create sender
            let tx = runtime.sender();
            
            // Spawn processing task
            let handle = tokio::spawn(async move {
                runtime.run().await
            });
            
            // Send process message
            let (response_tx, response_rx) = oneshot::channel();
            tx.send(TaskMessage::Process {
                data: vec![1, 2, 3],
                response: response_tx,
            }).await.unwrap();
            
            // Wait for response
            let result = response_rx.await.unwrap();
            assert!(result.is_ok());
            
            // Complete processing
            tx.send(TaskMessage::Complete).await.unwrap();
            
            // Wait for runtime to finish
            handle.await.unwrap().unwrap();
            
            // Verify stats
            assert_eq!(stats.total_files(), 1);
            assert_eq!(stats.total_size(), 3);
        });
    }

    #[test]
    fn test_progress_tracking() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let stats = Arc::new(Stats::new());
            let mut runtime = AsyncRuntime::new(2, Arc::clone(&stats)).unwrap();
            
            // Create sender
            let tx = runtime.sender();
            
            // Spawn processing task
            let handle = tokio::spawn(async move {
                runtime.run().await
            });
            
            // Send progress updates
            tx.send(TaskMessage::Progress { current: 50, total: 100 }).await.unwrap();
            tx.send(TaskMessage::Complete).await.unwrap();
            
            // Wait for runtime to finish
            handle.await.unwrap().unwrap();
        });
    }
}
