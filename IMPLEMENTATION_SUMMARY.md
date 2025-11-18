# TeraCopy-Style File Transfer Engine - Milestone 1 Implementation Summary

## Executive Summary

I have completed a comprehensive design for **Milestone 1** of a TeraCopy-style file transfer tool for Windows in Rust. This document summarizes the key decisions and provides an implementation roadmap.

The full design document is in `DESIGN.md`. This summary covers the critical points needed to begin implementation.

---

## 1. Requirements Refined

**What we're building:**
A headless, CLI-testable file transfer engine that:
- Copies/moves files and directory trees recursively
- Never aborts on single-file errors
- Tracks per-file state and progress
- Allows a future GUI to hook in via a trait-based callback system

**What's NOT in Milestone 1:**
- Checksums, verification, or integrity checks
- Multi-job queuing or scheduling
- Database persistence (SQLite)
- Locked-file handling, VSS, or symlinks
- Move with rollback (deferred; Move is a placeholder)

---

## 2. Crate Structure

```
BackUP/
├── Cargo.toml                (workspace root)
├── DESIGN.md                 (complete design doc)
├── README.md                 (quick start)
├── engine/                   (library crate)
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs            (public API exports)
│   │   ├── model.rs          (Job, FileItem, enums)
│   │   ├── error.rs          (EngineError type)
│   │   ├── fs_ops.rs         (enumerate, copy_file, mkdir)
│   │   ├── job.rs            (Job orchestration, run_job)
│   │   └── progress.rs       (ProgressCallback trait)
│   └── tests/
│       └── integration_tests.rs
│
└── cli/                      (binary crate)
    ├── Cargo.toml
    ├── src/
    │   └── main.rs           (CLI parsing, job execution, output)
    └── tests/
```

---

## 3. Core Data Model (Pseudocode)

### TransferJob
```rust
pub struct TransferJob {
    id: Uuid,
    mode: Mode,                    // Copy or Move
    source_path: PathBuf,
    destination_path: PathBuf,
    overwrite_policy: OverwritePolicy,
    
    files: Vec<FileItem>,          // All files to copy
    state: JobState,               // Pending, Running, Completed
    error: Option<EngineError>,    // Job-level error
    
    total_bytes_to_copy: u64,
    total_bytes_copied: u64,
    current_file_index: Option<usize>,
    start_time: Option<SystemTime>,
    end_time: Option<SystemTime>,
}
```

### FileItem
```rust
pub struct FileItem {
    id: Uuid,
    source_path: PathBuf,
    destination_path: PathBuf,
    file_size: u64,
    
    state: FileState,              // Pending, Copying, Done, Skipped, Failed
    bytes_copied: u64,
    
    error_code: Option<u32>,       // OS error code if Failed/Skipped
    error_message: Option<String>,
    
    is_dir: bool,
    last_modified: Option<u64>,    // Windows FILETIME
}
```

### Enums
```rust
enum Mode { Copy, Move }
enum FileState { Pending, Copying, Done, Skipped, Failed }
enum JobState { Pending, Running, Completed }
enum OverwritePolicy { Skip, Overwrite, Ask, SmartUpdate }
```

---

## 4. Engine Public API

### Main Functions
```rust
// Create a new job
pub fn create_job(
    source: impl AsRef<Path>,
    destination: impl AsRef<Path>,
    mode: Mode,
    overwrite_policy: OverwritePolicy,
) -> Result<TransferJob, EngineError>

// Plan the job (enumerate files)
pub fn plan_job(job: &mut TransferJob) -> Result<(), EngineError>

// Run the job (execute copies)
pub fn run_job(
    job: &mut TransferJob,
    progress_callback: Option<&dyn ProgressCallback>,
) -> Result<(), EngineError>

// Get summary of results
pub fn get_job_summary(job: &TransferJob) -> JobSummary
```

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

**Key design principle:** The callback trait decouples the engine from the UI. CLI and GUI both implement this trait independently.

---

## 5. Copy Algorithm (Simplified Flow)

