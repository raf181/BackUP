# TeraCopy-Style File Transfer Engine - Milestone 1 Design Document

## 1. Requirements Restatement and Refinement

### What We're Building

A **headless file transfer engine** in Rust that mimics TeraCopy's core functionality. It's designed as the foundation for a future GUI, but for Milestone 1 we provide a CLI for manual testing.

### Key Scope for Milestone 1

- **Copy and move operations** on files and directories
- **Recursive tree enumeration** with error resilience
- **Per-file tracking** (state, progress, errors)
- **Configurable overwrite policy** (Skip, Overwrite, Ask, Smart)
- **Progress callbacks** suitable for CLI bars and future UI
- **No single-file error** should kill the entire job
- **CLI binary** for testing and basic use

### What's NOT in Milestone 1
- Checksums or integrity verification
- Multiple concurrent jobs or queuing
- SQLite persistence
- Shell integration or explorer context menus
- Locked-file handling (VSS)
- Symlink handling (deferred for later)
- Delta/incremental transfer (future milestone)

### Key Assumptions
1. **Windows 10+, x86_64**: No macOS/Linux support initially.
2. **Rust stable** only, no nightly-specific features.
3. **Long path support**: Will use `\\?\` prefix for paths > 260 chars.
4. **UTF-8 paths**: Windows uses UTF-16 internally; we'll wrap winapi correctly.
5. **Synchronous execution**: No async/tokio in Milestone 1; keep it simple.
6. **Single job at a time**: CLI runs one job; engine library is job-agnostic.
7. **"Ask" policy default**: For CLI, "Ask" becomes "Skip" by default (no user interaction).
8. **Failure isolation**: A file copy failure doesn't rollback or retry automatically.
9. **Empty directories are preserved** in destination tree.
10. **No permission changes**: Copy file contents and metadata (modify time) only.

---

## 2. Proposed Crate Structure

### Workspace Layout
```
BackUP/
├── Cargo.toml                       (workspace root)
├── DESIGN.md                        (this file)
├── engine/                          (library crate)
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs
│   │   ├── model.rs                (Job, FileItem, enums)
│   │   ├── error.rs                (Error types)
│   │   ├── fs_ops.rs               (Enumeration, copy, directory creation)
│   │   ├── job.rs                  (Job orchestration and state machine)
│   │   └── progress.rs             (Progress callback interface)
│   └── tests/                       (integration tests)
│
├── cli/                             (binary crate)
│   ├── Cargo.toml
│   ├── src/
│   │   └── main.rs                 (CLI argument parsing, job execution, output)
│   └── tests/
│
└── README.md
```

### Crate Responsibilities

#### `engine` (library)
Provides the core transfer logic:
- **model.rs**: Type definitions (TransferJob, FileItem, Mode, State, OverwritePolicy)
- **error.rs**: Custom error type (EngineError) wrapping OS/IO errors
- **fs_ops.rs**: Filesystem operations (enumerate, copy_file, create_dir_recursive)
- **job.rs**: Job orchestration (planning, execution, state transitions)
- **progress.rs**: Progress reporting trait/callback definition
- **lib.rs**: Public API exports

#### `cli` (binary)
Provides a command-line interface:
- Argument parsing (--src, --dst, --mode, --overwrite-policy)
- Creates and runs a TransferJob
- Prints progress to stdout (simple bars or summaries)
- Exits with proper codes (0 = all OK, 1 = some errors, 2 = fatal error)

---

## 3. Core Data Model Design

### TransferJob Entity
Represents a single copy/move operation.

**Fields:**
```
id: Uuid                                   // Unique job identifier
mode: Mode                                 // Copy or Move
source_path: PathBuf                       // Source directory root
destination_path: PathBuf                  // Destination directory root
overwrite_policy: OverwritePolicy          // How to handle existing files
created_at: SystemTime                     // Job creation timestamp

files: Vec<FileItem>                       // All files in the job
state: JobState                            // Pending / Running / Completed
error: Option<EngineError>                 // Job-level error (e.g., source path doesn't exist)

