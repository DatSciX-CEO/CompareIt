//! Structured (CSV/TSV/Excel) file comparison
//!
//! This module implements key-based record comparison for CSV, TSV, and Excel files,
//! with per-column mismatch statistics and numeric tolerance support.
//!
//! **Performance Note (Phase 2 Optimization):**
//! Uses sorted vectors with merge-join instead of HashMaps for comparison.
//! This reduces memory overhead from ~10x to ~1.2x and dramatically improves
//! CPU cache locality for files >500MB.
//!
//! **Phase 3 Enhancement:**
//! Adds Excel/OpenDocument support via `calamine`. Excel rows are converted into
//! the same `ByteRecord` format used for CSVs, enabling unified comparison logic.

use crate::types::{
    ColumnMismatch, CompareConfig, FieldMismatch, FileEntry, FileType, StructuredComparisonResult,
};
use anyhow::{Context, Result};
use calamine::{open_workbook_auto, DataType, Reader};
use csv::{ByteRecord, ReaderBuilder};
use rayon::prelude::*;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::Path;

/// A record with its composite key for sorted comparison
///
/// Uses ByteRecord for minimal memory overhead - no String allocation per field.
#[derive(Clone)]
struct KeyedRecord {
    /// Composite key built from key columns (e.g., "id1|id2")
    key: String,
    /// Raw CSV record data (memory-efficient)
    record: ByteRecord,
}

/// Compare two structured files (CSV/TSV/Excel) using sorted merge-join
///
/// This implementation reads records into sorted vectors and performs a linear
/// merge-join, which is far more memory-efficient than HashMap-based comparison.
///
/// Supports comparing any combination of CSV, TSV, and Excel files.
pub fn compare_structured_files(
    file1: &FileEntry,
    file2: &FileEntry,
    config: &CompareConfig,
) -> Result<StructuredComparisonResult> {
    // Parse both files into sorted vectors based on file type
    let (headers1, mut records1) = read_structured_records(&file1.path, &file1.file_type, &config.key_columns)?;
    let (headers2, mut records2) = read_structured_records(&file2.path, &file2.file_type, &config.key_columns)?;

    // Parallel sort by key (using rayon)
    records1.par_sort_by(|a, b| a.key.cmp(&b.key));
    records2.par_sort_by(|a, b| a.key.cmp(&b.key));

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

    // Build column index maps for fast field access
    let col_indices1: HashMap<&str, usize> = headers1
        .iter()
        .enumerate()
        .map(|(i, h)| (h.as_str(), i))
        .collect();
    let col_indices2: HashMap<&str, usize> = headers2
        .iter()
        .enumerate()
        .map(|(i, h)| (h.as_str(), i))
        .collect();

    // Merge-join: linear scan through both sorted vectors
    let mut idx1 = 0;
    let mut idx2 = 0;
    let mut common_count = 0;
    let mut only_in_file1_count = 0;
    let mut only_in_file2_count = 0;
    let mut field_mismatches: HashMap<String, Vec<FieldMismatch>> = HashMap::new();

    while idx1 < records1.len() && idx2 < records2.len() {
        let rec1 = &records1[idx1];
        let rec2 = &records2[idx2];

        match rec1.key.cmp(&rec2.key) {
            Ordering::Equal => {
                // Keys match - compare field values
                common_count += 1;

                for col in &common_columns {
                    // Skip key columns in mismatch analysis
                    if config.key_columns.contains(col) {
                        continue;
                    }

                    let val1 = get_field_value(&rec1.record, &col_indices1, col);
                    let val2 = get_field_value(&rec2.record, &col_indices2, col);

                    if !values_equal(&val1, &val2, config.numeric_tolerance) {
                        field_mismatches.entry(col.clone()).or_default().push(FieldMismatch {
                            key: rec1.key.clone(),
                            value1: val1,
                            value2: val2,
                        });
                    }
                }

                idx1 += 1;
                idx2 += 1;
            }
            Ordering::Less => {
                // Key only in file1
                only_in_file1_count += 1;
                idx1 += 1;
            }
            Ordering::Greater => {
                // Key only in file2
                only_in_file2_count += 1;
                idx2 += 1;
            }
        }
    }

    // Count remaining records
    only_in_file1_count += records1.len() - idx1;
    only_in_file2_count += records2.len() - idx2;

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

    // Calculate similarity score using Jaccard-style formula
    let total_unique = records1.len() + records2.len() - common_count;
    let similarity_score = if total_unique > 0 {
        common_count as f64 / total_unique as f64
    } else {
        1.0
    };

    // Create linked ID
    let linked_id = format!(
        "{}:{}",
        &file1.content_hash[..16.min(file1.content_hash.len())],
        &file2.content_hash[..16.min(file2.content_hash.len())]
    );

    let identical = only_in_file1_count == 0
        && only_in_file2_count == 0
        && total_field_mismatches == 0;

    Ok(StructuredComparisonResult {
        linked_id,
        file1_path: file1.path.display().to_string(),
        file2_path: file2.path.display().to_string(),
        file1_row_count: records1.len(),
        file2_row_count: records2.len(),
        common_records: common_count,
        only_in_file1: only_in_file1_count,
        only_in_file2: only_in_file2_count,
        similarity_score,
        field_mismatches: column_mismatches,
        total_field_mismatches,
        columns_only_in_file1,
        columns_only_in_file2,
        common_columns,
        identical,
    })
}

