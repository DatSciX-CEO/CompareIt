// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! CompareIt Tauri Commands
//!
//! This module provides the bridge between the React frontend and the 
//! CompareIt core library. All commands run locally with zero network access.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use compare_it::{
    ComparisonEngine, ProgressReporter,
    export::calculate_summary,
    types::{
        CompareConfig, CompareMode, ComparisonResult, ComparisonSummary,
        NormalizationOptions, PairingStrategy, SimilarityAlgorithm,
    },
};

/// Configuration passed from the UI
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiCompareConfig {
    pub path1: String,
    pub path2: String,
    pub mode: Option<String>,
    pub pairing: Option<String>,
    pub top_k: Option<usize>,
    pub key_columns: Option<Vec<String>>,
    pub numeric_tolerance: Option<f64>,
    pub ignore_eol: Option<bool>,
    pub ignore_trailing_ws: Option<bool>,
    pub ignore_all_ws: Option<bool>,
    pub ignore_case: Option<bool>,
    pub skip_empty_lines: Option<bool>,
    pub exclude_patterns: Option<Vec<String>>,
    pub ignore_columns: Option<Vec<String>>,
    pub ignore_regex: Option<String>,
    pub results_base: Option<String>,
}

/// Progress event sent to the frontend
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub stage: String,
    pub message: String,
    pub current: u64,
    pub total: u64,
    pub percentage: f64,
}

/// Comparison result summary for the UI
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiComparisonSummary {
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

impl From<ComparisonSummary> for UiComparisonSummary {
    fn from(s: ComparisonSummary) -> Self {
        Self {
            total_files_set1: s.total_files_set1,
            total_files_set2: s.total_files_set2,
            pairs_compared: s.pairs_compared,
            identical_pairs: s.identical_pairs,
            different_pairs: s.different_pairs,
            error_pairs: s.error_pairs,
            average_similarity: s.average_similarity,
            min_similarity: s.min_similarity,
            max_similarity: s.max_similarity,
        }
    }
}

/// Full comparison response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareResponse {
    pub success: bool,
    pub summary: Option<UiComparisonSummary>,
    pub results: Vec<ComparisonResult>,
    pub error: Option<String>,
    pub results_dir: Option<String>,
}

/// Progress reporter that emits events to the Tauri frontend
struct TauriProgressReporter {
    app_handle: AppHandle,
    stage: std::sync::Mutex<String>,
    total: AtomicU64,
    current: AtomicU64,
}

impl TauriProgressReporter {
    fn new(app_handle: AppHandle) -> Self {
        Self {
            app_handle,
            stage: std::sync::Mutex::new(String::new()),
            total: AtomicU64::new(0),
            current: AtomicU64::new(0),
        }
    }
    
