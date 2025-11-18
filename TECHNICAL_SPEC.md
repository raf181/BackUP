# Milestone 1 - Technical Specification & Design Details

**Version:** 1.0  
**Status:** Ready for Implementation  
**Last Updated:** November 18, 2025  

---

## Overview

This document provides the complete technical specification for the TeraCopy-style file transfer engine. It complements the README and serves as the authoritative guide for implementation.

---

## 1. System Architecture

### High-Level Component Diagram

```
┌─────────────────────────────────────────────────────────┐
│                    CLI Binary (clap)                    │
│  - Parse arguments                                      │
│  - Create ProgressReporter                              │
│  - Call engine functions                                │
└────────────────────┬────────────────────────────────────┘
                     │ depends on
                     ▼
┌─────────────────────────────────────────────────────────┐
│              Engine Library (core logic)                 │
├─────────────────────────────────────────────────────────┤
│  model.rs        │ Data structures (Job, FileItem, etc) │
│  error.rs        │ Error types (EngineError)            │
│  fs_ops.rs       │ Enumerate, copy, mkdir operations    │
│  job.rs          │ Job orchestration (plan, run)        │
│  progress.rs     │ ProgressCallback trait               │
└─────────────────────────────────────────────────────────┘
                     │ uses
                     ▼
┌─────────────────────────────────────────────────────────┐
│            Standard Rust Libraries                       │
│  std::fs, std::io, std::path, std::time                 │
│  (+ winapi for Windows-specific operations)             │
└─────────────────────────────────────────────────────────┘
```

### Dependency Graph

```
cli/
  ├─ engine (path dependency)
  ├─ clap (argument parsing)
  └─ std, serde, chrono

engine/
  ├─ uuid (unique IDs)
  ├─ chrono (timestamps)
  ├─ winapi (Windows APIs, Windows only)
  └─ std, serde
```

---

## 2. Detailed Data Model

### 2.1 TransferJob

```rust
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
    pub error: Option<EngineError>,

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
}

pub struct JobMetadata {
    /// Custom user data (reserved for future use)
    pub custom_data: Option<String>,
}
```

**Notes:**
- `id` should be generated when the job is created (use `uuid::Uuid::new_v4()`).
- `total_bytes_to_copy` is calculated during the planning phase.
- `current_file_index` is `None` when job is Pending, becomes `Some(usize)` when Running.
- `start_time` and `end_time` bracket the actual execution.

### 2.2 FileItem

```rust
pub struct FileItem {
    /// Unique identifier for this file within the job
    pub id: Uuid,

    /// Full source path
    pub source_path: PathBuf,

    /// Full destination path (relative to job.destination_path)
    pub destination_path: PathBuf,

    /// File size in bytes
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

pub struct FileMetadata {
    /// Reserved for future checksums
    pub checksum: Option<String>,
    /// Reserved for future attributes
    pub attributes: Option<u32>,
}
```

**Notes:**
- `file_size` is 0 for directories.
- `bytes_copied` is updated incrementally as data is transferred.
- `last_modified` is in Windows FILETIME format; `None` means unchanged.
- Each file should have a unique `id` within the job.

### 2.3 Enums

#### Mode
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    Copy,  // Copy files; source remains
    Move,  // Move files; source deleted (deferred to Milestone 2)
}
```

#### FileState
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileState {
    Pending,  // Not yet processed
    Copying,  // Currently transferring
    Done,     // Successfully copied/created
    Skipped,  // Not copied due to policy or other reason
    Failed,   // Error occurred; file not copied
}

impl FileState {
    pub fn is_terminal(&self) -> bool {
        matches!(self, FileState::Done | FileState::Skipped | FileState::Failed)
    }
}
```

#### JobState
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobState {
    Pending,    // Created, not started
    Running,    // Currently executing
    Completed,  // All files processed
}
```

#### OverwritePolicy
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverwritePolicy {
    Skip,           // Don't overwrite; skip existing
    Overwrite,      // Always overwrite
    Ask,            // Ask user (Milestone 1: default to Skip)
    SmartUpdate,    // Overwrite if newer OR size differs
}
```

---

## 3. Error Model

### 3.1 EngineError