```
1. Create Job
   - Validate source exists
   - Validate destination is creatable
   - Return job in Pending state

2. Plan Job
   - Recursively enumerate source tree
   - For each file/dir, create FileItem
   - Sum total bytes

3. Run Job
   For each FileItem:
     a. Check overwrite policy (Skip vs. Copy)
     b. If Copy:
        - Ensure destination directory exists
        - Open source file, create destination file
        - Buffer-copy contents
        - Preserve modification time
        - Mark FileItem as Done
     c. If Skip:
        - Mark FileItem as Skipped
     d. On error:
        - Record error_code and error_message
        - Mark FileItem as Failed
        - Continue to next file (DO NOT STOP)
     e. Invoke progress callbacks

4. Job Completed
   - All files processed
   - Return to caller with final state
```

---

## 6. Error Handling

### Job-Level Errors (EngineError)
These stop the job early:
- SourceNotFound
- DestinationAccessDenied
- EnumerationFailed (at root level)
- PathTooLong

### File-Level Errors
These are recorded in FileItem but do NOT stop the job:
- ReadError
- WriteError
- DirectoryCreationFailed

**Key principle:** One file failure ≠ job failure. Caller inspects results afterward.

---

## 7. CLI UX

### Usage
```bash
transfer --src "C:\Users\Alice\Documents" --dst "D:\Backup" [options]

Options:
  --mode copy|move              (default: copy)
  --overwrite-policy POLICY     (default: skip)
    POLICY: skip | overwrite | ask | smart-update

Examples:
  transfer --src C:\data --dst D:\backup
  transfer --src C:\data --dst D:\backup --overwrite-policy smart-update
```

### Console Output (Example)
```
Preparing job...
  Source: C:\data
  Destination: D:\backup
  Files to transfer: 127
  Total size: 2.3 GB

Enumerating...
[████████████░░░░░░] 60% | 1.4 GB / 2.3 GB | file.iso (450 MB / 512 MB)

Transfer complete!
Summary: 125 copied, 2 skipped, 0 failed
Elapsed: 2m 34s
```

### Exit Codes
- `0` = Success (all files copied, no failures)
- `1` = Some files failed or skipped
- `2` = Job-level error (source not found, access denied, etc.)

---

## 8. Implementation Roadmap

### Phase 1: Foundation (Setup & Data Model)
1. Initialize Rust workspace with engine and cli crates
2. Define enums: Mode, FileState, JobState, OverwritePolicy
3. Define TransferJob and FileItem structs
4. Implement EngineError type

### Phase 2: Core Engine Logic
1. Implement filesystem enumeration (recursive directory walk)
2. Implement job planning (populate file list, sum sizes)
3. Implement overwrite policy logic (Skip, Overwrite, SmartUpdate)
4. Implement file copy with metadata preservation
5. Implement directory creation logic
6. Handle long paths (\\?\ prefix) and Unicode

### Phase 3: Job Orchestration & Progress
1. Implement job execution loop (iterate files, apply policy, copy)
2. Implement ProgressCallback trait and simple CLI implementation
3. Implement job summary and result inspection

### Phase 4: CLI Implementation
1. Use clap for argument parsing
2. Implement main loop (create → plan → run)
3. Attach CliProgress callback
4. Add help and error messages
5. Ensure proper exit codes

### Phase 5: Testing
1. Unit tests for each module (enumeration, copy, policy, etc.)
2. Integration tests (real directory operations)
3. Manual testing on Windows with real files
4. Test error scenarios (permissions, missing files, disk full)

### Phase 6: Code Quality & Finalization
1. Run rustfmt and clippy; fix all issues
2. Add doc comments to public API
3. Benchmark performance
4. Prepare for future milestones (TODOs, extension points)
5. Update README and tag release

---

## 9. Key Design Decisions

| Decision | Rationale | Future Flexibility |
|----------|-----------|-------------------|
| **Synchronous execution** | Simple for Milestone 1, easier to test | Can add async/tokio later |
| **Progress via trait** | Decouples engine from UI technology | CLI and GUI use same callback |
| **Error isolation** | One file error doesn't crash job | Caller implements retry logic |
| **Lazy enumeration** | Separate planning from execution | Can stream both phases later |
| **No checksums M1** | Reduces scope, improves speed | Easy to add in Milestone 2 |
| **Single job at a time** | Simpler state management | Add job queue in later milestone |
| **Long path support** | Windows requirement for >260 chars | Built-in via \\?\ prefix |

