//! Core data types for CompareIt
//!
//! This module defines all the shared types used across the comparison pipeline.
//! It includes configuration structures, result types, and supporting enums.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// File type detected during indexing
///
/// The file type determines which comparison algorithm is used:
/// - `Text`: Line-by-line diff comparison using the Myers algorithm
/// - `Csv`/`Tsv`: Key-based record comparison with field-level mismatch tracking
/// - `Binary`: Hash-only comparison (identical or different)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileType {
    /// Plain text file - compared line-by-line
    Text,
    /// CSV file - compared by key-based record matching
    Csv,
    /// TSV file (tab-separated) - compared like CSV
    Tsv,
    /// Binary file - only hash comparison is performed
    Binary,
    /// Unknown or unreadable file type
    Unknown,
}

impl FileType {
    /// Returns true if this is a structured file type (CSV or TSV)
    ///
    /// Structured files are compared using key-based record matching
    /// rather than line-by-line diffing.
    pub fn is_structured(&self) -> bool {
        matches!(self, FileType::Csv | FileType::Tsv)
    }
}

/// Represents a single indexed file with metadata and fingerprints
///
/// This struct is populated during the indexing phase and contains all
/// information needed for matching and comparison:
///
/// - **Identity**: Path, size, extension
/// - **Fingerprints**: Content hash (exact match), simhash (similarity estimation)
/// - **Structure**: For CSV/TSV files, schema signature and column names
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// Absolute path to the file
    pub path: PathBuf,

    /// File size in bytes (used for blocking rules in all-vs-all matching)
    pub size: u64,

    /// Detected file type (determines comparison algorithm)
    pub file_type: FileType,

    /// File extension (lowercase, without dot) - e.g., "csv", "txt"
    pub extension: String,

    /// Blake3 hash of file contents (hex string)
    ///
    /// Used for exact match detection. Files with identical hashes
    /// are considered identical without further comparison.
    pub content_hash: String,

    /// Simhash fingerprint for fast similarity estimation
    ///
    /// Simhash is a locality-sensitive hash where similar content produces
    /// similar hashes. The Hamming distance between two simhashes gives
    /// an approximate similarity score in O(1) time.
    pub simhash: Option<u64>,

    /// Schema signature for structured files
    ///
    /// A hash of sorted column names. Files with the same schema signature
    /// have compatible structures for comparison.
    pub schema_signature: Option<String>,

    /// Number of lines (text files) or data rows (structured files)
    pub line_count: usize,

    /// Column names for structured files (CSV/TSV headers)
    pub columns: Option<Vec<String>>,
}

/// Comparison mode selection
///
/// Determines which algorithm is used to compare files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum CompareMode {
    /// Auto-detect based on file extension and content
    ///
    /// Uses structured mode for `.csv`/`.tsv` files, text mode for others.
    #[default]
    Auto,
    /// Force text/line-based comparison (Myers diff algorithm)
    Text,
    /// Force structured (CSV/TSV) key-based comparison
    Structured,
}

/// Similarity scoring algorithm
///
/// Determines how the `similarity_score` metric is calculated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum SimilarityAlgorithm {
    /// Diff-based scoring: `common / (common + only_in_1 + only_in_2)`
    ///
    /// Best for line-by-line comparison where position matters.
    #[default]
    Diff,
    /// Character-level Jaro-Winkler similarity
    ///
    /// Better for short strings or when typo tolerance is needed.
    CharJaro,
}

/// Pairing strategy for folder comparison
///
/// Determines how files from two directories are matched for comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum PairingStrategy {
    /// Match files with identical relative paths
    ///
    /// `folder1/sub/file.txt` matches `folder2/sub/file.txt`
    SamePath,
    /// Match files with the same filename (ignoring directory)
    ///
    /// `folder1/a/file.txt` matches `folder2/b/file.txt`
    SameName,
    /// Find best matches using fingerprint similarity
    ///
    /// Applies blocking rules and top-k selection to find the most
    /// similar files across both directories.
    #[default]
    AllVsAll,
}

