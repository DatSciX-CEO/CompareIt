//! File indexing and type detection
//!
//! This module handles recursive directory scanning, file type detection,
//! and initial metadata collection.
//!
//! **Phase 3 Enhancement:** Now supports Excel/OpenDocument spreadsheet detection
//! using the `calamine` crate.

use crate::types::{FileEntry, FileType};
use anyhow::{Context, Result};
use calamine::{open_workbook_auto, Reader};
use globset::{Glob, GlobSetBuilder};
use log::warn;
use rayon::prelude::*;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Excel/OpenDocument spreadsheet extensions supported by calamine
const EXCEL_EXTENSIONS: &[&str] = &["xlsx", "xls", "xlsm", "xlsb", "xla", "xlam", "ods"];

/// Index files from a path (file or directory)
///
/// # Arguments
/// * `path` - The file or directory path to index
/// * `exclude_patterns` - Glob patterns for paths to exclude (e.g., `["*.tmp", "node_modules/"]`)
pub fn index_path(path: &Path, exclude_patterns: &[String]) -> Result<Vec<FileEntry>> {
    if path.is_file() {
        let entry = index_single_file(path)?;
        Ok(vec![entry])
    } else if path.is_dir() {
        index_directory(path, exclude_patterns)
    } else {
        anyhow::bail!("Path does not exist or is not accessible: {}", path.display());
    }
}

/// Index all files in a directory recursively
///
/// Applies exclusion patterns to filter out unwanted files and directories.
pub fn index_directory(dir: &Path, exclude_patterns: &[String]) -> Result<Vec<FileEntry>> {
    // Build glob set for exclusion patterns
    let exclude_set = build_exclude_set(exclude_patterns)?;

    let paths: Vec<PathBuf> = WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            // Check if this entry should be excluded
            let path = e.path();
            if let Some(ref glob_set) = exclude_set {
                // Check both the full path and relative path
                if glob_set.is_match(path) {
                    return false;
                }
                // Also check with just the filename for simple patterns like "*.tmp"
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if glob_set.is_match(name) {
                        return false;
                    }
                }
            }
            true
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .collect();

    // Process files in parallel
    let mut entries: Vec<FileEntry> = paths
        .par_iter()
        .filter_map(|p| index_single_file(p).ok())
        .collect();

    // Deterministic ordering by path
    entries.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(entries)
}

/// Build a glob set from exclusion patterns
fn build_exclude_set(patterns: &[String]) -> Result<Option<globset::GlobSet>> {
    if patterns.is_empty() {
        return Ok(None);
    }

    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        match Glob::new(pattern) {
            Ok(glob) => {
                builder.add(glob);
            }
            Err(e) => {
                warn!("Invalid exclude pattern '{}': {}", pattern, e);
            }
        }
    }

    let set = builder.build().context("Failed to build exclude glob set")?;
    Ok(Some(set))
}

/// Index a single file
pub fn index_single_file(path: &Path) -> Result<FileEntry> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("Failed to read metadata for {}", path.display()))?;

    let size = metadata.len();
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    // Detect file type
    let (file_type, line_count, columns) = detect_file_type(path, &extension)?;

    // Compute content hash (will be done in fingerprint stage, placeholder here)
    let content_hash = String::new();

    Ok(FileEntry {
        path: path.to_path_buf(),
        size,
        file_type,
        extension,
        content_hash,
        simhash: None,
        schema_signature: None,
        line_count,
        columns,
    })
}

