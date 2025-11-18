use engine::{TransferJob, FileItem, ProgressCallback};
use crossbeam_channel::Sender;

#[derive(Debug, Clone)]
pub enum ProgressUpdate {
    JobStarted {
        total_files: usize,
        total_bytes: u64,
    },
    FileStarted {
        name: String,
    },
    FileProgress {
        bytes_copied: u64,
    },
    FileCompleted {
        result: String, // "done", "skipped", "failed"
    },
}

/// A ProgressCallback implementation that sends updates to the GUI via a channel.
pub struct GuiProgressCallback {
    sender: Sender<ProgressUpdate>,
}

impl GuiProgressCallback {
    pub fn new(sender: Sender<ProgressUpdate>) -> Self {
        GuiProgressCallback { sender }
    }
}

impl ProgressCallback for GuiProgressCallback {
    fn on_job_started(&self, job: &TransferJob) {
        let total_files = job.files.len();
        let total_bytes = job.total_bytes_to_copy;
        let _ = self.sender.send(ProgressUpdate::JobStarted {
            total_files,
            total_bytes,
        });
    }

    fn on_file_started(&self, _job: &TransferJob, _file_index: usize, file: &FileItem) {
        let name = file.source_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string();
        let _ = self.sender.send(ProgressUpdate::FileStarted { name });
    }

    fn on_file_progress(&self, job: &TransferJob, _file_index: usize, _bytes_this_file: u64) {
        let bytes_copied = job.total_bytes_copied;
        let _ = self.sender.send(ProgressUpdate::FileProgress { bytes_copied });
    }

    fn on_file_completed(&self, _job: &TransferJob, _file_index: usize, file: &FileItem) {
        let result = match file.state {
            engine::FileState::Done => "done",
            engine::FileState::Skipped => "skipped",
            engine::FileState::Failed => "failed",
            _ => "unknown",
        }
        .to_string();
        let _ = self.sender.send(ProgressUpdate::FileCompleted { result });
    }

    fn on_job_completed(&self, _job: &TransferJob) {
        // Job completion is handled separately in worker thread
    }
}
