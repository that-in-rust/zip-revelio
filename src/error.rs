use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnalysisError {
    #[error("IO error at offset {offset}: {source}")]
    Io { 
        source: Box<std::io::Error>,
        offset: u64 
    },
    
    #[error("ZIP error: {source}")]
    Zip { 
        source: Box<zip::result::ZipError>
    },
    
    #[error("Invalid input: {reason}")]
    InvalidInput { 
        reason: String 
    },
    
    #[error("Memory limit exceeded: needed {required}MB, limit {max_allowed}MB")]
    Memory { 
        required: u64, 
        max_allowed: u64,
        current_usage: u64,
    },

    #[error("Corrupt ZIP at {offset}, processed {processed_bytes} bytes")]
    Corrupt { 
        offset: u64, 
        processed_bytes: u64,
        partial_results: Option<PartialAnalysis>,
        recovery_possible: bool,
    },

    #[error("Channel error: {msg}")]
    Channel { 
        msg: String 
    },

    #[error("Task cancelled")]
    Cancelled,

    #[error("Other error: {source}")]
    Other { 
        source: String 
    },
}

#[derive(Debug, Clone)]
pub struct PartialAnalysis {
    pub processed_files: Vec<PathBuf>,
    pub bytes_processed: u64,
    pub is_recoverable: bool,
}

impl AnalysisError {
    pub fn is_recoverable(&self) -> bool {
        matches!(self,
            Self::Memory { .. } |
            Self::Corrupt { recovery_possible: true, .. } |
            Self::Channel { .. }
        )
    }

    pub fn should_retry(&self) -> bool {
        matches!(self, 
            Self::Memory { .. } |
            Self::Channel { .. }
        )
    }
}

pub type Result<T> = std::result::Result<T, AnalysisError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_recovery_logic() {
        let recoverable = AnalysisError::Memory {
            required: 100,
            max_allowed: 50,
            current_usage: 75,
        };
        assert!(recoverable.is_recoverable());
        assert!(recoverable.should_retry());

        let unrecoverable = AnalysisError::Corrupt {
            offset: 0,
            processed_bytes: 100,
            partial_results: None,
            recovery_possible: false,
        };
        assert!(!unrecoverable.is_recoverable());
        assert!(!unrecoverable.should_retry());
    }
}
