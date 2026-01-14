//! Core data types for CompareIt
//!
//! This module defines all the shared types used across the comparison pipeline.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// File type detected during indexing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileType {
    /// Plain text file (line-based comparison)
    Text,
    /// CSV file (structured comparison)
    Csv,
    /// TSV file (structured comparison)
    Tsv,
    /// Binary file (hash-only comparison)
    Binary,
    /// Unknown/unreadable
    Unknown,
}

impl FileType {
    pub fn is_structured(&self) -> bool {
        matches!(self, FileType::Csv | FileType::Tsv)
    }

    pub fn is_text(&self) -> bool {
        matches!(self, FileType::Text)
    }
}

/// Represents a single indexed file with metadata and fingerprints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// Absolute path to the file
    pub path: PathBuf,
    /// File size in bytes
    pub size: u64,
    /// Detected file type
    pub file_type: FileType,
    /// File extension (lowercase, without dot)
    pub extension: String,
    /// Blake3 hash of file contents (hex string)
    pub content_hash: String,
    /// Simhash fingerprint for similarity matching (text files)
    pub simhash: Option<u64>,
    /// Schema signature for structured files (sorted column names hash)
    pub schema_signature: Option<String>,
    /// Number of lines (for text) or rows (for structured)
    pub line_count: usize,
    /// Column names for structured files
    pub columns: Option<Vec<String>>,
}

/// Comparison mode selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum CompareMode {
    /// Auto-detect based on file type
    #[default]
    Auto,
    /// Force text/line-based comparison
    Text,
    /// Force structured (CSV/TSV) comparison
    Structured,
}

/// Similarity scoring algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum SimilarityAlgorithm {
    /// Diff-based: common / (common + only_in_1 + only_in_2)
    #[default]
    Diff,
    /// Character-level Jaro-Winkler similarity
    CharJaro,
}

/// Pairing strategy for folder comparison
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum PairingStrategy {
    /// Match files with same relative path
    SamePath,
    /// Match files with same filename only
    SameName,
    /// Find best matches using similarity (all-vs-all with pruning)
    #[default]
    AllVsAll,
}

/// Text normalization options
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NormalizationOptions {
    /// Normalize line endings (CRLF -> LF)
    pub ignore_eol: bool,
    /// Ignore trailing whitespace on each line
    pub ignore_trailing_ws: bool,
    /// Ignore all whitespace differences
    pub ignore_all_ws: bool,
    /// Case-insensitive comparison
    pub ignore_case: bool,
    /// Skip empty lines
    pub skip_empty_lines: bool,
}

/// Configuration for the compare operation
#[derive(Debug, Clone)]
pub struct CompareConfig {
    /// Comparison mode
    pub mode: CompareMode,
    /// Pairing strategy for folders
    pub pairing: PairingStrategy,
    /// Top-K candidates per file in all-vs-all mode
    pub top_k: usize,
    /// Maximum number of pairs to compare
    pub max_pairs: Option<usize>,
    /// Key columns for structured comparison
    pub key_columns: Vec<String>,
    /// Numeric tolerance for structured comparison
    pub numeric_tolerance: f64,
    /// Text normalization options
    pub normalization: NormalizationOptions,
    /// Similarity algorithm
    pub similarity_algorithm: SimilarityAlgorithm,
    /// Maximum bytes for detailed diff output
    pub max_diff_bytes: usize,
    /// Output paths for exports
    pub output_jsonl: Option<PathBuf>,
    pub output_csv: Option<PathBuf>,
    pub output_dir: Option<PathBuf>,
    /// Verbose output
    pub verbose: bool,
}

impl Default for CompareConfig {
    fn default() -> Self {
        Self {
            mode: CompareMode::Auto,
            pairing: PairingStrategy::AllVsAll,
            top_k: 3,
            max_pairs: None,
            key_columns: Vec::new(),
            numeric_tolerance: 0.0001,
            normalization: NormalizationOptions::default(),
            similarity_algorithm: SimilarityAlgorithm::Diff,
            max_diff_bytes: 1024 * 1024, // 1MB default
            output_jsonl: None,
            output_csv: None,
            output_dir: None,
            verbose: false,
        }
    }
}