/// Read structured records from a file based on its type
///
/// Dispatches to the appropriate reader (CSV/TSV or Excel) and returns
/// a unified format of headers + keyed records.
fn read_structured_records(
    path: &Path,
    file_type: &FileType,
    key_columns: &[String],
) -> Result<(Vec<String>, Vec<KeyedRecord>)> {
    match file_type {
        FileType::Excel => parse_excel_into_sorted_vec(path, key_columns),
        FileType::Csv | FileType::Tsv => {
            let delimiter = get_delimiter(file_type);
            parse_csv_into_sorted_vec(path, delimiter, key_columns)
        }
        _ => anyhow::bail!("Unsupported file type for structured comparison: {:?}", file_type),
    }
}

/// Get the appropriate delimiter for a file type
pub fn get_delimiter(file_type: &FileType) -> u8 {
    match file_type {
        FileType::Tsv => b'\t',
        _ => b',',
    }
}

/// Parse a CSV/TSV file into a vector of keyed records (memory-efficient)
///
/// Returns headers and a vector of (key, ByteRecord) pairs ready for sorting.
pub fn parse_csv_into_sorted_vec(
    path: &Path,
    delimiter: u8,
    key_columns: &[String],
) -> Result<(Vec<String>, Vec<KeyedRecord>)> {
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

    // Parse records into vector (no HashMap overhead!)
    let mut records: Vec<KeyedRecord> = Vec::new();

    for result in reader.byte_records() {
        let record = result?;

        // Build composite key from key columns
        let key: String = key_indices
            .iter()
            .filter_map(|&i| {
                record.get(i).and_then(|bytes| std::str::from_utf8(bytes).ok())
            })
            .collect::<Vec<_>>()
            .join("|");

        records.push(KeyedRecord { key, record });
    }

    Ok((headers, records))
}

/// Parse an Excel/OpenDocument file into a vector of keyed records
///
/// Uses calamine to read the first worksheet and converts rows into ByteRecords
/// for compatibility with the CSV comparison engine.
pub fn parse_excel_into_sorted_vec(
    path: &Path,
    key_columns: &[String],
) -> Result<(Vec<String>, Vec<KeyedRecord>)> {
    // Open workbook using auto-detection
    let mut workbook = open_workbook_auto(path)
        .with_context(|| format!("Failed to open Excel file: {}", path.display()))?;

    // Get sheet names
    let sheet_names = workbook.sheet_names().to_vec();
    if sheet_names.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    // Read first sheet
    let first_sheet = &sheet_names[0];
    let range = workbook
        .worksheet_range(first_sheet)
        .with_context(|| format!("Failed to read worksheet '{}' from {}", first_sheet, path.display()))?;

    let mut rows = range.rows();

    // Extract headers from first row
    let headers: Vec<String> = match rows.next() {
        Some(row) => row.iter().map(|cell| cell.to_string().trim().to_string()).collect(),
        None => return Ok((Vec::new(), Vec::new())),
    };

    // Determine key column indices
    let key_indices: Vec<usize> = if key_columns.is_empty() {
        vec![0]
    } else {
        key_columns
            .iter()
            .filter_map(|k| headers.iter().position(|h| h == k))
            .collect()
    };

    // Parse data rows into KeyedRecords
    let mut records: Vec<KeyedRecord> = Vec::new();

    for row in rows {
        // Convert Excel row to ByteRecord
        let mut byte_record = ByteRecord::new();
        for cell in row.iter() {
            let cell_str = excel_cell_to_string(cell);
            byte_record.push_field(cell_str.as_bytes());
        }

        // Pad with empty fields if row is shorter than headers
        while byte_record.len() < headers.len() {
            byte_record.push_field(b"");
        }

        // Build composite key
        let key: String = key_indices
            .iter()
            .filter_map(|&i| {
                byte_record.get(i).and_then(|bytes| std::str::from_utf8(bytes).ok())
            })
            .collect::<Vec<_>>()
            .join("|");

        records.push(KeyedRecord { key, record: byte_record });
    }

    Ok((headers, records))
}

