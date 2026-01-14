# CompareIt

A high-performance file comparison tool built in Rust. CompareIt provides fast, accurate comparison of files and folders with support for both text-based diffs and structured CSV/TSV comparisons.

## Features

- **Blazing Fast**: Parallel processing using Rayon for multi-core utilization
- **Multiple Comparison Modes**:
  - Text/line-based diff (like unified diff)
  - Structured CSV/TSV with key-based record comparison
  - Binary file hash comparison
- **Smart Folder Matching**: All-vs-all pairing with fingerprint-based candidate pruning
- **Flexible Filtering**:
  - Exclude files/folders by glob pattern
  - Ignore columns in structured comparison
  - Filter text with regex patterns
- **Rich Metrics**:
  - Similarity scores (diff-based or Jaro-Winkler)
  - Line/record counts
  - Detailed position tracking
- **Multiple Export Formats**: JSONL, CSV, HTML reports with interactive diff viewer
- **Zero Dependencies for Users**: Single executable, no runtime required

## Installation

### Pre-built Binaries

Download the latest release for your platform:
- Windows: `CompareIt.exe`
- macOS: `CompareIt`
- Linux: `CompareIt`

### Build from Source

#### Install Rust (if not already installed)

**Windows:**
```powershell
# Download and run the Rust installer
winget install Rustlang.Rustup

# Or download manually from https://rustup.rs/
# Then restart your terminal
```

**macOS/Linux:**
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

#### Build CompareIt

```bash
# Navigate to the project
cd C:\CompareIt

# Build release binary (optimized)
cargo build --release

# Binary will be at:
#   Windows: target/release/CompareIt.exe
#   macOS/Linux: target/release/CompareIt
```

## Usage

### Compare Two Files

```bash
# Basic comparison
CompareIt compare file1.txt file2.txt

# With CSV export
CompareIt compare file1.txt file2.txt --out-csv results.csv

# Structured CSV comparison with key columns
CompareIt compare data1.csv data2.csv --mode structured --key id,date
```

### Compare Two Folders

```bash
# All-vs-all matching (finds best matches)
CompareIt compare ./folder1 ./folder2 --pairing all-vs-all --topk 3

# Same filename matching only
CompareIt compare ./folder1 ./folder2 --pairing same-name

# Exclude common unwanted directories
CompareIt compare ./dir1 ./dir2 --exclude ".git,node_modules,target,*.tmp"

# Export everything
CompareIt compare ./dir1 ./dir2 --out-jsonl results.jsonl --out-csv summary.csv --out-dir artifacts/
```

### Generate HTML Report

```bash
# Generate from JSONL results
CompareIt report --input results.jsonl --html report.html

# With artifact links
CompareIt report --input results.jsonl --html report.html --artifacts ./artifacts
```

### Advanced Options

```bash
# Text normalization options
CompareIt compare file1.txt file2.txt \
  --ignore-trailing-ws \
  --ignore-case \
  --skip-empty-lines

# Ignore dynamic content (timestamps, IDs) with regex
CompareIt compare log1.txt log2.txt \
  --ignore-regex "\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}"

# Structured comparison with column exclusion
CompareIt compare data1.csv data2.csv \
  --mode structured \
  --key invoice_id \
  --ignore-columns updated_at,created_at,id \
  --numeric-tol 0.001

# Alternative similarity scoring
CompareIt compare file1.txt file2.txt --similarity char-jaro

# Limit comparison scope
CompareIt compare ./dir1 ./dir2 --max-pairs 100 --topk 1
```

## CLI Reference

### `compare` subcommand

| Option | Description | Default |
|--------|-------------|---------|
| `--mode` | Comparison mode: `auto`, `text`, `structured` | `auto` |
| `--pairing` | Folder pairing: `same-path`, `same-name`, `all-vs-all` | `all-vs-all` |
| `--topk` | Top-K candidates per file | `3` |
| `--max-pairs` | Maximum pairs to compare | unlimited |
| `--key` | Key columns for structured mode (comma-separated) | first column |
| `--numeric-tol` | Numeric tolerance for structured comparison | `0.0001` |
| `--similarity` | Algorithm: `diff`, `char-jaro` | `diff` |
| `--exclude` | Glob patterns for files/folders to exclude (comma-separated) | none |
| `--ignore-columns` | Columns to skip in structured comparison (comma-separated) | none |
| `--ignore-regex` | Regex pattern for content to ignore in text comparison | none |
| `--ignore-eol` | Normalize line endings | false |
| `--ignore-trailing-ws` | Ignore trailing whitespace | false |
| `--ignore-all-ws` | Ignore all whitespace differences | false |
| `--ignore-case` | Case-insensitive comparison | false |
| `--skip-empty-lines` | Skip empty lines | false |
| `--max-diff-bytes` | Max bytes for detailed diff | 1MB |
| `--out-jsonl` | Output JSONL file path | none |
| `--out-csv` | Output CSV summary path | none |
| `--out-dir` | Output directory for artifacts | none |
| `--verbose` | Show all results and diff snippets | false |

