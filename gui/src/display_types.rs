use std::fmt;
use engine::{Mode, OverwritePolicy, ChecksumAlgorithm};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    Copy,
    Move,
}

impl fmt::Display for DisplayMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DisplayMode::Copy => write!(f, "Copy"),
            DisplayMode::Move => write!(f, "Move"),
        }
    }
}

impl DisplayMode {
    pub fn from_engine_mode(mode: Mode) -> Self {
        match mode {
            Mode::Copy => DisplayMode::Copy,
            Mode::Move => DisplayMode::Move,
        }
    }

    pub fn to_engine_mode(&self) -> Mode {
        match self {
            DisplayMode::Copy => Mode::Copy,
            DisplayMode::Move => Mode::Move,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayPolicy {
    Skip,
    Overwrite,
    SmartUpdate,
    Ask,
}

impl fmt::Display for DisplayPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DisplayPolicy::Skip => write!(f, "Skip"),
            DisplayPolicy::Overwrite => write!(f, "Overwrite"),
            DisplayPolicy::SmartUpdate => write!(f, "Smart Update"),
            DisplayPolicy::Ask => write!(f, "Ask"),
        }
    }
}

impl DisplayPolicy {
    pub fn from_engine_policy(policy: OverwritePolicy) -> Self {
        match policy {
            OverwritePolicy::Skip => DisplayPolicy::Skip,
            OverwritePolicy::Overwrite => DisplayPolicy::Overwrite,
            OverwritePolicy::SmartUpdate => DisplayPolicy::SmartUpdate,
            OverwritePolicy::Ask => DisplayPolicy::Ask,
        }
    }

    pub fn to_engine_policy(&self) -> OverwritePolicy {
        match self {
            DisplayPolicy::Skip => OverwritePolicy::Skip,
            DisplayPolicy::Overwrite => OverwritePolicy::Overwrite,
            DisplayPolicy::SmartUpdate => OverwritePolicy::SmartUpdate,
            DisplayPolicy::Ask => OverwritePolicy::Ask,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayAlgorithm {
    Crc32,
    Md5,
    Sha256,
    Blake3,
}

impl fmt::Display for DisplayAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DisplayAlgorithm::Crc32 => write!(f, "CRC32"),
            DisplayAlgorithm::Md5 => write!(f, "MD5"),
            DisplayAlgorithm::Sha256 => write!(f, "SHA-256"),
            DisplayAlgorithm::Blake3 => write!(f, "BLAKE3"),
        }
    }
}

impl DisplayAlgorithm {
    pub fn from_engine_algo(algo: ChecksumAlgorithm) -> Self {
        match algo {
            ChecksumAlgorithm::Crc32 => DisplayAlgorithm::Crc32,
            ChecksumAlgorithm::Md5 => DisplayAlgorithm::Md5,
            ChecksumAlgorithm::Sha256 => DisplayAlgorithm::Sha256,
            ChecksumAlgorithm::Blake3 => DisplayAlgorithm::Blake3,
        }
    }

    pub fn to_engine_algo(&self) -> ChecksumAlgorithm {
        match self {
            DisplayAlgorithm::Crc32 => ChecksumAlgorithm::Crc32,
            DisplayAlgorithm::Md5 => ChecksumAlgorithm::Md5,
            DisplayAlgorithm::Sha256 => ChecksumAlgorithm::Sha256,
            DisplayAlgorithm::Blake3 => ChecksumAlgorithm::Blake3,
        }
    }
}
