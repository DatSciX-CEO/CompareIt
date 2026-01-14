//! Structured (CSV/TSV) file comparison
//!
//! This module implements key-based record comparison for CSV and TSV files,
//! with per-column mismatch statistics and numeric tolerance support.

use crate::types::{
    ColumnMismatch, CompareConfig, FieldMismatch, FileEntry, FileType, StructuredComparisonResult,
};
use anyhow::{Context, Result};
use csv::ReaderBuilder;
use std::collections::{HashMap, HashSet};
use std::fs::File;

/// Compare two structured files (CSV/TSV)
pub fn compare_structured_files(
    file1: &FileEntry,
    file2: &FileEntry,
    config: &CompareConfig,
) -> Result<StructuredComparisonResult> {
    // Determine delimiter
    let delimiter1 = get_delimiter(&file1.file_type);
    let delimiter2 = get_delimiter(&file2.file_type);

    // Parse both files
    let (headers1, records1) = parse_structured_file(&file1.path, delimiter1, &config.key_columns)?;
    let (headers2, records2) = parse_structured_file(&file2.path, delimiter2, &config.key_columns)?;

    // Filter out ignored columns
    let ignored_cols: HashSet<&str> = config.ignore_columns.iter().map(|s| s.as_str()).collect();

    // Analyze columns (excluding ignored ones)
    let columns1: HashSet<&str> = headers1
        .iter()
        .map(|s| s.as_str())
        .filter(|s| !ignored_cols.contains(s))
        .collect();
    let columns2: HashSet<&str> = headers2
        .iter()
        .map(|s| s.as_str())
        .filter(|s| !ignored_cols.contains(s))
        .collect();

    let common_columns: Vec<String> = columns1
        .intersection(&columns2)
        .map(|s| s.to_string())
        .collect();
    let columns_only_in_file1: Vec<String> = columns1
        .difference(&columns2)
        .map(|s| s.to_string())
        .collect();
    let columns_only_in_file2: Vec<String> = columns2
        .difference(&columns1)
        .map(|s| s.to_string())
        .collect();

    // Build key sets
    let keys1: HashSet<&str> = records1.keys().map(|k| k.as_str()).collect();
    let keys2: HashSet<&str> = records2.keys().map(|k| k.as_str()).collect();

    let common_keys: HashSet<&str> = keys1.intersection(&keys2).copied().collect();
    let only_in_file1_keys: Vec<&str> = keys1.difference(&keys2).copied().collect();
    let only_in_file2_keys: Vec<&str> = keys2.difference(&keys1).copied().collect();

    // Compare field values for common keys
    let mut field_mismatches: HashMap<String, Vec<FieldMismatch>> = HashMap::new();

    for key in &common_keys {
        // Safety: key is guaranteed to exist in both maps since it comes from the intersection
        let Some(row1) = records1.get(*key) else {
            continue;
        };
        let Some(row2) = records2.get(*key) else {
            continue;
        };

        for col in &common_columns {
            // Skip key columns in mismatch analysis
            if config.key_columns.contains(col) {
                continue;
            }

            let val1 = row1.get(col).map(|s| s.as_str()).unwrap_or("");
            let val2 = row2.get(col).map(|s| s.as_str()).unwrap_or("");

            if !values_equal(val1, val2, config.numeric_tolerance) {
                field_mismatches.entry(col.clone()).or_default().push(FieldMismatch {
                    key: key.to_string(),
                    value1: val1.to_string(),
                    value2: val2.to_string(),
                });
            }
        }
    }

    // Build column mismatch summary
    let column_mismatches: Vec<ColumnMismatch> = common_columns
        .iter()
        .filter(|col| !config.key_columns.contains(*col))
        .filter_map(|col| {
            let mismatches = field_mismatches.get(col);
            if let Some(m) = mismatches {
                if !m.is_empty() {
                    return Some(ColumnMismatch {
                        column_name: col.clone(),
                        mismatch_count: m.len(),
                        sample_mismatches: m.iter().take(5).cloned().collect(),
                    });
                }
            }
            None
        })
        .collect();

    let total_field_mismatches: usize = column_mismatches.iter().map(|c| c.mismatch_count).sum();

    // Calculate similarity score
    let total_records = keys1.len() + keys2.len() - common_keys.len();
    let similarity_score = if total_records > 0 {
        common_keys.len() as f64 / total_records as f64
    } else {
        1.0
    };

    // Create linked ID
    let linked_id = format!(
        "{}:{}",
        &file1.content_hash[..16.min(file1.content_hash.len())],
        &file2.content_hash[..16.min(file2.content_hash.len())]
    );

    let identical = only_in_file1_keys.is_empty()
        && only_in_file2_keys.is_empty()
        && total_field_mismatches == 0;

    Ok(StructuredComparisonResult {
        linked_id,
        file1_path: file1.path.display().to_string(),
        file2_path: file2.path.display().to_string(),
        file1_row_count: records1.len(),
        file2_row_count: records2.len(),
        common_records: common_keys.len(),
        only_in_file1: only_in_file1_keys.len(),
        only_in_file2: only_in_file2_keys.len(),
        similarity_score,
        field_mismatches: column_mismatches,
        total_field_mismatches,
        columns_only_in_file1,
        columns_only_in_file2,
        common_columns,
        identical,
    })
}