```rust
#[derive(Debug)]
pub enum EngineError {
    /// Source directory does not exist
    SourceNotFound {
        path: PathBuf,
    },

    /// Source directory is not accessible (permissions)
    SourceAccessDenied {
        path: PathBuf,
        source: io::Error,
    },

    /// Destination is not accessible
    DestinationAccessDenied {
        path: PathBuf,
        source: io::Error,
    },

    /// Failed to read from source file
    ReadError {
        path: PathBuf,
        source: io::Error,
    },

    /// Failed to write to destination file
    WriteError {
        path: PathBuf,
        source: io::Error,
    },

    /// Path exceeds Windows limits or is invalid
    PathTooLong {
        path: PathBuf,
    },

    /// Path contains invalid characters
    InvalidPath {
        path: PathBuf,
        reason: String,
    },

    /// Failed to enumerate source directory
    EnumerationFailed {
        path: PathBuf,
        source: io::Error,
    },

    /// Failed to create a directory
    DirectoryCreationFailed {
        path: PathBuf,
        source: io::Error,
    },

    /// Catch-all for unexpected errors
    Unknown {
        message: String,
    },
}

impl Display for EngineError { /* ... */ }
impl Error for EngineError { /* ... */ }
impl From<io::Error> for EngineError { /* ... */ }
```

**Error Handling Guidelines:**
- Job-level errors (source not found, access denied) stop the job and are returned from `create_job()`, `plan_job()`, or `run_job()`.
- File-level errors are recorded in `FileItem.error_code` and `FileItem.error_message`; they do NOT stop the job.
- The caller should always inspect `job.files` for failed items after execution completes.

### 3.2 File-Level Error Recording

When a file operation fails:

```rust
file.state = FileState::Failed;
file.error_code = Some(error.raw_os_error().unwrap_or(0));
file.error_message = Some(error.to_string());
// Continue to next file; do NOT return from run_job()
```

---

## 4. Public API Specification

### 4.1 Job Creation

```rust
/// Create a new transfer job.
///
/// # Arguments
/// * `source` - Source directory (must exist)
/// * `destination` - Destination directory (created if needed)
/// * `mode` - Copy or Move
/// * `overwrite_policy` - How to handle existing files
///
/// # Returns
/// * `Ok(TransferJob)` - A new job in Pending state
/// * `Err(EngineError)` - If source doesn't exist or is invalid
pub fn create_job<P: AsRef<Path>>(
    source: P,
    destination: P,
    mode: Mode,
    overwrite_policy: OverwritePolicy,
) -> Result<TransferJob, EngineError>
```

**Behavior:**
1. Validate source exists and is a directory.
2. Validate destination path is valid (but creation is deferred).
3. Return a Job in `Pending` state with empty `files` vec.

### 4.2 Job Planning

```rust
/// Plan the job by enumerating the source tree.
///
/// Populates `job.files` with all files and directories,
/// and calculates `job.total_bytes_to_copy`.
///
/// # Errors
/// Returns `EngineError` if enumeration fails (e.g., source becomes inaccessible).
pub fn plan_job(job: &mut TransferJob) -> Result<(), EngineError>
```

**Behavior:**
1. Recursively enumerate source tree.
2. For each file, create a `FileItem` in Pending state.
3. For each directory, create a `FileItem` with `is_dir=true` and `file_size=0`.
4. Sum all file sizes into `job.total_bytes_to_copy`.
5. Return error if enumeration fails at root level.
6. Continue enumeration even if a subdirectory is inaccessible (record error on that dir).

### 4.3 Job Execution

```rust
/// Run the job, copying/moving files as configured.
///
/// Transitions job state: Pending -> Running -> Completed.
/// Never returns Err due to individual file failures;
/// errors are recorded in FileItem.
///
/// # Arguments
/// * `job` - Job to execute (must be in Pending state)
/// * `progress_callback` - Optional callback for progress updates
///
/// # Errors
/// Returns `EngineError` only for unrecoverable job-level issues
/// (e.g., source becomes inaccessible mid-run).
pub fn run_job(
    job: &mut TransferJob,
    progress_callback: Option<&dyn ProgressCallback>,
) -> Result<(), EngineError>
```

**Behavior:**
1. Validate job is in Pending state; fail if not.
2. Transition job to Running; set `start_time`.
3. For each file (in order):
   a. Set `job.current_file_index`.
   b. Invoke `progress_callback.on_file_started()`.
   c. Apply overwrite policy; decide to copy or skip.
   d. If skip: set `file.state = Skipped`; continue.
   e. If copy:
      - Ensure parent directory exists.
      - Copy file; invoke progress callback per-file.
      - Preserve modification time.
      - Set `file.state = Done`; update progress counters.
   f. On error: record error in file; set `file.state = Failed`; continue.
   g. Invoke `progress_callback.on_file_completed()`.