/// Text normalization options
///
/// These options are applied before comparison to reduce noise from
/// formatting differences that may not be semantically significant.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NormalizationOptions {
    /// Normalize line endings (CRLF -> LF)
    pub ignore_eol: bool,
    /// Trim trailing whitespace from each line
    pub ignore_trailing_ws: bool,
    /// Collapse all whitespace to single spaces
    pub ignore_all_ws: bool,
    /// Convert to lowercase before comparison
    pub ignore_case: bool,
    /// Remove empty lines from comparison
    pub skip_empty_lines: bool,
}

/// Configuration for the compare operation
///
/// This struct holds all user-specified options that control how
/// comparisons are performed and how results are output.
#[derive(Debug, Clone)]
pub struct CompareConfig {
    /// Comparison mode (auto, text, or structured)
    pub mode: CompareMode,

    /// Pairing strategy for folder comparison
    pub pairing: PairingStrategy,

    /// Top-K candidates per file in all-vs-all mode
    ///
    /// Each file in the first set is compared with its K most similar
    /// files from the second set.
    pub top_k: usize,

    /// Maximum total number of pairs to compare
    ///
    /// Useful for limiting resource usage on large directories.
    pub max_pairs: Option<usize>,

    /// Key columns for structured comparison
    ///
    /// Records are matched by these columns. If empty, the first column is used.
    pub key_columns: Vec<String>,

    /// Numeric tolerance for structured comparison
    ///
    /// Values within this tolerance are considered equal.
    /// Both absolute and relative tolerance are checked.
    pub numeric_tolerance: f64,

    /// Text normalization options (whitespace, case, etc.)
    pub normalization: NormalizationOptions,

    /// Similarity algorithm for scoring
    pub similarity_algorithm: SimilarityAlgorithm,

    /// Maximum bytes for detailed diff output
    ///
    /// Prevents memory issues with very large diffs.
    pub max_diff_bytes: usize,

    /// Output path for JSONL results (one JSON object per line)
    pub output_jsonl: Option<PathBuf>,

    /// Output path for CSV summary
    pub output_csv: Option<PathBuf>,

    /// Output directory for patch files and mismatch artifacts
    pub output_dir: Option<PathBuf>,

    /// Enable verbose output (show all results, diff snippets)
    pub verbose: bool,

    /// Glob patterns for files/folders to exclude from indexing
    ///
    /// Examples: `"*.tmp"`, `"node_modules"`, `".git"`
    pub exclude_patterns: Vec<String>,

    /// Columns to ignore in structured comparison
    ///
    /// Useful for skipping timestamps, auto-generated IDs, etc.
    pub ignore_columns: Vec<String>,

    /// Regex pattern for content to ignore in text comparison
    ///
    /// Matches are replaced with `<IGNORED>` before comparison.
    /// Useful for filtering timestamps, UUIDs, etc.
    pub ignore_regex: Option<String>,
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
            exclude_patterns: Vec::new(),
            ignore_columns: Vec::new(),
            ignore_regex: None,
        }
    }
}

/// Result of comparing two files in text mode
///
/// Contains line-by-line diff statistics and optionally the full diff output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextComparisonResult {
    /// Stable linked ID for cross-referencing artifacts
    ///
    /// Format: `<hash1_prefix>:<hash2_prefix>`
    pub linked_id: String,

    /// Path to the first file
    pub file1_path: String,
    /// Path to the second file
    pub file2_path: String,

    /// Number of lines in file 1 (after normalization)
    pub file1_line_count: usize,
    /// Number of lines in file 2 (after normalization)
    pub file2_line_count: usize,

    /// Lines present in both files at matching positions
    pub common_lines: usize,
    /// Lines only in file 1 (deletions relative to file 2)
    pub only_in_file1: usize,
    /// Lines only in file 2 (additions relative to file 1)
    pub only_in_file2: usize,

    /// Similarity score from 0.0 (completely different) to 1.0 (identical)
    pub similarity_score: f64,

    /// Compact representation of diff positions (e.g., "1-5,8,10-15")
    pub different_positions: String,

    /// Full unified diff output (may be truncated for large files)
    pub detailed_diff: String,
    /// True if the diff was truncated due to size limits
    pub diff_truncated: bool,

    /// True if files are byte-for-byte identical
    pub identical: bool,
}

