// ZIP-Revelio Core Library

// Module declarations
pub mod buffer;
pub mod report;
pub mod utils;
pub mod worker;
pub mod zip;

// Public exports
pub use utils::error::{ErrorContext, ZipError};

// Re-export key types
pub use buffer::pool::BufferPool;
pub use worker::pool::WorkerPool;

// Prelude for convenient imports
pub mod prelude {
    pub use super::{BufferPool, ErrorContext, WorkerPool, ZipError};
}