4. After all files: transition job to Completed; set `end_time`.
5. Invoke `progress_callback.on_job_completed()`.

### 4.4 Result Inspection

```rust
pub struct JobSummary {
    pub total_files: usize,
    pub files_copied: usize,
    pub files_skipped: usize,
    pub files_failed: usize,
    pub total_bytes_copied: u64,
    pub elapsed_time: Duration,
}

/// Get a summary of job results.
pub fn get_job_summary(job: &TransferJob) -> JobSummary
```

---

## 5. Progress Callback System

### 5.1 Trait Definition

```rust
pub trait ProgressCallback: Send + Sync {
    /// Called when job execution starts.
    fn on_job_started(&self, job: &TransferJob);

    /// Called when a file is about to be processed.
    fn on_file_started(&self, job: &TransferJob, file_index: usize, file: &FileItem);

    /// Called periodically as bytes are copied.
    /// `bytes_this_file` is bytes copied for the current file.
    fn on_file_progress(&self, job: &TransferJob, file_index: usize, bytes_this_file: u64);

    /// Called when a file is done (copied, skipped, or failed).
    fn on_file_completed(&self, job: &TransferJob, file_index: usize, file: &FileItem);

    /// Called when job is complete (all files processed).
    fn on_job_completed(&self, job: &TransferJob);
}
```

### 5.2 CLI Implementation

The CLI crate provides a simple implementation:

```rust
struct CliProgress {
    // State for updating console output
}

impl ProgressCallback for CliProgress {
    fn on_job_started(&self, job: &TransferJob) {
        eprintln!("Preparing transfer...");
        eprintln!("  Source: {}", job.source_path.display());
        eprintln!("  Destination: {}", job.destination_path.display());
        eprintln!("  Total: {} bytes, {} files", 
                  job.total_bytes_to_copy, job.files.len());
    }

    fn on_file_progress(&self, job: &TransferJob, _file_index: usize, bytes_this_file: u64) {
        let pct = (job.total_bytes_copied as f64 / job.total_bytes_to_copy as f64 * 100.0) as u32;
        // Print progress bar
    }

    // ... other methods ...
}
```

### 5.3 Callback Invocation Pattern

```
on_job_started()
  on_file_started(0)
    on_file_progress(0, 1MB)
    on_file_progress(0, 2MB)
    ...
  on_file_completed(0)
  on_file_started(1)
    on_file_progress(1, 1MB)
    ...
  on_file_completed(1)
  ...
on_job_completed()
```

---

## 6. Copy Algorithm Details

### 6.1 Enumeration Phase

```
fn enumerate_tree(source: &Path) -> Vec<FileItem>:
  let mut items = Vec::new()
  
  fn recurse(path: &Path, rel_path: &Path):
    for entry in fs::read_dir(path):
      let metadata = entry.metadata()
      let rel_name = entry.file_name()
      let full_rel_path = rel_path.join(&rel_name)
      let dest_path = dest_root.join(&full_rel_path)
      
      if metadata.is_dir():
        items.push(FileItem {
          source_path: entry.path(),
          destination_path: dest_path,
          is_dir: true,
          file_size: 0,
          ...
        })
        recurse(entry.path(), full_rel_path)
      else:
        items.push(FileItem {
          source_path: entry.path(),
          destination_path: dest_path,
          is_dir: false,
          file_size: metadata.len(),
          ...
        })
  
  recurse(source, Path::new(""))
  items
```