// Progress tracking
total_bytes_to_copy: u64                   // Sum of all file sizes
total_bytes_copied: u64                    // Updated as files complete
current_file_index: Option<usize>          // Index into `files` currently being processed
start_time: Option<SystemTime>             // When execution started
end_time: Option<SystemTime>               // When execution ended
```

**State Transitions:**
- `Pending` → `Running` (when execution starts)
- `Running` → `Completed` (when all files processed, regardless of success)

---

### FileItem Entity
Represents a single file within a job.

**Fields:**
```
id: Uuid                                   // Unique file identifier
source_path: PathBuf                       // Full source path
destination_path: PathBuf                  // Full destination path
file_size: u64                             // File size in bytes

state: FileState                           // Pending / Copying / Done / Skipped / Failed
bytes_copied: u64                          // Bytes transferred for this file (for progress)

// Error tracking
error_code: Option<u32>                    // OS error code if state is Failed/Skipped
error_message: Option<String>              // Human-readable error

// Metadata
is_dir: bool                               // True if this is a directory entry (no copy, just create)
last_modified: Option<u64>                 // File modification time (Windows FILETIME)
```

**Why these fields:**
- `id`: Allows external systems to track files uniquely.
- `state`: Caller must know which files succeeded/failed.
- `bytes_copied`: Progress callbacks need per-file progress.
- `error_code` + `error_message`: Caller can decide retry logic later.
- `is_dir`: Directories are tracked but don't require copying, only creation.

---

### Enumeration Types

#### Mode
```
enum Mode {
    Copy,   // Source files remain
    Move,   // Source files deleted after successful copy (Milestone 1: not fully implemented, marked for future)
}
```

#### FileState
```
enum FileState {
    Pending,    // Not yet processed
    Copying,    // Currently transferring
    Done,       // Successfully copied
    Skipped,    // Not copied due to overwrite policy or other skip condition
    Failed,     // Error encountered, not recovered
}
```

#### JobState
```
enum JobState {
    Pending,     // Created, not started
    Running,     // Currently executing
    Completed,   // All files processed (some may have failed)
}
```

#### OverwritePolicy
```
enum OverwritePolicy {
    Skip,                    // Don't overwrite; skip the file
    Overwrite,               // Always overwrite
    Ask,                     // Placeholder for future UI; default to Skip for CLI
    SmartUpdate,             // Overwrite if source newer OR different size
}
```

---

## 4. Engine API Design (Conceptual)

### Public Types
```rust
pub use crate::model::{TransferJob, FileItem, Mode, FileState, JobState, OverwritePolicy};
pub use crate::error::EngineError;
pub use crate::progress::ProgressCallback;
```

### Main Functions

#### Job Creation
```
fn create_job(
    source: impl AsRef<Path>,
    destination: impl AsRef<Path>,
    mode: Mode,
    overwrite_policy: OverwritePolicy,
) -> Result<TransferJob, EngineError>
```
- Validates paths exist (source must exist; destination may not).
- Returns a Pending job ready to run.
- Does NOT perform enumeration (lazy enumeration on run).

#### Job Enumeration (Planning)
```
fn plan_job(job: &mut TransferJob) -> Result<(), EngineError>
```
- Recursively enumerates source tree.
- Populates `job.files` with all FileItems.
- Calculates `job.total_bytes_to_copy`.
- Returns error if source enumeration fails (job-level error).

#### Job Execution
```
fn run_job(
    job: &mut TransferJob,
    progress_callback: Option<&dyn ProgressCallback>,
) -> Result<(), EngineError>
```
- Transitions job state: `Pending` → `Running` → `Completed`.
- For each file, applies overwrite policy and copies if needed.
- Invokes progress_callback at key points (start, per-file update, completion).
- Never returns Err due to individual file failures; file errors recorded in FileItem.
- Returns Err only for unrecoverable job-level issues (source path becomes inaccessible mid-run).

#### Result Inspection
```
fn get_job_summary(job: &TransferJob) -> JobSummary
```
- Returns counts: total files, copied, skipped, failed.
- Provides caller easy access to final state.

### Progress Callback Trait

```rust
pub trait ProgressCallback: Send {
    fn on_job_started(&self, job: &TransferJob);
    fn on_file_started(&self, job: &TransferJob, file_index: usize, file: &FileItem);
    fn on_file_progress(&self, job: &TransferJob, file_index: usize, bytes_this_file: u64);
    fn on_file_completed(&self, job: &TransferJob, file_index: usize, file: &FileItem);
    fn on_job_completed(&self, job: &TransferJob);
}
```

**Why a trait:**
- Decouples engine from progress sink (stdout, GUI window, file, etc.).
- CLI implements a simple struct that prints to stdout.
- Future GUI implements a struct that updates UI.
- Testable: mock implementation can verify callback order.

### Error Handling

```rust
pub enum EngineError {
    SourceNotFound(PathBuf),
    DestinationAccessDenied(PathBuf),
    ReadError { path: PathBuf, source: io::Error },
    WriteError { path: PathBuf, source: io::Error },
    PathTooLong(PathBuf),
    InvalidPath(PathBuf),
    EnumerationFailed { path: PathBuf, source: io::Error },
    Unknown(String),
}
```

**File-level errors** are stored in `FileItem.error_code` and `FileItem.error_message`.

---

## 5. Copy Algorithm for Milestone 1

### Overall Flow

```
1. Create Job (validate paths, set overwrite policy)
2. Plan Job (enumerate source tree, build file list, sum sizes)
3. Run Job:
   a. For each FileItem in order:
      i.   Check overwrite policy
      ii.  If skip: mark Skipped, continue
      iii. If copy:
           - Create destination directory (if needed)
           - Copy file contents
           - Preserve modification time
           - Mark Done
      iv. On error: record error details, mark Failed, continue
   b. After all files: mark job Completed