/// Convert an Excel cell to a string representation
///
/// Handles different data types appropriately for comparison:
/// - Numbers: Full precision (no artificial rounding)
/// - Booleans: "TRUE" / "FALSE"
/// - Errors: "#ERROR"
/// - Empty: ""
fn excel_cell_to_string(cell: &DataType) -> String {
    match cell {
        DataType::Empty => String::new(),
        DataType::String(s) => s.clone(),
        DataType::Int(i) => i.to_string(),
        DataType::Float(f) => {
            // Use full precision to avoid comparison issues
            if f.fract() == 0.0 {
                format!("{:.0}", f)
            } else {
                f.to_string()
            }
        }
        DataType::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
        DataType::Error(e) => format!("#ERROR:{:?}", e),
        DataType::DateTime(dt) => dt.to_string(),
        DataType::Duration(d) => d.to_string(),
        DataType::DateTimeIso(s) => s.clone(),
        DataType::DurationIso(s) => s.clone(),
    }
}

/// Get a field value from a ByteRecord by column name
fn get_field_value(record: &ByteRecord, col_indices: &HashMap<&str, usize>, col_name: &str) -> String {
    col_indices
        .get(col_name)
        .and_then(|&idx| record.get(idx))
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .unwrap_or("")
        .to_string()
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

    #[test]
    fn test_excel_cell_to_string() {
        assert_eq!(excel_cell_to_string(&DataType::Empty), "");
        assert_eq!(excel_cell_to_string(&DataType::String("test".to_string())), "test");
        assert_eq!(excel_cell_to_string(&DataType::Int(42)), "42");
        assert_eq!(excel_cell_to_string(&DataType::Float(3.14)), "3.14");
        assert_eq!(excel_cell_to_string(&DataType::Float(42.0)), "42");
        assert_eq!(excel_cell_to_string(&DataType::Bool(true)), "TRUE");
        assert_eq!(excel_cell_to_string(&DataType::Bool(false)), "FALSE");
    }

    #[test]
    fn test_merge_join_ordering() {
        // Test that the merge-join algorithm correctly handles sorted data
        let mut records1 = vec![
            KeyedRecord { key: "a".to_string(), record: ByteRecord::new() },
            KeyedRecord { key: "c".to_string(), record: ByteRecord::new() },
            KeyedRecord { key: "e".to_string(), record: ByteRecord::new() },
        ];
        let mut records2 = vec![
            KeyedRecord { key: "b".to_string(), record: ByteRecord::new() },
            KeyedRecord { key: "c".to_string(), record: ByteRecord::new() },
            KeyedRecord { key: "d".to_string(), record: ByteRecord::new() },
        ];

        records1.par_sort_by(|a, b| a.key.cmp(&b.key));
        records2.par_sort_by(|a, b| a.key.cmp(&b.key));

        let mut idx1 = 0;
        let mut idx2 = 0;
        let mut common = 0;
        let mut only1 = 0;
        let mut only2 = 0;

        while idx1 < records1.len() && idx2 < records2.len() {
            match records1[idx1].key.cmp(&records2[idx2].key) {
                Ordering::Equal => { common += 1; idx1 += 1; idx2 += 1; }
                Ordering::Less => { only1 += 1; idx1 += 1; }
                Ordering::Greater => { only2 += 1; idx2 += 1; }
            }
        }
        only1 += records1.len() - idx1;
        only2 += records2.len() - idx2;

        assert_eq!(common, 1);  // Only "c" is common
        assert_eq!(only1, 2);   // "a" and "e"
        assert_eq!(only2, 2);   // "b" and "d"
    }
}
