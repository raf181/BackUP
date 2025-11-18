mod state;
mod progress;
mod worker;

use iced::{Element, Sandbox, Settings, Length, Alignment};
use iced::widget::{column, row, text, text_input, button, checkbox, pick_list, container};
use engine::{Mode, OverwritePolicy, ChecksumAlgorithm};
use state::AppState;

pub fn main() -> iced::Result {
    GuiApp::run(Settings::default())
}

#[derive(Debug, Clone)]
pub enum Message {
    SourcePathChanged(String),
    DestinationPathChanged(String),
    ModeChanged(Mode),
    OverwritePolicyChanged(OverwritePolicy),
    VerifyToggled(bool),
    ChecksumAlgorithmChanged(ChecksumAlgorithm),
    BrowseSourcePressed,
    BrowseDestinationPressed,
    StartTransferPressed,
    JobProgressUpdated(progress::ProgressUpdate),
    JobCompleted(Result<JobSummary, String>),
    ErrorOccurred(String),
}

#[derive(Debug, Clone)]
pub struct JobSummary {
    pub total_files: usize,
    pub done_count: usize,
    pub skipped_count: usize,
    pub failed_count: usize,
    pub total_bytes: u64,
    pub total_bytes_copied: u64,
    pub failed_items: Vec<(String, String)>,
}

pub struct GuiApp {
    state: AppState,
}

impl Sandbox for GuiApp {
    type Message = Message;

    fn new() -> Self {
        GuiApp {
            state: AppState::new(),
        }
    }