4. Report results
```

### Detailed Substeps

#### Step 1: Enumerate Source Tree
```
Input: source_path, job.files (initially empty)
Output: job.files populated with all FileItems

Pseudocode:
  fn enumerate(path, rel_path_from_source):
    for entry in fs::read_dir(path):
      metadata = entry.metadata()
      rel_name = entry.file_name()
      rel_path = rel_path_from_source / rel_name
      dest_path = job.destination_path / rel_path
      
      if entry.is_dir():
        add FileItem(source_path=path/rel_name, destination_path=dest_path, is_dir=true, size=0)
        recurse into entry
      else:
        size = metadata.len()
        add FileItem(source_path=path/rel_name, destination_path=dest_path, is_dir=false, size=size)
        job.total_bytes_to_copy += size
```

**Error handling:** If enumeration of any subdirectory fails, record the error on the parent directory's FileItem (if it exists) or return a job-level error if the root fails.

#### Step 2: Apply Overwrite Policy
```
Input: FileItem with destination_path, job.overwrite_policy
Output: Decision (Skip, Copy, or error)

Pseudocode:
  match overwrite_policy:
    Skip:
      if destination_path exists:
        return Skip
      else:
        return Copy
    
    Overwrite:
      return Copy  (will replace if exists)
    
    Ask:
      # Milestone 1: no UI, default to Skip
      if destination_path exists:
        return Skip
      else:
        return Copy
    
    SmartUpdate:
      if destination_path doesn't exist:
        return Copy
      else:
        src_mtime = source file modification time
        src_size = source file size
        dst_mtime = destination file modification time
        dst_size = destination file size
        
        if src_mtime > dst_mtime OR src_size != dst_size:
          return Copy
        else:
          return Skip
```

#### Step 3: Copy File
```
Input: FileItem with source_path, destination_path
Output: FileItem with state=Done or state=Failed with error details

Pseudocode:
  try:
    // Ensure destination directory exists
    ensure_parent_dir_exists(destination_path)
    
    // Copy file contents (buffered read/write)
    source_file = File::open(source_path)
    dest_file = File::create(destination_path)
    bytes_copied = io::copy(source_file, dest_file)
    
    // Preserve modification time
    src_metadata = source_file.metadata()
    set_file_mtime(dest_file, src_metadata.modified())
    
    file.bytes_copied = bytes_copied
    file.state = Done
    job.total_bytes_copied += bytes_copied
    
  catch (error):
    file.error_code = error.raw_os_error()
    file.error_message = error.to_string()
    file.state = Failed
    // DO NOT stop job, continue to next file
