# BackUP - TeraCopy-Style File Transfer Engine

A robust, headless file transfer engine for Windows written in Rust. Designed as the foundation for a full-featured GUI application, but this milestone provides a working CLI and library that can be integrated into other tools.

## Features (Milestone 1)

- ✅ Recursive copy and move of files and directories
- ✅ Per-file state tracking (Pending, Copying, Done, Skipped, Failed)
- ✅ Error resilience: single-file errors don't kill the job
- ✅ Configurable overwrite policies (Skip, Overwrite, SmartUpdate, Ask)
- ✅ Progress reporting via callback trait (CLI and future GUI compatible)
- ✅ Comprehensive error handling with recovery
- ✅ Long path support (\\?\\ prefix for >260 chars)
- ✅ Unicode filename support
- ✅ CLI binary for testing and manual use
- ✅ Library API for future UIs and tools

## Planned Features (Future Milestones)

- Checksums and file integrity verification
- Multi-job queuing and scheduling
- SQLite persistence (history, resume interrupted jobs)
- Shell integration (context menus, drag-and-drop)
- Advanced filtering (include/exclude patterns)
- Bandwidth throttling
- ACL and alternate data stream (ADS) preservation

## System Requirements

- **OS**: Windows 10 or later (x86_64)
- **Rust**: 1.70+ (stable)
- **Dependencies**: Minimal (clap, uuid, chrono, and a few others)

## Building

### Build the Engine Library and CLI

```bash
cd BackUP
cargo build --release
```

The CLI binary will be at `target/release/cli.exe` (Windows).

### Build Individual Crates

```bash
# Engine library only
cargo build -p engine --release

# CLI binary only
cargo build -p cli --release
```

## Running the CLI

### Basic Copy

```bash
./transfer --src "C:\Users\Alice\Documents" --dst "D:\Backup"
```

### With Options

```bash
# Skip existing files
./transfer --src "C:\data" --dst "D:\backup" --overwrite-policy skip

# Overwrite all
./transfer --src "C:\data" --dst "D:\backup" --overwrite-policy overwrite

# Smart update (overwrite if newer or different size)
./transfer --src "C:\data" --dst "D:\backup" --overwrite-policy smart-update

# Move instead of copy
./transfer --src "C:\data" --dst "D:\backup" --mode move
```

### Help

```bash
./transfer --help
./transfer --version
```

## Usage as a Library

```rust
use engine::{create_job, plan_job, run_job, Mode, OverwritePolicy, ProgressCallback, FileItem, TransferJob};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a job
    let mut job = create_job(
        "C:\\source",
        "D:\\destination",
        Mode::Copy,
        OverwritePolicy::Skip,
    )?;

    // Plan (enumerate files)
    plan_job(&mut job)?;
    println!("Will copy {} files", job.files.len());

    // Create a progress reporter
    let progress = MyProgressReporter;

    // Run the job
    run_job(&mut job, Some(&progress))?;

    // Check results
    for file in &job.files {
        if file.state == FileState::Failed {
            println!("Failed: {} - {}", file.source_path.display(), 
                     file.error_message.as_deref().unwrap_or("unknown"));
        }
    }

    Ok(())
}

// Implement the ProgressCallback trait for your UI
struct MyProgressReporter;

impl ProgressCallback for MyProgressReporter {
    fn on_job_started(&self, job: &TransferJob) {
        println!("Starting transfer of {} bytes", job.total_bytes_to_copy);
    }

    fn on_file_started(&self, job: &TransferJob, file_index: usize, file: &FileItem) {
        println!("Copying: {}", file.source_path.display());
    }

    fn on_file_progress(&self, job: &TransferJob, file_index: usize, bytes_this_file: u64) {
        // Update UI or progress bar
    }

    fn on_file_completed(&self, job: &TransferJob, file_index: usize, file: &FileItem) {
        println!("Done: {}", file.source_path.display());
    }

    fn on_job_completed(&self, job: &TransferJob) {
        println!("Transfer complete");
    }
}
```

## Architecture

The project is organized as a Rust workspace with two crates:

