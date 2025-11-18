use engine::{Mode, OverwritePolicy, ChecksumAlgorithm};
use crate::progress::ProgressUpdate;
use crate::JobSummary;

/// Application state, holding all UI and job-related data.
#[derive(Debug)]
pub struct AppState {
    // Input fields
    pub source_path: String,
    pub destination_path: String,
    pub selected_mode: Mode,
    pub selected_overwrite_policy: OverwritePolicy,
    pub verify_after_copy: bool,
    pub checksum_algorithm: Option<ChecksumAlgorithm>,

    // Job state
    pub is_running: bool,
    pub total_files: usize,
    pub done_count: usize,
    pub skipped_count: usize,
    pub failed_count: usize,
    pub total_bytes_to_copy: u64,
    pub total_bytes_copied: u64,
    pub current_file_name: String,

    // UI state
    pub error_message: Option<String>,
    pub last_job_summary: Option<JobSummary>,
}

impl AppState {
    pub fn new() -> Self {
        AppState {
            source_path: String::new(),
            destination_path: String::new(),
            selected_mode: Mode::Copy,
            selected_overwrite_policy: OverwritePolicy::Skip,
            verify_after_copy: false,
            checksum_algorithm: Some(ChecksumAlgorithm::Sha256),
            
            is_running: false,
            total_files: 0,
            done_count: 0,
            skipped_count: 0,
            failed_count: 0,
            total_bytes_to_copy: 0,
            total_bytes_copied: 0,
            current_file_name: String::new(),
            
            error_message: None,
            last_job_summary: None,
        }
    }

    pub fn handle_progress_update(&mut self, update: ProgressUpdate) {
        match update {
            ProgressUpdate::JobStarted {
                total_files,
                total_bytes,
            } => {
                self.total_files = total_files;
                self.total_bytes_to_copy = total_bytes;
                self.total_bytes_copied = 0;
                self.done_count = 0;
                self.skipped_count = 0;
                self.failed_count = 0;
            }
            ProgressUpdate::FileStarted { name } => {
                self.current_file_name = name;
            }
            ProgressUpdate::FileProgress { bytes_copied } => {
                self.total_bytes_copied = bytes_copied;
            }
            ProgressUpdate::FileCompleted { result } => {
                match result.as_str() {
                    "done" => self.done_count += 1,
                    "skipped" => self.skipped_count += 1,
                    "failed" => self.failed_count += 1,
                    _ => {}
                }
            }
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
