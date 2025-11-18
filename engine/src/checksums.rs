//! Checksum and verification functionality.
//!
//! This module provides:
//! - Multiple checksum algorithms (CRC32, MD5, SHA-256, BLAKE3)
//! - File-level checksum computation
//! - Checksum file generation and verification

use crate::error::EngineError;
use std::fmt;
use std::path::Path;

/// Supported checksum algorithms
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecksumAlgorithm {
    /// CRC32 (fast, 32-bit)
    Crc32,
    /// MD5 (deprecated, but included for compatibility)
    Md5,
    /// SHA-256 (cryptographic, 256-bit)
    Sha256,
    /// BLAKE3 (modern, fast, 256-bit)
    Blake3,
}

impl fmt::Display for ChecksumAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Crc32 => write!(f, "crc32"),
            Self::Md5 => write!(f, "md5"),
            Self::Sha256 => write!(f, "sha256"),
            Self::Blake3 => write!(f, "blake3"),
        }
    }
}

impl ChecksumAlgorithm {
    /// Parse algorithm from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "crc32" => Some(Self::Crc32),
            "md5" => Some(Self::Md5),
            "sha256" => Some(Self::Sha256),
            "blake3" => Some(Self::Blake3),
            _ => None,
        }
    }
}

/// A computed checksum value
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecksumValue {
    algorithm: ChecksumAlgorithm,
    hex: String,
}

impl ChecksumValue {
    /// Create a new checksum value
    pub fn new(algorithm: ChecksumAlgorithm, hex: String) -> Self {
        ChecksumValue { algorithm, hex }
    }

    /// Get the algorithm
    pub fn algorithm(&self) -> ChecksumAlgorithm {
        self.algorithm
    }

    /// Get the hex string representation
    pub fn hex(&self) -> &str {
        &self.hex
    }

    /// Format as "algo:hex"
    pub fn to_string_with_algo(&self) -> String {
        format!("{}:{}", self.algorithm, self.hex)
    }
}

impl fmt::Display for ChecksumValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.hex)
    }
}

/// Trait for computing checksums
pub trait ChecksumHasher {
    /// Update the hasher with new data
    fn update(&mut self, data: &[u8]);

    /// Finalize and return the checksum value
    fn finalize(self) -> ChecksumValue;
}

/// CRC32 hasher
struct Crc32Hasher {
    crc: u32,
}

impl Crc32Hasher {
    fn new() -> Self {
        Crc32Hasher { crc: 0 }
    }
}

impl ChecksumHasher for Crc32Hasher {
    fn update(&mut self, data: &[u8]) {
        for &byte in data {
            let mut crc = self.crc;
            crc ^= byte as u32;
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xedb88320
                } else {
                    crc >> 1
                };
            }
            self.crc = crc;
        }
    }

    fn finalize(self) -> ChecksumValue {
        ChecksumValue::new(
            ChecksumAlgorithm::Crc32,
            format!("{:08x}", self.crc ^ 0xffffffff),
        )
    }
}

/// MD5 hasher (backed by md5 crate)
struct Md5Hasher {
    context: md5::Context,
}

impl Md5Hasher {
    fn new() -> Self {
        Md5Hasher {
            context: md5::Context::new(),
        }
    }
}

impl ChecksumHasher for Md5Hasher {
    fn update(&mut self, data: &[u8]) {
        self.context.consume(data);
    }

    fn finalize(self) -> ChecksumValue {
        let digest = self.context.compute();
        ChecksumValue::new(
            ChecksumAlgorithm::Md5,
            format!("{:x}", digest),
        )
    }
}

/// SHA-256 hasher (backed by sha2 crate)
struct Sha256Hasher {
    hasher: sha2::Sha256,
}

impl Sha256Hasher {
    fn new() -> Self {
        Sha256Hasher {
            hasher: sha2::Sha256::default(),
        }
    }
}

impl ChecksumHasher for Sha256Hasher {
    fn update(&mut self, data: &[u8]) {
        use sha2::Digest;
        self.hasher.update(data);
    }

    fn finalize(self) -> ChecksumValue {
        use sha2::Digest;
        let digest = self.hasher.finalize();
        ChecksumValue::new(
            ChecksumAlgorithm::Sha256,
            format!("{:x}", digest),
        )
    }
}

/// BLAKE3 hasher (backed by blake3 crate)
struct Blake3Hasher {
    hasher: blake3::Hasher,
}

impl Blake3Hasher {
    fn new() -> Self {
        Blake3Hasher {
            hasher: blake3::Hasher::new(),
        }
    }
}

impl ChecksumHasher for Blake3Hasher {
    fn update(&mut self, data: &[u8]) {
        let mut hasher = self.hasher.clone();
        hasher.update(data);
        self.hasher = hasher;
    }