    fn emit_progress(&self) {
        let total = self.total.load(Ordering::SeqCst);
        let current = self.current.load(Ordering::SeqCst);
        let stage = self.stage.lock().unwrap().clone();
        
        let percentage = if total > 0 {
            (current as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        
        let event = ProgressEvent {
            stage: stage.clone(),
            message: format!("{}: {}/{}", stage, current, total),
            current,
            total,
            percentage,
        };
        
        let _ = self.app_handle.emit("compare-progress", event);
    }
}

impl ProgressReporter for TauriProgressReporter {
    fn start(&self, total: u64, message: &str) {
        *self.stage.lock().unwrap() = message.to_string();
        self.total.store(total, Ordering::SeqCst);
        self.current.store(0, Ordering::SeqCst);
        self.emit_progress();
    }

    fn inc(&self, delta: u64) {
        self.current.fetch_add(delta, Ordering::SeqCst);
        self.emit_progress();
    }

    fn finish(&self, message: &str) {
        *self.stage.lock().unwrap() = message.to_string();
        let total = self.total.load(Ordering::SeqCst);
        self.current.store(total, Ordering::SeqCst);
        self.emit_progress();
    }
}

/// Convert UI config to library config
fn ui_config_to_compare_config(ui_config: &UiCompareConfig) -> CompareConfig {
    let mode = match ui_config.mode.as_deref() {
        Some("text") => CompareMode::Text,
        Some("structured") => CompareMode::Structured,
        _ => CompareMode::Auto,
    };
    
    let pairing = match ui_config.pairing.as_deref() {
        Some("same-path") => PairingStrategy::SamePath,
        Some("same-name") => PairingStrategy::SameName,
        _ => PairingStrategy::AllVsAll,
    };
    
    CompareConfig {
        mode,
        pairing,
        top_k: ui_config.top_k.unwrap_or(3).min(100), // Clamp top_k to reasonable max
        max_pairs: None,
        key_columns: ui_config.key_columns.clone().unwrap_or_default(),
        numeric_tolerance: validate_numeric_tolerance(ui_config.numeric_tolerance),
        normalization: NormalizationOptions {
            ignore_eol: ui_config.ignore_eol.unwrap_or(false),
            ignore_trailing_ws: ui_config.ignore_trailing_ws.unwrap_or(false),
            ignore_all_ws: ui_config.ignore_all_ws.unwrap_or(false),
            ignore_case: ui_config.ignore_case.unwrap_or(false),
            skip_empty_lines: ui_config.skip_empty_lines.unwrap_or(false),
        },
        similarity_algorithm: SimilarityAlgorithm::Diff,
        max_diff_bytes: 1024 * 1024,
        output_jsonl: None,
        output_csv: None,
        output_dir: None,
        results_base: PathBuf::from(ui_config.results_base.as_deref().unwrap_or("results")),
        verbose: false,
        exclude_patterns: ui_config.exclude_patterns.clone().unwrap_or_default(),
        ignore_columns: ui_config.ignore_columns.clone().unwrap_or_default(),
        ignore_regex: validate_regex_pattern(ui_config.ignore_regex.clone()),
    }
}

/// Maximum allowed path length to prevent buffer-related issues
const MAX_PATH_LENGTH: usize = 4096;

/// Maximum allowed regex pattern length to prevent ReDoS via long patterns
const MAX_REGEX_LENGTH: usize = 1000;

/// Validate and canonicalize a path to prevent path traversal attacks
///
/// Security measures:
/// 1. Path length limit to prevent buffer issues
/// 2. Canonicalization to resolve symlinks and `..` components
/// 3. Absolute path enforcement
/// 4. Blacklist for sensitive system directories
fn validate_path(path_str: &str) -> Result<PathBuf, String> {
    // Check path length to prevent buffer-related issues
    if path_str.len() > MAX_PATH_LENGTH {
        return Err(format!("Path too long (max {} characters)", MAX_PATH_LENGTH));
    }
    
    // Check for empty path
    if path_str.trim().is_empty() {
        return Err("Path cannot be empty".to_string());
    }
    
    let path = PathBuf::from(path_str);
    
    // Check if path exists
    if !path.exists() {
        return Err(format!("Path does not exist: {}", path_str));
    }
    
    // Canonicalize to resolve symlinks and relative components (../, ./)
    // This is the key security measure - it resolves all `..` and `.` components
    // and follows symlinks to get the actual absolute path
    let canonical = path.canonicalize()
        .map_err(|e| format!("Failed to resolve path '{}': {}", path_str, e))?;
    
    // Ensure the path is absolute after canonicalization
    if !canonical.is_absolute() {
        return Err(format!("Path must be absolute: {}", path_str));
    }
    
    // Block access to common sensitive directories (basic protection)
    let path_str_lower = canonical.to_string_lossy().to_lowercase();
    #[cfg(windows)]
    {
        if path_str_lower.contains("\\windows\\system32")
            || path_str_lower.contains("\\programdata")
            || path_str_lower.contains("\\appdata\\local\\microsoft")
            || path_str_lower.ends_with("\\sam")
            || path_str_lower.ends_with("\\security")
            || path_str_lower.ends_with("\\system")
        {
            return Err("Access to system directories is not allowed".to_string());
        }
    }
    #[cfg(unix)]
    {
        if path_str_lower.starts_with("/etc")
            || path_str_lower.starts_with("/var")
            || path_str_lower.starts_with("/root")
            || path_str_lower.starts_with("/proc")
            || path_str_lower.starts_with("/sys")
            || path_str_lower.starts_with("/dev")
        {
            return Err("Access to system directories is not allowed".to_string());
        }
    }
    
    Ok(canonical)
}

/// Validate numeric tolerance value
fn validate_numeric_tolerance(tolerance: Option<f64>) -> f64 {
    match tolerance {
        Some(t) if t.is_finite() && t >= 0.0 && t <= 1.0 => t,
        Some(t) if t.is_finite() && t > 1.0 => 1.0, // Clamp to max
        Some(t) if t.is_finite() && t < 0.0 => 0.0, // Clamp to min
        _ => 0.0001, // Default for NaN, Infinity, or None
    }
}

/// Validate and sanitize regex pattern
fn validate_regex_pattern(pattern: Option<String>) -> Option<String> {
    pattern.and_then(|p| {
        if p.len() > MAX_REGEX_LENGTH {
            None // Reject overly long patterns
        } else if p.trim().is_empty() {
            None // Reject empty patterns
        } else {
            Some(p)
        }
    })
}

/// Run a comparison - main Tauri command
#[tauri::command]
async fn run_comparison(
    app_handle: AppHandle,
    config: UiCompareConfig,
) -> Result<CompareResponse, String> {
    let progress = TauriProgressReporter::new(app_handle.clone());
    
    let compare_config = ui_config_to_compare_config(&config);
    
    // Validate and canonicalize paths to prevent path traversal
    let path1 = match validate_path(&config.path1) {
        Ok(p) => p,
        Err(e) => {
            return Ok(CompareResponse {
                success: false,
                summary: None,
                results: vec![],
                error: Some(e),
                results_dir: None,
            });
        }
    };
    
    let path2 = match validate_path(&config.path2) {
        Ok(p) => p,
        Err(e) => {
            return Ok(CompareResponse {
                success: false,
                summary: None,
                results: vec![],
                error: Some(e),
                results_dir: None,
            });
        }
    };
    
    // Run comparison in a blocking task
    let results_base = compare_config.results_base.clone();
    let result = tokio::task::spawn_blocking(move || {
        let engine = ComparisonEngine::new(&compare_config).with_progress(&progress);
        engine.run(&path1, &path2)
    }).await;
    
    match result {
        Ok(Ok(results)) => {
            let summary = calculate_summary(&results, 0, 0);
            let results_dir = results_base.canonicalize()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| results_base.display().to_string());
            
            Ok(CompareResponse {
                success: true,
                summary: Some(summary.into()),
                results,
                error: None,
                results_dir: Some(results_dir),
            })
        }
        Ok(Err(e)) => Ok(CompareResponse {
            success: false,
            summary: None,
            results: vec![],
            error: Some(e.to_string()),
            results_dir: None,
        }),
        Err(e) => Ok(CompareResponse {
            success: false,
            summary: None,
            results: vec![],
            error: Some(format!("Task panicked: {}", e)),
            results_dir: None,
        }),
    }
}

/// Get app version
#[tauri::command]
fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            run_comparison,
            get_version,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
