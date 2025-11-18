# Getting Started - Milestone 1 Implementation

**For:** Rust Developer(s) about to implement the first phase  
**What to Read First:** This document, then IMPLEMENTATION_SUMMARY.md  
**Time to Read:** 10 minutes

---

## The Five Most Important Things

1. **Architecture**: Engine (library) + CLI (binary). Separate concerns.
2. **Data Model**: TransferJob (entire operation) + FileItem (one file). Simple and clear.
3. **Error Isolation**: File errors don't kill the job. Always record and continue.
4. **Progress Callbacks**: Via trait. Decouples engine from UI.
5. **Two Phases**: Plan (enumerate) → Run (copy). Accurate progress reporting.

---

## File Organization

After you're done, your workspace will look like:

```
BackUP/
├── .git/, .github/, .gitignore, .codacy/   (already exist)
├── DESIGN.md                               (complete design - 3000+ lines)
├── TECHNICAL_SPEC.md                       (implementation detail)
├── IMPLEMENTATION_SUMMARY.md               (roadmap)
├── DESIGN_DELIVERY_SUMMARY.md              (this plan reviewed)
├── README.md                               (user doc)
├── GETTING_STARTED.md                      (you're reading this)
│
├── Cargo.toml                              (workspace root)
├── engine/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs                 (pub use model, error, fs_ops, job, progress)
│   │   ├── model.rs               (Job, FileItem, Mode, FileState, etc.)
│   │   ├── error.rs               (EngineError enum + Display)
│   │   ├── fs_ops.rs              (enumerate, copy_file, mkdir)
│   │   ├── job.rs                 (create_job, plan_job, run_job)
│   │   └── progress.rs            (ProgressCallback trait)
│   └── tests/
│       └── integration_tests.rs
│
├── cli/
│   ├── Cargo.toml
│   ├── src/
│   │   └── main.rs                (arg parsing, CliProgress, main loop)
│   └── tests/
│
└── target/ (build artifacts - gitignored)
```

---

## Implementation Phases (At a Glance)

### Phase 1: Foundation (2-3 days)
**Goal**: Data structures and project setup

```bash
# What you'll do:
1. cargo init --name engine engine/
2. cargo init --bin --name cli cli/
3. cargo new --lib engine/src (already done by cargo init)
4. Implement model.rs (Job, FileItem, Mode, FileState, JobState, OverwritePolicy)
5. Implement error.rs (EngineError enum)
6. Create stub functions in lib.rs
7. Cargo.toml: add uuid, chrono, serde dependencies
```

**Test**: `cargo build` and `cargo test` should succeed (no implementations yet).

### Phase 2: Core Logic (3-4 days)
**Goal**: Filesystem operations and algorithms

```bash
# What you'll do:
1. Implement fs_ops.rs:
   - fn enumerate_tree(path: &Path) -> Result<Vec<FileItem>, EngineError>
   - fn copy_file_with_metadata(src: &Path, dst: &Path) -> Result<u64, EngineError>
   - fn ensure_parent_dir_exists(path: &Path) -> Result<(), EngineError>
2. Implement job.rs:
   - fn create_job(...) -> Result<TransferJob, EngineError>
   - fn plan_job(job: &mut TransferJob) -> Result<(), EngineError>
   - fn run_job(job: &mut TransferJob, callback: Option<&dyn ProgressCallback>) -> Result<(), EngineError>
3. Implement helper functions (overwrite policy, error recording)
4. Add Windows long-path support (\\?\ prefix)
5. Handle Unicode paths (Rust handles transparently)
```

**Test**: Unit tests for each function. Real filesystem operations.

### Phase 3: Progress & Orchestration (2-3 days)
**Goal**: Callbacks and job execution integration

```bash
# What you'll do:
1. Implement progress.rs:
   - pub trait ProgressCallback { 5 methods }
2. In cli/src/main.rs, implement CliProgress struct
3. Update run_job() to invoke callbacks at correct points
4. Implement get_job_summary() function
5. Test callback invocation order
```

**Test**: Integration tests verifying callbacks are invoked in correct order.

### Phase 4: CLI (1-2 days)
**Goal**: Command-line interface

```bash
# What you'll do:
1. cargo add --path engine clap in cli/
2. Implement main.rs:
   - Argument parsing (clap derive macro)
   - Call create_job(), plan_job(), run_job()
   - Print progress to stdout
   - Exit with correct code (0, 1, or 2)
3. Add --help and --version
4. Nice error messages
```

**Test**: Manual testing with real directories.

### Phase 5: Testing (2-3 days)
**Goal**: Comprehensive test coverage

```bash
# What you'll do:
1. Add tempfile dependency
2. Write unit tests for each module
3. Write integration tests
4. Test error scenarios (permission denied, missing file, etc.)
5. Test all overwrite policies
6. Manual testing with real files
```

**Test**: `cargo test` shows 80%+ coverage. Manual test on Windows.

### Phase 6: Polish (1-2 days)
**Goal**: Production readiness