/// Get the appropriate delimiter for a file type
fn get_delimiter(file_type: &FileType) -> u8 {
    match file_type {
        FileType::Tsv => b'\t',
        _ => b',',
    }
}

/// Parse a structured file and return headers + records keyed by composite key
fn parse_structured_file(
    path: &std::path::Path,
    delimiter: u8,
    key_columns: &[String],
) -> Result<(Vec<String>, HashMap<String, HashMap<String, String>>)> {
    let file = File::open(path).with_context(|| format!("Failed to open {}", path.display()))?;

    let mut reader = ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .flexible(true)
        .from_reader(file);

    // Get headers
    let headers: Vec<String> = reader
        .headers()
        .with_context(|| format!("Failed to read headers from {}", path.display()))?
        .iter()
        .map(|s| s.to_string())
        .collect();

    // Determine key column indices
    let key_indices: Vec<usize> = if key_columns.is_empty() {
        // Use first column as key by default
        vec![0]
    } else {
        key_columns
            .iter()
            .filter_map(|k| headers.iter().position(|h| h == k))
            .collect()
    };

    // Parse records
    let mut records: HashMap<String, HashMap<String, String>> = HashMap::new();

    for result in reader.records() {
        let record = result?;

        // Build composite key
        let key: String = key_indices
            .iter()
            .filter_map(|&i| record.get(i))
            .collect::<Vec<_>>()
            .join("|");

        // Build row map
        let row: HashMap<String, String> = headers
            .iter()
            .enumerate()
            .filter_map(|(i, h)| record.get(i).map(|v| (h.clone(), v.to_string())))
            .collect();

        records.insert(key, row);
    }

    Ok((headers, records))
}

/// Check if two string values are equal, with numeric tolerance support
fn values_equal(val1: &str, val2: &str, tolerance: f64) -> bool {
    // Direct string comparison first
    if val1 == val2 {
        return true;
    }

    // Try numeric comparison with tolerance
    if let (Ok(n1), Ok(n2)) = (val1.parse::<f64>(), val2.parse::<f64>()) {
        let diff = (n1 - n2).abs();
        let max_val = n1.abs().max(n2.abs());

        // Absolute tolerance for small numbers
        if diff <= tolerance {
            return true;
        }

        // Relative tolerance for larger numbers
        if max_val > 0.0 && diff / max_val <= tolerance {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_values_equal() {
        assert!(values_equal("hello", "hello", 0.0001));
        assert!(!values_equal("hello", "world", 0.0001));
        assert!(values_equal("1.0", "1.0", 0.0001));
        assert!(values_equal("1.0000", "1.0001", 0.001));
        assert!(!values_equal("1.0", "2.0", 0.0001));
    }
}
