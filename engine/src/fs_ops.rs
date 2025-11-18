//! Filesystem operations module.
//!
//! This module provides low-level operations for:
//! - Enumerating directory trees
//! - Copying files with metadata preservation
//! - Creating directories recursively
//!
//! Implemented in Phase 2.

use std::fs;
use std::io;
use std::path::Path;
use uuid::Uuid;
use crate::model::{FileItem, FileState, FileMetadata};
use crate::error::EngineError;

/// Enumerate the source directory tree and return all files and subdirectories.
///
/// # Arguments
/// * `source` - Source directory to enumerate
/// * `destination_root` - Root destination directory (for building relative paths)
///
/// # Returns
/// Vec<FileItem> with all files and directories found
///
/// # Errors
/// Returns EngineError if enumeration fails at the root level.
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
        match fs::read_dir(path) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry.map_err(|e| EngineError::EnumerationFailed {
                        path: path.to_path_buf(),
                        source: e,
                    })?;

                    let metadata = entry.metadata().map_err(|e| {
                        EngineError::EnumerationFailed {
                            path: path.to_path_buf(),
                            source: e,
                        }
                    })?;

                    let file_name = entry.file_name();
                    let rel_name = Path::new(&file_name);
                    let rel_full_path = rel_path.join(rel_name);
                    let dest_path = destination_root.join(&rel_full_path);
                    let entry_path = entry.path();

                    if metadata.is_dir() {
                        items.push(FileItem {
                            id: Uuid::new_v4(),
                            source_path: entry_path.clone(),
                            destination_path: dest_path.clone(),
                            file_size: 0,
                            state: FileState::Pending,
                            bytes_copied: 0,
                            error_code: None,
                            error_message: None,
                            is_dir: true,
                            last_modified: None,
                            metadata: FileMetadata {
                                source_checksum: None,
                                dest_checksum: None,
                                verification_passed: None,
                                attributes: None,
                            },
                        });

                        // Recurse into subdirectory
                        if let Err(e) = recurse(&entry_path, &rel_full_path, destination_root, items) {
                            // Record error on the directory item and continue
                            if let Some(last_item) = items.last_mut() {
                                if last_item.is_dir && last_item.source_path == entry_path {
                                    last_item.state = FileState::Failed;
                                    last_item.error_code = e.raw_os_error();
                                    last_item.error_message = Some(e.to_string());
                                }
                            }
                        }
                    } else {
                        items.push(FileItem {
                            id: Uuid::new_v4(),
                            source_path: entry_path,
                            destination_path: dest_path,
                            file_size: metadata.len(),
                            state: FileState::Pending,
                            bytes_copied: 0,
                            error_code: None,
                            error_message: None,
                            is_dir: false,
                            last_modified: None,
                            metadata: FileMetadata {
                                source_checksum: None,
                                dest_checksum: None,
                                verification_passed: None,
                                attributes: None,
                            },
                        });
                    }
                }
                Ok(())
            }
            Err(e) => Err(EngineError::EnumerationFailed {
                path: path.to_path_buf(),
                source: e,
            }),
        }
    }

    recurse(source, Path::new(""), destination_root, &mut items)?;
    Ok(items)
}

/// Copy a file from source to destination with metadata preservation.
///
/// # Arguments
/// * `src` - Source file path
/// * `dst` - Destination file path
///
/// # Returns
/// Number of bytes copied
///
/// # Errors
/// Returns EngineError if the copy fails
pub fn copy_file_with_metadata(src: &Path, dst: &Path) -> Result<u64, EngineError> {
    // Ensure parent directory exists
    ensure_parent_dir_exists(dst)?;

    // Open source file
    let mut src_file = fs::File::open(src).map_err(|e| EngineError::ReadError {
        path: src.to_path_buf(),
        source: e,
    })?;

    // Get source metadata for modification time
    let src_metadata = src_file.metadata().map_err(|e| EngineError::ReadError {
        path: src.to_path_buf(),
        source: e,
    })?;
    let src_mtime = src_metadata.modified().ok();

    // Create destination file
    let mut dst_file = fs::File::create(dst).map_err(|e| EngineError::WriteError {
        path: dst.to_path_buf(),
        source: e,
    })?;

    // Copy file contents
    let bytes_copied = io::copy(&mut src_file, &mut dst_file).map_err(|e| {
        if e.kind() == io::ErrorKind::PermissionDenied {
            EngineError::WriteError {
                path: dst.to_path_buf(),
                source: e,
            }
        } else {
            EngineError::ReadError {
                path: src.to_path_buf(),
                source: e,
            }
        }
    })?;

    // Preserve modification time if available
    if let Some(mtime) = src_mtime {
        let _ = fs::metadata(dst).and_then(|_| {
            filetime::set_file_mtime(dst, filetime::FileTime::from_system_time(mtime))
        });
    }

    Ok(bytes_copied)
}

