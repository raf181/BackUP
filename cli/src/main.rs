//! BackUP - Command-line interface for the file transfer engine.
//!
//! This is a simple CLI for testing and manual use of the transfer engine.
//! It provides argument parsing and progress reporting to stdout.

use clap::Parser;
use std::path::PathBuf;
use std::time::Instant;
use engine::{
    job::{create_job, plan_job, run_job},
    model::{FileState, Mode, OverwritePolicy, TransferJob},
    progress::ProgressCallback,
    ChecksumAlgorithm,
};

/// BackUP - A simple file transfer tool
#[derive(Parser, Debug)]
#[command(name = "transfer")]
#[command(version = "0.1.0")]
#[command(about = "Copy files and directories with progress tracking")]
struct Args {
    /// Source directory
    #[arg(long, value_name = "PATH")]
    src: PathBuf,

    /// Destination directory
    #[arg(long, value_name = "PATH")]
    dst: PathBuf,

    /// Operation mode: copy or move
    #[arg(long, value_name = "MODE", default_value = "copy")]
    mode: String,

    /// Overwrite policy: skip, overwrite, ask, or smart
    #[arg(long, value_name = "POLICY", default_value = "skip")]
    overwrite: String,

    /// Enable verbose output
    #[arg(long)]
    verbose: bool,

    /// Enable verification after copy (compares checksums)
    #[arg(long)]
    verify: bool,

    /// Checksum algorithm for verification: crc32, md5, sha256, blake3
    #[arg(long, value_name = "ALGORITHM", default_value = "sha256", requires = "verify")]
    hash: String,
}

/// CLI implementation of ProgressCallback for displaying transfer progress
struct CliProgress {
    verbose: bool,
    start_time: Instant,
    last_progress_update: Instant,
}

impl CliProgress {
    fn new(verbose: bool) -> Self {
        let now = Instant::now();
        CliProgress {
            verbose,
            start_time: now,
            last_progress_update: now,
        }
    }

    fn format_bytes(bytes: u64) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
        let mut size = bytes as f64;
        let mut unit_idx = 0;

        while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
            size /= 1024.0;
            unit_idx += 1;
        }

        format!("{:.2} {}", size, UNITS[unit_idx])
    }

    fn format_duration(elapsed: std::time::Duration) -> String {
        let secs = elapsed.as_secs();
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        let secs = secs % 60;

        if hours > 0 {
            format!("{}h {}m {}s", hours, mins, secs)
        } else if mins > 0 {
            format!("{}m {}s", mins, secs)
        } else {
            format!("{}s", secs)
        }
    }

    fn print_progress_bar(percent: u32) -> String {
        let filled = (percent / 5) as usize;
        let empty = 20 - filled;
        format!(
            "[{}{}] {}%",
            "=".repeat(filled),
            " ".repeat(empty),
            percent
        )
    }
}

impl ProgressCallback for CliProgress {
    fn on_job_started(&self, job: &TransferJob) {
        eprintln!("Preparing transfer...");
        eprintln!("  Source: {}", job.source_path.display());
        eprintln!("  Destination: {}", job.destination_path.display());
        eprintln!("  Mode: {:?}", job.mode);
        eprintln!(
            "  Total: {} bytes across {} items",
            Self::format_bytes(job.total_bytes_to_copy),
            job.files.len()
        );
        eprintln!();
    }

    fn on_file_started(&self, _job: &TransferJob, file_index: usize, file: &engine::model::FileItem) {
        if self.verbose {
            let name = file
                .source_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("(unknown)");
            eprintln!("[{:3}] Starting: {}", file_index, name);
        }
    }

    fn on_file_progress(
        &self,
        job: &TransferJob,
        _file_index: usize,
        _bytes_this_file: u64,
    ) {
        // Throttle progress updates to avoid spam (max once per 200ms)
        let elapsed = self.last_progress_update.elapsed();
        if elapsed.as_millis() < 200 {
            return;
        }

        let total_bytes = if job.total_bytes_to_copy == 0 {
            1
        } else {
            job.total_bytes_to_copy
        };
        let percent = (job.total_bytes_copied as f64 / total_bytes as f64 * 100.0) as u32;

        eprint!(
            "\rProgress: {} | {}/{} bytes",
            Self::print_progress_bar(percent),
            Self::format_bytes(job.total_bytes_copied),
            Self::format_bytes(total_bytes)
        );
        let _ = std::io::Write::flush(&mut std::io::stderr());
    }

    fn on_file_completed(
        &self,
        _job: &TransferJob,
        file_index: usize,
        file: &engine::model::FileItem,
    ) {
        if self.verbose {
            let name = file
                .source_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("(unknown)");
            let status = match file.state {
                FileState::Done => "Done",
                FileState::Skipped => "Skipped",
                FileState::Failed => "Failed",
                _ => "Unknown",
            };
            eprintln!("[{:3}] {}: {}", file_index, status, name);
        }
    }

