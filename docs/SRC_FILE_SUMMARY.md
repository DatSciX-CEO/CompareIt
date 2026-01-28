# Source File Summary

This document provides a detailed technical breakdown of the `CompareIt` source code (`src/`). It covers the purpose, internal logic, efficiency optimizations, and network/crate integrations for each module.

---

## 📂 Core Logic Modules

### 1. `lib.rs`
**Role:** Central Orchestrator & Library Entry Point
*   **Description:** The heart of the application. It ties all other modules together into a cohesive pipeline: **Index → Fingerprint → Candidate Generation → Comparison → Export**.
*   **Key Logic:**
    *   **`ComparisonEngine`**: The main struct that holds configuration and execution state.
    *   **Dynamic Memory Management**: Automatically calculates safe RAM limits (defaulting to 5% of system memory) using `sysinfo` to prevent crashes on large files.
    *   **Process Statistics**: Captures execution time, throughput (MB/s), and peak memory usage.
*   **Efficiency**: Minimal overhead; delegates heavy lifting to specialized modules while managing global state and progress reporting.
*   **Integrations**: `sysinfo` (Hardware detection), `chrono` (Timestamps), `rayon` (Parallelism).

### 2. `main.rs`
**Role:** CLI Interface
*   **Description:** The command-line front-end. It translates user arguments into internal configuration and presenting results.
*   **Key Logic:**
    *   **Argument Parsing**: Uses `clap` to handle complex subcommands (`compare`, `report`) and flags.
    *   **User Feedback**: Manages progress bars (`indicatif`) and formatted terminal tables (`comfy-table`).
    *   **Logging**: Initializes `env_logger` for debug output.
*   **Efficiency**: A thin wrapper; does not contain core business logic, ensuring the library remains reusable for UI apps.
*   **Integrations**: `clap` (CLI Args), `indicatif` (Progress Bars), `comfy-table` (Tables), `console` (Colors), `env_logger`.

---

## ⚙️ Comparison Engines

### 3. `compare_text.rs`
**Role:** Text & Code Comparison Engine
*   **Description:** Handles line-by-line comparison of text-based files (code, logs, documentation).
*   **Key Logic:**
    *   **Algorithms**: Implements Myers Diff (default), plus 12+ others (Jaccard, Cosine, Levenshtein, etc.).
    *   **Normalization**: Pre-processes text (trim whitespace, lower-case, ignore regex patterns) to focus on semantic changes.
    *   **Diff Generation**: Produces unified diff output for reports.
*   **Efficiency - Zero-Copy Slicing**: Uses `diff_slices` to compare vector references instead of allocating massive new strings. This allows comparing 700MB+ files without OOM errors.
*   **Integrations**: `similar` (Diff algorithms), `strsim` (String metrics), `regex`.

### 4. `compare_structured.rs`
**Role:** Structured Data Engine (CSV/Excel)
*   **Description:** Performs "semantic" comparison of spreadsheets and databases. Instead of line-diffs, it compares records by **Primary Key**.
*   **Key Logic:**
    *   **Key-Based Matching**: Identifies rows by a composite key (e.g., "ID+Date") even if they are reordered.
    *   **Field-Level Deltas**: Tracks specific column mismatches (e.g., "Price changed from 10.00 to 12.00").
    *   **Numeric Tolerance**: Allows floating-point comparisons with user-defined tolerance (e.g., `0.0001`).
*   **Efficiency - Sort-Merge Join**: Instead of loading everything into a HashMap (high RAM), it sorts both files in parallel (`rayon`) and performs a linear merge scan. This is cache-friendly and low-memory.
*   **Integrations**: `csv` (Parsing), `calamine` (Excel reading), `rayon` (Parallel Sort).

---

## 🔍 Discovery & Matching

### 5. `index.rs`
**Role:** File Discovery & Type Detection
*   **Description:** Scans directories to build an inventory of files.
*   **Key Logic:**
    *   **Recursive Scanning**: Walking directory trees while respecting `.gitignore`-style exclusion patterns.
    *   **Type Detection**: Auto-detects Text vs. Binary vs. CSV vs. Excel by reading the first 8KB of header data.
*   **Efficiency**: Parallel recursion; "fail-fast" type detection avoids reading full files unnecessarily.
*   **Integrations**: `walkdir` (Filesystem traversal), `globset` (Pattern matching), `calamine`.

### 6. `fingerprint.rs`
**Role:** Hashing & Similarity Estimation
*   **Description:** Generates "fingerprints" to quickly compare huge files without reading them fully during the matching phase.
*   **Key Logic:**
    *   **Blake3 Hash**: For instant exact-match detection.
    *   **Simhash**: A perceptual hash for text; files with similar content have similar hashes (Hamming distance).
    *   **Schema Signature**: Hashes column names to ensure structured files have compatible schemas before comparing.
*   **Efficiency**:
    *   **Streaming**: Computes Blake3 in chunks (constant RAM).
    *   **Smart Fallback**: Skips expensive Simhash for files > `max_fingerprint_size` (default 5% RAM) to prevent hangs.
*   **Integrations**: `blake3` (Fast Hashing), `rayon`.

### 7. `match_files.rs`
**Role:** Candidate Pair Generation
*   **Description:** Decides *which* files in Folder A should be compared to files in Folder B.
*   **Key Logic:**
    *   **Strategies**: Supports `SamePath`, `SameName`, and `AllVsAll`.
    *   **All-Vs-All**: Uses fingerprints to find the "Top-K" most similar matches, allowing it to find renamed or moved files.
    *   **Blocking Rules**: Rapidly prunes impossible matches (e.g., different file extensions, huge size variance) to save CPU.
*   **Efficiency**: Reduces an O(N*M) problem to a manageable subset using heuristics.
*   **Integrations**: Internal logic.

---

## 📊 Output & Reporting

### 8. `report.rs`
**Role:** HTML Report Generator
*   **Description:** Creates the self-contained `report.html`.
*   **Key Logic:**
    *   **Embedded Assets**: Inlines all CSS and JavaScript so the report is a single portable file.
    *   **Visualizations**: CSS-conic-gradient pie charts, sortable JS tables, and side-by-side diff views.
    *   **Run Details**: Renders the process statistics (Time, RAM, Speed) captured in `lib.rs`.
*   **Integrations**: Standard library string manipulation (no heavy template engine for speed).

### 9. `export.rs`
**Role:** Data Exporter
*   **Description:** Serializes results to machine-readable formats.
*   **Key Logic:**
    *   **JSON \--> JSONL**: Streams results line-by-line (NDJSON) to support datasets larger than available RAM.
    *   **Artifacts**: Writes separate `.diff` patch files and `.json` mismatch logs for external tools.
*   **Integrations**: `serde` (Serialization), `serde_json`, `csv`.

---

## 🧱 Shared Types

### 10. `types.rs`
**Role:** Data Structures & Config
*   **Description:** The "Schema" of the application. Defines the data types passed between all other modules.
*   **Key Definitions:**
    *   `CompareConfig`: Global settings (tolerances, thresholds, modes).
    *   `ComparisonResult`: enum (`Text`, `Structured`, `Binary`, `Error`) handling polymorphic result data.
    *   `FileEntry`: Metadata for an indexed file.
*   **Integrations**: `serde` (Macros), `clap` (Enum variants for CLI).
