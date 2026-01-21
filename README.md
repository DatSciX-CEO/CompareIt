# CompareIt

<div align="center">

![Rust](https://img.shields.io/badge/built_with-Rust-dca282.svg?style=flat-square&logo=rust)
![Tauri](https://img.shields.io/badge/UI-Tauri_v2-blue?style=flat-square&logo=tauri)
![React](https://img.shields.io/badge/frontend-React-61dafb?style=flat-square&logo=react)
![Performance](https://img.shields.io/badge/performance-blazing-blue?style=flat-square)
![License](https://img.shields.io/badge/license-MIT-green?style=flat-square)

### The Next-Generation File Intelligence & Comparison Engine

[Desktop App](#desktop-experience) • [CLI Power](#cli-power-user) • [Getting Started](#getting-started) • [Installation](#installation) • [Architecture](#system-architecture)

</div>

---

**CompareIt** redefines how engineers verify data and code. It is a dual-interface **industrial-grade comparison engine**—available as both a high-performance **CLI tool** and a modern **Desktop Application**. Built in Rust, it moves beyond simple line-by-line checks to understand the *structure* of your data and the *context* of your file systems.

Whether you are auditing massive CSV datasets, verifying migration integrity across complex directory trees, or simply checking code changes, CompareIt delivers **automated, deep insights** in seconds.

---

## 🖥️ Desktop Experience

The CompareIt Desktop App (built with Tauri & React) provides a powerful, local-first interface for interactive data auditing.

### Interface & Controls

*   **Path Selection**: Large dedicated drop-zones for "Source (Path A)" and "Target (Path B)" with native folder selection dialogs.
*   **Engine Control Panel**: Collapsible settings to fine-tune the comparison:
    *   **Comparison Mode**: Toggle between `Auto-detect`, `Text (Line-by-line)`, or `Structured (CSV/TSV)`.
    *   **Pairing Strategy**: Choose `Smart Match (All-vs-All)`, `Same Path`, or `Same Name`.
    *   **Data Fine-tuning**: dedicated inputs for `Numeric Tolerance`, `Key Columns (CSV)`, and `Exclude Patterns`.
    *   **Normalization**: Quick-toggles for `Ignore Whitespace`, `Ignore Case`, and `Skip Empty Lines`.

### Real-time Visualization

*   **Live Pipeline Tracking**: A real-time progress bar with stage-by-stage updates (Indexing ➔ Hashing ➔ Matching ➔ Diffing).
*   **KPI Summary Dashboard**: Instant overview of Pairs Compared, Identical vs. Different counts, and Average Similarity scores.
*   **Interactive Result Table**: Sortable rows with visual similarity bars and status badges.
*   **Deep-Dive Detail View**:
    *   **Text Analysis**: Interactive split-view diff preview with syntax highlighting for additions and deletions.
    *   **Structured Audit**: Table of field mismatches showing exact keys, old values, and new values for every discrepancy.

---

## ⌨️ CLI Power User

For automated workflows, CI/CD, and server-side processing, the CompareIt CLI remains the high-performance choice.

### Main Commands

*   **`compare <path1> <path2>`**: Runs the full comparison pipeline.
*   **`report --input <jsonl> --html <output>`**: Generates a self-contained interactive HTML dashboard from raw JSONL results.

### CLI Flags & Arguments

| Flag | Description | Default |
| :--- | :--- | :--- |
| `-m, --mode` | Comparison mode: `auto`, `text`, or `structured`. | `auto` |
| `--pairing` | Folder pairing strategy: `same-path`, `same-name`, `all-vs-all`. | `all-vs-all` |
| `-k, --key` | Comma-separated column names to use as primary keys for CSV matching. | `Column 0` |
| `--numeric-tol` | Tolerance for floating-point equality (e.g., `0.001`). | `0.0001` |
| `--exclude` | Glob patterns for files/folders to ignore (e.g., `*.tmp,node_modules`). | `None` |
| `--ignore-regex` | Regex pattern for content to ignore in text comparison. | `None` |
| `--ignore-all-ws` | Ignore all whitespace differences. | `false` |
| `--ignore-case` | Perform case-insensitive comparison. | `false` |
| `-B, --results-base` | Base directory for automatic results. | `./results` |
| `-v, --verbose` | Enable verbose output (show detailed diffs in terminal). | `false` |

### Command Examples

```bash
# Basic folder comparison with smart matching
CompareIt compare ./folder_v1 ./folder_v2

# Deep CSV audit with specific business keys and numeric tolerance
CompareIt compare ./data_a.csv ./data_b.csv --key "customer_id,transaction_id" --numeric-tol 0.01

# Filtered code comparison ignoring comments
CompareIt compare ./src_old ./src_new --ignore-regex "//.*" --ignore-all-ws

# Generate a standalone HTML report from a previous run
CompareIt report --input results/last_run.jsonl --html my_report.html
```

---

## 🚀 Getting Started

Follow these steps to launch CompareIt for the first time.

### Prerequisites

*   **Rust**: [Install Rust](https://www.rust-lang.org/tools/install) (latest stable recommended).
*   **Node.js & npm**: (For Desktop UI only) [Install Node.js](https://nodejs.org/).
*   **OS Dependencies**: Tauri requires certain system libraries (e.g., `build-essential`, `libwebkit2gtk` on Linux, or C++ build tools on Windows).

### Starting the Desktop UI

1.  **Install root dependencies** (from the project root):
    ```bash
    npm install
    ```
2.  **Install UI dependencies**:
    ```bash
    npm run setup
    ```
3.  **Launch the App in Development Mode**:
    ```bash
    npm run dev
    ```

### Starting the CLI

You can run the CLI directly using cargo:

```bash
# Basic file comparison
cargo run -- compare ./file1.txt ./file2.txt

# Folder comparison with verbose output
cargo run -- compare ./dir1 ./dir2 --verbose
```
*Note: The `--` separates cargo arguments from CompareIt arguments.*

---

## 🛠️ Installation

### Build for Production

To create a standalone executable for your machine:

#### Desktop App

```bash
npm install
npm run setup
npm run build
```
*Binaries will be located in `src-tauri/target/release/bundle/` (e.g., `.msi` on Windows, `.app` or `.dmg` on macOS).*

#### CLI Tool

```bash
cargo install --path .
```
*This compiles the binary in release mode and adds `CompareIt` to your system's PATH.*

---

## 🔄 System Architecture

CompareIt is architected as a shared library (`compare_it`) consumed by both the Tauri backend and the CLI wrapper.

```mermaid
graph TD
    CLI([CompareIt CLI]) --> Engine[Core Library Engine]
    UI([CompareIt Desktop]) --> Bridge[Tauri Bridge]
    Bridge --> Engine
    
    subgraph Engine [The Comparison Pipeline]
        Engine --> Index[1. Indexing & Glob Filtering]
        Index --> Fingerprint[2. Blake3 & Simhash Hashing]
        Fingerprint --> Match[3. Smart Candidate Selection]
        Match --> Exact[4. Parallel Exact Comparison]
    end
    
    subgraph Output [Actionable Insights]
        Exact --> Dashboard[Interactive HTML Report]
        Exact --> Artifacts[Patches & Mismatch JSONs]
        Exact --> Live[Real-time UI / CLI Stream]
    end
```

---

## 🧠 The CompareIt Engine

### 1. Fingerprinting (Blake3 & Simhash)

*   **Blake3**: High-speed cryptographic hashing for identifying 100% identical files instantly.
*   **Simhash**: Locality-sensitive hashing that allows the engine to find "moved or renamed" files by measuring content similarity in O(1) time.

### 2. Structural Intelligence (CSV/TSV)

*   **Record Alignment**: Matches rows based on unique business keys, making the comparison immune to row order changes.
*   **Field-Level Auditing**: Performs precision-aware comparisons for numeric data and character-level similarity for text fields.
*   **Schema Detection**: Automatically identifies and reports missing or added columns across files.

### 3. Parallel Execution (Rayon)

*   Utilizes a work-stealing scheduler to saturate all CPU cores, processing millions of records or thousands of files with sub-second latency.

---

## 📊 Artifacts & Exports

CompareIt believes in **Persistent Observability**. Every run generates a timestamped results directory:

*   **`report.html`**: The primary exploration tool—a searchable, interactive dashboard.
*   **`results.jsonl`**: Machine-readable stream of every single comparison result.
*   **`artifacts/mismatches/`**: Detailed JSON logs of every structured data field that failed verification.
*   **`artifacts/patches/`**: Standard unified diff files compatible with `patch` or `git`.

---

## ❓ FAQ

### How do I use the library in my own project?

CompareIt is a standard Rust crate. Add it to your `Cargo.toml` and use the `ComparisonEngine` struct to run comparisons programmatically with custom `ProgressReporter` implementations.

### Does it support binary files?

Yes, it performs a high-speed `Blake3` hash comparison for binary assets and reports them as either identical or different.

### Can it handle files larger than RAM?

Yes. CompareIt uses streaming readers for CSV/TSV data and memory-efficient indexing to process multi-gigabyte files without crashing.

---

<div align="center">

*Built with precision by the CompareIt Engineering Team.*

</div>