    fn finalize(self) -> ChecksumValue {
        let digest = self.hasher.finalize();
        ChecksumValue::new(
            ChecksumAlgorithm::Blake3,
            digest.to_hex().to_string(),
        )
    }
}

/// Create a new hasher for the given algorithm
pub fn create_hasher(algorithm: ChecksumAlgorithm) -> Box<dyn ChecksumHasher> {
    match algorithm {
        ChecksumAlgorithm::Crc32 => Box::new(Crc32Hasher::new()),
        ChecksumAlgorithm::Md5 => Box::new(Md5Hasher::new()),
        ChecksumAlgorithm::Sha256 => Box::new(Sha256Hasher::new()),
        ChecksumAlgorithm::Blake3 => Box::new(Blake3Hasher::new()),
    }
}

/// Compute checksum for a file
pub fn compute_file_checksum(
    path: &Path,
    algorithm: ChecksumAlgorithm,
) -> Result<ChecksumValue, EngineError> {
    use std::fs::File;
    use std::io::Read;

    let mut file = File::open(path).map_err(|e| EngineError::ReadError {
        path: path.to_path_buf(),
        source: e,
    })?;

    let mut buffer = [0u8; 65536]; // 64 KB buffer

    let result = match algorithm {
        ChecksumAlgorithm::Crc32 => {
            let mut hasher = Crc32Hasher::new();
            loop {
                match file.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => hasher.update(&buffer[..n]),
                    Err(e) => {
                        return Err(EngineError::ReadError {
                            path: path.to_path_buf(),
                            source: e,
                        })
                    }
                }
            }
            hasher.finalize()
        }
        ChecksumAlgorithm::Md5 => {
            let mut hasher = Md5Hasher::new();
            loop {
                match file.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => hasher.update(&buffer[..n]),
                    Err(e) => {
                        return Err(EngineError::ReadError {
                            path: path.to_path_buf(),
                            source: e,
                        })
                    }
                }
            }
            hasher.finalize()
        }
        ChecksumAlgorithm::Sha256 => {
            let mut hasher = Sha256Hasher::new();
            loop {
                match file.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => hasher.update(&buffer[..n]),
                    Err(e) => {
                        return Err(EngineError::ReadError {
                            path: path.to_path_buf(),
                            source: e,
                        })
                    }
                }
            }
            hasher.finalize()
        }
        ChecksumAlgorithm::Blake3 => {
            let mut hasher = Blake3Hasher::new();
            loop {
                match file.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => hasher.update(&buffer[..n]),
                    Err(e) => {
                        return Err(EngineError::ReadError {
                            path: path.to_path_buf(),
                            source: e,
                        })
                    }
                }
            }
            hasher.finalize()
        }
    };

    Ok(result)
}

/// Generate a checksum file for multiple files
///
/// Format: "<hex_checksum> <relative_path>" per line
pub fn generate_checksum_file(
    file_checksums: &[(String, ChecksumValue)], // (relative_path, checksum)
    algorithm: ChecksumAlgorithm,
) -> String {
    let mut result = String::new();

    // Add header comment
    result.push_str(&format!("; Checksum file generated by BackUP\n"));
    result.push_str(&format!("; Algorithm: {}\n", algorithm));
    result.push_str("\n");

    for (rel_path, checksum) in file_checksums {
        result.push_str(&format!("{} {}\n", checksum.hex(), rel_path));
    }

    result
}

/// Parse and verify a checksum file
///
/// Returns list of (relative_path, expected_checksum, actual_checksum, matches)
pub fn verify_checksum_file(
    checksum_content: &str,
    file_get_checksum: impl Fn(&str) -> Result<ChecksumValue, EngineError>,
) -> Result<Vec<(String, ChecksumValue, ChecksumValue, bool)>, EngineError> {
    let mut results = Vec::new();

    for line in checksum_content.lines() {
        // Skip comments and empty lines
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }

        // Parse "hex path" format
        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.len() != 2 {
            continue;
        }

        let expected_hex = parts[0];
        let rel_path = parts[1];

        // Get actual checksum
        let actual = file_get_checksum(rel_path)?;

        // Compare
        let matches = actual.hex() == expected_hex;

        let expected = ChecksumValue::new(actual.algorithm(), expected_hex.to_string());
        results.push((rel_path.to_string(), expected, actual, matches));
    }

    Ok(results)
}

