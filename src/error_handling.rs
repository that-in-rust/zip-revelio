use std::{
    error::Error,
    fmt::{self, Display},
    io,
    sync::Arc,
    time::Duration,
};
use parking_lot::Mutex;
use tokio::sync::broadcast;

/// Error context for detailed error information
#[derive(Debug, Clone)]
pub struct ErrorContext {
    /// Error message
    message: String,
    /// Error source file
    file: String,
    /// Error line number
    line: u32,
    /// Error timestamp
    timestamp: chrono::DateTime<chrono::Utc>,
    /// Additional context
    context: Option<String>,
}

impl ErrorContext {
    /// Creates a new error context
    pub fn new(message: impl Into<String>, file: impl Into<String>, line: u32) -> Self {
        Self {
            message: message.into(),
            file: file.into(),
            line,
            timestamp: chrono::Utc::now(),
            context: None,
        }
    }

    /// Adds additional context
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }
}

/// Error recovery strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryStrategy {
    /// Retry the operation
    Retry {
        /// Maximum number of retries
        max_retries: u32,
        /// Delay between retries
        delay: Duration,
    },
    /// Skip the operation
    Skip,
    /// Abort the operation
    Abort,
}

/// Error handler for managing error recovery
#[derive(Debug)]
pub struct ErrorHandler {
    /// Error contexts
    contexts: Arc<Mutex<Vec<ErrorContext>>>,
    /// Error notification channel
    notifier: broadcast::Sender<ErrorContext>,
    /// Default recovery strategy
    default_strategy: RecoveryStrategy,
    /// Maximum stored contexts
    max_contexts: usize,
}

impl ErrorHandler {
    /// Creates a new error handler
    pub fn new(max_contexts: usize, default_strategy: RecoveryStrategy) -> Self {
        let (notifier, _) = broadcast::channel(100);
        Self {
            contexts: Arc::new(Mutex::new(Vec::with_capacity(max_contexts))),
            notifier,
            default_strategy,
            max_contexts,
        }
    }

    /// Handles an error with context
    pub fn handle(&self, error: impl Error, context: ErrorContext) -> RecoveryStrategy {
        let mut contexts = self.contexts.lock();
        if contexts.len() >= self.max_contexts {
            contexts.remove(0);
        }
        contexts.push(context.clone());
        let _ = self.notifier.send(context);
        self.default_strategy
    }

    /// Subscribes to error notifications
    pub fn subscribe(&self) -> broadcast::Receiver<ErrorContext> {
        self.notifier.subscribe()
    }

    /// Gets all error contexts
    pub fn contexts(&self) -> Vec<ErrorContext> {
        self.contexts.lock().clone()
    }

    /// Clears error contexts
    pub fn clear(&self) {
        self.contexts.lock().clear();
    }
}

/// Error recovery executor
#[derive(Debug)]
pub struct RecoveryExecutor {
    /// Error handler
    handler: Arc<ErrorHandler>,
}

impl RecoveryExecutor {
    /// Creates a new recovery executor
    pub fn new(handler: Arc<ErrorHandler>) -> Self {
        Self { handler }
    }

    /// Executes an operation with recovery
    pub async fn execute<F, T>(&self, operation: F) -> Result<T, Box<dyn Error + Send + Sync>>
    where
        F: Fn() -> Result<T, Box<dyn Error + Send + Sync>> + Send + Sync,
    {
        let mut retries = 0;
        loop {
            match operation() {
                Ok(result) => return Ok(result),
                Err(error) => {
                    let context = ErrorContext::new(
                        error.to_string(),
                        file!(),
                        line!(),
                    );
                    
                    match self.handler.handle(error, context) {
                        RecoveryStrategy::Retry { max_retries, delay } => {
                            if retries >= max_retries {
                                return Err("Maximum retries exceeded".into());
                            }
                            retries += 1;
                            tokio::time::sleep(delay).await;
                            continue;
                        }
                        RecoveryStrategy::Skip => return Ok(unsafe { std::mem::zeroed() }),
                        RecoveryStrategy::Abort => return Err("Operation aborted".into()),
                    }
                }
            }
        }
    }
}

/// Error reporter for formatting and logging errors
#[derive(Debug)]
pub struct ErrorReporter {
    /// Error handler
    handler: Arc<ErrorHandler>,
}

impl ErrorReporter {
    /// Creates a new error reporter
    pub fn new(handler: Arc<ErrorHandler>) -> Self {
        Self { handler }
    }

    /// Generates an error report
    pub fn generate_report(&self) -> String {
        let contexts = self.handler.contexts();
        let mut report = String::new();

        report.push_str("Error Report\n");
        report.push_str("===========\n\n");

        for (i, context) in contexts.iter().enumerate() {
            report.push_str(&format!("Error #{}\n", i + 1));
            report.push_str(&format!("Message: {}\n", context.message));
            report.push_str(&format!("Location: {}:{}\n", context.file, context.line));
            report.push_str(&format!("Timestamp: {}\n", context.timestamp));
            if let Some(ctx) = &context.context {
                report.push_str(&format!("Context: {}\n", ctx));
            }
            report.push_str("\n");
        }

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    use tokio::runtime::Runtime;

    #[test]
    fn test_error_context() {
        let context = ErrorContext::new("Test error", "test.rs", 42)
            .with_context("Additional info");
        
        assert_eq!(context.message, "Test error");
        assert_eq!(context.file, "test.rs");
        assert_eq!(context.line, 42);
        assert_eq!(context.context, Some("Additional info".to_string()));
    }

    #[test]
    fn test_error_handler() {
        let handler = ErrorHandler::new(10, RecoveryStrategy::Skip);
        
        let context = ErrorContext::new("Test error", "test.rs", 42);
        let strategy = handler.handle(io::Error::new(io::ErrorKind::Other, "Test"), context);
        
        assert_eq!(strategy, RecoveryStrategy::Skip);
        assert_eq!(handler.contexts().len(), 1);
    }

    #[tokio::test]
    async fn test_recovery_executor() {
        let handler = Arc::new(ErrorHandler::new(
            10,
            RecoveryStrategy::Retry {
                max_retries: 3,
                delay: Duration::from_millis(100),
            },
        ));
        
        let executor = RecoveryExecutor::new(Arc::clone(&handler));
        let mut attempts = 0;
        
        let result = executor.execute(|| {
            attempts += 1;
            if attempts < 3 {
                Err("Temporary error".into())
            } else {
                Ok(42)
            }
        }).await;
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts, 3);
    }

    #[test]
    fn test_error_reporter() {
        let handler = Arc::new(ErrorHandler::new(10, RecoveryStrategy::Skip));
        let reporter = ErrorReporter::new(Arc::clone(&handler));
        
        let context = ErrorContext::new("Test error", "test.rs", 42)
            .with_context("Additional info");
        handler.handle(io::Error::new(io::ErrorKind::Other, "Test"), context);
        
        let report = reporter.generate_report();
        assert!(report.contains("Test error"));
        assert!(report.contains("test.rs:42"));
        assert!(report.contains("Additional info"));
    }
}
