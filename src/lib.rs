//! CompareIt - High-performance file comparison library
//!
//! This library provides core functionality for comparing files and folders.
//! It supports both CLI and UI interfaces by sharing the same high-performance engine.

pub mod compare_structured;
pub mod compare_text;
pub mod export;
pub mod fingerprint;
pub mod index;
pub mod match_files;
pub mod report;
pub mod types;

use anyhow::{Context, Result};
use chrono::Local;
use rayon::prelude::*;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::compare_structured::compare_structured_files;
use crate::compare_text::compare_text_files;
use crate::export::{calculate_summary, export_all};
use crate::fingerprint::compute_fingerprints;
use crate::index::index_path;
use crate::match_files::generate_candidates;
use crate::report::generate_html_report;
use crate::types::{
    CandidatePair, CompareConfig, CompareMode, ComparisonResult, FileEntry, FileType,
};

/// Trait for reporting progress during long-running operations
pub trait ProgressReporter: Send + Sync {
    fn start(&self, total: u64, message: &str);
    fn inc(&self, delta: u64);
    fn finish(&self, message: &str);
}

/// A no-op progress reporter that does nothing
pub struct NoopProgressReporter;
impl ProgressReporter for NoopProgressReporter {
    fn start(&self, _total: u64, _message: &str) {}
    fn inc(&self, _delta: u64) {}
    fn finish(&self, _message: &str) {}
}

/// Core comparison engine that can be used by both CLI and UI
pub struct ComparisonEngine<'a> {
    pub config: &'a CompareConfig,
    pub progress: Option<&'a dyn ProgressReporter>,
}

impl<'a> ComparisonEngine<'a> {
    pub fn new(config: &'a CompareConfig) -> Self {
        Self {
            config,
            progress: None,
        }
    }

    pub fn with_progress(mut self, progress: &'a dyn ProgressReporter) -> Self {
        self.progress = Some(progress);
        self
    }

    /// Run the full comparison pipeline
    pub fn run(&self, path1: &PathBuf, path2: &PathBuf) -> Result<Vec<ComparisonResult>> {
        // Set up automatic results directory using config.results_base
        let results_dir = ensure_results_dir(&self.config.results_base)?;
        let (auto_jsonl_path, auto_html_path, auto_artifacts_dir) = get_auto_export_paths(&results_dir);

        // Stage 1: Index files
        if let Some(p) = self.progress { p.start(0, "Indexing files..."); }
        let mut files1 = index_path(path1, &self.config.exclude_patterns).context("Failed to index path1")?;
        let mut files2 = index_path(path2, &self.config.exclude_patterns).context("Failed to index path2")?;

        // Stage 2: Compute fingerprints
        if let Some(p) = self.progress { 
            p.start((files1.len() + files2.len()) as u64, "Computing fingerprints..."); 
        }
        
        compute_fingerprints(&mut files1, &self.config.normalization);
        if let Some(p) = self.progress { p.inc(files1.len() as u64); }
        
        compute_fingerprints(&mut files2, &self.config.normalization);
        if let Some(p) = self.progress { p.finish("Fingerprinting complete"); }

        // Stage 3: Generate candidate pairs
        if let Some(p) = self.progress { p.start(0, "Generating candidates..."); }
        let candidates = generate_candidates(&files1, &files2, self.config);

        // Stage 4: Exact comparison
        if let Some(p) = self.progress { 
            p.start(candidates.len() as u64, "Comparing files..."); 
        }

        let results: Vec<ComparisonResult> = candidates
            .par_iter()
            .map(|pair| {
                let result = self.compare_pair(pair);
                if let Some(p) = self.progress { p.inc(1); }
                result
            })
            .collect();

        if let Some(p) = self.progress { p.finish("Comparison complete"); }

        // Calculate summary
        let summary = calculate_summary(&results, files1.len(), files2.len());

        // Export results
        let jsonl_path = self.config.output_jsonl.as_deref().unwrap_or(&auto_jsonl_path);
        let artifacts_path = self.config.output_dir.as_deref().unwrap_or(&auto_artifacts_dir);
        
        export_all(
            &results,
            Some(jsonl_path),
            self.config.output_csv.as_deref(),
            Some(artifacts_path),
        )?;

        // Always generate HTML report
        generate_html_report(&results, &summary, &auto_html_path, Some(artifacts_path))?;

        Ok(results)
    }