### `report` subcommand

| Option | Description |
|--------|-------------|
| `--input` | Input JSONL file with comparison results |
| `--html` | Output HTML report path |
| `--artifacts` | Path to artifacts directory (for linking) |

## Output Formats

### JSONL (Streaming)
One JSON object per line, ideal for large result sets:
```json
{"type":"Text","linked_id":"abc123:def456","file1_path":"...","similarity_score":0.95,...}
```

### CSV Summary
Flattened format for spreadsheet analysis:
```csv
linked_id,file1_path,file2_path,type,similarity_score,identical,common,only_in_file1,only_in_file2
```

### HTML Report
Self-contained HTML featuring:
- **Dashboard**: Pie chart showing status distribution
- **Sortable Table**: Filter and sort results
- **Embedded Diff Viewer**: Side-by-side diff modal with syntax highlighting
- **Structured Diff**: Table view of mismatched fields for CSV comparisons

### Artifacts
- `patches/<linked_id>.diff` - Unified diff patches for text comparisons
- `mismatches/<linked_id>.json` - Detailed mismatch data for structured comparisons

## Metrics

### Text Mode
- `similarity_score`: `common_lines / (common + only_in_file1 + only_in_file2)`
- `common_lines`: Lines present in both files
- `only_in_file1/2`: Lines unique to each file
- `different_positions`: Range-encoded indices of differences

### Structured Mode
- `similarity_score`: `common_records / total_unique_records`
- `common_records`: Records (by key) present in both files
- `field_mismatches`: Per-column mismatch counts with samples

## Linked ID

Each comparison result includes a `linked_id` field. This is a stable identifier generated by combining truncated content hashes of both files (`file1_hash:file2_hash`). It serves several purposes:

1. **Artifact Linking**: Connects JSONL/CSV results to patch files and mismatch JSON
2. **Reproducibility**: Same files always produce the same ID
3. **Cross-Reference**: Link results across different output formats

## Performance

CompareIt uses several strategies for high performance:

1. **Parallel Processing**: All CPU-intensive operations use Rayon
2. **Fingerprint Pruning**: Blake3 hashing + simhash for O(1) similarity estimation
3. **Candidate Filtering**: Size ratio and extension blocking rules
4. **Streaming Exports**: JSONL format for memory-efficient large exports

### Blocking Rules

When using `all-vs-all` pairing, CompareIt applies "blocking rules" to avoid comparing obviously incompatible files:

- **Extension Compatibility**: Only compare files with compatible extensions (e.g., `.csv` with `.tsv`, `.rs` with `.py`)
- **Size Ratio**: Skip pairs where file sizes differ by more than 10x
- **File Type**: Binary files only match other binary files

### Simhash Fingerprinting

For similarity estimation, CompareIt uses [simhash](https://en.wikipedia.org/wiki/SimHash) - a locality-sensitive hashing technique. Files with similar content produce similar hash values, enabling fast approximate similarity scoring without reading entire file contents. This allows efficient pruning in all-vs-all matching scenarios.

Typical performance on a modern machine:
- 1,000 file pairs: ~2-5 seconds
- 10,000 file pairs: ~20-60 seconds
- Structured comparison: ~10,000 rows/second per file

## Logging

CompareIt uses the `env_logger` crate. Set the `RUST_LOG` environment variable to control log output:

```bash
# Show warnings (default)
RUST_LOG=warn CompareIt compare ./dir1 ./dir2

# Show info messages
RUST_LOG=info CompareIt compare ./dir1 ./dir2

# Debug mode (verbose)
RUST_LOG=debug CompareIt compare ./dir1 ./dir2
```
