# CompareIt 🚀

**Industrial-Grade File Intelligence & Automated Comparison Engine**

CompareIt is a high-performance, parallelized comparison engine built in Rust, designed for deep analysis of massive datasets and complex directory structures. It moves beyond simple line-by-line diffing to provide **structural awareness**, **intelligent matching**, and **automated reporting** for the modern engineering workflow.

---

## 💎 Why CompareIt?

*   🚀 **Extreme Performance**: Leveraging Rust's memory safety and `Rayon` for multi-core parallel processing.
*   📊 **Structural Intelligence**: Deep audit of CSV/TSV data with key-based record matching, schema validation, and numeric tolerance.
*   🔍 **All-vs-All Smart Matching**: Proprietary fingerprinting (Blake3 + Simhash) automatically pairs similar files across directories, even if names differ.
*   📈 **Automated Insight Delivery**: Zero-config generation of interactive HTML dashboards, JSONL datasets, and terminal-optimized summaries.
*   🛠️ **Customizable Workspace**: Full control over results placement and reporting artifacts.

---

## 🔄 Workflow Architecture

```mermaid
graph TD
    CLI([User CLI Command]) --> Config[Configuration Parsing]
    Config --> Results[Results Dir Initialization]
    
    subgraph Engine [The CompareIt Core]
        Results --> Index[1. Indexing & Glob Filtering]
        Index --> Fingerprint[2. Blake3 & Simhash Fingerprinting]
        Fingerprint --> Match[3. Smart Candidate Generation]
        Match --> Exact[4. Parallel Exact Comparison]
    end
    
    subgraph Output [Insight Delivery]
        Exact --> Summary[Terminal: Executive Summary]
        Exact --> Detailed[Terminal: Field-Level Audit]
        Exact --> JSONL[Export: Machine-Readable JSONL]
        Exact --> HTML[Report: Interactive HTML Dashboard]
    end
    
    Summary --> Success([Complete])
    HTML --> Success
```

---

## 🛠️ Installation

### Quick Start (PowerShell / Bash)

**Windows:**
```powershell
# Install Rust
winget install Rustlang.Rustup
# Build and install CompareIt
cargo install --path .
```

**macOS / Linux:**
```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# Build and install CompareIt
cargo install --path .
```

---

## 🚀 Command Mastery

CompareIt is built to be intuitive but powerful. Use the `compare` command for primary operations.

### Basic Comparison
Compare two files or folders with auto-detection of content types.
```bash
CompareIt compare ./old_version ./new_version
```

### Deep CSV/TSV Audit
Match records by unique keys, ignore noise, and set numeric tolerances.
```bash
CompareIt compare data1.csv data2.csv -k "id,email" --ignore-columns "timestamp" --numeric-tol 0.001
```

### Result Management
Direct all output artifacts to a specific location.
```bash
CompareIt compare dir1 dir2 -B C:\audits\january_run
```

### Power User Flags

| Flag | Shortcut | Description | Default |
| :--- | :--- | :--- | :--- |
| `--mode` | `-m` | Force mode: `auto`, `text`, `structured` | `auto` |
| `--results-base`| `-B` | Custom directory for results and reports | `../results` |
| `--key` | `-k` | Key columns for record matching (CSV/TSV) | First Column |
| `--numeric-tol` | | Float comparison tolerance | `0.0001` |
| `--pairing` | | Folder strategy: `all-vs-all`, `same-path`, `same-name` | `all-vs-all` |
| `--exclude` | | Glob patterns to skip (e.g., `"*.tmp"`) | None |
| `--verbose` | `-v` | Show detailed terminal value samples | `false` |

---

## 📊 Automated Reporting

CompareIt eliminates the need for manual output redirection. Every run produces:

1.  **Terminal Executive Summary**: A high-level view of similarity, matched pairs, and status.
2.  **Structured Data Analysis**: Inline view of schema changes and sample field mismatches.
3.  **JSONL Dataset**: A full machine-readable export of every comparison result.
4.  **Interactive HTML Dashboard**: A self-contained, searchable report with detailed diff viewers and mismatch tables.

---

## 📈 Performance Benchmarks

Engineered for scale on an 8-core machine:
*   **1,000 File Pairs**: < 3.0 Seconds
*   **10,000 File Pairs**: < 25.0 Seconds
*   **High-Volume Structured Audit**: ~15,000 records/second

---

## 🧪 Development & Quality

For contributors and advanced users, see our [Development Guide](DEVELOPMENT.md) for details on:
*   Unit and Integration Testing
*   Performance Benchmarking with Criterion
*   Code Quality Standards (Clippy & Rustfmt)

---

*Built with ❤️ by the CompareIt Engineering Team.*
