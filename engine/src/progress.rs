//! Progress reporting trait.
//!
//! This module defines the ProgressCallback trait, which allows decoupling
//! the transfer engine from any specific UI technology (CLI, GUI, etc.).
//!
//! Both CLI and GUI implementations can subscribe to job progress.

use crate::model::{TransferJob, FileItem};

/// Trait for receiving progress updates from a transfer job.
///
/// Implement this trait to receive callbacks during job execution.
/// The CLI provides a simple implementation for stdout output.
/// Future UI implementations (GUI, web, etc.) can also implement this trait.
///
/// All methods are called synchronously during job execution.
pub trait ProgressCallback: Send {
    /// Called when job execution starts.
    fn on_job_started(&self, job: &TransferJob);

    /// Called when a file is about to be processed.
    fn on_file_started(&self, job: &TransferJob, file_index: usize, file: &FileItem);

    /// Called periodically as bytes are copied for the current file.
    ///
    /// `bytes_this_file` is the number of bytes copied for the current file.
    fn on_file_progress(&self, job: &TransferJob, file_index: usize, bytes_this_file: u64);

    /// Called when a file is done (copied, skipped, or failed).
    fn on_file_completed(&self, job: &TransferJob, file_index: usize, file: &FileItem);

    /// Called when job execution is complete (all files processed).
    fn on_job_completed(&self, job: &TransferJob);
}