    fn on_job_completed(&self, job: &TransferJob) {
        eprintln!();
        eprintln!("Transfer complete!");

        // Count files by state
        let mut done = 0;
        let mut skipped = 0;
        let mut failed = 0;
        let mut verified_ok = 0;
        let mut verified_mismatch = 0;

        for file in &job.files {
            match file.state {
                FileState::Done => {
                    done += 1;
                    // Check verification status if available
                    if let Some(true) = file.metadata.verification_passed {
                        verified_ok += 1;
                    } else if let Some(false) = file.metadata.verification_passed {
                        verified_mismatch += 1;
                    }
                }
                FileState::Skipped => skipped += 1,
                FileState::Failed => failed += 1,
                _ => {}
            }
        }

        let elapsed = self.start_time.elapsed();

        eprintln!(
            "Summary: {} done, {} skipped, {} failed",
            done, skipped, failed
        );

        // Display verification results if verification was performed
        if job.verify_after_copy && job.checksum_algorithm.is_some() {
            eprintln!(
                "Verification: {} OK, {} mismatch",
                verified_ok, verified_mismatch
            );
        }

        eprintln!("Bytes copied: {}", Self::format_bytes(job.total_bytes_copied));
        eprintln!("Elapsed: {}", Self::format_duration(elapsed));

        if failed > 0 {
            eprintln!();
            eprintln!("Failed files:");
            for file in &job.files {
                if file.state == FileState::Failed {
                    let name = file
                        .source_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("(unknown)");
                    if let Some(ref msg) = file.error_message {
                        eprintln!("  {}: {}", name, msg);
                    } else {
                        eprintln!("  {}: (unknown error)", name);
                    }
                }
            }
        }

        // Display verification mismatches if any
        if verified_mismatch > 0 {
            eprintln!();
            eprintln!("Verification mismatches:");
            for file in &job.files {
                if let Some(false) = file.metadata.verification_passed {
                    let name = file
                        .source_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("(unknown)");
                    eprintln!("  {}: source and destination checksums differ", name);
                }
            }
        }
    }
}

/// Parse and validate command-line arguments, then run the job
fn main() {
    let args = Args::parse();

    // Exit code tracking
    let exit_code = match run_cli(&args) {
        Ok(()) => 0,
        Err(msg) => {
            eprintln!("Error: {}", msg);
            2
        }
    };

    std::process::exit(exit_code);
}