```bash
# What you'll do:
1. cargo fmt
2. cargo clippy -- -D warnings
3. Add doc comments to all public items
4. Benchmark copy speed
5. Final code review
6. Update README with examples
7. Tag release
```

**Test**: `cargo build --release` succeeds. Binary runs correctly.

---

## Key Implementation Details (Copy-Paste Ready)

### Phase 1: Data Model Skeleton

```rust
// engine/src/model.rs

use std::path::PathBuf;
use std::time::SystemTime;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct TransferJob {
    pub id: Uuid,
    pub mode: Mode,
    pub source_path: PathBuf,
    pub destination_path: PathBuf,
    pub overwrite_policy: OverwritePolicy,
    pub files: Vec<FileItem>,
    pub state: JobState,
    pub error: Option<crate::error::EngineError>,
    pub total_bytes_to_copy: u64,
    pub total_bytes_copied: u64,
    pub current_file_index: Option<usize>,
    pub created_at: SystemTime,
    pub start_time: Option<SystemTime>,
    pub end_time: Option<SystemTime>,
}

#[derive(Debug, Clone)]
pub struct FileItem {
    pub id: Uuid,
    pub source_path: PathBuf,
    pub destination_path: PathBuf,
    pub file_size: u64,
    pub state: FileState,
    pub bytes_copied: u64,
    pub error_code: Option<u32>,
    pub error_message: Option<String>,
    pub is_dir: bool,
    pub last_modified: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Copy,
    Move,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileState {
    Pending,
    Copying,
    Done,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Pending,
    Running,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverwritePolicy {
    Skip,
    Overwrite,
    Ask,
    SmartUpdate,
}
```

### Phase 1: Error Type Skeleton

```rust
// engine/src/error.rs

use std::fmt::{Display, self};
use std::path::PathBuf;
use std::io;

#[derive(Debug)]
pub enum EngineError {
    SourceNotFound { path: PathBuf },
    SourceAccessDenied { path: PathBuf, source: io::Error },
    DestinationAccessDenied { path: PathBuf, source: io::Error },
    ReadError { path: PathBuf, source: io::Error },
    WriteError { path: PathBuf, source: io::Error },
    PathTooLong { path: PathBuf },
    InvalidPath { path: PathBuf, reason: String },
    EnumerationFailed { path: PathBuf, source: io::Error },
    DirectoryCreationFailed { path: PathBuf, source: io::Error },
    Unknown { message: String },
}

impl Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceNotFound { path } => write!(f, "Source not found: {}", path.display()),
            Self::SourceAccessDenied { path, .. } => write!(f, "Access denied: {}", path.display()),
            Self::ReadError { path, .. } => write!(f, "Read error: {}", path.display()),
            Self::WriteError { path, .. } => write!(f, "Write error: {}", path.display()),
            Self::PathTooLong { path } => write!(f, "Path too long: {}", path.display()),
            Self::InvalidPath { path, reason } => write!(f, "Invalid path: {} ({})", path.display(), reason),
            _ => write!(f, "Engine error"),
        }
    }
}

impl std::error::Error for EngineError {}

impl From<io::Error> for EngineError {
    fn from(err: io::Error) -> Self {
        Self::Unknown { message: err.to_string() }
    }
}
```

### Phase 2: Enumeration Skeleton

```rust
// engine/src/fs_ops.rs

use std::path::Path;
use crate::model::FileItem;
use crate::error::EngineError;
use uuid::Uuid;

pub fn enumerate_tree(
    source: &Path,
    destination_root: &Path,
) -> Result<Vec<FileItem>, EngineError> {
    let mut items = Vec::new();

    fn recurse(
        path: &Path,
        rel_path: &Path,
        destination_root: &Path,
        items: &mut Vec<FileItem>,
    ) -> Result<(), EngineError> {
        for entry in std::fs::read_dir(path)
            .map_err(|e| EngineError::EnumerationFailed { path: path.to_path_buf(), source: e })?
        {
            let entry = entry.map_err(|e| EngineError::EnumerationFailed {
                path: path.to_path_buf(),
                source: e,
            })?;

            let metadata = entry.metadata().map_err(|e| EngineError::EnumerationFailed {
                path: path.to_path_buf(),
                source: e,
            })?;

            let file_name = entry.file_name();
            let rel_name = Path::new(&file_name);
            let rel_full_path = rel_path.join(rel_name);
            let dest_path = destination_root.join(&rel_full_path);

            if metadata.is_dir() {
                items.push(FileItem {
                    id: Uuid::new_v4(),
                    source_path: entry.path(),
                    destination_path: dest_path,
                    file_size: 0,
                    state: crate::model::FileState::Pending,
                    bytes_copied: 0,
                    error_code: None,
                    error_message: None,
                    is_dir: true,
                    last_modified: None,
                });

                recurse(entry.path().as_path(), &rel_full_path, destination_root, items)?;
            } else {
                items.push(FileItem {
                    id: Uuid::new_v4(),
                    source_path: entry.path(),
                    destination_path: dest_path,
                    file_size: metadata.len(),
                    state: crate::model::FileState::Pending,
                    bytes_copied: 0,
                    error_code: None,
                    error_message: None,
                    is_dir: false,
                    last_modified: None,
                });
            }
        }

        Ok(())
    }

    recurse(source, Path::new(""), destination_root, &mut items)?;
    Ok(items)
}
```

