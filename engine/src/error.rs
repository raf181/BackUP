//! Error types for the transfer engine.
//!
//! The primary error type is `EngineError`, which represents job-level errors
//! that prevent a transfer from being executed. File-level errors are recorded
//! in the FileItem struct, not as EngineError.

use std::fmt::{Display, self};
use std::path::PathBuf;
use std::io;
use std::error::Error;

/// Errors that can occur at the job level (preventing execution or recovery).
///
/// These errors are typically non-recoverable and should stop the job.
/// File-level errors (per-file read/write failures) are recorded in FileItem,
/// not in this enum.
///
/// Note: EngineError wraps io::Error but is not directly serializable
/// (io::Error itself is not Serialize). For job persistence in future milestones,
/// convert to a serializable error type.
#[derive(Debug)]
pub enum EngineError {
    /// Source directory does not exist
    SourceNotFound { path: PathBuf },

    /// Source directory is not accessible (permissions)
    SourceAccessDenied { path: PathBuf, source: io::Error },

    /// Destination is not accessible
    DestinationAccessDenied { path: PathBuf, source: io::Error },

    /// Failed to read from source file
    ReadError { path: PathBuf, source: io::Error },

    /// Failed to write to destination file
    WriteError { path: PathBuf, source: io::Error },

    /// Path exceeds Windows limits or is invalid
    PathTooLong { path: PathBuf },

    /// Path contains invalid characters
    InvalidPath { path: PathBuf, reason: String },

    /// Failed to enumerate source directory
    EnumerationFailed { path: PathBuf, source: io::Error },

    /// Failed to create a directory
    DirectoryCreationFailed { path: PathBuf, source: io::Error },

    /// Catch-all for unexpected errors
    Unknown { message: String },
}

impl Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceNotFound { path } => {
                write!(f, "Source directory not found: {}", path.display())
            }
            Self::SourceAccessDenied { path, .. } => {
                write!(f, "Source directory access denied: {}", path.display())
            }
            Self::DestinationAccessDenied { path, .. } => {
                write!(f, "Destination directory access denied: {}", path.display())
            }
            Self::ReadError { path, .. } => {
                write!(f, "Failed to read file: {}", path.display())
            }
            Self::WriteError { path, .. } => {
                write!(f, "Failed to write file: {}", path.display())
            }
            Self::PathTooLong { path } => {
                write!(f, "Path exceeds maximum length: {}", path.display())
            }
            Self::InvalidPath { path, reason } => {
                write!(f, "Invalid path: {} ({})", path.display(), reason)
            }
            Self::EnumerationFailed { path, .. } => {
                write!(f, "Failed to enumerate directory: {}", path.display())
            }
            Self::DirectoryCreationFailed { path, .. } => {
                write!(f, "Failed to create directory: {}", path.display())
            }
            Self::Unknown { message } => {
                write!(f, "Engine error: {}", message)
            }
        }
    }
}

impl Error for EngineError {}

impl EngineError {
    /// Extract the OS error code from this error, if available.
    pub fn raw_os_error(&self) -> Option<u32> {
        match self {
            Self::SourceAccessDenied { source, .. }
            | Self::DestinationAccessDenied { source, .. }
            | Self::ReadError { source, .. }
            | Self::WriteError { source, .. }
            | Self::EnumerationFailed { source, .. }
            | Self::DirectoryCreationFailed { source, .. } => {
                source.raw_os_error().map(|e| e as u32)
            }
            _ => None,
        }
    }
}

impl From<io::Error> for EngineError {
    fn from(err: io::Error) -> Self {
        EngineError::Unknown {
            message: err.to_string(),
        }
    }
}
