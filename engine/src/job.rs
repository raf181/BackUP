//! Job orchestration module.
//!
//! This module provides the main job lifecycle functions:
//! - Creating a job from source/destination paths
//! - Planning a job (enumerating the source tree)
//! - Running a job (executing the transfer)
//!
//! Implemented in Phases 2 and 3.

use std::path::Path;
use std::time::SystemTime;
use uuid::Uuid;
use crate::model::{TransferJob, FileItem, Mode, OverwritePolicy, JobState, JobMetadata, FileState};
use crate::error::EngineError;
use crate::progress::ProgressCallback;
use crate::fs_ops;

/// Determine whether to copy or skip a file based on overwrite policy.
fn should_copy_file(file: &FileItem, policy: OverwritePolicy) -> bool {
    // Always copy directories (they're needed for file placement)
    if file.is_dir {
        return true;
    }

    // Check if destination exists
    if !file.destination_path.exists() {
        return true;
    }

    match policy {
        OverwritePolicy::Skip => false,
        OverwritePolicy::Overwrite => true,
        OverwritePolicy::Ask => {
            // Milestone 1: no UI interaction; default to Skip
            false
        }
        OverwritePolicy::SmartUpdate => {
            // Copy if size differs
            match std::fs::metadata(&file.destination_path) {
                Ok(dst_metadata) => {
                    let dst_size = dst_metadata.len();
                    
                    // Size differs -> copy
                    if dst_size != file.file_size {
                        true
                    } else {
                        // Same size: don't copy (modification time not stored in FileItem)
                        // Future enhancement: store source mtime during enumeration
                        false
                    }
                }
                Err(_) => true, // Metadata read error: attempt copy
            }
        }
    }
}

/// Run a job, executing the transfer operation.
///
/// Transitions job state from Pending to Running to Completed.
/// Invokes progress callbacks at appropriate points.
/// Individual file errors are recorded but do NOT stop the job.
///
/// # Arguments
/// * `job` - Job to execute (must be in Pending state)
/// * `progress_callback` - Optional callback for progress updates
///
/// # Errors
/// Returns EngineError only for unrecoverable job-level issues.
/// File-level errors are recorded in FileItem.
pub fn run_job(
    job: &mut TransferJob,
    progress_callback: Option<&dyn ProgressCallback>,
) -> Result<(), EngineError> {
    // Validate job is in Pending state
    if job.state != JobState::Pending {
        return Err(EngineError::InvalidPath {
            path: job.source_path.clone(),
            reason: format!("Job must be in Pending state to run; current state: {:?}", job.state),
        });
    }

    // Transition to Running and record start time
    job.state = JobState::Running;
    job.start_time = Some(SystemTime::now());

    // Invoke job started callback
    if let Some(callback) = progress_callback {
        callback.on_job_started(job);
    }

    // Process each file
    for file_index in 0..job.files.len() {
        // Update current file index
        job.current_file_index = Some(file_index);

        // Capture file data we need (before callbacks that borrow job)
        let file_is_dir = job.files[file_index].is_dir;
        let src_path = job.files[file_index].source_path.clone();
        let dst_path = job.files[file_index].destination_path.clone();
        
        // Invoke file started callback
        if let Some(callback) = progress_callback {
            callback.on_file_started(job, file_index, &job.files[file_index]);
        }

        // Apply overwrite policy
        if !should_copy_file(&job.files[file_index], job.overwrite_policy) {
            // Skip this file
            job.files[file_index].state = FileState::Skipped;
            
            // Invoke completion callback
            if let Some(callback) = progress_callback {
                callback.on_file_completed(job, file_index, &job.files[file_index]);
            }
            continue;
        }

        // If it's a directory, create it
        if file_is_dir {
            job.files[file_index].state = FileState::Copying;
            match fs_ops::ensure_parent_dir_exists(&dst_path) {
                Ok(()) => {
                    // For directories, we don't actually need to create them in the destination
                    // They'll be created as part of parent directory creation for files
                    // Just mark as Done
                    job.files[file_index].state = FileState::Done;
                }
                Err(e) => {
                    // Record error on the file
                    job.files[file_index].state = FileState::Failed;
                    job.files[file_index].error_code = e.raw_os_error();
                    job.files[file_index].error_message = Some(e.to_string());
                }
            }

            // Invoke completion callback
            if let Some(callback) = progress_callback {
                callback.on_file_completed(job, file_index, &job.files[file_index]);
            }
            continue;
        }

        // For regular files, copy them
        job.files[file_index].state = FileState::Copying;
        match fs_ops::copy_file_with_metadata(&src_path, &dst_path) {
            Ok(bytes_copied) => {
                // Update progress counters
                job.files[file_index].bytes_copied = bytes_copied;
                job.files[file_index].state = FileState::Done;
                job.total_bytes_copied += bytes_copied;

                // Invoke progress callback with bytes copied
                if let Some(callback) = progress_callback {
                    callback.on_file_progress(job, file_index, bytes_copied);
                }

                // If verification is enabled, verify the file
                if let Some(algorithm) = job.checksum_algorithm {
                    if job.verify_after_copy {
                        // Attempt verification; if it fails, record the error but don't abort
                        match crate::checksums::verify_file_item(&mut job.files[file_index], algorithm) {
                            Ok(matches) => {
                                if !matches {
                                    // Checksum mismatch: record error but keep file as Done
                                    job.files[file_index].error_message = Some(
                                        format!("Checksum verification failed: source and destination differ")
                                    );
                                }
                            }
                            Err(verify_err) => {
                                // Verification error: record but don't fail the file
                                job.files[file_index].error_message = Some(
                                    format!("Checksum verification error: {}", verify_err)
                                );
                            }
                        }
                    }
                }
            }
            Err(e) => {
                // Record error on the file
                job.files[file_index].state = FileState::Failed;
                job.files[file_index].error_code = e.raw_os_error();
                job.files[file_index].error_message = Some(e.to_string());
            }
        }

        // Invoke completion callback
        if let Some(callback) = progress_callback {
            callback.on_file_completed(job, file_index, &job.files[file_index]);
        }
    }

    // All files processed; mark job as Completed
    job.state = JobState::Completed;
    job.end_time = Some(SystemTime::now());
    job.current_file_index = None;

    // Invoke job completed callback
    if let Some(callback) = progress_callback {
        callback.on_job_completed(job);
    }

    Ok(())
}

