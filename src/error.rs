use std::path::PathBuf;
use thiserror::Error;
use crate::writer::ProgressUpdate;

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
    
    #[error("Corruption at {offset}, processed {processed_bytes} bytes")]
    Corrupt { 
        offset: u64, 
        processed_bytes: u64,
        partial_results: Option<PartialAnalysis>,
        recovery_possible: bool,
    },
    
    #[error("Memory limit exceeded: needed {required}MB, limit {max_allowed}MB")]
    Memory { 
        required: u64, 
        max_allowed: u64,
        current_usage: u64,
    },

    #[error("Progress reporting error: {msg}")]
    Progress { 
        msg: String 
    },

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
        match self {
            Self::Io { .. } => false,
            Self::Zip { .. } => false,
            Self::Corrupt { recovery_possible, .. } => *recovery_possible,
            Self::Memory { .. } => true,
            Self::Progress { .. } => true,
            Self::Other { .. } => true,
        }
    }

    pub fn should_retry(&self) -> bool {
        matches!(self, Self::Memory { .. } | Self::Progress { .. })
    }

    pub fn get_partial_results(&self) -> Option<&PartialAnalysis> {
        if let Self::Corrupt { partial_results, .. } = self {
            partial_results.as_ref()
        } else {
            None
        }
    }
}

pub type Result<T> = std::result::Result<T, AnalysisError>;

impl From<std::io::Error> for AnalysisError {
    fn from(error: std::io::Error) -> Self {
        AnalysisError::Io { 
            source: Box::new(error),
            offset: 0
        }
    }
}

impl From<anyhow::Error> for AnalysisError {
    fn from(error: anyhow::Error) -> Self {
        AnalysisError::Other { 
            source: error.to_string() 
        }
    }
}

impl From<zip::result::ZipError> for AnalysisError {
    fn from(error: zip::result::ZipError) -> Self {
        Self::Zip { source: Box::new(error) }
    }
}

impl From<tokio::sync::mpsc::error::SendError<ProgressUpdate>> for AnalysisError {
    fn from(error: tokio::sync::mpsc::error::SendError<ProgressUpdate>) -> Self {
        Self::Progress { 
            msg: error.to_string() 
        }
    }
}
