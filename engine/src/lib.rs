//! # BackUP Engine - File Transfer Library
//!
//! A robust, headless file transfer engine for Windows in Rust.
//! Designed as the foundation for multiple UIs (CLI, GUI, automation).
//!
//! ## Overview
//!
//! The engine provides a core library for copying and moving files and directory trees.
//! It features:
//! - Recursive directory enumeration
//! - Per-file state tracking and error isolation
//! - Configurable overwrite policies
//! - Progress reporting via callbacks (decoupled from UI technology)
//! - Comprehensive error handling
//!
//! ## Basic Usage
//!
//! ```no_run
//! use engine::{create_job, plan_job, run_job, Mode, OverwritePolicy};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a job
//! let mut job = create_job(
//!     "C:\\source",
//!     "D:\\destination",
//!     Mode::Copy,
//!     OverwritePolicy::Skip,
//! )?;
//!
//! // Plan the job (enumerate source tree)
//! plan_job(&mut job)?;
//! println!("Will copy {} files", job.files.len());
//!
//! // Run the job (execute transfer)
//! run_job(&mut job, None)?;
//!
//! // Check results
//! for file in &job.files {
//!     println!("{:?}: {:?}", file.source_path, file.state);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Modules
//!
//! - **model**: Core data structures (TransferJob, FileItem, enums)
//! - **error**: Error types and handling
//! - **fs_ops**: Low-level filesystem operations
//! - **job**: Job orchestration (create, plan, run)
//! - **progress**: Progress callback trait
//! - **checksums**: Checksum computation and verification

pub mod model;
pub mod error;
pub mod fs_ops;
pub mod job;
pub mod progress;
pub mod checksums;

// Re-export main types and functions
pub use model::{
    TransferJob, FileItem, Mode, FileState, JobState, OverwritePolicy, JobMetadata, FileMetadata,
};
pub use error::EngineError;
pub use job::{create_job, plan_job, run_job};
pub use progress::ProgressCallback;
pub use checksums::{ChecksumAlgorithm, ChecksumValue, verify_file_item, compute_file_checksum};