```

#### Step 4: Directory Creation
```
Input: destination_path for a directory
Output: Directory created or error recorded

Pseudocode:
  fn ensure_parent_dir_exists(path):
    if parent == destination root:
      return OK (root should exist or be creatable)
    
    try:
      fs::create_dir_all(parent)
    catch (error):
      if error == "already exists and is_dir":
        return OK
      else:
        record error on the directory FileItem
        return ERROR
```

#### Step 5: Error Recording
```
Input: FileItem, OS error
Output: FileItem with error_code and error_message

For each file error:
  file.error_code = Some(os_error.raw_os_error())
  file.error_message = Some(os_error.to_string())
  file.state = Failed (or Skipped if due to policy)
  
Job continues to next file.
```

#### Step 6: Handle Long Paths
```
On Windows, if path > 260 chars:
  Prepend \\?\ (or \\?\UNC\ for UNC paths)
  Pass to winapi functions that accept this prefix
  
Rust's std::fs abstracts much of this, but we'll be explicit
in code comments and use helper functions.
```

---

## 6. Error Handling Strategy

### Error Representation

**Job-level errors** (EngineError):
- Source path doesn't exist
- Source path inaccessible (permissions)
- Enumeration fails at root level
- Destination path is invalid or inaccessible

These are returned from `create_job()`, `plan_job()`, or `run_job()`.

**File-level errors**:
- Individual file read failure
- Individual file write failure
- Directory creation failure for a specific subtree

These are recorded in `FileItem.error_code` and `FileItem.error_message`. The job completes, but the file item is marked `Failed` or `Skipped`.

### Error Type Definition

```rust
#[derive(Debug)]
pub enum EngineError {
    SourceNotFound(PathBuf),
    DestinationAccessDenied(PathBuf),
    ReadError { path: PathBuf, source: io::Error },
    WriteError { path: PathBuf, source: io::Error },
    PathTooLong(PathBuf),
    InvalidPath(PathBuf, String),
    EnumerationFailed { path: PathBuf, source: io::Error },
    DirectoryCreationFailed { path: PathBuf, source: io::Error },
    Unknown(String),
}

impl From<io::Error> for EngineError { /* ... */ }
impl Display for EngineError { /* ... */ }
```

### CLI Error Reporting

```
Exit codes:
  0  = All files copied/skipped successfully, no failures.
  1  = Some files failed or skipped, but job completed.
  2  = Job-level error (source not found, access denied, etc.).

Output:
  Summary before exit:
    "Summary: 100 files copied, 5 skipped, 2 failed."
    "Failures:"
    "  file1.txt: Permission denied (5)"
    "  file2.dat: No space left on device (28)"
```

---

## 7. CLI UX for Milestone 1

### Arguments

```
Syntax:
  transfer --src <source_dir> --dst <destination_dir> [--mode copy|move] [--overwrite-policy policy]

Examples:
  transfer --src "C:\Users\Alice\Documents" --dst "D:\Backup"
  transfer --src "C:\data" --dst "\\server\backup" --mode copy --overwrite-policy skip
  transfer --src "C:\source" --dst "C:\dest" --overwrite-policy smart-update

Policies:
  skip         = Don't overwrite, skip existing files
  overwrite    = Overwrite all existing files
  ask          = Prompt per file (Milestone 1: default to skip)
  smart-update = Overwrite if source newer or different size
```

### Error Handling

- Missing or invalid arguments: Print usage and exit(2).
- Non-existent source: Print error and exit(2).
- Job-level error during enumeration: Print error and exit(2).
- File-level errors during copy: Accumulate, print summary at end, exit(1).
- No errors: Print summary and exit(0).

### Console Output During Execution

```
Console (example):

$ transfer --src C:\source --dst D:\backup --overwrite-policy skip

