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
use clap::{Parser, Subcommand};
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Cell, Color, Table};
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::path::PathBuf;

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

    // Export results
    if config.output_jsonl.is_some() || config.output_csv.is_some() || config.output_dir.is_some() {
        println!("\n{}", style("Exporting results...").dim());
        export_all(
            &results,
            config.output_jsonl.as_deref(),
            config.output_csv.as_deref(),
            config.output_dir.as_deref(),
        )?;

        if let Some(ref path) = config.output_jsonl {
            println!("  JSONL: {}", path.display());
        }
        if let Some(ref path) = config.output_csv {
            println!("  CSV: {}", path.display());
        }
        if let Some(ref path) = config.output_dir {
            println!("  Artifacts: {}/", path.display());
        }
    }

    println!("\n{}", style("✓ Complete").green().bold());
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

    table.add_row(vec![
        Cell::new("Pairs Compared"),
        Cell::new(summary.pairs_compared),
    ]);
    table.add_row(vec![
        Cell::new("Identical"),
        Cell::new(summary.identical_pairs).fg(Color::Green),
    ]);
    table.add_row(vec![
        Cell::new("Different"),
        Cell::new(summary.different_pairs).fg(Color::Yellow),
    ]);
    table.add_row(vec![
        Cell::new("Errors"),
        Cell::new(summary.error_pairs).fg(if summary.error_pairs > 0 {
            Color::Red
        } else {
            Color::White
        }),
    ]);
    table.add_row(vec![
        Cell::new("Avg Similarity"),
        Cell::new(format!("{:.1}%", summary.average_similarity * 100.0)),
    ]);
    table.add_row(vec![
        Cell::new("Min Similarity"),
        Cell::new(format!("{:.1}%", summary.min_similarity * 100.0)),
    ]);

    println!("{table}");
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

        // Show diff snippets for high-similarity files in verbose mode
        if verbose {
            display_diff_snippets(&modified);
        }
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

/// Display diff snippets for modified files with high similarity
fn display_diff_snippets(results: &[&ComparisonResult]) {
    let high_sim_results: Vec<&&ComparisonResult> = results
        .iter()
        .filter(|r| {
            let sim = r.similarity_score();
            sim >= 0.9 && sim < 1.0
        })
        .take(3) // Show at most 3 diff snippets
        .collect();

    if high_sim_results.is_empty() {
        return;
    }

    println!("\n{}", style("Diff Snippets (High Similarity)").cyan().bold());
    println!("{}", style("─".repeat(50)).dim());

    for result in high_sim_results {
        if let ComparisonResult::Text(r) = result {
            let (file1, _file2) = result.file_paths();
            println!("\n{}", style(truncate_path(file1, 50)).bold());

            // Show first 5 lines of diff
            let diff_lines: Vec<&str> = r.detailed_diff.lines().take(8).collect();
            for line in diff_lines {
                if line.starts_with('+') && !line.starts_with("+++") {
                    println!("  {}", style(line).green());
                } else if line.starts_with('-') && !line.starts_with("---") {
                    println!("  {}", style(line).red());
                } else if !line.starts_with("@@") && !line.starts_with("---") && !line.starts_with("+++") {
                    println!("  {}", style(line).dim());
                }
            }

            if r.diff_truncated || r.detailed_diff.lines().count() > 8 {
                println!("  {}", style("... (truncated)").dim());
            }
        }
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