---

## 10. Non-Functional Requirements

| Requirement | Approach | Status |
|---|---|---|
| **Windows 10+, x86_64** | Use winapi for native calls; test on Windows 10+ | Design ✓ |
| **Rust stable** | No nightly features; published crates only | Design ✓ |
| **Long paths** | Prepend \\?\ for paths >260 chars | Design ✓ |
| **Unicode paths** | Rust handles UTF-16 ↔ UTF-8 conversion | Design ✓ |
| **No global state** | All state in TransferJob; callbacks are stateless | Design ✓ |
| **Testability** | Core logic separated from CLI; mock callbacks | Design ✓ |
| **Extensibility** | Trait-based progress, enum-based errors | Design ✓ |

---

## 11. Testing Strategy

### Unit Tests
- Enumerate a test directory structure; verify file count and total size
- Apply each overwrite policy; verify skip vs. copy decisions
- Copy a file; verify contents and modification time match
- Job state transitions; verify Pending → Running → Completed

### Integration Tests
- Flat directory copy (no nesting)
- Deep nested directories
- Skip existing files, then overwrite, then smart-update
- Error scenarios: missing source, permission denied, no space
- Large files (>1 GB) with progress tracking

### Manual Tests
- Real Windows filesystem
- Unicode and long paths
- Performance baseline on large trees

---

## 12. Files to Create/Modify

### Files to Create
1. `Cargo.toml` (workspace root)
2. `engine/Cargo.toml`
3. `engine/src/lib.rs`
4. `engine/src/model.rs`
5. `engine/src/error.rs`
6. `engine/src/fs_ops.rs`
7. `engine/src/job.rs`
8. `engine/src/progress.rs`
9. `cli/Cargo.toml`
10. `cli/src/main.rs`
11. `README.md` (update with build/usage)

### Files to Keep
- `.gitignore`, `.github/`, `.codacy/`, `.git/` (unchanged)

### Files to Remove
- Delete old Go files (already deleted in working tree)

---

## 13. Assumptions for Implementation

1. **Windows-only code**: We'll use `#[cfg(target_os = "windows")]` and winapi as needed
2. **Buffered I/O**: Use std::io::copy with default buffer (8 KB)
3. **Modification time**: Use Windows FILETIME; preserve during copy
4. **Empty directories**: Created in destination; tracked in file list but not copied
5. **Symlinks**: Not supported in Milestone 1 (error or skip)
6. **Move operation**: Recorded in Model, but actual deletion deferred to Milestone 2
7. **Permissions**: Not modified; only file contents and timestamps

---

## 14. Next Steps

The design is complete and ready for implementation. The recommended approach:

1. **Create the workspace structure** (Phase 1, Task 1.1)
2. **Define data model** (Phase 1, Tasks 1.2–1.3)
3. **Implement core logic** (Phase 2, Tasks 2.1–2.5)
4. **Implement orchestration** (Phase 3, Tasks 3.1–3.3)
5. **Implement CLI** (Phase 4, Tasks 4.1–4.3)
6. **Add tests** (Phase 5)
7. **Polish and release** (Phase 6)

Each phase is designed to be independently testable before moving to the next.

---

## Questions for Clarification Before Implementation

1. **Symlinks**: Should we skip silently, error, or copy as regular files?
2. **Large files**: What's the target copy speed (MBps)? Any throttling in Milestone 1?
3. **Progress granularity**: Update callback every 1 MB, 10 MB, or per file?
4. **Move semantics**: Should Move track source deletion separately, or as a post-copy step?
5. **Testing environment**: Should I use tempdir crate for tests, or actual filesystem?

---

## Conclusion

This design provides a **solid, extensible foundation** for Milestone 1. The separation of concerns allows the team to:
- Develop engine and CLI in parallel
- Test each module independently
- Add future features without major refactoring
- Support multiple UIs (CLI, GUI, automation) from the same engine

The implementation is ready to begin.