### `engine/` - Core Library

The heart of the transfer logic. Responsible for:
- File enumeration and path handling
- Copy operations and error recovery
- Job orchestration and state management
- Progress reporting via callbacks

**Modules:**
- `model.rs` - Type definitions (Job, FileItem, enums)
- `error.rs` - Error handling (EngineError type)
- `fs_ops.rs` - Filesystem operations (enumerate, copy, mkdir)
- `job.rs` - Job creation, planning, and execution
- `progress.rs` - ProgressCallback trait definition

### `cli/` - Command-Line Interface

A simple CLI for testing and manual use. Depends on the engine library.

- Argument parsing (clap)
- Progress reporting to stdout
- Exit codes and error reporting

## Design Principles

1. **Resilience**: Single file errors don't crash the job. Errors are recorded per-file and reported at the end.

2. **Extensibility**: Progress is reported via a trait, allowing multiple UIs to plug in without modifying the engine.

3. **Testability**: Core logic is separated from CLI. Each module can be tested independently.

4. **Windows-First**: Optimized for Windows (10+) with proper handling of long paths and Unicode.

5. **Synchronous**: No async I/O in Milestone 1. Keeps code simple and testable. Async can be added later if needed.

## Error Handling

### Job-Level Errors

These stop the job early and are returned as `EngineError`:
- Source directory not found
- Destination not accessible
- Root enumeration failed
- Path too long or invalid

### File-Level Errors

These are recorded in the FileItem but **do not stop the job**:
- Read/write failures
- Directory creation failures
- Permission denied
- Disk full

The job completes successfully even if some files fail. The caller inspects `job.files` to find failures.

**Example:**
```rust
for file in &job.files {
    if file.state == FileState::Failed {
        eprintln!("Failed: {} ({})", 
                  file.source_path.display(),
                  file.error_message.as_deref().unwrap_or("unknown error"));
    }
}
```

## Testing

### Unit Tests

```bash
cargo test -p engine --lib
```

Tests are located in each module and cover:
- Enumeration (directory structure parsing)
- Overwrite policies (Skip, Overwrite, SmartUpdate)
- File copying with metadata preservation
- Error handling and recovery

### Integration Tests

```bash
cargo test -p engine --test '*'
```

Real filesystem tests using temporary directories:
- Copy flat and nested directory structures
- Handle various overwrite policies
- Simulate and recover from errors
- Verify progress callbacks are invoked in correct order

### Manual Testing

For realistic scenarios with large files and deep paths:

```bash
# Create a test source directory
mkdir C:\test_source
# ... add some files ...

# Run transfer
./transfer --src C:\test_source --dst C:\test_dest

# Verify results
tree C:\test_dest
```

## Exit Codes

- **0** - Success: All files copied, no errors
- **1** - Partial success: Some files failed or skipped
- **2** - Fatal error: Job could not start (source not found, access denied, etc.)

## Performance

Milestone 1 targets:
- **Copy speed**: ~50-200 MB/s (depends on source/destination speeds)
- **Enumeration**: <1 second for 10,000 files
- **Memory**: <100 MB even for jobs with 100,000+ files

See `docs/PERFORMANCE.md` for benchmarks and optimization guidance.

## Documentation

- **DESIGN.md** - Complete design document (architecture, algorithms, data model)
- **IMPLEMENTATION_SUMMARY.md** - Quick reference for developers

## Contributing

See `CONTRIBUTING.md` for development guidelines.

## License

This project is provided as-is. See LICENSE for details.

## Roadmap

### Milestone 1 (Current) ✅
- Core file transfer engine
- CLI for testing
- Error handling and recovery
- Progress reporting

### Milestone 2 (Planned)
- Checksum verification
- Pause/resume (via SQLite persistence)
- Better progress reporting (bandwidth, ETA)

### Milestone 3 (Planned)
- GUI (WinForms or WPF)
- Multi-job queuing
- Advanced filtering

### Milestone 4+ (Future)
- Shell integration
- ACL/ADS preservation
- Network transfers

## Support

For issues, feature requests, or questions, please open an issue on GitHub.