Preparing job...
  Source: C:\source
  Destination: D:\backup
  Mode: Copy
  Overwrite Policy: Skip
  
Enumerating files...
  Found 127 files, 2.3 GB total
  
Starting transfer...
[=====>         ] 42% | 987 MB / 2.3 GB | File: document.pdf (234 KB / 512 KB)

Transfer complete!
Summary: 125 copied, 2 skipped, 0 failed
Elapsed: 45s
```

### Progress Display

- Use simple text-based progress bar (overwrite previous line).
- Show:
  - Percentage complete
  - Bytes transferred / Total bytes
  - Current file name and per-file progress
  - Elapsed time

Later, GUI can subscribe to the same ProgressCallback trait and render progress visually.

---

## 8. Concrete Implementation Plan

### Phase 1: Foundation (Setup & Data Model)

**Task 1.1: Initialize Rust Project**
- Create workspace with engine and cli crates.
- Add dependencies: clap (CLI parsing), uuid (unique IDs), chrono (timestamps).
- Establish build and test infrastructure.

**Task 1.2: Define Data Model**
- Implement enum types: Mode, FileState, JobState, OverwritePolicy.
- Implement TransferJob struct with all fields.
- Implement FileItem struct with all fields.
- Add serialization support (Debug, Clone, etc.) as needed.

**Task 1.3: Define Error Type**
- Create EngineError enum.
- Implement From<io::Error> and Display traits.
- Test that error messages are user-friendly.

**Dependencies:** Tasks 1.1 → 1.2 → 1.3 (sequential).

---

### Phase 2: Core Engine Logic

**Task 2.1: Filesystem Enumeration**
- Implement `enumerate_tree(source_path) -> Vec<FileItem>`.
- Recursively walk directory tree, collecting files and directories.
- Calculate total size.
- Handle long paths (\\?\\ prefix).
- Handle Unicode paths.
- Test: enumerate a known directory structure and verify output.

**Task 2.2: Job Planning**
- Implement `plan_job(job: &mut TransferJob) -> Result<(), EngineError>`.
- Calls enumerate_tree, populates job.files, sums sizes.
- Handle errors: non-existent source, access denied.
- Test: plan a job and verify job.files is correct.

**Task 2.3: Overwrite Policy Logic**
- Implement `apply_overwrite_policy(item: &FileItem, policy: OverwritePolicy) -> Decision`.
- Decision enum: Skip, Copy, or Error.
- Implement SmartUpdate logic (compare mtime and size).
- Test: verify each policy's behavior.

**Task 2.4: File Copy Logic**
- Implement `copy_file(src: &Path, dst: &Path) -> Result<u64, EngineError>`.
- Open source, create/truncate destination, io::copy() with buffer.
- Preserve modification time using winapi or std::fs.
- Handle errors: read error, write error, no space, etc.
- Test: copy a file, verify contents and mtime.

**Task 2.5: Directory Handling**
- Implement `ensure_parent_dir_exists(path: &Path) -> Result<(), EngineError>`.
- Create directories recursively as needed.
- Handle "already exists" case gracefully.
- Test: run job, verify directory tree created correctly.

**Dependencies:** 2.1 → 2.2 (enumeration before planning). 2.3, 2.4, 2.5 can be parallel. All feed into Task 3.1.

---

### Phase 3: Job Orchestration & Progress

**Task 3.1: Job Execution Loop**
- Implement `run_job(job: &mut TransferJob, progress_callback: Option<&dyn ProgressCallback>) -> Result<(), EngineError>`.
- Iterate over job.files.
- For each file:
  - Update job.current_file_index.
  - Apply overwrite policy.
  - Call progress callback (on_file_started).
  - Copy file (or skip), update state and bytes_copied.
  - Call progress callback (on_file_completed).
- On error: record error on FileItem, do NOT stop job.
- At end: mark job.state = Completed.
- Test: run a job with mock files, verify final state.

**Task 3.2: Progress Callback Trait**
- Define ProgressCallback trait with methods: on_job_started, on_file_started, on_file_progress, on_file_completed, on_job_completed.
- Implement a simple CliProgress struct that prints to stdout.
- Test: run job with CliProgress, verify console output.

**Task 3.3: Job Summary & Results**
- Implement `get_job_summary(job: &TransferJob) -> JobSummary`.
- JobSummary includes: total_files, copied, skipped, failed, elapsed_time.
- Implement method to extract list of failed files with errors.
- Test: summarize a completed job, verify counts match.

**Dependencies:** 2.1 → 2.5 (core logic) → 3.1 (orchestration). 3.2 and 3.3 can be parallel with 3.1.

---

### Phase 4: CLI Implementation

**Task 4.1: Argument Parsing**
- Use clap to define --src, --dst, --mode, --overwrite-policy.
- Validate arguments: source must exist, destination parent must exist or be creatable.
- Parse mode and policy enums from strings.
- Test: various argument combinations, verify errors for bad input.

**Task 4.2: CLI Main Loop**
- Parse arguments → create job → plan job → run job.
- Attach CliProgress callback.
- Handle and report job-level errors (exit code 2).
- On completion: print summary, exit with appropriate code.
- Test: run CLI with test data, verify console output and exit codes.

**Task 4.3: Help & Error Messages**
- Implement --help and --version flags.
- Provide clear, actionable error messages (e.g., "Source directory not found: C:\nonexistent").
- Ensure error messages fit in typical terminal width.
- Test: run CLI with --help, bad arguments, etc.

**Dependencies:** Phase 2 and 3 complete → 4.1 (parallel with 4.2) → 4.2 → 4.3.

---

### Phase 5: Testing & Documentation

**Task 5.1: Unit Tests (Engine)**
- Test each function in isolation:
  - Enumeration with nested directories.
  - Overwrite policy for all variants.
  - File copy with various error conditions.
  - Job planning and execution.
- Aim for >80% coverage of engine crate.
- Use temporary directories (tempfile crate) for file I/O tests.

**Task 5.2: Integration Tests**
- Create test scenarios:
  - Copy flat directory (no nesting).
  - Copy nested directories.
  - Skip existing files.
  - Overwrite files.
  - SmartUpdate policy.
  - Handle errors gracefully (missing source, access denied).
- Run each scenario with both library API and CLI.
- Verify final state (directory structure, file contents, error counts).

**Task 5.3: Documentation**
- Add doc comments to all public types and functions.
- Document error cases and recovery strategies.
- Provide examples in lib.rs for typical usage.
- Update README with build instructions and usage examples.

**Task 5.4: Manual Testing**
- Test on real Windows filesystem with various path types.
- Test with large files (>2 GB) to verify progress reporting.
- Test with deeply nested directories (>100 levels).
- Test with long paths (>260 chars).
- Test with Unicode filenames.
- Document any OS-specific issues found.

**Dependencies:** Phase 2, 3, 4 complete → 5.1 (unit tests in parallel with 5.2) → 5.2 → 5.3 → 5.4.

---

### Phase 6: Code Quality & Finalization

**Task 6.1: Code Review & Cleanup**
- Ensure consistent style (rustfmt).
- Run clippy; fix all warnings.
- Review for security issues (no unsafe code without justification).
- Simplify any overly complex logic.

**Task 6.2: Performance Baseline**
- Benchmark copy operation with various file sizes.
- Profile for unnecessary allocations or copying.
- Document bottlenecks for future optimization.

**Task 6.3: Prepare for Future Milestones**
- Leave clear TODOs for features marked "future" (e.g., checksums, Move with rollback).
- Design extension points (e.g., plugin system for filters).
- Document assumptions to revisit in later milestones.

**Task 6.4: Release & Tag**
- Update version in Cargo.toml to 0.1.0 (Milestone 1).
- Create CHANGELOG summarizing Milestone 1 features.
- Tag release in git.

**Dependencies:** 5.1 → 5.2 → 5.3 → 5.4 (in order) → 6.1 → 6.2 → 6.3 → 6.4.

---

## 9. Justification for Order

### Phase 1 (Foundation)
We start with setup and data model because:
- Rust requires well-defined types upfront.
- All subsequent code depends on Job and FileItem.
- Early definition prevents rework.

### Phase 2 (Core Logic)
Enumeration before planning before execution ensures:
- We understand the source tree before making decisions.
- Planning step is testable independently from execution.
- Copy logic is isolated and reusable.

### Phase 3 (Orchestration)
Jobs and progress reporters come after core logic because:
- We need the pieces (copy, enumerate) to orchestrate.
- Progress callback is a non-critical abstraction.
- Job summary is easily added once execution is done.

### Phase 4 (CLI)
CLI comes last because:
- It depends on the entire engine library.
- We can test engine without CLI.
- If CLI has bugs, engine remains usable for other frontends.

### Phase 5 (Testing)
Testing follows implementation because:
- It's faster to test a complete feature set.
- Unit tests guide refactoring late in the process.
- Integration tests catch cross-module issues.

### Phase 6 (Polish)
Final quality checks come last because:
- We don't want to optimize/refactor during implementation.
- Code review is more efficient on complete features.
- Milestones are finalized when all functionality is done.

---

## 10. Key Design Decisions

### Synchronous, Not Async
- **Rationale**: Milestone 1 is a headless utility, not a service. Sync keeps code simple and testable.
- **Future**: Async can be added in a later milestone if concurrent job queuing is needed.

### Single Trait for Progress
- **Rationale**: Decouples engine from UI technology. CLI and GUI both implement the same trait.
- **Future**: Could add channels-based progress if async is added.

### File State Machine
- **Rationale**: Clear states prevent logic errors (e.g., marking Copying twice).
- **Future**: Can expand states (e.g., Verified, Retrying) without breaking existing code.

### Error Isolation
- **Rationale**: One file error should not crash the entire job. Callers decide retry strategy.
- **Future**: Add retry logic, error recovery, or "apply to all" in later milestones.

### Lazy Enumeration Postponed
- **Rationale**: Plan phase separates enumeration from execution, allowing progress to be reported accurately.
- **Future**: For very large trees, can stream enumeration and execution together.

### No Async I/O in Milestone 1
- **Rationale**: Simpler testing, fewer dependencies, easier to reason about progress.
- **Future**: Tokio-based async can boost throughput in later milestones.

---

## 11. Testing Strategy

### Unit Tests (per module)
- **model.rs**: Verify enum values, state transitions.
- **error.rs**: Verify error messages are sensible.
- **fs_ops.rs**: Mock filesystem or use tempdir; test enumerate, copy, mkdir.
- **job.rs**: Mock callbacks; verify job state transitions.
- **progress.rs**: Test callback invocations in order.

### Integration Tests
- Create test directories with known structure.
- Run jobs with different policies and verify results.
- Simulate error conditions (permission denied, disk full, etc.) where possible.
- Clean up test artifacts.

### Manual Test Scenarios
- Copy a directory with 1000+ files.
- Copy a directory with files >1 GB.
- Copy a directory with Unicode names and long paths.
- Use CLI with various argument combinations.

---

## 12. Non-Functional Requirements Coverage

| Requirement | Approach |
|---|---|
| Windows 10+, x86_64 | Use winapi for native APIs; test on Windows 10 VM. |
| Rust stable | No nightly-only features; use published crate versions. |
| Long paths (\\?\) | Wrap std::fs calls; prepend prefix for paths >260 chars. |
| Unicode paths | Rust's Path handles UTF-16 ↔ UTF-8 conversion implicitly. |
| No global state | All state in TransferJob; ProgressCallback is stateless. |
| Testable design | Core logic separate from CLI; mock callback for unit tests. |
| Extensible | Trait-based progress, enum-based error types, separate crate. |

---

## Conclusion

This design provides a **clean, modular foundation** for a TeraCopy-like file transfer engine. The separation of concerns (model, logic, orchestration, CLI) allows independent development and testing. The progress callback trait and error isolation strategies enable future features without breaking existing code.

Milestone 1 will deliver a working, testable engine that can be used from the CLI and can later be wrapped by a GUI or integrated into other tools.
