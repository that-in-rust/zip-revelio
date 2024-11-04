use std::path::PathBuf;
use chrono::{DateTime, Utc};
use crate::error::AnalysisError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    pub path: PathBuf,
    pub size: u64,
    pub compressed_size: u64,
    pub compression_method: CompressionMethod,
    pub crc32: u32,
    pub modified: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompressionMethod {
    Stored,
    Deflated,
    Other(u16),
}

impl From<zip::CompressionMethod> for CompressionMethod {
    fn from(method: zip::CompressionMethod) -> Self {
        match method {
            zip::CompressionMethod::Stored => Self::Stored,
            zip::CompressionMethod::Deflated => Self::Deflated,
            other => Self::Other(other.to_u16()),
        }
    }
}

impl std::fmt::Display for CompressionMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stored => write!(f, "Stored"),
            Self::Deflated => write!(f, "Deflated"),
            Self::Other(n) => write!(f, "Method({})", n)
        }
    }
}

impl TryFrom<u16> for CompressionMethod {
    type Error = AnalysisError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Stored),
            8 => Ok(Self::Deflated),
            other => Ok(Self::Other(other)),
        }
    }
}