/// Main CLI logic - separated for testability
fn run_cli(args: &Args) -> Result<(), String> {
    // Validate source directory exists
    if !args.src.exists() {
        return Err(format!("Source directory does not exist: {}", args.src.display()));
    }

    if !args.src.is_dir() {
        return Err(format!("Source is not a directory: {}", args.src.display()));
    }

    // Validate destination directory path is valid
    if let Some(parent) = args.dst.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(format!(
                "Parent of destination does not exist: {}",
                parent.display()
            ));
        }
    }

    // Parse mode
    let mode = match args.mode.to_lowercase().as_str() {
        "copy" => Mode::Copy,
        "move" => Mode::Move,
        _ => {
            return Err(format!(
                "Invalid mode '{}'. Must be 'copy' or 'move'",
                args.mode
            ))
        }
    };

    // Parse overwrite policy
    let policy = match args.overwrite.to_lowercase().as_str() {
        "skip" => OverwritePolicy::Skip,
        "overwrite" => OverwritePolicy::Overwrite,
        "ask" => {
            return Err(
                "Policy 'ask' is not supported in CLI mode (requires interactive input). \
                 Use 'skip', 'overwrite', or 'smart'"
                    .to_string(),
            );
        }
        "smart" | "smart-update" => OverwritePolicy::SmartUpdate,
        _ => {
            return Err(format!(
                "Invalid overwrite policy '{}'. Must be 'skip', 'overwrite', or 'smart'",
                args.overwrite
            ))
        }
    };

    // Parse checksum algorithm if verification is enabled
    let checksum_algorithm = if args.verify {
        match ChecksumAlgorithm::from_str(&args.hash) {
            Some(algo) => Some(algo),
            None => {
                return Err(format!(
                    "Invalid hash algorithm '{}'. Must be 'crc32', 'md5', 'sha256', or 'blake3'",
                    args.hash
                ))
            }
        }
    } else {
        None
    };

    // Create the job
    let mut job =
        create_job(&args.src, &args.dst, mode, policy).map_err(|e| format!("Job creation failed: {}", e))?;

    // Configure verification if enabled
    if args.verify {
        job.verify_after_copy = true;
        job.checksum_algorithm = checksum_algorithm;
    }

    // Plan the job (enumerate and calculate sizes)
    plan_job(&mut job).map_err(|e| format!("Job planning failed: {}", e))?;

    // Create progress callback
    let progress = CliProgress::new(args.verbose);

    // Run the job
    run_job(&mut job, Some(&progress)).map_err(|e| format!("Job execution failed: {}", e))?;

    // Determine exit code based on job result
    let has_failures = job.files.iter().any(|f| f.state == FileState::Failed);

    if has_failures {
        Err("One or more files failed to transfer".to_string())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_cli_with_valid_directories() {
        let src_dir = TempDir::new().expect("Failed to create temp dir");
        let dst_dir = TempDir::new().expect("Failed to create temp dir");

        // Create a simple test file in source
        std::fs::write(src_dir.path().join("test.txt"), "hello").expect("Failed to write file");

        let args = Args {
            src: src_dir.path().to_path_buf(),
            dst: dst_dir.path().to_path_buf(),
            mode: "copy".to_string(),
            overwrite: "skip".to_string(),
            verbose: false,
            verify: false,
            hash: "sha256".to_string(),
        };

        let result = run_cli(&args);
        assert!(result.is_ok(), "CLI should succeed with valid directories");
    }

    #[test]
    fn test_cli_with_verification() {
        let src_dir = TempDir::new().expect("Failed to create temp dir");
        let dst_dir = TempDir::new().expect("Failed to create temp dir");

        // Create a test file in source
        std::fs::write(src_dir.path().join("test.txt"), "hello").expect("Failed to write file");

        let args = Args {
            src: src_dir.path().to_path_buf(),
            dst: dst_dir.path().to_path_buf(),
            mode: "copy".to_string(),
            overwrite: "overwrite".to_string(),
            verbose: false,
            verify: true,
            hash: "sha256".to_string(),
        };

        let result = run_cli(&args);
        assert!(result.is_ok(), "CLI should succeed with verification enabled");
    }

    #[test]
    fn test_cli_rejects_missing_source() {
        let dst_dir = TempDir::new().expect("Failed to create temp dir");

        let args = Args {
            src: PathBuf::from("/nonexistent/path"),
            dst: dst_dir.path().to_path_buf(),
            mode: "copy".to_string(),
            overwrite: "skip".to_string(),
            verbose: false,
            verify: false,
            hash: "sha256".to_string(),
        };

        let result = run_cli(&args);
        assert!(result.is_err(), "CLI should reject missing source");
    }

    #[test]
    fn test_cli_rejects_invalid_mode() {
        let src_dir = TempDir::new().expect("Failed to create temp dir");
        let dst_dir = TempDir::new().expect("Failed to create temp dir");

        let args = Args {
            src: src_dir.path().to_path_buf(),
            dst: dst_dir.path().to_path_buf(),
            mode: "invalid".to_string(),
            overwrite: "skip".to_string(),
            verbose: false,
            verify: false,
            hash: "sha256".to_string(),
        };

        let result = run_cli(&args);
        assert!(result.is_err(), "CLI should reject invalid mode");
    }

    #[test]
    fn test_cli_rejects_invalid_policy() {
        let src_dir = TempDir::new().expect("Failed to create temp dir");
        let dst_dir = TempDir::new().expect("Failed to create temp dir");

        let args = Args {
            src: src_dir.path().to_path_buf(),
            dst: dst_dir.path().to_path_buf(),
            mode: "copy".to_string(),
            overwrite: "invalid".to_string(),
            verbose: false,
            verify: false,
            hash: "sha256".to_string(),
        };

        let result = run_cli(&args);
        assert!(result.is_err(), "CLI should reject invalid policy");
    }

    #[test]
    fn test_cli_rejects_ask_policy() {
        let src_dir = TempDir::new().expect("Failed to create temp dir");
        let dst_dir = TempDir::new().expect("Failed to create temp dir");

        let args = Args {
            src: src_dir.path().to_path_buf(),
            dst: dst_dir.path().to_path_buf(),
            mode: "copy".to_string(),
            overwrite: "ask".to_string(),
            verbose: false,
            verify: false,
            hash: "sha256".to_string(),
        };

        let result = run_cli(&args);
        assert!(result.is_err(), "CLI should reject 'ask' policy");
    }

    #[test]
    fn test_cli_rejects_invalid_hash_algorithm() {
        let src_dir = TempDir::new().expect("Failed to create temp dir");
        let dst_dir = TempDir::new().expect("Failed to create temp dir");

        let args = Args {
            src: src_dir.path().to_path_buf(),
            dst: dst_dir.path().to_path_buf(),
            mode: "copy".to_string(),
            overwrite: "skip".to_string(),
            verbose: false,
            verify: true,
            hash: "invalid_algo".to_string(),
        };

        let result = run_cli(&args);
        assert!(result.is_err(), "CLI should reject invalid hash algorithm");
    }
}
