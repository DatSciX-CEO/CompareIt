//! CompareIt - High-performance file comparison tool
//!
//! A standalone Rust executable for comparing files and folders with:
//! - Text-based diff comparison (line-level)
//! - Structured CSV/TSV comparison (key-based)
//! - All-vs-all folder matching with fingerprint-based candidate pruning
//! - Multiple export formats (JSONL, CSV, HTML)

mod compare_structured;
mod compare_text;
mod export;
mod fingerprint;
mod index;
mod match_files;
mod report;
mod types;

use anyhow::{Context, Result};
use chrono::Local;
use clap::{Parser, Subcommand};
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Cell, Color, Table};
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

use crate::compare_structured::compare_structured_files;
use crate::compare_text::compare_text_files;
use crate::export::{calculate_summary, export_all};
use crate::fingerprint::compute_fingerprints;
use crate::index::index_path;
use crate::match_files::generate_candidates;
use crate::report::{generate_html_report, load_results_from_jsonl};
use crate::types::{
    CandidatePair, CompareConfig, CompareMode, ComparisonResult, FileEntry, FileType,
    NormalizationOptions, PairingStrategy, SimilarityAlgorithm,
};

/// CompareIt - High-performance file comparison tool
#[derive(Parser)]
#[command(name = "CompareIt")]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compare files or folders
    Compare {
        /// First file or folder path
        path1: PathBuf,

        /// Second file or folder path
        path2: PathBuf,

        /// Comparison mode (auto, text, structured)
        #[arg(short, long, default_value = "auto")]
        mode: CompareMode,

        /// Pairing strategy for folders (same-path, same-name, all-vs-all)
        #[arg(long, default_value = "all-vs-all")]
        pairing: PairingStrategy,

        /// Top-K candidates per file in all-vs-all mode
        #[arg(long, default_value = "3")]
        topk: usize,

        /// Maximum number of pairs to compare
        #[arg(long)]
        max_pairs: Option<usize>,

        /// Key columns for structured comparison (comma-separated)
        #[arg(short, long, value_delimiter = ',')]
        key: Vec<String>,

        /// Numeric tolerance for structured comparison
        #[arg(long, default_value = "0.0001")]
        numeric_tol: f64,

        /// Similarity algorithm (diff, char-jaro)
        #[arg(long, default_value = "diff")]
        similarity: SimilarityAlgorithm,

        /// Normalize line endings
        #[arg(long)]
        ignore_eol: bool,

        /// Ignore trailing whitespace
        #[arg(long)]
        ignore_trailing_ws: bool,

        /// Ignore all whitespace
        #[arg(long)]
        ignore_all_ws: bool,

        /// Case-insensitive comparison
        #[arg(long)]
        ignore_case: bool,

        /// Skip empty lines
        #[arg(long)]
        skip_empty_lines: bool,

        /// Maximum bytes for detailed diff output
        #[arg(long, default_value = "1048576")]
        max_diff_bytes: usize,

        /// Exclude patterns (glob syntax, e.g., "*.tmp", "node_modules/")
        #[arg(long, value_delimiter = ',')]
        exclude: Vec<String>,

        /// Columns to ignore in structured comparison (comma-separated)
        #[arg(long, value_delimiter = ',')]
        ignore_columns: Vec<String>,

        /// Regex pattern for lines to ignore in text comparison
        #[arg(long)]
        ignore_regex: Option<String>,

        /// Output JSONL file path
        #[arg(long)]
        out_jsonl: Option<PathBuf>,

        /// Output CSV file path
        #[arg(long)]
        out_csv: Option<PathBuf>,

        /// Output directory for patches and artifacts
        #[arg(long)]
        out_dir: Option<PathBuf>,

        /// Base directory for results (contains timestamped JSONL, HTML, artifacts)
        #[arg(short = 'B', long, default_value = "results")]
        results_base: PathBuf,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Generate HTML report from comparison results
    Report {
        /// Input JSONL file with comparison results
        #[arg(short, long)]
        input: PathBuf,

        /// Output HTML file path
        #[arg(long)]
        html: PathBuf,

        /// Path to artifacts directory (for linking)
        #[arg(long)]
        artifacts: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    // Initialize logger (controlled by RUST_LOG env var)
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Compare {
            path1,
            path2,
            mode,
            pairing,
            topk,
            max_pairs,
            key,
            numeric_tol,
            similarity,
            ignore_eol,
            ignore_trailing_ws,
            ignore_all_ws,
            ignore_case,
            skip_empty_lines,
            max_diff_bytes,
            exclude,
            ignore_columns,
            ignore_regex,
            out_jsonl,
            out_csv,
            out_dir,
            results_base,
            verbose,
        } => {
            let config = CompareConfig {
                mode,
                pairing,
                top_k: topk,
                max_pairs,
                key_columns: key,
                numeric_tolerance: numeric_tol,
                normalization: NormalizationOptions {
                    ignore_eol,
                    ignore_trailing_ws,
                    ignore_all_ws,
                    ignore_case,
                    skip_empty_lines,
                },
                similarity_algorithm: similarity,
                max_diff_bytes,
                output_jsonl: out_jsonl,
                output_csv: out_csv,
                output_dir: out_dir,
                results_base,
                verbose,
                exclude_patterns: exclude,
                ignore_columns,
                ignore_regex,
            };

            run_compare(&path1, &path2, &config)?;
        }

        Commands::Report {
            input,
            html,
            artifacts,
        } => {
            run_report(&input, &html, artifacts.as_deref())?;
        }
    }

    Ok(())
}