    fn title(&self) -> String {
        "BackUP - File Transfer".to_string()
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::SourcePathChanged(path) => {
                self.state.source_path = path;
                self.state.error_message = None;
            }
            Message::DestinationPathChanged(path) => {
                self.state.destination_path = path;
                self.state.error_message = None;
            }
            Message::ModeChanged(mode) => {
                self.state.selected_mode = mode;
            }
            Message::OverwritePolicyChanged(policy) => {
                self.state.selected_overwrite_policy = policy;
            }
            Message::VerifyToggled(enabled) => {
                self.state.verify_after_copy = enabled;
            }
            Message::ChecksumAlgorithmChanged(algo) => {
                self.state.checksum_algorithm = Some(algo);
            }
            Message::BrowseSourcePressed => {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    self.state.source_path = path.display().to_string();
                    self.state.error_message = None;
                }
            }
            Message::BrowseDestinationPressed => {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    self.state.destination_path = path.display().to_string();
                    self.state.error_message = None;
                }
            }
            Message::StartTransferPressed => {
                if self.state.source_path.trim().is_empty() {
                    self.state.error_message = Some("Source path is required".to_string());
                    return;
                }
                if self.state.destination_path.trim().is_empty() {
                    self.state.error_message = Some("Destination path is required".to_string());
                    return;
                }
                if self.state.source_path == self.state.destination_path {
                    self.state.error_message = Some("Source and destination cannot be the same".to_string());
                    return;
                }

                self.state.is_running = true;
                self.state.error_message = None;
                self.state.last_job_summary = None;
                
                let source = self.state.source_path.clone();
                let dest = self.state.destination_path.clone();
                let mode = self.state.selected_mode;
                let policy = self.state.selected_overwrite_policy;
                let verify = self.state.verify_after_copy;
                let algo = self.state.checksum_algorithm;
                
                worker::spawn_job(source, dest, mode, policy, verify, algo);
            }
            Message::JobProgressUpdated(update) => {
                self.state.handle_progress_update(update);
            }
            Message::JobCompleted(result) => {
                self.state.is_running = false;
                match result {
                    Ok(summary) => {
                        self.state.last_job_summary = Some(summary);
                    }
                    Err(err) => {
                        self.state.error_message = Some(format!("Job failed: {}", err));
                    }
                }
            }
            Message::ErrorOccurred(err) => {
                self.state.is_running = false;
                self.state.error_message = Some(err);
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let input_section = column![
            text("Source Path"),
            row![
                text_input("Enter source path", &self.state.source_path)
                    .on_input(Message::SourcePathChanged)
                    .width(Length::Fill),
                button("Browse...").on_press(Message::BrowseSourcePressed),
            ]
            .spacing(10)
            .align_items(Alignment::Center),
            
            text("Destination Path"),
            row![
                text_input("Enter destination path", &self.state.destination_path)
                    .on_input(Message::DestinationPathChanged)
                    .width(Length::Fill),
                button("Browse...").on_press(Message::BrowseDestinationPressed),
            ]
            .spacing(10)
            .align_items(Alignment::Center),
        ]
        .spacing(10)
        .padding(10);

        let mode_options = vec![Mode::Copy, Mode::Move];
        let policy_options = vec![
            OverwritePolicy::Skip,
            OverwritePolicy::Overwrite,
            OverwritePolicy::SmartUpdate,
            OverwritePolicy::Ask,
        ];
        let algo_options = vec![
            ChecksumAlgorithm::Sha256,
            ChecksumAlgorithm::Blake3,
            ChecksumAlgorithm::Md5,
            ChecksumAlgorithm::Crc32,
        ];

        let mut options_column = column![
            row![
                column![
                    text("Mode"),
                    pick_list(
                        mode_options,
                        Some(self.state.selected_mode),
                        Message::ModeChanged,
                    )
                ]
                .width(Length::FillPortion(1)),
                
                column![
                    text("Overwrite Policy"),
                    pick_list(
                        policy_options,
                        Some(self.state.selected_overwrite_policy),
                        Message::OverwritePolicyChanged,
                    )
                ]
                .width(Length::FillPortion(1)),
            ]
            .spacing(15),
            
            checkbox("Verify after copy", self.state.verify_after_copy)
                .on_toggle(Message::VerifyToggled),
        ]
        .spacing(10)
        .padding(10);

        if self.state.verify_after_copy {
            options_column = options_column.push(
                pick_list(
                    algo_options,
                    self.state.checksum_algorithm,
                    Message::ChecksumAlgorithmChanged,
                )
            );
        }

        let start_button = button(
            if self.state.is_running { "Running..." } else { "Start Transfer" }
        )
        .on_press_maybe(
            if self.state.is_running { None } else { Some(Message::StartTransferPressed) }
        )
        .padding(10);

        let progress_percent = if self.state.total_bytes_to_copy > 0 {
            (self.state.total_bytes_copied as f32 / self.state.total_bytes_to_copy as f32 * 100.0) as u32
        } else {
            0
        };

        let progress_section: Element<Message> = if self.state.is_running {
            column![
                text(format!("Progress: {}%", progress_percent)),
                text(format!(
                    "{} / {} files",
                    self.state.done_count + self.state.skipped_count + self.state.failed_count,
                    self.state.total_files
                )),
                text(format!(
                    "Done: {} | Skipped: {} | Failed: {}",
                    self.state.done_count,
                    self.state.skipped_count,
                    self.state.failed_count
                )),
                if !self.state.current_file_name.is_empty() {
                    text(format!("Current: {}", self.state.current_file_name))
                } else {
                    text("")
                },
            ]
            .spacing(10)
            .padding(10)
            .into()
        } else if let Some(summary) = &self.state.last_job_summary {
            let mut col = column![
                text("Transfer Complete"),
                text(format!("Done: {} | Skipped: {} | Failed: {}", 
                    summary.done_count,
                    summary.skipped_count,
                    summary.failed_count
                )),
            ]
            .spacing(5);
            
            if !summary.failed_items.is_empty() {
                col = col.push(text("Failed Files (first 10):"));
                for (path, err) in summary.failed_items.iter().take(10) {
                    col = col.push(text(format!("  {}: {}", path, err)));
                }
            }

            col.spacing(10).padding(10).into()
        } else {
            text("Ready to transfer").into()
        };

        let error_section: Element<Message> = if let Some(error) = &self.state.error_message {
            container(text(format!("ERROR: {}", error)))
                .padding(10)
                .into()
        } else {
            text("").into()
        };

        column![
            text("BackUP - File Transfer Tool").size(24),
            input_section,
            options_column,
            start_button,
            progress_section,
            error_section,
        ]
        .spacing(20)
        .padding(20)
        .into()
    }
}
