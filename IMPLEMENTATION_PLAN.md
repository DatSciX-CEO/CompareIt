# Implementation Plan: CompareIt Enhancements

This plan outlines the steps to improve the CompareIt application based on the review. The goal is to enhance usability, safety, and reporting without altering the core high-performance architecture.

## Phase 1: Code Safety & Cleanup
**Goal**: Make the application crash-proof and easier to read.

1.  **Remove `unwrap()` calls**:
    -   Audit `src/main.rs`, `src/compare_structured.rs`, and `src/match_files.rs`.
    -   Replace `unwrap()` with `?` (error propagation) or `unwrap_or_default()` / `unwrap_or_else()` where appropriate.
    -   *Example*: In `compare_structured.rs`, `records1.get(*key).unwrap()` is unsafe if the key logic has a bug. Change to `if let Some(row) = ...`.

2.  **Refactor `match_files.rs`**:
    -   Break `all_vs_all_match` into `find_exact_matches`, `find_candidate_matches`, and `apply_blocking_rules`.
    -   Add unit tests for the blocking rules to ensure they behave as expected.

3.  **Enhance Logging**:
    -   Replace `eprintln!` warnings with a proper logging facade (like `log` crate + `env_logger`) or structured print statements that can be silenced.

## Phase 2: Core Feature Enhancements
**Goal**: Add essential features for better real-world usage.

1.  **File/Folder Exclusion**:
    -   Add `--exclude <pattern>` argument to CLI (supports glob patterns like `*.tmp`, `node_modules/`).
    -   Update `index.rs` to respect these patterns during `WalkDir`.

2.  **Column Exclusion (Structured Mode)**:
    -   Add `--ignore-columns <cols>` argument.
    -   Update `compare_structured.rs` to filter out these columns *before* comparison and *before* schema signature generation.

3.  **Regex Filtering (Text Mode)**:
    -   Add `--ignore-regex <pattern>` argument.
    -   In `compare_text.rs`, apply regex replacement (e.g., replacing matches with `<IGNORED>`) on lines before diffing.

## Phase 3: Reporting Overhaul (Critical)
**Goal**: Make results immediately understandable.

1.  **Enhanced CLI Output**:
    -   **Grouped Summary**: Instead of one long table, print:
        -   "Identical Files (X)" - collapsed/summary only.
        -   "Modified Files (Y)" - detailed list with similarity scores.
        -   "Unique Files (Z)" - list of files only in A or B.
    -   **Visual Diff Snippets**: For text files with high similarity (>90%), show the first 3 lines of difference directly in the CLI output if `--verbose` is set.

2.  **Next-Gen HTML Report (`report.rs`)**:
    -   **Dashboard**: Add a header with a Pie Chart (using simple CSS/SVG) showing the ratio of Identical vs. Different files.
    -   **Side-by-Side Diff**:
        -   Instead of linking to `.diff` files, embed the diff data in a hidden JS variable or separate data file.
        -   Add a "View Diff" button in the table row that expands a modal or inline row showing a side-by-side comparison (left: File 1, right: File 2) with red/green highlighting.
    -   **Structured View**:
        -   For CSVs, show a table of the *mismatched rows only*, highlighting the specific cells that differ.

## Phase 4: Documentation & Polish
**Goal**: Make it easy for new users to start.

1.  **Update `README.md`**:
    -   Add examples for the new exclusion flags.
    -   Explain the "Unique ID" concept clearly: "A stable identifier generated from the content hashes of the pair, used to link results across different output formats."
2.  **Internal Docs**:
    -   Add doc comments to all public structs in `types.rs`.
    -   Explain the `simhash` and `blocking rules` logic in comments for future maintainers.

## Execution Order
1.  Phase 1 (Safety) - Low risk, high value.
2.  Phase 3 (Reporting) - High visibility, addresses primary user request.
3.  Phase 2 (Features) - Adds capability.
4.  Phase 4 (Docs) - Final polish.