/// Result of comparing two files (text mode)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextComparisonResult {
    /// Stable linked ID (blake3(path1):blake3(path2))
    pub linked_id: String,
    /// Human-readable file paths
    pub file1_path: String,
    pub file2_path: String,
    /// Line counts after normalization
    pub file1_line_count: usize,
    pub file2_line_count: usize,
    /// Diff statistics
    pub common_lines: usize,
    pub only_in_file1: usize,
    pub only_in_file2: usize,
    /// Similarity score (0.0 to 1.0)
    pub similarity_score: f64,
    /// Positions where differences occur (ranged encoding)
    pub different_positions: String,
    /// Unified diff patch (may be truncated)
    pub detailed_diff: String,
    /// Whether the diff was truncated
    pub diff_truncated: bool,
    /// Whether files are identical
    pub identical: bool,
}

/// Per-column mismatch statistics for structured comparison
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnMismatch {
    pub column_name: String,
    pub mismatch_count: usize,
    pub sample_mismatches: Vec<FieldMismatch>,
}

/// A single field mismatch sample
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldMismatch {
    pub key: String,
    pub value1: String,
    pub value2: String,
}

/// Result of comparing two structured files (CSV/TSV mode)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredComparisonResult {
    /// Stable linked ID
    pub linked_id: String,
    /// Human-readable file paths
    pub file1_path: String,
    pub file2_path: String,
    /// Row counts
    pub file1_row_count: usize,
    pub file2_row_count: usize,
    /// Record statistics
    pub common_records: usize,
    pub only_in_file1: usize,
    pub only_in_file2: usize,
    /// Similarity score
    pub similarity_score: f64,
    /// Per-column mismatch details
    pub field_mismatches: Vec<ColumnMismatch>,
    /// Total field-level mismatches
    pub total_field_mismatches: usize,
    /// Column analysis
    pub columns_only_in_file1: Vec<String>,
    pub columns_only_in_file2: Vec<String>,
    pub common_columns: Vec<String>,
    /// Whether files are identical
    pub identical: bool,
}

/// Unified comparison result (either text or structured)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ComparisonResult {
    Text(TextComparisonResult),
    Structured(StructuredComparisonResult),
    /// Hash-only comparison for binary files
    HashOnly {
        linked_id: String,
        file1_path: String,
        file2_path: String,
        file1_size: u64,
        file2_size: u64,
        identical: bool,
    },
    /// Error during comparison
    Error {
        file1_path: String,
        file2_path: String,
        error: String,
    },
}

impl ComparisonResult {
    pub fn linked_id(&self) -> &str {
        match self {
            ComparisonResult::Text(r) => &r.linked_id,
            ComparisonResult::Structured(r) => &r.linked_id,
            ComparisonResult::HashOnly { linked_id, .. } => linked_id,
            ComparisonResult::Error { file1_path, .. } => file1_path,
        }
    }

    pub fn similarity_score(&self) -> f64 {
        match self {
            ComparisonResult::Text(r) => r.similarity_score,
            ComparisonResult::Structured(r) => r.similarity_score,
            ComparisonResult::HashOnly { identical, .. } => {
                if *identical {
                    1.0
                } else {
                    0.0
                }
            }
            ComparisonResult::Error { .. } => 0.0,
        }
    }

    pub fn is_identical(&self) -> bool {
        match self {
            ComparisonResult::Text(r) => r.identical,
            ComparisonResult::Structured(r) => r.identical,
            ComparisonResult::HashOnly { identical, .. } => *identical,
            ComparisonResult::Error { .. } => false,
        }
    }

    pub fn file_paths(&self) -> (&str, &str) {
        match self {
            ComparisonResult::Text(r) => (&r.file1_path, &r.file2_path),
            ComparisonResult::Structured(r) => (&r.file1_path, &r.file2_path),
            ComparisonResult::HashOnly {
                file1_path,
                file2_path,
                ..
            } => (file1_path, file2_path),
            ComparisonResult::Error {
                file1_path,
                file2_path,
                ..
            } => (file1_path, file2_path),
        }
    }
}

/// Candidate pair for comparison (from matching stage)
#[derive(Debug, Clone)]
pub struct CandidatePair {
    pub file1: FileEntry,
    pub file2: FileEntry,
    /// Estimated similarity from fingerprints (0.0 to 1.0)
    pub estimated_similarity: f64,
    /// Whether this is an exact hash match
    pub exact_hash_match: bool,
}

/// Summary statistics for a comparison run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonSummary {
    pub total_files_set1: usize,
    pub total_files_set2: usize,
    pub pairs_compared: usize,
    pub identical_pairs: usize,
    pub different_pairs: usize,
    pub error_pairs: usize,
    pub average_similarity: f64,
    pub min_similarity: f64,
    pub max_similarity: f64,
}
