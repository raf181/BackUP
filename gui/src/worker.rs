use engine::{Mode, OverwritePolicy, ChecksumAlgorithm, create_job, plan_job, run_job, FileState};
use std::thread;
use crossbeam_channel::unbounded;
use crate::progress::{GuiProgressCallback, ProgressUpdate};
use crate::JobSummary;

/// Spawn a background worker thread to execute a transfer job.
pub fn spawn_job(
    source: String,
    dest: String,
    mode: Mode,
    policy: OverwritePolicy,
    verify: bool,
    checksum_algo: Option<ChecksumAlgorithm>,
) {
    thread::spawn(move || {
        match execute_transfer(&source, &dest, mode, policy, verify, checksum_algo) {
            Ok(summary) => {
                println!("Job completed successfully");
                println!("Done: {}, Skipped: {}, Failed: {}", 
                    summary.done_count, summary.skipped_count, summary.failed_count);
            }
            Err(e) => {
                eprintln!("Job failed: {}", e);
            }
        }
    });
}

fn execute_transfer(
    source: &str,
    dest: &str,
    mode: Mode,
    policy: OverwritePolicy,
    verify: bool,
    checksum_algo: Option<ChecksumAlgorithm>,
) -> Result<JobSummary, String> {
    // Create job
    let mut job = create_job(source, dest, mode, policy)
        .map_err(|e| format!("Failed to create job: {}", e))?;

    // Configure verification if requested
    if verify {
        job.verify_after_copy = true;
        job.checksum_algorithm = checksum_algo;
    }

    // Plan job (enumerate files)
    plan_job(&mut job)
        .map_err(|e| format!("Failed to plan job: {}", e))?;

    // Create progress callback
    let (tx, _rx) = unbounded::<ProgressUpdate>();
    let callback = GuiProgressCallback::new(tx);

    // Run job with progress callback
    run_job(&mut job, Some(&callback))
        .map_err(|e| format!("Job execution failed: {}", e))?;

    // Collect summary
    let mut done_count = 0;
    let mut skipped_count = 0;
    let mut failed_count = 0;
    let mut failed_items = Vec::new();

    for file in &job.files {
        match file.state {
            FileState::Done => done_count += 1,
            FileState::Skipped => skipped_count += 1,
            FileState::Failed => {
                failed_count += 1;
                let file_name = file.source_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("Unknown")
                    .to_string();
                let error = file.error_message
                    .clone()
                    .unwrap_or_else(|| "Unknown error".to_string());
                failed_items.push((file_name, error));
            }
            _ => {}
        }
    }

    Ok(JobSummary {
        total_files: job.files.len(),
        done_count,
        skipped_count,
        failed_count,
        total_bytes: job.total_bytes_to_copy,
        total_bytes_copied: job.total_bytes_copied,
        failed_items,
    })
}
