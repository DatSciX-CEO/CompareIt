//! Export functionality for comparison results
//!
//! This module handles writing comparison results to various formats:
//! - JSONL (streaming, one result per line)
//! - CSV (summary format)
//! - Patch/artifact files

use crate::types::{ComparisonResult, ComparisonSummary};
use anyhow::{Context, Result};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

/// Export results to JSONL format (one JSON object per line)
pub fn export_jsonl(results: &[ComparisonResult], output_path: &Path) -> Result<()> {
    let file = File::create(output_path)
        .with_context(|| format!("Failed to create {}", output_path.display()))?;
    let mut writer = BufWriter::new(file);

    for result in results {
        let json = serde_json::to_string(result)?;
        writeln!(writer, "{}", json)?;
    }

    writer.flush()?;
    Ok(())
}

/// Export results to CSV summary format
pub fn export_csv(results: &[ComparisonResult], output_path: &Path) -> Result<()> {
    let file = File::create(output_path)
        .with_context(|| format!("Failed to create {}", output_path.display()))?;
    let mut writer = csv::Writer::from_writer(file);

    // Write header
    writer.write_record([
        "linked_id",
        "file1_path",
        "file2_path",
        "type",
        "similarity_score",
        "identical",
        "file1_count",
        "file2_count",
        "common",
        "only_in_file1",
        "only_in_file2",
        "total_mismatches",
    ])?;

    // Write records
    for result in results {
        match result {
            ComparisonResult::Text(r) => {
                writer.write_record([
                    &r.linked_id,
                    &r.file1_path,
                    &r.file2_path,
                    "text",
                    &format!("{:.4}", r.similarity_score),
                    &r.identical.to_string(),
                    &r.file1_line_count.to_string(),
                    &r.file2_line_count.to_string(),
                    &r.common_lines.to_string(),
                    &r.only_in_file1.to_string(),
                    &r.only_in_file2.to_string(),
                    &(r.only_in_file1 + r.only_in_file2).to_string(),
                ])?;
            }
            ComparisonResult::Structured(r) => {
                writer.write_record([
                    &r.linked_id,
                    &r.file1_path,
                    &r.file2_path,
                    "structured",
                    &format!("{:.4}", r.similarity_score),
                    &r.identical.to_string(),
                    &r.file1_row_count.to_string(),
                    &r.file2_row_count.to_string(),
                    &r.common_records.to_string(),
                    &r.only_in_file1.to_string(),
                    &r.only_in_file2.to_string(),
                    &r.total_field_mismatches.to_string(),
                ])?;
            }
            ComparisonResult::HashOnly {
                linked_id,
                file1_path,
                file2_path,
                file1_size,
                file2_size,
                identical,
            } => {
                let sim_str = if *identical { "1.0000" } else { "0.0000" };
                let common_str = if *identical { "1" } else { "0" };
                let mismatch_str = if *identical { "0" } else { "1" };
                writer.write_record([
                    linked_id.as_str(),
                    file1_path.as_str(),
                    file2_path.as_str(),
                    "binary",
                    sim_str,
                    &identical.to_string(),
                    &file1_size.to_string(),
                    &file2_size.to_string(),
                    common_str,
                    "0",
                    "0",
                    mismatch_str,
                ])?;
            }
            ComparisonResult::Error {
                file1_path,
                file2_path,
                error,
            } => {
                writer.write_record([
                    "",
                    file1_path,
                    file2_path,
                    "error",
                    "0.0000",
                    "false",
                    "",
                    "",
                    "",
                    "",
                    "",
                    error,
                ])?;
            }
        }
    }

    writer.flush()?;
    Ok(())
}

/// Write patch files for text comparison results
pub fn write_patches(results: &[ComparisonResult], output_dir: &Path) -> Result<()> {
    let patches_dir = output_dir.join("patches");
    fs::create_dir_all(&patches_dir)?;

    for result in results {
        if let ComparisonResult::Text(r) = result {
            if !r.detailed_diff.is_empty() && !r.identical {
                let filename = sanitize_filename(&r.linked_id) + ".diff";
                let path = patches_dir.join(&filename);

                fs::write(&path, &r.detailed_diff)
                    .with_context(|| format!("Failed to write patch {}", path.display()))?;
            }
        }
    }

    Ok(())
}