/// Run the compare command
fn run_compare(path1: &PathBuf, path2: &PathBuf, config: &CompareConfig) -> Result<()> {
    println!("{}", style("CompareIt").cyan().bold());
    println!("{}", style("═".repeat(60)).dim());

    // Set up automatic results directory using config.results_base
    let results_dir = ensure_results_dir(&config.results_base)?;
    let (auto_jsonl_path, auto_html_path, auto_artifacts_dir) = get_auto_export_paths(&results_dir);

    // Stage 1: Index files
    println!("\n{} Indexing files...", style("[1/4]").bold());
    let mut files1 = index_path(path1, &config.exclude_patterns).context("Failed to index path1")?;
    let mut files2 = index_path(path2, &config.exclude_patterns).context("Failed to index path2")?;

    println!(
        "  Found {} files in path1, {} files in path2",
        style(files1.len()).green(),
        style(files2.len()).green()
    );

    // Stage 2: Compute fingerprints
    println!("\n{} Computing fingerprints...", style("[2/4]").bold());
    let pb = create_progress_bar((files1.len() + files2.len()) as u64);
    
    compute_fingerprints(&mut files1, &config.normalization);
    pb.inc(files1.len() as u64);
    
    compute_fingerprints(&mut files2, &config.normalization);
    pb.finish_with_message("Done");

    // Stage 3: Generate candidate pairs
    println!("\n{} Generating candidates...", style("[3/4]").bold());
    let candidates = generate_candidates(&files1, &files2, config);
    println!(
        "  Generated {} candidate pairs (strategy: {:?})",
        style(candidates.len()).green(),
        config.pairing
    );

    // Stage 4: Exact comparison
    println!("\n{} Comparing files...", style("[4/4]").bold());
    let pb = create_progress_bar(candidates.len() as u64);

    let results: Vec<ComparisonResult> = candidates
        .par_iter()
        .map(|pair| {
            let result = compare_pair(pair, config);
            pb.inc(1);
            result
        })
        .collect();

    pb.finish_with_message("Done");

    // Calculate summary
    let summary = calculate_summary(&results, files1.len(), files2.len());

    // Display results table
    println!("\n{}", style("Results Summary").cyan().bold());
    println!("{}", style("─".repeat(60)).dim());
    display_summary_table(&summary);

    // Display detailed results
    if !results.is_empty() {
        println!("\n{}", style("Comparison Details").cyan().bold());
        println!("{}", style("─".repeat(60)).dim());
        display_results_table(&results, config.verbose);
    }

    // Export results - always export to results folder automatically
    println!("\n{}", style("Exports").cyan().bold());
    println!("{}", style("─".repeat(60)).dim());
    
    // Show the results base directory
    let canonical_results = results_dir.canonicalize().unwrap_or_else(|_| results_dir.clone());
    println!(
        "  {} {}",
        style("Results Directory:").dim(),
        style(canonical_results.display()).white().bold()
    );
    
    // Determine which paths to use (user-specified or automatic)
    let jsonl_path = config.output_jsonl.as_deref().unwrap_or(&auto_jsonl_path);
    let artifacts_path = config.output_dir.as_deref().unwrap_or(&auto_artifacts_dir);
    
    // Export JSONL and artifacts
    export_all(
        &results,
        Some(jsonl_path),
        config.output_csv.as_deref(),
        Some(artifacts_path),
    )?;

    println!("  {} {}", style("JSONL:").dim(), jsonl_path.display());
    if let Some(ref path) = config.output_csv {
        println!("  {} {}", style("CSV:").dim(), path.display());
    }
    println!("  {} {}/", style("Artifacts:").dim(), artifacts_path.display());

    // Always generate HTML report
    let html_path = auto_html_path;
    generate_html_report(&results, &summary, &html_path, Some(artifacts_path))?;
    println!("  {} {}", style("HTML Report:").dim(), html_path.display());

    println!("\n{}", style("✓ Complete").green().bold());
    println!(
        "{}",
        style(format!("  Open {} in your browser to view the detailed report", html_path.display())).dim()
    );
    Ok(())
}

