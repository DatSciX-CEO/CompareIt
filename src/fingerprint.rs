//! Fingerprinting for fast similarity estimation
//!
//! This module computes:
//! - Blake3 content hashes for exact match detection
//! - Simhash fingerprints for text similarity estimation
//! - Schema signatures for structured file matching

use crate::types::{FileEntry, FileType, NormalizationOptions};
use anyhow::Result;
use log::warn;
use rayon::prelude::*;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Read};

/// Compute all fingerprints for a set of file entries
pub fn compute_fingerprints(
    entries: &mut [FileEntry],
    normalization: &NormalizationOptions,
    max_file_size: u64,
) {
    entries.par_iter_mut().for_each(|entry| {
        if let Err(e) = compute_fingerprint_for_entry(entry, normalization, max_file_size) {
            warn!(
                "Failed to fingerprint {}: {}",
                entry.path.display(),
                e
            );
        }
    });
}

/// Compute fingerprints for a single file entry
///
/// Uses streaming Blake3 hash computation to handle files of any size with
/// constant memory usage. Memory-intensive operations (Simhash) are skipped
/// for files exceeding `max_file_size`.
fn compute_fingerprint_for_entry(
    entry: &mut FileEntry,
    normalization: &NormalizationOptions,
    max_file_size: u64,
) -> Result<()> {
    // Get file metadata to check size
    let metadata = fs::metadata(&entry.path)?;
    let file_size = metadata.len();

    // Streaming Blake3 hash computation - works with any file size
    // using constant memory (reads in chunks internally)
    let file = fs::File::open(&entry.path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = blake3::Hasher::new();
    
    // Stream the file through the hasher
    let mut buffer = [0u8; 16384]; // 16KB buffer for efficient streaming
    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    
    entry.content_hash = hasher.finalize().to_hex().to_string();

    // Skip memory-intensive Simhash for large files to prevent OOM
    if file_size > max_file_size {
        if matches!(entry.file_type, FileType::Text | FileType::Csv | FileType::Tsv) {
            warn!(
                "File too large for similarity fingerprinting ({} bytes > {} byte limit), using hash-only: {}",
                file_size, max_file_size, entry.path.display()
            );
        }
        // Still compute schema signature for structured files (it's lightweight)
        if let (FileType::Csv | FileType::Tsv | FileType::Excel, Some(ref columns)) = (&entry.file_type, &entry.columns) {
            entry.schema_signature = Some(compute_schema_signature(columns));
        }
        return Ok(());
    }

    // For files within the size limit, read content for Simhash computation
    let content = fs::read(&entry.path)?;

    // Compute type-specific fingerprints
    match entry.file_type {
        FileType::Text => {
            let text = String::from_utf8_lossy(&content);
            entry.simhash = Some(compute_simhash(&text, normalization));
        }
        FileType::Csv | FileType::Tsv => {
            // Compute schema signature from columns
            if let Some(ref columns) = entry.columns {
                entry.schema_signature = Some(compute_schema_signature(columns));
            }
            // Compute simhash on the text content for similarity matching
            let text = String::from_utf8_lossy(&content);
            entry.simhash = Some(compute_simhash(&text, normalization));
        }
        FileType::Excel => {
            // Excel files are binary ZIP archives - do NOT compute simhash on raw bytes.
            // Only use schema signature for matching Excel files.
            if let Some(ref columns) = entry.columns {
                entry.schema_signature = Some(compute_schema_signature(columns));
            }
            // No simhash for Excel - the raw bytes are not meaningful text
        }
        FileType::Binary | FileType::Unknown => {
            // No simhash for binary files
        }
    }

    Ok(())
}

/// Compute simhash fingerprint for text content
///
/// Simhash is a locality-sensitive hash (LSH) that produces similar hashes
/// for similar content. This enables O(1) similarity estimation using
/// Hamming distance.
///
/// ## Algorithm
///
/// 1. **Normalize**: Apply text normalization (case, whitespace, etc.)
/// 2. **Shingle**: Generate n-gram tokens (both word-level and line-level)
/// 3. **Hash**: Hash each shingle to a 64-bit value
/// 4. **Aggregate**: For each bit position, sum +1 for 1-bits, -1 for 0-bits
/// 5. **Threshold**: Final hash has 1 where sum > 0, 0 otherwise
///
/// ## Properties
///
/// - Similar documents produce hashes with low Hamming distance
/// - Hamming distance of 3-5 typically indicates >90% similarity
/// - Completely different documents have ~32 bit differences (50%)
///
/// ## References
///
/// - Charikar, M. S. (2002). "Similarity estimation techniques from rounding algorithms"
pub fn compute_simhash(text: &str, normalization: &NormalizationOptions) -> u64 {
    let lines = normalize_text(text, normalization);

    // Generate shingles (3-grams of words)
    let shingles = generate_shingles(&lines, 3);

    // Compute simhash from shingles
    let mut v = [0i32; 64];

    for shingle in shingles {
        let hash = hash_string(&shingle);
        for i in 0..64 {
            if (hash >> i) & 1 == 1 {
                v[i] += 1;
            } else {
                v[i] -= 1;
            }
        }
    }

    // Convert to final hash
    let mut result: u64 = 0;
    for i in 0..64 {
        if v[i] > 0 {
            result |= 1 << i;
        }
    }

    result
}

/// Normalize text according to options
fn normalize_text(text: &str, opts: &NormalizationOptions) -> Vec<String> {
    text.lines()
        .map(|line| {
            let mut s = line.to_string();

            // Normalize EOL (already handled by .lines())

            // Ignore trailing whitespace
            if opts.ignore_trailing_ws {
                s = s.trim_end().to_string();
            }

            // Ignore all whitespace
            if opts.ignore_all_ws {
                s = s.split_whitespace().collect::<Vec<_>>().join(" ");
            }

            // Case insensitive
            if opts.ignore_case {
                s = s.to_lowercase();
            }

            s
        })
        .filter(|line| {
            // Skip empty lines if requested
            if opts.skip_empty_lines {
                !line.trim().is_empty()
            } else {
                true
            }
        })
        .collect()
}

/// Generate n-gram shingles from lines
fn generate_shingles(lines: &[String], n: usize) -> Vec<String> {
    let mut shingles = Vec::new();

    // Word-level shingles across all lines
    let words: Vec<&str> = lines.iter().flat_map(|line| line.split_whitespace()).collect();

    if words.len() >= n {
        for window in words.windows(n) {
            shingles.push(window.join(" "));
        }
    } else if !words.is_empty() {
        // If fewer than n words, use what we have
        shingles.push(words.join(" "));
    }

    // Also add line-level shingles for structural similarity
    if lines.len() >= n {
        for window in lines.windows(n) {
            shingles.push(window.join("\n"));
        }
    }

    shingles
}

/// Hash a string to u64 using DefaultHasher
fn hash_string(s: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

/// Compute schema signature from column names
///
/// Creates a deterministic signature from sorted column names
pub fn compute_schema_signature(columns: &[String]) -> String {
    let mut sorted = columns.to_vec();
    sorted.sort();

    let combined = sorted.join("|");
    let hash = blake3::hash(combined.as_bytes());
    hash.to_hex()[..16].to_string()
}

/// Compute Hamming distance between two simhashes
///
/// Returns the number of differing bits (0-64)
pub fn hamming_distance(hash1: u64, hash2: u64) -> u32 {
    (hash1 ^ hash2).count_ones()
}

/// Convert Hamming distance to similarity score (0.0 to 1.0)
pub fn simhash_similarity(hash1: u64, hash2: u64) -> f64 {
    let distance = hamming_distance(hash1, hash2);
    1.0 - (distance as f64 / 64.0)
}

/// Read and normalize file content for comparison
pub fn read_normalized_lines(
    path: &std::path::Path,
    normalization: &NormalizationOptions,
) -> Result<Vec<String>> {
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut lines = Vec::new();

    for line in reader.lines() {
        let mut s = line?;

        // Normalize trailing whitespace
        if normalization.ignore_trailing_ws {
            s = s.trim_end().to_string();
        }

        // Normalize all whitespace
        if normalization.ignore_all_ws {
            s = s.split_whitespace().collect::<Vec<_>>().join(" ");
        }

        // Case insensitive
        if normalization.ignore_case {
            s = s.to_lowercase();
        }

        // Skip empty lines if requested
        if normalization.skip_empty_lines && s.trim().is_empty() {
            continue;
        }

        lines.push(s);
    }

    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hamming_distance() {
        assert_eq!(hamming_distance(0, 0), 0);
        assert_eq!(hamming_distance(0, 1), 1);
        assert_eq!(hamming_distance(0b1111, 0b0000), 4);
    }

    #[test]
    fn test_simhash_similarity() {
        assert_eq!(simhash_similarity(0, 0), 1.0);
        assert!(simhash_similarity(0, u64::MAX) < 0.1);
    }

    #[test]
    fn test_schema_signature() {
        let cols1 = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let cols2 = vec!["c".to_string(), "b".to_string(), "a".to_string()];
        // Same columns, different order should produce same signature
        assert_eq!(
            compute_schema_signature(&cols1),
            compute_schema_signature(&cols2)
        );
    }
}