/// Per-column mismatch statistics for structured comparison
///
/// Aggregates all mismatches for a single column, with sample values
/// for inspection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnMismatch {
    /// Name of the column with mismatches
    pub column_name: String,
    /// Total number of records with different values in this column
    pub mismatch_count: usize,
    /// Sample mismatches (up to 5) for inspection
    pub sample_mismatches: Vec<FieldMismatch>,
}

/// A single field-level mismatch sample
///
/// Shows the key that identifies the record and the differing values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldMismatch {
    /// The key value(s) identifying this record
    pub key: String,
    /// Value from file 1
    pub value1: String,
    /// Value from file 2
    pub value2: String,
}

/// Result of comparing two structured files (CSV/TSV mode)
///
/// Provides record-level and field-level comparison statistics,
/// including per-column mismatch details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredComparisonResult {
    /// Stable linked ID for cross-referencing artifacts
    pub linked_id: String,

    /// Path to the first file
    pub file1_path: String,
    /// Path to the second file
    pub file2_path: String,

    /// Number of data rows in file 1 (excluding header)
    pub file1_row_count: usize,
    /// Number of data rows in file 2 (excluding header)
    pub file2_row_count: usize,

    /// Records present in both files (matched by key columns)
    pub common_records: usize,
    /// Records only in file 1
    pub only_in_file1: usize,
    /// Records only in file 2
    pub only_in_file2: usize,

    /// Similarity score based on record overlap
    pub similarity_score: f64,

    /// Per-column mismatch details for common records
    pub field_mismatches: Vec<ColumnMismatch>,
    /// Total number of field-level mismatches across all columns
    pub total_field_mismatches: usize,

    /// Columns present only in file 1
    pub columns_only_in_file1: Vec<String>,
    /// Columns present only in file 2
    pub columns_only_in_file2: Vec<String>,
    /// Columns present in both files
    pub common_columns: Vec<String>,

    /// True if files are structurally identical (same records, same values)
    pub identical: bool,
}

/// Unified comparison result enum
///
/// The `type` field in serialized JSON indicates the variant:
/// - `"Text"`: Line-by-line diff result
/// - `"Structured"`: Key-based CSV/TSV result
/// - `"HashOnly"`: Binary file hash comparison
/// - `"Error"`: Comparison failed
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ComparisonResult {
    /// Text/line-based comparison result
    Text(TextComparisonResult),
    /// Structured CSV/TSV comparison result
    Structured(StructuredComparisonResult),
    /// Hash-only comparison for binary files
    ///
    /// Binary files cannot be meaningfully diffed, so only
    /// hash equality is checked.
    HashOnly {
        linked_id: String,
        file1_path: String,
        file2_path: String,
        file1_size: u64,
        file2_size: u64,
        identical: bool,
    },
    /// Comparison failed with an error
    ///
    /// This can happen if files are unreadable, have encoding issues,
    /// or other I/O errors occur.
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
///
/// Generated during the candidate selection phase before actual comparison.
/// Includes estimated similarity based on fingerprints to allow prioritization.
#[derive(Debug, Clone)]
pub struct CandidatePair {
    /// First file to compare
    pub file1: FileEntry,
    /// Second file to compare
    pub file2: FileEntry,
    /// Estimated similarity from fingerprints (0.0 to 1.0)
    ///
    /// Based on simhash Hamming distance or schema matching.
    /// Used to prioritize which pairs to compare first.
    pub estimated_similarity: f64,
    /// True if content hashes match exactly
    ///
    /// If true, files are identical and no detailed comparison is needed.
    pub exact_hash_match: bool,
}

/// Summary statistics for a comparison run
///
/// Provides aggregate metrics for reporting and dashboards.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonSummary {
    /// Total files indexed in the first path
    pub total_files_set1: usize,
    /// Total files indexed in the second path
    pub total_files_set2: usize,
    /// Number of file pairs actually compared
    pub pairs_compared: usize,
    /// Pairs where files are identical
    pub identical_pairs: usize,
    /// Pairs where files differ
    pub different_pairs: usize,
    /// Pairs where comparison failed
    pub error_pairs: usize,
    /// Average similarity score across all successful comparisons
    pub average_similarity: f64,
    /// Minimum similarity score (most different pair)
    pub min_similarity: f64,
    /// Maximum similarity score (most similar non-identical pair)
    pub max_similarity: f64,
}