/// Verify a file item's checksums.
///
/// This function computes the checksum for both source and destination files,
/// and updates the FileItem's metadata with the results.
/// 
/// # Arguments
/// * `file` - FileItem to verify (must have source and destination paths)
/// * `algorithm` - Checksum algorithm to use
///
/// # Behavior
/// - Computes source checksum (or uses cached value if already present)
/// - Computes destination checksum
/// - Stores both in file.metadata
/// - Sets verification_passed flag based on match
/// - Returns Ok(true) if checksums match, Ok(false) if they don't
/// - Returns Err if checksums cannot be computed
pub fn verify_file_item(
    file: &mut crate::model::FileItem,
    algorithm: ChecksumAlgorithm,
) -> Result<bool, EngineError> {
    // Don't verify directories
    if file.is_dir {
        file.metadata.verification_passed = Some(true);
        return Ok(true);
    }

    // Compute source checksum if not already present
    let source_checksum = if let Some(ref cs) = file.metadata.source_checksum {
        cs.clone()
    } else {
        let cs = compute_file_checksum(&file.source_path, algorithm)?;
        file.metadata.source_checksum = Some(cs.clone());
        cs
    };

    // Compute destination checksum
    let dest_checksum = compute_file_checksum(&file.destination_path, algorithm)?;
    file.metadata.dest_checksum = Some(dest_checksum.clone());

    // Compare and set verification_passed
    let matches = source_checksum.hex() == dest_checksum.hex();
    file.metadata.verification_passed = Some(matches);

    Ok(matches)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_algorithm_display() {
        assert_eq!(ChecksumAlgorithm::Crc32.to_string(), "crc32");
        assert_eq!(ChecksumAlgorithm::Md5.to_string(), "md5");
        assert_eq!(ChecksumAlgorithm::Sha256.to_string(), "sha256");
        assert_eq!(ChecksumAlgorithm::Blake3.to_string(), "blake3");
    }

    #[test]
    fn test_algorithm_from_str() {
        assert_eq!(ChecksumAlgorithm::from_str("crc32"), Some(ChecksumAlgorithm::Crc32));
        assert_eq!(ChecksumAlgorithm::from_str("md5"), Some(ChecksumAlgorithm::Md5));
        assert_eq!(ChecksumAlgorithm::from_str("sha256"), Some(ChecksumAlgorithm::Sha256));
        assert_eq!(ChecksumAlgorithm::from_str("blake3"), Some(ChecksumAlgorithm::Blake3));
        assert_eq!(ChecksumAlgorithm::from_str("invalid"), None);
    }

    #[test]
    fn test_crc32_hasher() {
        let mut hasher = Crc32Hasher::new();
        hasher.update(b"hello");
        let checksum = hasher.finalize();
        assert_eq!(checksum.algorithm(), ChecksumAlgorithm::Crc32);
        // CRC32 of "hello" should be consistent
        let checksum2 = {
            let mut h = Crc32Hasher::new();
            h.update(b"hello");
            h.finalize()
        };
        assert_eq!(checksum.hex(), checksum2.hex());
    }

    #[test]
    fn test_md5_hasher() {
        let mut hasher = Md5Hasher::new();
        hasher.update(b"hello");
        let checksum = hasher.finalize();
        assert_eq!(checksum.algorithm(), ChecksumAlgorithm::Md5);
        assert_eq!(checksum.hex(), "5d41402abc4b2a76b9719d911017c592");
    }

    #[test]
    fn test_sha256_hasher() {
        let mut hasher = Sha256Hasher::new();
        hasher.update(b"hello");
        let checksum = hasher.finalize();
        assert_eq!(checksum.algorithm(), ChecksumAlgorithm::Sha256);
        assert_eq!(
            checksum.hex(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn test_blake3_hasher() {
        let mut hasher = Blake3Hasher::new();
        hasher.update(b"hello");
        let checksum = hasher.finalize();
        assert_eq!(checksum.algorithm(), ChecksumAlgorithm::Blake3);
        // BLAKE3 is deterministic
        let checksum2 = {
            let mut h = Blake3Hasher::new();
            h.update(b"hello");
            h.finalize()
        };
        assert_eq!(checksum.hex(), checksum2.hex());
    }

    #[test]
    fn test_checksum_value_display() {
        let cs = ChecksumValue::new(ChecksumAlgorithm::Sha256, "abc123".to_string());
        assert_eq!(cs.to_string(), "abc123");
        assert_eq!(cs.to_string_with_algo(), "sha256:abc123");
    }

    #[test]
    fn test_generate_checksum_file() {
        let checksums = vec![
            (
                "file1.txt".to_string(),
                ChecksumValue::new(ChecksumAlgorithm::Sha256, "abc123".to_string()),
            ),
            (
                "file2.txt".to_string(),
                ChecksumValue::new(ChecksumAlgorithm::Sha256, "def456".to_string()),
            ),
        ];

        let content = generate_checksum_file(&checksums, ChecksumAlgorithm::Sha256);
        assert!(content.contains("abc123 file1.txt"));
        assert!(content.contains("def456 file2.txt"));
        assert!(content.contains("Algorithm: sha256"));
    }

    #[test]
    fn test_verify_file_item_matching_checksums() {
        use std::fs::File;
        use std::io::Write;
        use crate::model::{FileItem, FileMetadata};
        use uuid::Uuid;

        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let src_path = temp_dir.path().join("source.txt");
        let dst_path = temp_dir.path().join("dest.txt");

        // Create source and destination with identical content
        let content = b"identical content";
        let mut src_file = File::create(&src_path).expect("Failed to create source file");
        src_file.write_all(content).expect("Failed to write source file");
        drop(src_file);

        let mut dst_file = File::create(&dst_path).expect("Failed to create dest file");
        dst_file.write_all(content).expect("Failed to write dest file");
        drop(dst_file);

        // Create a FileItem
        let mut file = FileItem {
            id: Uuid::new_v4(),
            source_path: src_path,
            destination_path: dst_path,
            file_size: content.len() as u64,
            state: crate::model::FileState::Done,
            bytes_copied: content.len() as u64,
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
        };

        // Verify the file
        let result = verify_file_item(&mut file, ChecksumAlgorithm::Sha256)
            .expect("Verification should succeed");

        // Checksums should match
        assert!(result, "Checksums should match");
        assert!(file.metadata.source_checksum.is_some());
        assert!(file.metadata.dest_checksum.is_some());
        assert_eq!(
            file.metadata.source_checksum.as_ref().unwrap().hex(),
            file.metadata.dest_checksum.as_ref().unwrap().hex()
        );
        assert_eq!(file.metadata.verification_passed, Some(true));
    }

    #[test]
    fn test_verify_file_item_mismatched_checksums() {
        use std::fs::File;
        use std::io::Write;
        use crate::model::{FileItem, FileMetadata};
        use uuid::Uuid;

        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let src_path = temp_dir.path().join("source.txt");
        let dst_path = temp_dir.path().join("dest.txt");

        // Create source with one content and destination with different content
        let src_content = b"source content";
        let dst_content = b"different dest content";

        let mut src_file = File::create(&src_path).expect("Failed to create source file");
        src_file.write_all(src_content).expect("Failed to write source file");
        drop(src_file);

        let mut dst_file = File::create(&dst_path).expect("Failed to create dest file");
        dst_file.write_all(dst_content).expect("Failed to write dest file");
        drop(dst_file);

        // Create a FileItem
        let mut file = FileItem {
            id: Uuid::new_v4(),
            source_path: src_path,
            destination_path: dst_path,
            file_size: src_content.len() as u64,
            state: crate::model::FileState::Done,
            bytes_copied: src_content.len() as u64,
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
        };

        // Verify the file
        let result = verify_file_item(&mut file, ChecksumAlgorithm::Sha256)
            .expect("Verification should succeed even with mismatch");

        // Checksums should NOT match
        assert!(!result, "Checksums should not match");
        assert!(file.metadata.source_checksum.is_some());
        assert!(file.metadata.dest_checksum.is_some());
        assert_ne!(
            file.metadata.source_checksum.as_ref().unwrap().hex(),
            file.metadata.dest_checksum.as_ref().unwrap().hex()
        );
        assert_eq!(file.metadata.verification_passed, Some(false));
    }

    #[test]
    fn test_verify_directory_skips_verification() {
        use crate::model::{FileItem, FileMetadata};
        use uuid::Uuid;

        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let dir_path = temp_dir.path();

        // Create a directory FileItem
        let mut file = FileItem {
            id: Uuid::new_v4(),
            source_path: dir_path.to_path_buf(),
            destination_path: dir_path.to_path_buf(),
            file_size: 0,
            state: crate::model::FileState::Done,
            bytes_copied: 0,
            error_code: None,
            error_message: None,
            is_dir: true,  // This is a directory
            last_modified: None,
            metadata: FileMetadata {
                source_checksum: None,
                dest_checksum: None,
                verification_passed: None,
                attributes: None,
            },
        };

        // Verify the directory (should return true without computing checksums)
        let result = verify_file_item(&mut file, ChecksumAlgorithm::Sha256)
            .expect("Verification should succeed for directories");

        assert!(result, "Directory verification should return true");
        assert!(file.metadata.source_checksum.is_none(), "Should not compute checksum for directory");
        assert!(file.metadata.dest_checksum.is_none(), "Should not compute checksum for directory");
        assert_eq!(file.metadata.verification_passed, Some(true));
    }
}