/// Detect file type by examining content and extension
fn detect_file_type(path: &Path, extension: &str) -> Result<(FileType, usize, Option<Vec<String>>)> {
    // Check for Excel/OpenDocument spreadsheet first (by extension)
    if EXCEL_EXTENSIONS.contains(&extension) {
        return detect_excel_type(path);
    }

    // Check extension for CSV/TSV
    let is_csv_ext = extension == "csv";
    let is_tsv_ext = extension == "tsv" || extension == "tab";

    // Try to read first few KB to determine type
    let file = fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut buffer: Vec<u8> = Vec::with_capacity(8192);

    // Read up to 8KB for detection
    let mut total_read = 0;
    let mut line_count = 0;
    let mut has_null_byte = false;
    let mut first_line = String::new();

    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(n) => {
                total_read += n;
                line_count += 1;

                if line_count == 1 {
                    first_line = line.trim().to_string();
                }

                // Check for binary content (null bytes)
                if line.bytes().any(|b| b == 0) {
                    has_null_byte = true;
                    break;
                }

                buffer.extend(line.as_bytes());
                if total_read >= 8192 {
                    break;
                }
            }
            Err(_) => {
                has_null_byte = true;
                break;
            }
        }
    }

    // Count remaining lines if we hit 8KB limit
    if total_read >= 8192 {
        for result in reader.lines() {
            if result.is_ok() {
                line_count += 1;
            }
        }
    }

    // Determine file type
    if has_null_byte {
        return Ok((FileType::Binary, 0, None));
    }

    // Check for CSV/TSV structure
    if is_csv_ext || is_tsv_ext {
        let delimiter = if is_tsv_ext { '\t' } else { ',' };
        if let Some(columns) = parse_header(&first_line, delimiter) {
            let file_type = if is_tsv_ext {
                FileType::Tsv
            } else {
                FileType::Csv
            };
            return Ok((file_type, line_count, Some(columns)));
        }
    }

    // Auto-detect CSV/TSV by content
    if !first_line.is_empty() {
        // Try comma delimiter
        if let Some(columns) = try_detect_structured(&first_line, ',') {
            return Ok((FileType::Csv, line_count, Some(columns)));
        }
        // Try tab delimiter
        if let Some(columns) = try_detect_structured(&first_line, '\t') {
            return Ok((FileType::Tsv, line_count, Some(columns)));
        }
    }

    // Default to text
    Ok((FileType::Text, line_count, None))
}

/// Detect Excel/OpenDocument spreadsheet type using calamine
///
/// Extracts headers from the first row of the first worksheet.
fn detect_excel_type(path: &Path) -> Result<(FileType, usize, Option<Vec<String>>)> {
    // Open workbook using auto-detection (supports xlsx, xls, ods, etc.)
    let mut workbook = open_workbook_auto(path)
        .with_context(|| format!("Failed to open Excel file: {}", path.display()))?;

    // Get sheet names
    let sheet_names = workbook.sheet_names().to_vec();
    if sheet_names.is_empty() {
        return Ok((FileType::Excel, 0, None));
    }

    // Read first sheet
    let first_sheet = &sheet_names[0];
    let range = workbook
        .worksheet_range(first_sheet)
        .with_context(|| format!("Failed to read worksheet '{}' from {}", first_sheet, path.display()))?;

    // Count rows (excluding header)
    let row_count = if range.rows().count() > 0 {
        range.rows().count() - 1
    } else {
        0
    };

    // Extract headers from first row
    let columns: Option<Vec<String>> = range.rows().next().map(|row| {
        row.iter()
            .map(|cell| cell.to_string().trim().to_string())
            .collect()
    });

    Ok((FileType::Excel, row_count, columns))
}

/// Parse a header line with the given delimiter
fn parse_header(line: &str, delimiter: char) -> Option<Vec<String>> {
    let parts: Vec<&str> = line.split(delimiter).collect();
    if parts.len() >= 2 {
        Some(parts.iter().map(|s| s.trim().to_string()).collect())
    } else {
        None
    }
}

/// Try to detect if a line looks like a structured header
fn try_detect_structured(line: &str, delimiter: char) -> Option<Vec<String>> {
    let parts: Vec<&str> = line.split(delimiter).collect();

    // Need at least 2 columns to be considered structured
    if parts.len() < 2 {
        return None;
    }

    // Check if all parts look like valid column names (non-empty, reasonable length)
    let valid_columns = parts.iter().all(|p| {
        let trimmed = p.trim();
        !trimmed.is_empty() && trimmed.len() < 100 && !trimmed.contains('\n')
    });

    if valid_columns {
        Some(parts.iter().map(|s| s.trim().to_string()).collect())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_header() {
        let header = "id,name,value";
        let columns = parse_header(header, ',');
        assert_eq!(columns, Some(vec!["id".to_string(), "name".to_string(), "value".to_string()]));
    }

    #[test]
    fn test_try_detect_structured() {
        assert!(try_detect_structured("a,b,c", ',').is_some());
        assert!(try_detect_structured("single_column", ',').is_none());
    }

    #[test]
    fn test_excel_extensions() {
        assert!(EXCEL_EXTENSIONS.contains(&"xlsx"));
        assert!(EXCEL_EXTENSIONS.contains(&"xls"));
        assert!(EXCEL_EXTENSIONS.contains(&"ods"));
        assert!(!EXCEL_EXTENSIONS.contains(&"csv"));
    }
}