/// Ensure the parent directory of a path exists, creating it if necessary.
///
/// # Arguments
/// * `path` - Path for which the parent directory should be created
///
/// # Errors
/// Returns EngineError if directory creation fails
pub fn ensure_parent_dir_exists(path: &Path) -> Result<(), EngineError> {
    if let Some(parent) = path.parent() {
        // Skip if parent is empty path (Windows root or relative root)
        if parent.as_os_str().is_empty() {
            return Ok(());
        }

        match fs::metadata(parent) {
            Ok(metadata) => {
                if metadata.is_dir() {
                    Ok(())
                } else {
                    Err(EngineError::DirectoryCreationFailed {
                        path: parent.to_path_buf(),
                        source: io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "Parent path exists but is not a directory",
                        ),
                    })
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                // Parent doesn't exist; try to create it recursively
                fs::create_dir_all(parent).map_err(|e| EngineError::DirectoryCreationFailed {
                    path: parent.to_path_buf(),
                    source: e,
                })?;
                Ok(())
            }
            Err(e) => Err(EngineError::DirectoryCreationFailed {
                path: parent.to_path_buf(),
                source: e,
            }),
        }
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_enumerate_flat_directory() {
        // Create temporary directory with test files
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let src = temp_dir.path().join("src");
        fs::create_dir(&src).expect("Failed to create src dir");

        // Create test files
        let mut file1 = fs::File::create(src.join("file1.txt")).expect("Failed to create file1");
        file1.write_all(b"test data 1").expect("Failed to write file1");
        drop(file1);

        let mut file2 = fs::File::create(src.join("file2.txt")).expect("Failed to create file2");
        file2.write_all(b"test data 2").expect("Failed to write file2");
        drop(file2);

        let dst = temp_dir.path().join("dst");
        fs::create_dir(&dst).expect("Failed to create dst dir");

        // Enumerate
        let items = enumerate_tree(&src, &dst).expect("Failed to enumerate");

        // Should have 2 files
        let files: Vec<_> = items.iter().filter(|f| !f.is_dir).collect();
        assert_eq!(files.len(), 2, "Expected 2 files, got {}", files.len());
        
        let total_size: u64 = files.iter().map(|f| f.file_size).sum();
        assert_eq!(total_size, 22, "Expected 22 total bytes, got {}", total_size);
    }

    #[test]
    fn test_enumerate_nested_directory() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let src = temp_dir.path().join("src");
        fs::create_dir(&src).expect("Failed to create src dir");

        // Create nested structure
        let subdir = src.join("subdir");
        fs::create_dir(&subdir).expect("Failed to create subdir");

        let mut file1 = fs::File::create(src.join("file1.txt")).expect("Failed to create file1");
        file1.write_all(b"data1").expect("Failed to write file1");

        let mut file2 = fs::File::create(subdir.join("file2.txt")).expect("Failed to create file2");
        file2.write_all(b"data2").expect("Failed to write file2");

        let dst = temp_dir.path().join("dst");
        fs::create_dir(&dst).expect("Failed to create dst dir");

        // Enumerate
        let items = enumerate_tree(&src, &dst).expect("Failed to enumerate");

        // Should have 1 directory and 2 files
        let dirs: Vec<_> = items.iter().filter(|f| f.is_dir).collect();
        let files: Vec<_> = items.iter().filter(|f| !f.is_dir).collect();
        assert_eq!(dirs.len(), 1);
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_copy_file_with_metadata() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let src_file = temp_dir.path().join("source.txt");
        let dst_file = temp_dir.path().join("dest.txt");

        // Create source file
        let mut file = fs::File::create(&src_file).expect("Failed to create source");
        file.write_all(b"test content").expect("Failed to write source");
        drop(file);

        // Copy file
        let bytes = copy_file_with_metadata(&src_file, &dst_file).expect("Failed to copy");
        assert_eq!(bytes, 12);

        // Verify destination exists and has same content
        let content = fs::read_to_string(&dst_file).expect("Failed to read dest");
        assert_eq!(content, "test content");
    }

    #[test]
    fn test_ensure_parent_dir_exists() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let path = temp_dir.path().join("subdir").join("file.txt");

        // Parent doesn't exist
        ensure_parent_dir_exists(&path).expect("Failed to create parent");

        // Parent should now exist
        assert!(path.parent().unwrap().exists());
    }

    #[test]
    fn test_enumerate_nonexistent_source() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let src = temp_dir.path().join("nonexistent");
        let dst = temp_dir.path().join("dst");

        // Should fail for nonexistent source
        let result = enumerate_tree(&src, &dst);
        assert!(result.is_err());
    }
}
