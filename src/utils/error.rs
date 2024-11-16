use std::error::Error;
use std::fmt;

/// Comprehensive error type for ZIP-Revelio processing
#[derive(Debug)]
pub enum ZipError {
    /// IO-related errors with rich context
    Io {
        context: String,
        source: std::io::Error,
    },
    /// Memory-related errors with size tracking
    Memory {
        context: String,
        size: Option<usize>,
        limit: Option<usize>,
    },
    /// Format-related errors with entry details
    Format {
        message: String,
        entry: Option<String>,
        details: Option<String>,
    },
    /// Resource management errors
    Resource {
        kind: ResourceErrorKind,
        message: String,
    },
    /// Worker and thread-related errors
    Worker {
        thread_id: Option<usize>,
        error_type: WorkerErrorType,
    },
    /// No buffer available
    NoBufferAvailable,
}

// Implement Clone manually to avoid Clone bound on std::io::Error
impl Clone for ZipError {
    fn clone(&self) -> Self {
        match self {
            ZipError::Io { context, source } => ZipError::Io {
                context: context.clone(),
                source: std::io::Error::new(source.kind(), source.to_string()),
            },
            ZipError::Memory {
                context,
                size,
                limit,
            } => ZipError::Memory {
                context: context.clone(),
                size: size.clone(),
                limit: limit.clone(),
            },
            ZipError::Format {
                message,
                entry,
                details,
            } => ZipError::Format {
                message: message.clone(),
                entry: entry.clone(),
                details: details.clone(),
            },
            ZipError::Resource { kind, message } => ZipError::Resource {
                kind: kind.clone(),
                message: message.clone(),
            },
            ZipError::Worker {
                thread_id,
                error_type,
            } => ZipError::Worker {
                thread_id: thread_id.clone(),
                error_type: error_type.clone(),
            },
            ZipError::NoBufferAvailable => ZipError::NoBufferAvailable,
        }
    }
}

/// Granular resource error categorization
#[derive(Debug, Clone)]
pub enum ResourceErrorKind {
    PoolExhausted,
    AllocationFailed,
    LimitExceeded,
}

/// Specific worker error types
#[derive(Debug, Clone)]
pub enum WorkerErrorType {
    Panic,
    ConfigurationError,
    TaskDispatchError,
}

impl Error for ZipError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ZipError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl fmt::Display for ZipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ZipError::Io { context, source } => write!(f, "IO Error: {} - {}", context, source),
            ZipError::Memory {
                context,
                size,
                limit,
            } => write!(
                f,
                "Memory Error: {} (Size: {:?}, Limit: {:?})",
                context, size, limit
            ),
            ZipError::Format {
                message,
                entry,
                details,
            } => write!(
                f,
                "Format Error: {} (Entry: {:?}, Details: {:?})",
                message, entry, details
            ),
            ZipError::Resource { kind, message } => {
                write!(f, "Resource Error: {:?} - {}", kind, message)
            }
            ZipError::Worker {
                thread_id,
                error_type,
            } => write!(
                f,
                "Worker Error: {:?} (Thread: {:?})",
                error_type, thread_id
            ),
            ZipError::NoBufferAvailable => write!(f, "No buffer available"),
        }
    }
}

/// Error context extension trait
pub trait ErrorContext<T> {
    /// Add context to an error
    fn context(self, message: impl ToString) -> Result<T, ZipError>;

    /// Add entry-specific context to an error
    fn with_entry(self, entry_name: impl ToString) -> Result<T, ZipError>;
}

impl<T, E: Error + 'static> ErrorContext<T> for Result<T, E> {
    fn context(self, message: impl ToString) -> Result<T, ZipError> {
        self.map_err(|e| ZipError::Format {
            message: message.to_string(),
            entry: None,
            details: Some(e.to_string()),
        })
    }

    fn with_entry(self, entry_name: impl ToString) -> Result<T, ZipError> {
        self.map_err(|e| ZipError::Format {
            message: "Error processing entry".into(),
            entry: Some(entry_name.to_string()),
            details: Some(e.to_string()),
        })
    }
}

// Conversion traits for common error types
impl From<std::io::Error> for ZipError {
    fn from(err: std::io::Error) -> Self {
        ZipError::Io {
            context: "IO operation failed".into(),
            source: err,
        }
    }
}