**Error Handling:**
- If `read_dir()` fails, record the error on the directory item and continue (don't recurse into children).
- If we can't read a subdirectory, skip that subtree but continue with siblings.

### 6.2 Overwrite Policy Application

```
fn decide_action(file_item: &FileItem, policy: OverwritePolicy) -> Action:
  if file_item.is_dir:
    return Action::CreateDir  // Always create dirs
  
  let dest_exists = file_item.destination_path.exists()
  
  match policy:
    Skip:
      if dest_exists: Action::Skip
      else: Action::Copy
    
    Overwrite:
      Action::Copy  // Always copy, overwrite if exists
    
    Ask:
      // Milestone 1: no UI; default to Skip
      if dest_exists: Action::Skip
      else: Action::Copy
    
    SmartUpdate:
      if !dest_exists:
        Action::Copy
      else:
        let src_mtime = file_item.last_modified
        let src_size = file_item.file_size
        let dst_mtime = fs::metadata(dest_path)?.modified()?.duration_since(EPOCH)?.as_secs()
        let dst_size = fs::metadata(dest_path)?.len()
        
        if src_mtime > dst_mtime OR src_size != dst_size:
          Action::Copy
        else:
          Action::Skip
```

### 6.3 File Copy Operation

```
fn copy_file_with_metadata(src: &Path, dst: &Path) -> Result<u64, EngineError>:
  let src_file = File::open(src)?  // Error: ReadError
  let src_meta = src_file.metadata()?
  let src_mtime = src_meta.modified()?
  
  // Create destination directory if needed
  if let Some(parent) = dst.parent():
    fs::create_dir_all(parent)?  // Error: DirectoryCreationFailed
  
  // Copy file contents
  let mut dst_file = File::create(dst)?  // Error: WriteError
  let bytes_copied = io::copy(&mut src_file, &mut dst_file)?  // Error: ReadError/WriteError
  
  // Preserve modification time
  if let Ok(mtime) = src_mtime {
    let _ = filetime::set_file_mtime(dst, filetime::FileTime::from_system_time(mtime));
  }
  
  Ok(bytes_copied)
```

**Notes:**
- Use buffered I/O (std::io::copy uses 8 KB buffer by default).
- Preserve modification time using winapi or filetime crate.
- Don't overwrite permissions or ACLs (Milestone 1).

### 6.4 Directory Creation

```
fn ensure_parent_dir_exists(path: &Path) -> Result<(), EngineError>:
  if path.parent() == None or path.parent() == Some(destination_root):
    return Ok(())  // Root or already at destination root
  
  match fs::create_dir_all(path.parent()):
    Ok(_) => Ok(()),
    Err(e) if e.kind() == AlreadyExists and path.parent().is_dir():
      Ok(()),  // Directory already exists, that's fine
    Err(e) => Err(DirectoryCreationFailed { path, source: e })
```

---

## 7. Long Path and Unicode Handling

### 7.1 Long Path Support

Windows supports paths up to 260 characters by default. For longer paths, use the `\\?\` prefix:

```rust
fn normalize_path_for_windows(path: &Path) -> PathBuf {
    let path_str = path.to_string_lossy();
    
    // If path is absolute and not already prefixed:
    if path.is_absolute() && !path_str.starts_with(r"\\?\") {
        if path_str.starts_with(r"\\") {
            // UNC path
            PathBuf::from(format!(r"\\?\UNC\{}", &path_str[2..]))
        } else {
            // Local path
            PathBuf::from(format!(r"\\?\{}", path_str))
        }
    } else {
        path.to_path_buf()
    }
}
```

Rust's `std::fs` handles this implicitly for most operations, but we should be explicit in error messages and documentation.

### 7.2 Unicode Paths

Rust `Path` and `PathBuf` handle UTF-16 ↔ UTF-8 conversion transparently on Windows. No special handling required, but ensure error messages are UTF-8 compatible.

---

## 8. Testing Strategy

### 8.1 Unit Tests (engine crate)

**Enumeration Tests:**
```rust
#[test]
fn test_enumerate_flat_directory() {
    // Create temp dir with 3 files
    // Enumerate and verify FileItem count and sizes
}

#[test]
fn test_enumerate_nested_directory() {
    // Create temp dir with nested structure
    // Verify all files found recursively
}

#[test]
fn test_enumerate_empty_directory() {
    // Enumerate empty dir; should return single item for root
}
```

**Overwrite Policy Tests:**
```rust
#[test]
fn test_policy_skip() {
    // Create source and destination files
    // Apply Skip policy; verify it skips existing
}

#[test]
fn test_policy_smart_update() {
    // Create files with different mtimes
    // Verify SmartUpdate decides correctly
}
```

**File Copy Tests:**
```rust
#[test]
fn test_copy_file_preserves_mtime() {
    // Copy a file, verify modification time matches
}

#[test]
fn test_copy_handles_errors() {
    // Try to copy from non-existent file; verify error recorded
}
```

**Job Tests:**
```rust
#[test]
fn test_job_state_transitions() {
    // Create job, plan, run; verify state transitions
}

#[test]
fn test_run_job_continues_on_file_error() {
    // Create job with one good file and one that will fail
    // Verify job completes; second file marked Failed
}
```

### 8.2 Integration Tests

**Test Scenarios:**
1. Copy flat directory (no nesting)
2. Copy nested directories (5+ levels)
3. Apply each overwrite policy
4. Handle error: missing source
5. Handle error: permission denied
6. Handle error: no space on device (simulate)
7. Large file (>1 GB) with progress tracking
8. Unicode filenames

**Test Framework:**
- Use `tempfile` crate to create temporary directories
- Verify results by checking destination filesystem
- Clean up temp directories automatically

### 8.3 Manual Testing Checklist

- [ ] Copy 1000+ files
- [ ] Copy files >2 GB each
- [ ] Copy deeply nested (100+ levels)
- [ ] Copy paths with Unicode characters
- [ ] Copy paths >260 characters (test \\?\ prefix)
- [ ] CLI --help and --version work
- [ ] CLI exit codes correct (0, 1, 2)
- [ ] Progress output is readable
- [ ] Resume interrupted job (Milestone 2 prep)

---

## 9. Dependencies and Versions

```toml
# engine/Cargo.toml
[dependencies]
uuid = { version = "1.0", features = ["v4", "serde"] }
chrono = { version = "0.4", optional = true }
winapi = { version = "0.3", features = ["fileapi", "winbase"], optional = true }
serde = { version = "1.0", features = ["derive"], optional = true }

[dev-dependencies]
tempfile = "3.8"
```

```toml
# cli/Cargo.toml
[dependencies]
engine = { path = "../engine" }
clap = { version = "4.0", features = ["derive"] }
chrono = "0.4"
uuid = "1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```

---

## 10. Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| Copy speed | 50-200 MB/s | Depends on I/O subsystem |
| Enumeration (10K files) | <1 second | Should be fast |
| Memory overhead | <100 MB | Even for 100K+ files |
| Single file latency | <100 ms | From start to end |

**Optimization Opportunities for Future Milestones:**
- Async I/O (Tokio) for higher throughput
- Memory-mapped I/O for very large files
- Parallel enumeration and copying

---

## 11. Future Extensions (Not Milestone 1)

### Checksums and Verification
```rust
pub struct FileItem {
    checksum: Option<String>,  // SHA256, MD5, etc.
}

pub fn verify_file(item: &FileItem) -> Result<bool, EngineError>
```

### Job Persistence
```rust
pub fn save_job_state(job: &TransferJob, db_path: &Path) -> Result<(), EngineError>
pub fn load_job_state(db_path: &Path, job_id: Uuid) -> Result<TransferJob, EngineError>
```

### Concurrent Jobs
```rust
pub struct JobQueue {
    jobs: Vec<TransferJob>,
}

pub fn queue_job(queue: &mut JobQueue, job: TransferJob)
pub fn process_queue(queue: &mut JobQueue, max_concurrent: usize)
```

### Filtering and Exclusions
```rust
pub struct FilterPolicy {
    include_patterns: Vec<String>,
    exclude_patterns: Vec<String>,
}

pub fn create_job_with_filter(..., filter: FilterPolicy) -> Result<TransferJob, EngineError>
```

---

## 12. Implementation Checklist

### Phase 1: Foundation
- [ ] Create Cargo workspace
- [ ] Define model.rs with all structs and enums
- [ ] Define error.rs with EngineError
- [ ] Add skeleton functions to lib.rs

### Phase 2: Core Logic
- [ ] Implement fs_ops.rs (enumerate, copy_file, mkdir)
- [ ] Implement job.rs (create_job, plan_job, run_job)
- [ ] Handle long paths (\\?\ prefix)
- [ ] Handle Unicode paths

### Phase 3: Progress & Orchestration
- [ ] Implement progress.rs (ProgressCallback trait)
- [ ] Implement CliProgress in CLI
- [ ] Ensure callbacks are invoked at correct points

### Phase 4: CLI
- [ ] Implement argument parsing (clap)
- [ ] Implement main loop
- [ ] Add help and version
- [ ] Ensure proper exit codes

### Phase 5: Testing
- [ ] Unit tests for each module
- [ ] Integration tests (real filesystem)
- [ ] Manual testing on Windows

### Phase 6: Polish
- [ ] Run rustfmt and clippy
- [ ] Add doc comments
- [ ] Benchmark performance
- [ ] Final documentation

---

## Conclusion

This technical specification provides all details needed to implement Milestone 1. Each section is actionable and testable. The design is extensible; future milestones can build on this foundation without major refactoring.