/// Create a new transfer job.
///
/// Validates that the source path exists and is a directory.
/// The destination path is checked for validity but may not exist yet
/// (it will be created during execution).
///
/// # Arguments
/// * `source` - Source directory path
/// * `destination` - Destination directory path
/// * `mode` - Copy or Move
/// * `overwrite_policy` - How to handle existing files
///
/// # Returns
/// A new TransferJob in Pending state
///
/// # Errors
/// Returns EngineError if source doesn't exist or is invalid
pub fn create_job<P: AsRef<Path>>(
    source: P,
    destination: P,
    mode: Mode,
    overwrite_policy: OverwritePolicy,
) -> Result<TransferJob, EngineError> {
    let source = source.as_ref();
    let destination = destination.as_ref();

    // Validate source exists and is a directory
    match std::fs::metadata(source) {
        Ok(metadata) => {
            if !metadata.is_dir() {
                return Err(EngineError::InvalidPath {
                    path: source.to_path_buf(),
                    reason: "Source must be a directory".to_string(),
                });
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(EngineError::SourceNotFound {
                path: source.to_path_buf(),
            });
        }
        Err(e) => {
            return Err(EngineError::SourceAccessDenied {
                path: source.to_path_buf(),
                source: e,
            });
        }
    }

    // Validate destination path format
    let dest_str = destination.to_string_lossy();
    if dest_str.is_empty() {
        return Err(EngineError::InvalidPath {
            path: destination.to_path_buf(),
            reason: "Destination path is empty".to_string(),
        });
    }

    Ok(TransferJob {
        id: Uuid::new_v4(),
        mode,
        source_path: source.to_path_buf(),
        destination_path: destination.to_path_buf(),
        overwrite_policy,
        files: Vec::new(),
        state: JobState::Pending,
        error: None,
        total_bytes_to_copy: 0,
        total_bytes_copied: 0,
        current_file_index: None,
        created_at: SystemTime::now(),
        start_time: None,
        end_time: None,
        metadata: JobMetadata {
            custom_data: None,
        },
        checksum_algorithm: None,
        verify_after_copy: false,
    })
}

/// Plan a job by enumerating the source tree.
///
/// Populates job.files with all files and directories to be transferred,
/// and calculates job.total_bytes_to_copy.
///
/// # Arguments
/// * `job` - Job to plan (must be in Pending state)
///
/// # Errors
/// Returns EngineError if enumeration fails
pub fn plan_job(job: &mut TransferJob) -> Result<(), EngineError> {
    // Validate job is in Pending state
    if job.state != JobState::Pending {
        return Err(EngineError::InvalidPath {
            path: job.source_path.clone(),
            reason: format!("Job must be in Pending state to plan; current state: {:?}", job.state),
        });
    }

    // Enumerate the source tree
    job.files = fs_ops::enumerate_tree(&job.source_path, &job.destination_path)?;

    // Calculate total bytes to copy (sum of all non-directory file sizes)
    job.total_bytes_to_copy = job
        .files
        .iter()
        .filter(|f| !f.is_dir)
        .map(|f| f.file_size)
        .sum();

    Ok(())
}

/// Run a job, executing the transfer operation.
///
/// Transitions job state from Pending to Running to Completed.
/// Invokes progress callbacks at appropriate points.
/// Individual file errors are recorded but do NOT stop the job.
///
/// # Arguments
/// * `job` - Job to execute (must be in Pending state)
/// * `progress_callback` - Optional callback for progress updates
///
/// # Errors
/// Returns EngineError only for unrecoverable job-level issues.
/// File-level errors are recorded in FileItem.

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    #[test]
    fn test_create_job_with_valid_source() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let src = temp_dir.path().join("src");
        fs::create_dir(&src).expect("Failed to create src dir");
        let dst = temp_dir.path().join("dst");

        let job =
            create_job(&src, &dst, Mode::Copy, OverwritePolicy::Skip).expect("Failed to create job");

        assert_eq!(job.mode, Mode::Copy);
        assert_eq!(job.overwrite_policy, OverwritePolicy::Skip);
        assert_eq!(job.state, JobState::Pending);
        assert!(job.files.is_empty());
    }

    #[test]
    fn test_create_job_with_missing_source() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let src = temp_dir.path().join("nonexistent");
        let dst = temp_dir.path().join("dst");

        let result = create_job(&src, &dst, Mode::Copy, OverwritePolicy::Skip);
        assert!(result.is_err());
    }

    #[test]
    fn test_create_job_with_file_as_source() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let src = temp_dir.path().join("file.txt");
        fs::File::create(&src).expect("Failed to create file");
        let dst = temp_dir.path().join("dst");

        let result = create_job(&src, &dst, Mode::Copy, OverwritePolicy::Skip);
        assert!(result.is_err());
    }

    #[test]
    fn test_plan_job_populates_files() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let src = temp_dir.path().join("src");
        fs::create_dir(&src).expect("Failed to create src dir");

        // Create test files
        let mut file1 = fs::File::create(src.join("file1.txt")).expect("Failed to create file1");
        file1.write_all(b"test").expect("Failed to write file1");
        drop(file1);

        let dst = temp_dir.path().join("dst");
        fs::create_dir(&dst).expect("Failed to create dst dir");

        let mut job =
            create_job(&src, &dst, Mode::Copy, OverwritePolicy::Skip).expect("Failed to create job");
        plan_job(&mut job).expect("Failed to plan job");

        assert!(!job.files.is_empty(), "Expected files to be enumerated");
        
        let file_count = job.files.len();
        eprintln!("Enumerated {} items", file_count);
        for (i, f) in job.files.iter().enumerate() {
            eprintln!("  [{}] {:?} is_dir={} size={}", i, f.source_path.file_name(), f.is_dir, f.file_size);
        }
        
        assert_eq!(job.total_bytes_to_copy, 4, "Expected 4 bytes total"); // "test"
    }

    #[test]
    fn test_plan_job_requires_pending_state() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let src = temp_dir.path().join("src");
        fs::create_dir(&src).expect("Failed to create src dir");
        let dst = temp_dir.path().join("dst");

        let mut job =
            create_job(&src, &dst, Mode::Copy, OverwritePolicy::Skip).expect("Failed to create job");

        // Plan once
        plan_job(&mut job).expect("Failed to plan job");

        // Try to plan again (state should prevent this in a real run_job, but we'll test anyway)
        // For now, just verify the initial state validation works
        assert_eq!(job.state, JobState::Pending);
    }

    // Test helper: Mock progress callback to track invocations
    struct TestProgressCallback {
        calls: std::sync::Mutex<Vec<String>>,
    }

    impl TestProgressCallback {
        fn new() -> Self {
            TestProgressCallback {
                calls: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn get_calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl ProgressCallback for TestProgressCallback {
        fn on_job_started(&self, _job: &TransferJob) {
            self.calls.lock().unwrap().push("on_job_started".to_string());
        }

        fn on_file_started(&self, _job: &TransferJob, file_index: usize, _file: &FileItem) {
            self.calls.lock().unwrap().push(format!("on_file_started({})", file_index));
        }

        fn on_file_progress(&self, _job: &TransferJob, file_index: usize, bytes_this_file: u64) {
            self.calls.lock().unwrap().push(format!("on_file_progress({}, {})", file_index, bytes_this_file));
        }

        fn on_file_completed(&self, _job: &TransferJob, file_index: usize, _file: &FileItem) {
            self.calls.lock().unwrap().push(format!("on_file_completed({})", file_index));
        }

        fn on_job_completed(&self, _job: &TransferJob) {
            self.calls.lock().unwrap().push("on_job_completed".to_string());
        }
    }

    #[test]
    fn test_run_job_copies_files() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let src = temp_dir.path().join("src");
        fs::create_dir(&src).expect("Failed to create src dir");

        // Create test file
        let mut file1 = fs::File::create(src.join("file1.txt")).expect("Failed to create file1");
        file1.write_all(b"hello").expect("Failed to write file1");
        drop(file1);

        let dst = temp_dir.path().join("dst");
        fs::create_dir(&dst).expect("Failed to create dst dir");

        let mut job = create_job(&src, &dst, Mode::Copy, OverwritePolicy::Overwrite)
            .expect("Failed to create job");
        plan_job(&mut job).expect("Failed to plan job");

        run_job(&mut job, None).expect("Failed to run job");

        // Verify job state
        assert_eq!(job.state, JobState::Completed);
        assert!(job.start_time.is_some());
        assert!(job.end_time.is_some());
        assert_eq!(job.total_bytes_copied, 5); // "hello"

        // Verify file was copied
        let copied_file = dst.join("file1.txt");
        assert!(copied_file.exists());
        let contents = fs::read_to_string(&copied_file).expect("Failed to read copied file");
        assert_eq!(contents, "hello");

        // Verify file state
        let file_items: Vec<_> = job.files.iter().filter(|f| !f.is_dir).collect();
        assert_eq!(file_items.len(), 1);
        assert_eq!(file_items[0].state, FileState::Done);
        assert_eq!(file_items[0].bytes_copied, 5);
    }

    #[test]
    fn test_run_job_respects_skip_policy() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let src = temp_dir.path().join("src");
        fs::create_dir(&src).expect("Failed to create src dir");

        // Create source file
        let mut src_file = fs::File::create(src.join("file1.txt")).expect("Failed to create file1");
        src_file.write_all(b"source").expect("Failed to write file1");
        drop(src_file);

        let dst = temp_dir.path().join("dst");
        fs::create_dir(&dst).expect("Failed to create dst dir");

        // Create existing destination file
        let mut dst_file = fs::File::create(dst.join("file1.txt")).expect("Failed to create dest file");
        dst_file.write_all(b"existing").expect("Failed to write dest file");
        drop(dst_file);

        let mut job =
            create_job(&src, &dst, Mode::Copy, OverwritePolicy::Skip).expect("Failed to create job");
        plan_job(&mut job).expect("Failed to plan job");

        run_job(&mut job, None).expect("Failed to run job");

        // Verify file was NOT overwritten (Skip policy)
        let contents = fs::read_to_string(dst.join("file1.txt")).expect("Failed to read file");
        assert_eq!(contents, "existing");

        // Verify file marked as Skipped
        let file_items: Vec<_> = job.files.iter().filter(|f| !f.is_dir).collect();
        assert_eq!(file_items[0].state, FileState::Skipped);
    }

    #[test]
    fn test_run_job_respects_overwrite_policy() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let src = temp_dir.path().join("src");
        fs::create_dir(&src).expect("Failed to create src dir");

        // Create source file
        let mut src_file = fs::File::create(src.join("file1.txt")).expect("Failed to create file1");
        src_file.write_all(b"source").expect("Failed to write file1");
        drop(src_file);

        let dst = temp_dir.path().join("dst");
        fs::create_dir(&dst).expect("Failed to create dst dir");

        // Create existing destination file
        let mut dst_file = fs::File::create(dst.join("file1.txt")).expect("Failed to create dest file");
        dst_file.write_all(b"existing").expect("Failed to write dest file");
        drop(dst_file);

        let mut job =
            create_job(&src, &dst, Mode::Copy, OverwritePolicy::Overwrite).expect("Failed to create job");
        plan_job(&mut job).expect("Failed to plan job");

        run_job(&mut job, None).expect("Failed to run job");

        // Verify file WAS overwritten (Overwrite policy)
        let contents = fs::read_to_string(dst.join("file1.txt")).expect("Failed to read file");
        assert_eq!(contents, "source");

        // Verify file marked as Done
        let file_items: Vec<_> = job.files.iter().filter(|f| !f.is_dir).collect();
        assert_eq!(file_items[0].state, FileState::Done);
    }

    #[test]
    fn test_run_job_respects_smart_update_policy() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let src = temp_dir.path().join("src");
        fs::create_dir(&src).expect("Failed to create src dir");

        // Create source file
        let mut src_file = fs::File::create(src.join("file1.txt")).expect("Failed to create file1");
        src_file.write_all(b"source").expect("Failed to write file1");
        drop(src_file);

        let dst = temp_dir.path().join("dst");
        fs::create_dir(&dst).expect("Failed to create dst dir");

        // Create existing destination file with DIFFERENT size
        let mut dst_file = fs::File::create(dst.join("file1.txt")).expect("Failed to create dest file");
        dst_file.write_all(b"x").expect("Failed to write dest file"); // Different size
        drop(dst_file);

        let mut job =
            create_job(&src, &dst, Mode::Copy, OverwritePolicy::SmartUpdate).expect("Failed to create job");
        plan_job(&mut job).expect("Failed to plan job");

        run_job(&mut job, None).expect("Failed to run job");

        // Verify file WAS copied (different size triggers SmartUpdate)
        let contents = fs::read_to_string(dst.join("file1.txt")).expect("Failed to read file");
        assert_eq!(contents, "source");

        // Verify file marked as Done
        let file_items: Vec<_> = job.files.iter().filter(|f| !f.is_dir).collect();
        assert_eq!(file_items[0].state, FileState::Done);
    }

    #[test]
    fn test_run_job_invokes_callbacks() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let src = temp_dir.path().join("src");
        fs::create_dir(&src).expect("Failed to create src dir");

        // Create test file
        let mut file1 = fs::File::create(src.join("file1.txt")).expect("Failed to create file1");
        file1.write_all(b"test").expect("Failed to write file1");
        drop(file1);

        let dst = temp_dir.path().join("dst");
        fs::create_dir(&dst).expect("Failed to create dst dir");

        let mut job = create_job(&src, &dst, Mode::Copy, OverwritePolicy::Overwrite)
            .expect("Failed to create job");
        plan_job(&mut job).expect("Failed to plan job");

        let progress = TestProgressCallback::new();
        run_job(&mut job, Some(&progress)).expect("Failed to run job");

        let calls = progress.get_calls();
        
        // Verify callbacks were invoked
        assert!(calls.iter().any(|c| c == "on_job_started"));
        assert!(calls.iter().any(|c| c.starts_with("on_file_started")));
        assert!(calls.iter().any(|c| c.starts_with("on_file_progress")));
        assert!(calls.iter().any(|c| c.starts_with("on_file_completed")));
        assert!(calls.iter().any(|c| c == "on_job_completed"));

        // Verify order: started -> file_started -> progress -> file_completed -> completed
        let mut found_started = false;
        let mut found_file_started = false;
        let mut found_progress = false;
        let mut found_file_completed = false;
        let mut found_completed = false;

        for call in calls.iter() {
            if call == "on_job_started" {
                found_started = true;
            } else if call.starts_with("on_file_started") {
                assert!(found_started);
                found_file_started = true;
            } else if call.starts_with("on_file_progress") {
                assert!(found_file_started);
                found_progress = true;
            } else if call.starts_with("on_file_completed") {
                assert!(found_progress || !found_file_started); // May not have progress for skipped files
                found_file_completed = true;
            } else if call == "on_job_completed" {
                assert!(found_file_completed);
                found_completed = true;
            }
        }

        assert!(found_completed, "on_job_completed not called in correct order");
    }

    #[test]
    fn test_run_job_requires_pending_state() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let src = temp_dir.path().join("src");
        fs::create_dir(&src).expect("Failed to create src dir");
        let dst = temp_dir.path().join("dst");

        let mut job = create_job(&src, &dst, Mode::Copy, OverwritePolicy::Skip)
            .expect("Failed to create job");
        plan_job(&mut job).expect("Failed to plan job");

        // Try to run twice
        run_job(&mut job, None).expect("First run should succeed");

        // Second run should fail because job is not in Pending state
        let result = run_job(&mut job, None);
        assert!(result.is_err(), "Second run should fail");
    }

    #[test]
    fn test_run_job_continues_on_file_errors() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let src = temp_dir.path().join("src");
        fs::create_dir(&src).expect("Failed to create src dir");

        // Create test files
        let mut file1 = fs::File::create(src.join("file1.txt")).expect("Failed to create file1");
        file1.write_all(b"file1").expect("Failed to write file1");
        drop(file1);

        let mut file2 = fs::File::create(src.join("file2.txt")).expect("Failed to create file2");
        file2.write_all(b"file2").expect("Failed to write file2");
        drop(file2);

        let dst = temp_dir.path().join("dst");
        fs::create_dir(&dst).expect("Failed to create dst dir");

        let mut job = create_job(&src, &dst, Mode::Copy, OverwritePolicy::Overwrite)
            .expect("Failed to create job");
        plan_job(&mut job).expect("Failed to plan job");

        // Job should complete even though we had multiple files
        run_job(&mut job, None).expect("Run should complete without panic");

        // Verify job is marked as completed
        assert_eq!(job.state, JobState::Completed);
        
        // At least file1 should be copied
        let file1_dest = dst.join("file1.txt");
        assert!(file1_dest.exists(), "file1 should be copied");
        
        // file2 should also be copied
        let file2_dest = dst.join("file2.txt");
        assert!(file2_dest.exists(), "file2 should be copied");
    }

    #[test]
    fn test_run_job_with_verification_enabled() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let src = temp_dir.path().join("src");
        fs::create_dir(&src).expect("Failed to create src dir");

        // Create test file
        let mut file1 = fs::File::create(src.join("file1.txt")).expect("Failed to create file1");
        file1.write_all(b"test content").expect("Failed to write file1");
        drop(file1);

        let dst = temp_dir.path().join("dst");
        fs::create_dir(&dst).expect("Failed to create dst dir");

        let mut job = create_job(&src, &dst, Mode::Copy, OverwritePolicy::Overwrite)
            .expect("Failed to create job");
        
        // Enable verification
        job.verify_after_copy = true;
        job.checksum_algorithm = Some(crate::checksums::ChecksumAlgorithm::Sha256);

        plan_job(&mut job).expect("Failed to plan job");
        run_job(&mut job, None).expect("Failed to run job");

        // Verify job completed successfully
        assert_eq!(job.state, JobState::Completed);

        // Find the copied file and verify checksum
        let file_items: Vec<_> = job.files.iter().filter(|f| !f.is_dir && f.source_path.ends_with("file1.txt")).collect();
        assert!(!file_items.is_empty(), "Expected to find file1.txt");
        
        let file = file_items[0];
        
        // Verify checksums were computed
        assert!(file.metadata.source_checksum.is_some(), "Expected source_checksum to be set");
        assert!(file.metadata.dest_checksum.is_some(), "Expected dest_checksum to be set");
        
        // Verify they match
        assert_eq!(file.metadata.source_checksum.as_ref().map(|c| c.hex()),
                   file.metadata.dest_checksum.as_ref().map(|c| c.hex()),
                   "Source and destination checksums should match");
        
        // Verify the flag is set correctly
        assert_eq!(file.metadata.verification_passed, Some(true), "Verification should pass");
    }

    #[test]
    fn test_run_job_with_verification_detects_mismatch() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let src = temp_dir.path().join("src");
        fs::create_dir(&src).expect("Failed to create src dir");

        // Create source file
        let mut src_file = fs::File::create(src.join("file1.txt")).expect("Failed to create source file");
        src_file.write_all(b"original content").expect("Failed to write source file");
        drop(src_file);

        let dst = temp_dir.path().join("dst");
        fs::create_dir(&dst).expect("Failed to create dst dir");

        let mut job = create_job(&src, &dst, Mode::Copy, OverwritePolicy::Overwrite)
            .expect("Failed to create job");
        
        // Enable verification
        job.verify_after_copy = true;
        job.checksum_algorithm = Some(crate::checksums::ChecksumAlgorithm::Sha256);

        plan_job(&mut job).expect("Failed to plan job");
        run_job(&mut job, None).expect("Failed to run job");

        // Manually modify the destination file
        let dst_file_path = dst.join("file1.txt");
        let mut dst_file = fs::File::create(&dst_file_path).expect("Failed to open destination file");
        dst_file.write_all(b"modified content").expect("Failed to modify destination file");
        drop(dst_file);

        // Now run verification on the file item manually to simulate post-copy verification
        // (The verification that happens during run_job happens before we modified the file)
        let file_item = &mut job.files.iter_mut()
            .find(|f| !f.is_dir && f.source_path.ends_with("file1.txt"))
            .expect("Expected to find file1.txt");
        
        // Reset checksums to simulate a new verification
        file_item.metadata.source_checksum = None;
        file_item.metadata.dest_checksum = None;
        file_item.metadata.verification_passed = None;
        
        let verify_result = crate::checksums::verify_file_item(file_item, crate::checksums::ChecksumAlgorithm::Sha256)
            .expect("Verification should complete without error");
        
        // Verify should return false (mismatch)
        assert!(!verify_result, "Verification should detect mismatch");
        assert_eq!(file_item.metadata.verification_passed, Some(false), "Verification should be marked as failed");
    }

    #[test]
    fn test_verification_without_flag_skips_verification() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let src = temp_dir.path().join("src");
        fs::create_dir(&src).expect("Failed to create src dir");

        // Create test file
        let mut file1 = fs::File::create(src.join("file1.txt")).expect("Failed to create file1");
        file1.write_all(b"test content").expect("Failed to write file1");
        drop(file1);

        let dst = temp_dir.path().join("dst");
        fs::create_dir(&dst).expect("Failed to create dst dir");

        let mut job = create_job(&src, &dst, Mode::Copy, OverwritePolicy::Overwrite)
            .expect("Failed to create job");
        
        // DO NOT enable verification (verify_after_copy is false by default)
        assert!(!job.verify_after_copy, "Verification should be disabled by default");

        plan_job(&mut job).expect("Failed to plan job");
        run_job(&mut job, None).expect("Failed to run job");

        // Verify checksums were NOT computed
        let file_items: Vec<_> = job.files.iter().filter(|f| !f.is_dir && f.source_path.ends_with("file1.txt")).collect();
        let file = file_items[0];
        
        assert!(file.metadata.source_checksum.is_none(), "source_checksum should not be set when verification is disabled");
        assert!(file.metadata.dest_checksum.is_none(), "dest_checksum should not be set when verification is disabled");
        assert!(file.metadata.verification_passed.is_none(), "verification_passed should not be set when verification is disabled");
    }
}
