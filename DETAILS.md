# CompareIt Technical Architecture & File Details

This document provides a technical breakdown of the `CompareIt` codebase, explaining the responsibility and purpose of each module.

---

## 📂 Source Code Structure (`src/`)

### 1. `main.rs`
*   **What it does:** The application entry point. It handles CLI argument parsing using `clap`, initializes logging, and orchestrates the four stages of the comparison pipeline (Index -> Fingerprint -> Match -> Compare).
*   **Why it exists:** To serve as the "brain" of the CLI, coordinating between the various specialized modules and providing the user interface (terminal tables and progress bars).

### 2. `types.rs`
*   **What it does:** Defines the core data structures used throughout the app, including `FileEntry`, `CompareConfig`, and the `ComparisonResult` enum (which covers Text, Structured, and HashOnly modes).
*   **Why it exists:** To provide a single source of truth for data models. By centralizing these types, we ensure consistency across the indexing, matching, and reporting phases.

### 3. `index.rs`
*   **What it does:** Scans the provided paths, respects exclude patterns, detects file types (via extensions), and gathers metadata like file size.
*   **Why it exists:** To build a "manifest" of what needs to be compared before any heavy processing begins. This allows the engine to be efficient by only touching relevant files.

### 4. `fingerprint.rs`
*   **What it does:** Generates two types of "fingerprints" for every file:
    *   **Blake3 Hash:** For identifying bit-for-bit identical files.
    *   **Simhash:** A locality-sensitive hash that allows the app to estimate how similar two text files are without comparing them line-by-line.
*   **Why it exists:** To enable "All-vs-All" matching at scale. Simhash allows the app to find the best candidates for comparison in O(1) time rather than O(N^2).

### 5. `match_files.rs`
*   **What it does:** Implements the logic for pairing files. It applies "blocking rules" (like extension matching and size ratios) and uses fingerprint similarity to decide which files from Folder A should be compared against which files from Folder B.
*   **Why it exists:** To prune the search space. In a directory with thousands of files, we shouldn't compare everything to everything; this module ensures we only perform detailed diffs on likely matches.

### 6. `compare_text.rs`
*   **What it does:** Performs line-by-line diffing for text files using the Myers diff algorithm (via the `similar` crate). It also handles regex-based filtering to ignore noise like timestamps.
*   **Why it exists:** To provide the traditional "diff" experience for code, logs, and markdown files.

### 7. `compare_structured.rs`
*   **What it does:** Specifically handles CSV and TSV files. Instead of line-by-line, it matches records based on "Key Columns" and performs field-level audits with numeric tolerance.
*   **Why it exists:** Because line-by-line diffs fail on structured data if rows are reordered or if floating-point numbers have tiny insignificant differences.

### 8. `export.rs`
*   **What it does:** Serializes comparison results into JSONL and CSV formats. It also generates the `patches/` (diff files) and `mismatches/` (JSON detail files) artifacts.
*   **Why it exists:** To make the results usable by other tools or for manual deep-dives into specific file differences.

### 9. `report.rs`
*   **What it does:** Bundles the comparison results into a self-contained, interactive HTML dashboard.
*   **Why it exists:** To provide a human-friendly way to browse thousands of comparison results, allowing users to search, filter, and view diffs in a browser.

---

## 🚀 Execution Workflow

1.  **Main** parses inputs and calls **Index**.
2.  **Index** crawls directories; **Fingerprint** hashes everything in parallel.
3.  **Match** generates a list of "Candidate Pairs."
4.  **Main** uses `Rayon` to run **Compare (Text/Structured)** on all pairs in parallel.
5.  **Export** and **Report** save the findings to the `results/` directory.
