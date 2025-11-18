//! Core data model for transfer jobs.
//!
//! This module defines the main data structures for representing transfer operations:
//! - TransferJob: the entire copy/move operation
//! - FileItem: a single file within a job
//! - Mode, FileState, JobState, OverwritePolicy: enums controlling behavior

use std::path::PathBuf;
use std::time::SystemTime;
use uuid::Uuid;

/// Represents a single transfer job (e.g., copy or move operation).
///
/// A TransferJob encompasses:
/// - Source and destination directories
/// - All files and directories to be transferred
/// - Current state and progress tracking
/// - Optional error information
#[derive(Debug)]
pub struct TransferJob {
    /// Unique identifier for this job
    pub id: Uuid,

    /// Operation mode: Copy or Move
    pub mode: Mode,

    /// Root source directory
    pub source_path: PathBuf,

    /// Root destination directory
    pub destination_path: PathBuf,

    /// How to handle existing files
    pub overwrite_policy: OverwritePolicy,

    /// All files and directories in this job
    pub files: Vec<FileItem>,

    /// Current job state (Pending, Running, Completed)
    pub state: JobState,

    /// Job-level error (if any)
    pub error: Option<crate::error::EngineError>,

    /// Total bytes to copy (sum of all file sizes)
    pub total_bytes_to_copy: u64,

    /// Bytes copied so far
    pub total_bytes_copied: u64,

    /// Index of currently processing file (if Running)
    pub current_file_index: Option<usize>,

    /// When job was created
    pub created_at: SystemTime,

    /// When job execution started
    pub start_time: Option<SystemTime>,

    /// When job execution completed
    pub end_time: Option<SystemTime>,

    /// Optional metadata (extension point for future features)
    pub metadata: JobMetadata,

    /// Optional checksum algorithm for this job (for verify-after-copy)
    pub checksum_algorithm: Option<crate::checksums::ChecksumAlgorithm>,

    /// Whether to verify files after copying (compare source and dest checksums)
    pub verify_after_copy: bool,
}

/// Optional metadata for a job (reserved for future use).
#[derive(Debug, Clone)]
pub struct JobMetadata {
    /// Custom user data (reserved for future use)
    pub custom_data: Option<String>,
}

/// Represents a single file or directory within a transfer job.
#[derive(Debug, Clone)]
pub struct FileItem {
    /// Unique identifier for this file within the job
    pub id: Uuid,

    /// Full source path
    pub source_path: PathBuf,

    /// Full destination path
    pub destination_path: PathBuf,

    /// File size in bytes (0 for directories)
    pub file_size: u64,

    /// Current state of this file
    pub state: FileState,

    /// Bytes copied for this file (progress tracking)
    pub bytes_copied: u64,

    /// OS error code if state is Failed or Skipped
    pub error_code: Option<u32>,

    /// Human-readable error message
    pub error_message: Option<String>,

    /// True if this item represents a directory
    pub is_dir: bool,

    /// File modification time (Windows FILETIME, in 100ns intervals since 1601-01-01)
    pub last_modified: Option<u64>,

    /// Optional metadata for future extensions
    pub metadata: FileMetadata,
}

/// Optional metadata for a file item (reserved for future features).
#[derive(Debug, Clone)]
pub struct FileMetadata {
    /// Checksum of source file (computed during verification)
    pub source_checksum: Option<crate::checksums::ChecksumValue>,
    /// Checksum of destination file (computed during verification)
    pub dest_checksum: Option<crate::checksums::ChecksumValue>,
    /// Whether verification passed (true if checksums match after copy)
    pub verification_passed: Option<bool>,
    /// Reserved for future attributes
    pub attributes: Option<u32>,
}

/// The operation mode for a transfer job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Copy files; source remains unchanged
    Copy,
    /// Move files; source deleted after successful copy (deferred to Milestone 2)
    Move,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Copy => write!(f, "Copy"),
            Mode::Move => write!(f, "Move"),
        }
    }
}

/// The state of an individual file within a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileState {
    /// Not yet processed
    Pending,
    /// Currently transferring
    Copying,
    /// Successfully copied or directory created
    Done,
    /// Not copied due to overwrite policy or other skip condition
    Skipped,
    /// Error occurred; file not copied
    Failed,
}

impl FileState {
    /// Returns true if this state is terminal (no further changes expected).
    pub fn is_terminal(&self) -> bool {
        matches!(self, FileState::Done | FileState::Skipped | FileState::Failed)
    }
}

/// The state of an entire transfer job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    /// Created, not yet started
    Pending,
    /// Currently executing
    Running,
    /// All files processed (some may have failed)
    Completed,
}

/// Policy for handling existing files at the destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverwritePolicy {
    /// Don't overwrite; skip existing files
    Skip,
    /// Always overwrite existing files
    Overwrite,
    /// Ask user (Milestone 1: defaults to Skip in CLI, extensible for future UI)
    Ask,
    /// Overwrite if source is newer OR size differs
    SmartUpdate,
}

impl std::fmt::Display for OverwritePolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OverwritePolicy::Skip => write!(f, "Skip"),
            OverwritePolicy::Overwrite => write!(f, "Overwrite"),
            OverwritePolicy::SmartUpdate => write!(f, "SmartUpdate"),
            OverwritePolicy::Ask => write!(f, "Ask"),
        }
    }
}