---

## First Command to Run

```bash
cd BackUP
cargo build
```

If it succeeds (even with warnings), you're ready to implement Phase 1.

---

## Testing Commands

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run a specific test
cargo test test_name

# Run tests in release mode (faster)
cargo test --release

# Run integration tests only
cargo test --test '*'

# Check for clippy warnings (before submitting)
cargo clippy -- -D warnings

# Format code
cargo fmt
```

---

## Common Patterns

### Pattern 1: Error Handling

```rust
// Don't do this:
match some_io_operation() {
    Ok(x) => x,
    Err(e) => panic!("{}", e),  // WRONG: kills the job
}

// Do this:
match some_io_operation() {
    Ok(x) => x,
    Err(e) => {
        file.error_code = Some(e.raw_os_error().unwrap_or(0));
        file.error_message = Some(e.to_string());
        file.state = FileState::Failed;
        return;  // Continue to next file
    }
}
```

### Pattern 2: Progress Callback

```rust
// In run_job():
if let Some(callback) = progress_callback {
    callback.on_file_started(job, file_index, file);
    // ... do work, updating bytes_copied ...
    callback.on_file_progress(job, file_index, bytes_copied);
    callback.on_file_completed(job, file_index, file);
}
```

### Pattern 3: State Transitions

```rust
// Always follow: Pending -> Running -> Completed
job.state = JobState::Pending;  // create_job()
// ...
job.state = JobState::Running;  // run_job() starts
job.start_time = Some(SystemTime::now());
// ... process files ...
job.state = JobState::Completed;
job.end_time = Some(SystemTime::now());
```

---

## Debugging Tips

### Test Enumeration First
Before implementing copy, make sure enumeration works:

```rust
#[test]
fn test_enumerate() {
    // Create a test directory
    // Call enumerate_tree()
    // Verify FileItem count and sizes
}
```

### Check File Counts Often
```rust
println!("Enumerated: {} files, {} bytes", 
         job.files.len(), 
         job.total_bytes_to_copy);
```

### Log State Transitions
```rust
println!("Job state: {:?} -> {:?}", old_state, job.state);
println!("File state: {} {:?}", file.source_path.display(), file.state);
```

---

## Dependencies to Add

In `engine/Cargo.toml`:
```toml
[dependencies]
uuid = { version = "1.0", features = ["v4", "serde"] }
chrono = { version = "0.4" }
serde = { version = "1.0", features = ["derive"] }

[dev-dependencies]
tempfile = "3.8"
```

In `cli/Cargo.toml`:
```toml
[dependencies]
engine = { path = "../engine" }
clap = { version = "4.0", features = ["derive"] }
uuid = "1.0"
chrono = "0.4"
```

---

## Timeline

- **Days 1-3**: Phase 1 (Foundation)
- **Days 4-7**: Phase 2 (Core Logic)
- **Days 8-10**: Phase 3 (Progress & Orchestration)
- **Days 11-12**: Phase 4 (CLI)
- **Days 13-15**: Phase 5 (Testing)
- **Days 16-17**: Phase 6 (Polish)

**Total: ~3 weeks** (adjustable based on team size and experience).

---

## Questions During Implementation

Refer to:
- **DESIGN.md** for "why" (design rationale)
- **TECHNICAL_SPEC.md** for "how" (pseudocode, algorithms)
- **IMPLEMENTATION_SUMMARY.md** for "what" (roadmap, tasks)

---

## Success Criteria for Each Phase

### Phase 1
- [ ] Cargo workspace builds
- [ ] All types compile
- [ ] `cargo test` passes (no tests yet)

### Phase 2
- [ ] Enumeration works (unit test)
- [ ] Copy works (unit test)
- [ ] Mkdir works (unit test)
- [ ] Overwrite policy logic works (unit tests)

### Phase 3
- [ ] Jobs execute without panicking
- [ ] Callbacks are invoked in correct order
- [ ] Progress tracking works

### Phase 4
- [ ] CLI parses arguments correctly
- [ ] CLI runs jobs and prints progress
- [ ] Exit codes are correct (0, 1, 2)

### Phase 5
- [ ] 80%+ code coverage
- [ ] All error scenarios tested
- [ ] Manual test on real filesystem

### Phase 6
- [ ] No clippy warnings
- [ ] All doc comments present
- [ ] Binary is clean and small

---

## Let's Go!

You have a complete design, clear phases, implementation skeletons, and a roadmap. Start with Phase 1:

```bash
cd BackUP
cargo init --lib engine/
cargo init --bin --name cli cli/
cargo build
```

Then implement the data model (model.rs, error.rs). Good luck! 🚀