    /// Compare a single candidate pair
    pub fn compare_pair(&self, pair: &CandidatePair) -> ComparisonResult {
        // Quick check for identical files
        if pair.exact_hash_match {
            return create_identical_result(&pair.file1, &pair.file2);
        }

        // Determine comparison mode
        let mode = match self.config.mode {
            CompareMode::Auto => auto_detect_mode(&pair.file1, &pair.file2),
            CompareMode::Text => CompareMode::Text,
            CompareMode::Structured => CompareMode::Structured,
        };

        match mode {
            CompareMode::Text => {
                match compare_text_files(&pair.file1, &pair.file2, self.config) {
                    Ok(result) => ComparisonResult::Text(result),
                    Err(e) => ComparisonResult::Error {
                        file1_path: pair.file1.path.display().to_string(),
                        file2_path: pair.file2.path.display().to_string(),
                        error: e.to_string(),
                    },
                }
            }
            CompareMode::Structured => {
                match compare_structured_files(&pair.file1, &pair.file2, self.config) {
                    Ok(result) => ComparisonResult::Structured(result),
                    Err(e) => ComparisonResult::Error {
                        file1_path: pair.file1.path.display().to_string(),
                        file2_path: pair.file2.path.display().to_string(),
                        error: e.to_string(),
                    },
                }
            }
            CompareMode::Auto => {
                // Fallback to text if auto-detection fails
                match compare_text_files(&pair.file1, &pair.file2, self.config) {
                    Ok(result) => ComparisonResult::Text(result),
                    Err(e) => ComparisonResult::Error {
                        file1_path: pair.file1.path.display().to_string(),
                        file2_path: pair.file2.path.display().to_string(),
                        error: e.to_string(),
                    },
                }
            }
        }
    }
}

/// Auto-detect comparison mode based on file types
pub fn auto_detect_mode(file1: &FileEntry, file2: &FileEntry) -> CompareMode {
    if file1.file_type.is_structured() && file2.file_type.is_structured() {
        CompareMode::Structured
    } else if file1.file_type == FileType::Binary || file2.file_type == FileType::Binary {
        CompareMode::Text // Will fall through to hash-only
    } else {
        CompareMode::Text
    }
}

/// Create a result for identical files
pub fn create_identical_result(file1: &FileEntry, file2: &FileEntry) -> ComparisonResult {
    let linked_id = format!(
        "{}:{}",
        &file1.content_hash[..16.min(file1.content_hash.len())],
        &file2.content_hash[..16.min(file2.content_hash.len())]
    );

    if file1.file_type == FileType::Binary || file2.file_type == FileType::Binary {
        ComparisonResult::HashOnly {
            linked_id,
            file1_path: file1.path.display().to_string(),
            file2_path: file2.path.display().to_string(),
            file1_size: file1.size,
            file2_size: file2.size,
            identical: true,
        }
    } else if file1.file_type.is_structured() && file2.file_type.is_structured() {
        ComparisonResult::Structured(crate::types::StructuredComparisonResult {
            linked_id,
            file1_path: file1.path.display().to_string(),
            file2_path: file2.path.display().to_string(),
            file1_row_count: file1.line_count,
            file2_row_count: file2.line_count,
            common_records: file1.line_count,
            only_in_file1: 0,
            only_in_file2: 0,
            similarity_score: 1.0,
            field_mismatches: vec![],
            total_field_mismatches: 0,
            columns_only_in_file1: vec![],
            columns_only_in_file2: vec![],
            common_columns: file1.columns.clone().unwrap_or_default(),
            identical: true,
        })
    } else {
        ComparisonResult::Text(crate::types::TextComparisonResult {
            linked_id,
            file1_path: file1.path.display().to_string(),
            file2_path: file2.path.display().to_string(),
            file1_line_count: file1.line_count,
            file2_line_count: file2.line_count,
            common_lines: file1.line_count,
            only_in_file1: 0,
            only_in_file2: 0,
            similarity_score: 1.0,
            different_positions: String::new(),
            detailed_diff: String::new(),
            diff_truncated: false,
            identical: true,
        })
    }
}

/// Generate a short unique ID for the run
fn generate_run_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    
    let mut hasher = DefaultHasher::new();
    now.as_nanos().hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    
    let hash = hasher.finish();
    format!("{:08x}", hash as u32) // 8-char hex ID
}

/// Ensure the results directory exists and create a unique run subfolder
///
/// Creates a subfolder with format: `run_YYYYMMDD_HHMMSS_<unique-id>`
/// This keeps each comparison run isolated and prevents overwriting.
pub fn ensure_results_dir(base_path: &Path) -> Result<PathBuf> {
    // Ensure base directory exists
    if !base_path.exists() {
        fs::create_dir_all(base_path)
            .context("Failed to create base output directory")?;
    }
    
    // Create unique run subfolder (date_time_id format)
    let timestamp = Local::now().format("%Y%m%d_%H%M%S");
    let run_id = generate_run_id();
    let run_folder = base_path.join(format!("{}_{}", timestamp, run_id));
    
    fs::create_dir_all(&run_folder)
        .context("Failed to create run directory")?;
    
    Ok(run_folder)
}

/// Get full paths for automatic export files within the run directory
pub fn get_auto_export_paths(run_dir: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let jsonl_path = run_dir.join("results.jsonl");
    let html_path = run_dir.join("report.html");
    let artifacts_dir = run_dir.join("artifacts");
    (jsonl_path, html_path, artifacts_dir)
}
