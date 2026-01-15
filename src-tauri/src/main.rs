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
        top_k: ui_config.top_k.unwrap_or(3),
        max_pairs: None,
        key_columns: ui_config.key_columns.clone().unwrap_or_default(),
        numeric_tolerance: ui_config.numeric_tolerance.unwrap_or(0.0001),
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
        ignore_regex: ui_config.ignore_regex.clone(),
    }
}

/// Run a comparison - main Tauri command
#[tauri::command]
async fn run_comparison(
    app_handle: AppHandle,
    config: UiCompareConfig,
) -> Result<CompareResponse, String> {
    let progress = TauriProgressReporter::new(app_handle.clone());
    
    let compare_config = ui_config_to_compare_config(&config);
    let path1 = PathBuf::from(&config.path1);
    let path2 = PathBuf::from(&config.path2);
    
    // Validate paths exist
    if !path1.exists() {
        return Ok(CompareResponse {
            success: false,
            summary: None,
            results: vec![],
            error: Some(format!("Path does not exist: {}", config.path1)),
            results_dir: None,
        });
    }
    
    if !path2.exists() {
        return Ok(CompareResponse {
            success: false,
            summary: None,
            results: vec![],
            error: Some(format!("Path does not exist: {}", config.path2)),
            results_dir: None,
        });
    }
    
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