/// Write mismatch artifacts for structured comparison results
pub fn write_mismatch_artifacts(results: &[ComparisonResult], output_dir: &Path) -> Result<()> {
    let mismatches_dir = output_dir.join("mismatches");
    fs::create_dir_all(&mismatches_dir)?;

    for result in results {
        if let ComparisonResult::Structured(r) = result {
            if !r.identical {
                let filename = sanitize_filename(&r.linked_id) + ".json";
                let path = mismatches_dir.join(&filename);

                let json = serde_json::to_string_pretty(r)?;
                fs::write(&path, json)
                    .with_context(|| format!("Failed to write mismatch {}", path.display()))?;
            }
        }
    }

    Ok(())
}

/// Sanitize a string for use as a filename
fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

/// Process statistics captured during comparison execution
#[derive(Debug, Clone, Default)]
pub struct ProcessStats {
    /// Execution time in milliseconds
    pub execution_time_ms: Option<u64>,
    /// Processing speed in MB/s
    pub processing_speed_mb_per_sec: Option<f64>,
    /// Peak memory usage in bytes
    pub peak_memory_usage_bytes: Option<u64>,
    /// Total bytes processed
    pub total_data_processed_bytes: Option<u64>,
    /// Comparison mode string (e.g., "Auto", "Text")
    pub comparison_mode: Option<String>,
    /// Similarity algorithm string (e.g., "Diff", "Cosine")
    pub similarity_algorithm: Option<String>,
}

/// Calculate summary statistics for a set of results
pub fn calculate_summary(
    results: &[ComparisonResult],
    total1: usize,
    total2: usize,
    process_stats: Option<ProcessStats>,
) -> ComparisonSummary {
    let mut identical = 0;
    let mut different = 0;
    let mut errors = 0;
    let mut similarities = Vec::new();

    for result in results {
        match result {
            ComparisonResult::Error { .. } => errors += 1,
            _ => {
                if result.is_identical() {
                    identical += 1;
                } else {
                    different += 1;
                }
                similarities.push(result.similarity_score());
            }
        }
    }

    let average_similarity = if similarities.is_empty() {
        0.0
    } else {
        similarities.iter().sum::<f64>() / similarities.len() as f64
    };

    let min_similarity = similarities.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_similarity = similarities.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    // Extract process stats if provided
    let (exec_time, speed, memory, data_bytes, mode, algo) = match process_stats {
        Some(stats) => (
            stats.execution_time_ms,
            stats.processing_speed_mb_per_sec,
            stats.peak_memory_usage_bytes,
            stats.total_data_processed_bytes,
            stats.comparison_mode,
            stats.similarity_algorithm,
        ),
        None => (None, None, None, None, None, None),
    };

    ComparisonSummary {
        total_files_set1: total1,
        total_files_set2: total2,
        pairs_compared: results.len(),
        identical_pairs: identical,
        different_pairs: different,
        error_pairs: errors,
        average_similarity: if average_similarity.is_nan() { 0.0 } else { average_similarity },
        min_similarity: if min_similarity.is_infinite() { 0.0 } else { min_similarity },
        max_similarity: if max_similarity.is_infinite() { 0.0 } else { max_similarity },
        execution_time_ms: exec_time,
        processing_speed_mb_per_sec: speed,
        peak_memory_usage_bytes: memory,
        total_data_processed_bytes: data_bytes,
        comparison_mode: mode,
        similarity_algorithm: algo,
    }
}

/// Export all artifacts (JSONL, CSV, patches, mismatches)
pub fn export_all(
    results: &[ComparisonResult],
    jsonl_path: Option<&Path>,
    csv_path: Option<&Path>,
    output_dir: Option<&Path>,
) -> Result<()> {
    if let Some(path) = jsonl_path {
        export_jsonl(results, path)?;
    }

    if let Some(path) = csv_path {
        export_csv(results, path)?;
    }

    if let Some(dir) = output_dir {
        fs::create_dir_all(dir)?;
        write_patches(results, dir)?;
        write_mismatch_artifacts(results, dir)?;
    }

    Ok(())
}