/// Run the report command
fn run_report(input: &PathBuf, html: &PathBuf, artifacts: Option<&std::path::Path>) -> Result<()> {
    println!("{}", style("CompareIt Report Generator").cyan().bold());
    println!("{}", style("═".repeat(60)).dim());

    println!("\nLoading results from {}...", input.display());
    let results = load_results_from_jsonl(input)?;
    println!("  Loaded {} comparison results", style(results.len()).green());

    let summary = calculate_summary(&results, 0, 0);

    println!("\nGenerating HTML report...");
    generate_html_report(&results, &summary, html, artifacts)?;

    println!(
        "\n{} Report generated: {}",
        style("✓").green(),
        html.display()
    );
    Ok(())
}

/// Compare a single candidate pair
fn compare_pair(pair: &CandidatePair, config: &CompareConfig) -> ComparisonResult {
    // Quick check for identical files
    if pair.exact_hash_match {
        return create_identical_result(&pair.file1, &pair.file2);
    }

    // Determine comparison mode
    let mode = match config.mode {
        CompareMode::Auto => auto_detect_mode(&pair.file1, &pair.file2),
        CompareMode::Text => CompareMode::Text,
        CompareMode::Structured => CompareMode::Structured,
    };

    match mode {
        CompareMode::Text => {
            match compare_text_files(&pair.file1, &pair.file2, config) {
                Ok(result) => ComparisonResult::Text(result),
                Err(e) => ComparisonResult::Error {
                    file1_path: pair.file1.path.display().to_string(),
                    file2_path: pair.file2.path.display().to_string(),
                    error: e.to_string(),
                },
            }
        }
        CompareMode::Structured => {
            match compare_structured_files(&pair.file1, &pair.file2, config) {
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
            match compare_text_files(&pair.file1, &pair.file2, config) {
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

/// Auto-detect comparison mode based on file types
fn auto_detect_mode(file1: &FileEntry, file2: &FileEntry) -> CompareMode {
    if file1.file_type.is_structured() && file2.file_type.is_structured() {
        CompareMode::Structured
    } else if file1.file_type == FileType::Binary || file2.file_type == FileType::Binary {
        CompareMode::Text // Will fall through to hash-only
    } else {
        CompareMode::Text
    }
}

/// Create a result for identical files
fn create_identical_result(file1: &FileEntry, file2: &FileEntry) -> ComparisonResult {
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
        ComparisonResult::Structured(types::StructuredComparisonResult {
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
        ComparisonResult::Text(types::TextComparisonResult {
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

/// Create a progress bar
fn create_progress_bar(total: u64) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
            .expect("Invalid progress bar template - this is a bug in CompareIt")
            .progress_chars("█▓░"),
    );
    pb
}

/// Display summary statistics table
fn display_summary_table(summary: &types::ComparisonSummary) {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS);

    // Header row for the comparison overview
    table.set_header(vec![
        Cell::new("Metric").fg(Color::Cyan),
        Cell::new("Value").fg(Color::Cyan),
        Cell::new("Status").fg(Color::Cyan),
    ]);

    // Pairs Compared
    table.add_row(vec![
        Cell::new("Pairs Compared"),
        Cell::new(summary.pairs_compared).fg(Color::White),
        Cell::new(""),
    ]);

    // Identical - green checkmark
    let identical_status = if summary.identical_pairs == summary.pairs_compared && summary.pairs_compared > 0 {
        "✓ All match!"
    } else if summary.identical_pairs > 0 {
        "✓"
    } else {
        ""
    };
    table.add_row(vec![
        Cell::new("Identical"),
        Cell::new(summary.identical_pairs).fg(Color::Green),
        Cell::new(identical_status).fg(Color::Green),
    ]);

    // Different - yellow warning
    let different_status = if summary.different_pairs > 0 {
        "≠ Review needed"
    } else {
        ""
    };
    table.add_row(vec![
        Cell::new("Different"),
        Cell::new(summary.different_pairs).fg(Color::Yellow),
        Cell::new(different_status).fg(Color::Yellow),
    ]);

    // Errors - red if any
    let error_color = if summary.error_pairs > 0 { Color::Red } else { Color::White };
    let error_status = if summary.error_pairs > 0 { "✗ Check logs" } else { "" };
    table.add_row(vec![
        Cell::new("Errors"),
        Cell::new(summary.error_pairs).fg(error_color),
        Cell::new(error_status).fg(error_color),
    ]);

    // Similarity scores with visual indicator
    let avg_sim_pct = summary.average_similarity * 100.0;
    let avg_color = if avg_sim_pct >= 90.0 {
        Color::Green
    } else if avg_sim_pct >= 50.0 {
        Color::Yellow
    } else {
        Color::Red
    };
    let avg_bar = create_similarity_bar(summary.average_similarity);
    table.add_row(vec![
        Cell::new("Avg Similarity"),
        Cell::new(format!("{:.1}%", avg_sim_pct)).fg(avg_color),
        Cell::new(avg_bar).fg(avg_color),
    ]);

    let min_sim_pct = summary.min_similarity * 100.0;
    let min_color = if min_sim_pct >= 90.0 {
        Color::Green
    } else if min_sim_pct >= 50.0 {
        Color::Yellow
    } else {
        Color::Red
    };
    let min_bar = create_similarity_bar(summary.min_similarity);
    table.add_row(vec![
        Cell::new("Min Similarity"),
        Cell::new(format!("{:.1}%", min_sim_pct)).fg(min_color),
        Cell::new(min_bar).fg(min_color),
    ]);

    println!("{table}");
}

/// Create a visual similarity bar
fn create_similarity_bar(similarity: f64) -> String {
    let filled = (similarity * 10.0).round() as usize;
    let empty = 10 - filled;
    format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
}

/// Display detailed results grouped by status
fn display_results_table(results: &[ComparisonResult], verbose: bool) {
    // Group results by status
    let mut identical: Vec<&ComparisonResult> = Vec::new();
    let mut modified: Vec<&ComparisonResult> = Vec::new();
    let mut errors: Vec<&ComparisonResult> = Vec::new();

    for result in results {
        match result {
            ComparisonResult::Error { .. } => errors.push(result),
            _ if result.is_identical() => identical.push(result),
            _ => modified.push(result),
        }
    }

    // Sort modified by similarity (ascending, so most different first)
    modified.sort_by(|a, b| {
        a.similarity_score()
            .partial_cmp(&b.similarity_score())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Display Identical Files (collapsed by default)
    if !identical.is_empty() {
        println!(
            "\n{} {} {}",
            style("✓").green(),
            style("Identical Files").green().bold(),
            style(format!("({})", identical.len())).dim()
        );
        if verbose {
            display_file_list(&identical, 10);
        } else {
            println!("  {} Use --verbose to list all", style("...").dim());
        }
    }

    // Display Modified Files (detailed)
    if !modified.is_empty() {
        println!(
            "\n{} {} {}",
            style("≠").yellow(),
            style("Modified Files").yellow().bold(),
            style(format!("({})", modified.len())).dim()
        );
        display_detailed_table(&modified, verbose);

        // Always show field-level mismatches for structured files (this is the key enhancement!)
        display_field_mismatches(&modified, verbose);

        // Show text file analysis (always show for text files with differences)
        display_diff_snippets(&modified);
    }

    // Display Errors
    if !errors.is_empty() {
        println!(
            "\n{} {} {}",
            style("✗").red(),
            style("Errors").red().bold(),
            style(format!("({})", errors.len())).dim()
        );
        display_error_list(&errors);
    }
}

/// Display a simple list of file pairs
fn display_file_list(results: &[&ComparisonResult], limit: usize) {
    for result in results.iter().take(limit) {
        let (file1, file2) = result.file_paths();
        println!(
            "  {} {} {}",
            style(truncate_path(file1, 35)).dim(),
            style("↔").dim(),
            style(truncate_path(file2, 35)).dim()
        );
    }
    if results.len() > limit {
        println!(
            "  {} ({} more...)",
            style("...").dim(),
            results.len() - limit
        );
    }
}

/// Display detailed table for modified files
fn display_detailed_table(results: &[&ComparisonResult], verbose: bool) {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS);

    table.set_header(vec![
        "File 1",
        "File 2",
        "Similarity",
        "Type",
        "Common",
        "Only F1",
        "Only F2",
    ]);

    let limit = if verbose { results.len() } else { 15.min(results.len()) };

    for result in results.iter().take(limit) {
        let (file1, file2) = result.file_paths();
        let sim = result.similarity_score();

        let (type_str, common, only1, only2) = match result {
            ComparisonResult::Text(r) => (
                "text",
                r.common_lines.to_string(),
                r.only_in_file1.to_string(),
                r.only_in_file2.to_string(),
            ),
            ComparisonResult::Structured(r) => (
                "csv",
                r.common_records.to_string(),
                r.only_in_file1.to_string(),
                r.only_in_file2.to_string(),
            ),
            ComparisonResult::HashOnly { identical, .. } => (
                "binary",
                if *identical { "1" } else { "0" }.to_string(),
                "0".to_string(),
                "0".to_string(),
            ),
            ComparisonResult::Error { .. } => ("error", "-".to_string(), "-".to_string(), "-".to_string()),
        };

        // Color code similarity
        let sim_color = if sim >= 0.9 {
            Color::Green
        } else if sim >= 0.5 {
            Color::Yellow
        } else {
            Color::Red
        };

        table.add_row(vec![
            Cell::new(truncate_path(file1, 28)),
            Cell::new(truncate_path(file2, 28)),
            Cell::new(format!("{:.1}%", sim * 100.0)).fg(sim_color),
            Cell::new(type_str),
            Cell::new(common),
            Cell::new(only1),
            Cell::new(only2),
        ]);
    }

    if results.len() > limit {
        table.add_row(vec![
            Cell::new(format!("... {} more rows ...", results.len() - limit)),
            Cell::new(""),
            Cell::new(""),
            Cell::new(""),
            Cell::new(""),
            Cell::new(""),
            Cell::new(""),
        ]);
    }

    println!("{table}");
}

/// Display diff snippets for modified text files
fn display_diff_snippets(results: &[&ComparisonResult]) {
    // Get all text results that have differences
    let text_results: Vec<(&types::TextComparisonResult, &str, &str)> = results
        .iter()
        .filter_map(|r| {
            if let ComparisonResult::Text(t) = r {
                if !t.identical {
                    let (f1, f2) = r.file_paths();
                    return Some((t, f1, f2));
                }
            }
            None
        })
        .take(5)
        .collect();

    if text_results.is_empty() {
        return;
    }

    println!("\n{}", style("Text File Analysis").cyan().bold());
    println!("{}", style("═".repeat(60)).dim());

    for (result, file1, file2) in text_results {
        println!(
            "\n{} {}",
            style("▶").cyan().bold(),
            style(truncate_path(file1, 40)).bold()
        );
        println!(
            "  {} {}",
            style("vs").dim(),
            style(truncate_path(file2, 40)).bold()
        );

        // Quick stats
        println!();
        let mut stats_table = Table::new();
        stats_table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS);
        stats_table.set_header(vec![
            Cell::new("Metric").fg(Color::Cyan),
            Cell::new("File 1").fg(Color::Cyan),
            Cell::new("File 2").fg(Color::Cyan),
        ]);
        
        stats_table.add_row(vec![
            Cell::new("Total Lines"),
            Cell::new(result.file1_line_count),
            Cell::new(result.file2_line_count),
        ]);
        stats_table.add_row(vec![
            Cell::new("Common Lines"),
            Cell::new(result.common_lines),
            Cell::new(result.common_lines),
        ]);
        if result.only_in_file1 > 0 || result.only_in_file2 > 0 {
            stats_table.add_row(vec![
                Cell::new("Unique Lines"),
                Cell::new(result.only_in_file1).fg(Color::Red),
                Cell::new(result.only_in_file2).fg(Color::Green),
            ]);
        }
        println!("{stats_table}");

        // Show diff preview if available
        if !result.detailed_diff.is_empty() {
            println!();
            println!("  {}", style("Diff Preview").yellow().bold());
            
            let mut additions = 0;
            let mut deletions = 0;
            let mut shown_lines = 0;
            let max_lines = 12;

            for line in result.detailed_diff.lines() {
                if line.starts_with('+') && !line.starts_with("+++") {
                    additions += 1;
                    if shown_lines < max_lines {
                        let display_line = if line.len() > 70 {
                            format!("{}...", &line[..67])
                        } else {
                            line.to_string()
                        };
                        println!("    {}", style(display_line).green());
                        shown_lines += 1;
                    }
                } else if line.starts_with('-') && !line.starts_with("---") {
                    deletions += 1;
                    if shown_lines < max_lines {
                        let display_line = if line.len() > 70 {
                            format!("{}...", &line[..67])
                        } else {
                            line.to_string()
                        };
                        println!("    {}", style(display_line).red());
                        shown_lines += 1;
                    }
                } else if line.starts_with("@@") && shown_lines < max_lines {
                    // Show hunk headers
                    println!("    {}", style(line).cyan().dim());
                    shown_lines += 1;
                }
            }

            // Summary
            if result.diff_truncated || additions + deletions > max_lines {
                println!(
                    "    {} {} lines added, {} lines removed (showing first {})",
                    style("ℹ").blue(),
                    style(additions).green(),
                    style(deletions).red(),
                    shown_lines
                );
            }
        }
    }
}

/// Display field-level mismatches for structured (CSV/TSV) comparisons
fn display_field_mismatches(results: &[&ComparisonResult], verbose: bool) {
    // Filter to only structured results with mismatches
    let structured_with_mismatches: Vec<&types::StructuredComparisonResult> = results
        .iter()
        .filter_map(|r| {
            if let ComparisonResult::Structured(s) = r {
                if !s.field_mismatches.is_empty() 
                    || s.only_in_file1 > 0 
                    || s.only_in_file2 > 0 
                    || !s.columns_only_in_file1.is_empty()
                    || !s.columns_only_in_file2.is_empty()
                {
                    return Some(s);
                }
            }
            None
        })
        .collect();

    if structured_with_mismatches.is_empty() {
        return;
    }

    println!("\n{}", style("Structured Data Analysis (CSV/TSV)").cyan().bold());
    println!("{}", style("═".repeat(60)).dim());

    for result in structured_with_mismatches.iter().take(if verbose { 10 } else { 5 }) {
        // Show file pair header with row counts
        println!(
            "\n{} {}",
            style("▶").cyan().bold(),
            style(truncate_path(&result.file1_path, 40)).bold()
        );
        println!(
            "  {} {}",
            style("vs").dim(),
            style(truncate_path(&result.file2_path, 40)).bold()
        );

        // ─────────────────────────────────────────────────────────────
        // SECTION 1: Quick Stats Box
        // ─────────────────────────────────────────────────────────────
        println!();
        let mut stats_table = Table::new();
        stats_table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS);
        stats_table.set_header(vec![
            Cell::new("Metric").fg(Color::Cyan),
            Cell::new("File 1").fg(Color::Cyan),
            Cell::new("File 2").fg(Color::Cyan),
            Cell::new("Delta").fg(Color::Cyan),
        ]);

        // Row counts
        let row_delta = result.file2_row_count as i64 - result.file1_row_count as i64;
        let delta_str = if row_delta > 0 {
            format!("+{}", row_delta)
        } else {
            row_delta.to_string()
        };
        let delta_color = if row_delta == 0 {
            Color::White
        } else if row_delta > 0 {
            Color::Green
        } else {
            Color::Red
        };

        stats_table.add_row(vec![
            Cell::new("Total Rows"),
            Cell::new(result.file1_row_count),
            Cell::new(result.file2_row_count),
            Cell::new(&delta_str).fg(delta_color),
        ]);

        stats_table.add_row(vec![
            Cell::new("Matched Rows"),
            Cell::new(result.common_records),
            Cell::new(result.common_records),
            Cell::new("—"),
        ]);

        if result.only_in_file1 > 0 || result.only_in_file2 > 0 {
            stats_table.add_row(vec![
                Cell::new("Unmatched Rows"),
                Cell::new(result.only_in_file1).fg(Color::Red),
                Cell::new(result.only_in_file2).fg(Color::Green),
                Cell::new(""),
            ]);
        }

        // Column counts
        let total_cols_1 = result.common_columns.len() + result.columns_only_in_file1.len();
        let total_cols_2 = result.common_columns.len() + result.columns_only_in_file2.len();
        stats_table.add_row(vec![
            Cell::new("Total Columns"),
            Cell::new(total_cols_1),
            Cell::new(total_cols_2),
            Cell::new(if total_cols_1 == total_cols_2 { "—" } else { "≠" }),
        ]);

        println!("{stats_table}");

        // ─────────────────────────────────────────────────────────────
        // SECTION 2: Schema Differences (if any)
        // ─────────────────────────────────────────────────────────────
        if !result.columns_only_in_file1.is_empty() || !result.columns_only_in_file2.is_empty() {
            println!();
            println!("  {}", style("Schema Differences").yellow().bold());
            
            if !result.columns_only_in_file1.is_empty() {
                println!(
                    "    {} {} (in File1 only)",
                    style("−").red().bold(),
                    style(result.columns_only_in_file1.join(", ")).red()
                );
            }
            if !result.columns_only_in_file2.is_empty() {
                println!(
                    "    {} {} (in File2 only)",
                    style("+").green().bold(),
                    style(result.columns_only_in_file2.join(", ")).green()
                );
            }
        }

        // ─────────────────────────────────────────────────────────────
        // SECTION 3: Column-wise Mismatch Summary
        // ─────────────────────────────────────────────────────────────
        if !result.field_mismatches.is_empty() {
            println!();
            println!("  {}", style("Column Mismatch Summary").yellow().bold());
            
            let mut col_summary_table = Table::new();
            col_summary_table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS);
            col_summary_table.set_header(vec![
                Cell::new("Column").fg(Color::Cyan),
                Cell::new("Mismatches").fg(Color::Cyan),
                Cell::new("% of Matched").fg(Color::Cyan),
            ]);

            for col_mismatch in &result.field_mismatches {
                let pct = if result.common_records > 0 {
                    (col_mismatch.mismatch_count as f64 / result.common_records as f64) * 100.0
                } else {
                    0.0
                };
                
                let pct_color = if pct > 50.0 {
                    Color::Red
                } else if pct > 10.0 {
                    Color::Yellow
                } else {
                    Color::White
                };

                col_summary_table.add_row(vec![
                    Cell::new(&col_mismatch.column_name),
                    Cell::new(col_mismatch.mismatch_count).fg(Color::Yellow),
                    Cell::new(format!("{:.1}%", pct)).fg(pct_color),
                ]);
            }

            println!("{col_summary_table}");

            // ─────────────────────────────────────────────────────────────
            // SECTION 4: Sample Value Differences (most important!)
            // ─────────────────────────────────────────────────────────────
            println!();
            println!("  {}", style("Sample Value Differences").yellow().bold());
            
            let mut value_table = Table::new();
            value_table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS);
            value_table.set_header(vec![
                Cell::new("Column").fg(Color::Cyan),
                Cell::new("Key").fg(Color::Cyan),
                Cell::new("File 1 Value").fg(Color::Red),
                Cell::new("File 2 Value").fg(Color::Green),
            ]);

            // Show sample mismatches - prioritize showing variety across columns
            let samples_per_column = if verbose { 5 } else { 2 };
            let max_samples = if verbose { 20 } else { 8 };
            let mut shown = 0;

            for col_mismatch in &result.field_mismatches {
                if shown >= max_samples {
                    break;
                }
                for sample in col_mismatch.sample_mismatches.iter().take(samples_per_column) {
                    if shown >= max_samples {
                        break;
                    }
                    value_table.add_row(vec![
                        Cell::new(&col_mismatch.column_name),
                        Cell::new(truncate_value(&sample.key, 18)),
                        Cell::new(truncate_value(&sample.value1, 25)).fg(Color::Red),
                        Cell::new(truncate_value(&sample.value2, 25)).fg(Color::Green),
                    ]);
                    shown += 1;
                }
            }

            println!("{value_table}");

            // Show how many more are available
            if result.total_field_mismatches > shown {
                println!(
                    "  {} {} more differences in output files. Use {} for more samples.",
                    style("ℹ").blue(),
                    style(result.total_field_mismatches - shown).white().bold(),
                    style("--verbose").cyan()
                );
            }
        }
    }

    if structured_with_mismatches.len() > (if verbose { 10 } else { 5 }) {
        println!(
            "\n{} {} more file pairs with differences. Use {} to see all.",
            style("ℹ").blue(),
            structured_with_mismatches.len() - if verbose { 10 } else { 5 },
            style("--verbose").cyan()
        );
    }
}

/// Truncate a value for display, preserving meaning
fn truncate_value(value: &str, max_len: usize) -> String {
    let trimmed = value.trim();
    if trimmed.len() <= max_len {
        trimmed.to_string()
    } else if max_len > 6 {
        // Show beginning and end for context
        let half = (max_len - 3) / 2;
        format!("{}...{}", &trimmed[..half], &trimmed[trimmed.len() - half..])
    } else {
        format!("{}...", &trimmed[..max_len.saturating_sub(3)])
    }
}

/// Display error list
fn display_error_list(results: &[&ComparisonResult]) {
    for result in results {
        if let ComparisonResult::Error { file1_path, file2_path, error } = result {
            println!(
                "  {} {} {}: {}",
                style(truncate_path(file1_path, 25)).dim(),
                style("↔").dim(),
                style(truncate_path(file2_path, 25)).dim(),
                style(error).red()
            );
        }
    }
}

/// Truncate a path for display
fn truncate_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        path.to_string()
    } else {
        format!("...{}", &path[path.len() - max_len + 3..])
    }
}

/// Ensure the results directory exists at the specified path
fn ensure_results_dir(base_path: &Path) -> Result<PathBuf> {
    if !base_path.exists() {
        fs::create_dir_all(base_path)
            .context("Failed to create results directory")?;
    }
    Ok(base_path.to_path_buf())
}

/// Get full paths for automatic export files
fn get_auto_export_paths(results_dir: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let timestamp = Local::now().format("%Y%m%d_%H%M%S");
    let jsonl_path = results_dir.join(format!("compare_{}_results.jsonl", timestamp));
    let html_path = results_dir.join(format!("compare_{}_report.html", timestamp));
    let artifacts_dir = results_dir.join(format!("artifacts_{}", timestamp));
    (jsonl_path, html_path, artifacts_dir)
}
