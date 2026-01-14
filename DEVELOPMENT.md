# CompareIt Development Guide

This document outlines the development practices, workflows, and standards for contributing to CompareIt.

---

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Development Setup](#development-setup)
3. [Code Quality](#code-quality)
4. [Testing Strategy](#testing-strategy)
5. [Performance Benchmarking](#performance-benchmarking)
6. [Documentation](#documentation)
7. [Release Process](#release-process)
8. [CI/CD Pipeline](#cicd-pipeline)

---

## Prerequisites

- **Rust 1.70+** (latest stable recommended)
- **Cargo** (included with Rust)
- **Git** for version control

### Recommended Tools

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install additional components
rustup component add clippy rustfmt

# Install cargo-watch for auto-rebuild during development
cargo install cargo-watch

# Install cargo-audit for security vulnerability scanning
cargo install cargo-audit
```

---

## Development Setup

### Clone and Build

```bash
git clone https://github.com/your-org/CompareIt.git
cd CompareIt

# Debug build (fast compilation, slower runtime)
cargo build

# Release build (slower compilation, optimized runtime)
cargo build --release
```

### Development Workflow

Use `cargo-watch` for automatic rebuilds during development:

```bash
# Auto-rebuild on file changes
cargo watch -x check

# Auto-run tests on file changes
cargo watch -x test

# Auto-run the application on file changes
cargo watch -x "run -- compare file1.csv file2.csv"
```

---

## Code Quality

### Formatting

All code must be formatted with `rustfmt`. Run before every commit:

```bash
# Format all source files
cargo fmt

# Check formatting without modifying files (useful for CI)
cargo fmt --check
```

### Linting

Use `clippy` to catch common mistakes and improve code quality:

```bash
# Run clippy with all warnings
cargo clippy -- -W clippy::all

# Run clippy with pedantic lints (stricter)
cargo clippy -- -W clippy::pedantic

# Fix automatically fixable issues
cargo clippy --fix
```

### Pre-commit Checklist

Before committing code, ensure:

1. `cargo fmt` - Code is formatted
2. `cargo clippy` - No warnings
3. `cargo test` - All tests pass
4. `cargo doc` - Documentation builds without warnings

---

## Testing Strategy

### Test Organization

```
CompareIt/
├── src/
│   ├── main.rs
│   ├── compare_text.rs      # Unit tests at bottom of file
│   ├── compare_structured.rs
│   └── ...
└── tests/
    ├── integration_text.rs   # Integration tests for text comparison
    ├── integration_csv.rs    # Integration tests for CSV comparison
    └── fixtures/
        ├── sample1.csv
        ├── sample2.csv
        └── ...
```

### Unit Tests

Place unit tests in the same file as the code they test:

```rust
// src/compare_text.rs

pub fn compare_lines(a: &str, b: &str) -> f64 {
    // implementation
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_lines() {
        assert_eq!(compare_lines("hello", "hello"), 1.0);
    }

    #[test]
    fn test_different_lines() {
        assert!(compare_lines("hello", "world") < 1.0);
    }

    #[test]
    fn test_empty_lines() {
        assert_eq!(compare_lines("", ""), 1.0);
    }
}
```

### Integration Tests

Place integration tests in the `tests/` directory:

```rust
// tests/integration_csv.rs

use std::process::Command;

#[test]
fn test_csv_comparison_identical() {
    let output = Command::new("cargo")
        .args(["run", "--release", "--", "compare", 
               "tests/fixtures/identical1.csv", 
               "tests/fixtures/identical2.csv"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Identical"));
}

#[test]
fn test_csv_comparison_different() {
    let output = Command::new("cargo")
        .args(["run", "--release", "--", "compare",
               "tests/fixtures/different1.csv",
               "tests/fixtures/different2.csv"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Different"));
}
```

### Running Tests

```bash
# Run all tests
cargo test

# Run tests with output (see println! statements)
cargo test -- --nocapture

# Run specific test
cargo test test_csv_comparison

# Run tests in a specific module
cargo test compare_text::

# Run tests with verbose output
cargo test -- --test-threads=1
```

---

## Performance Benchmarking

### Setup Criterion

Add to `Cargo.toml`:

```toml
[dev-dependencies]
criterion = "0.5"

[[bench]]
name = "comparison_benchmarks"
harness = false
```

### Writing Benchmarks

Create `benches/comparison_benchmarks.rs`:

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use compare_it::compare_text::compare_text_files;
use compare_it::fingerprint::compute_simhash;

fn benchmark_simhash(c: &mut Criterion) {
    let content = "a]n".repeat(10000);
    
    c.bench_function("simhash_10k_chars", |b| {
        b.iter(|| compute_simhash(black_box(&content)))
    });
}

fn benchmark_text_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("text_comparison");
    
    for size in [100, 1000, 10000].iter() {
        group.bench_with_input(
            BenchmarkId::new("lines", size),
            size,
            |b, &size| {
                let content1 = (0..size).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
                let content2 = content1.clone();
                b.iter(|| compare_text(black_box(&content1), black_box(&content2)))
            },
        );
    }
    group.finish();
}

criterion_group!(benches, benchmark_simhash, benchmark_text_comparison);
criterion_main!(benches);
```

### Running Benchmarks

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench simhash

# Generate HTML report
cargo bench -- --save-baseline main
```

---

## Documentation

### Inline Documentation

Document all public items using rustdoc comments:

```rust
/// Compares two text files line-by-line using the Myers diff algorithm.
///
/// # Arguments
///
/// * `file1` - Reference to the first file entry
/// * `file2` - Reference to the second file entry
/// * `config` - Comparison configuration options
///
/// # Returns
///
/// Returns a `TextComparisonResult` containing:
/// - Line counts for both files
/// - Number of common and unique lines
/// - Similarity score (0.0 to 1.0)
/// - Detailed diff output (if under size limit)
///
/// # Errors
///
/// Returns an error if either file cannot be read or has encoding issues.
///
/// # Example
///
/// ```rust
/// let result = compare_text_files(&file1, &file2, &config)?;
/// println!("Similarity: {:.1}%", result.similarity_score * 100.0);
/// ```
pub fn compare_text_files(
    file1: &FileEntry,
    file2: &FileEntry,
    config: &CompareConfig,
) -> Result<TextComparisonResult> {
    // implementation
}
```

### Building Documentation

```bash
# Build documentation
cargo doc

# Build and open in browser
cargo doc --open

# Include private items (for internal development)
cargo doc --document-private-items
```

---

## Release Process

### Version Bumping

1. Update version in `Cargo.toml`
2. Update `CHANGELOG.md` (if maintained)
3. Commit with message: `chore: bump version to X.Y.Z`
4. Tag the release: `git tag vX.Y.Z`

### Building Release Binaries

```bash
# Build optimized release binary
cargo build --release

# Binary location
# Windows: target/release/CompareIt.exe
# Linux/macOS: target/release/CompareIt
```

### Cross-Compilation (Optional)

For multi-platform releases:

```bash
# Install cross-compilation targets
rustup target add x86_64-unknown-linux-gnu
rustup target add x86_64-apple-darwin
rustup target add x86_64-pc-windows-msvc

# Build for specific target
cargo build --release --target x86_64-unknown-linux-gnu
```

---

## CI/CD Pipeline

### GitHub Actions Example

Create `.github/workflows/ci.yml`:

```yaml
name: CI

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always

jobs:
  check:
    name: Check
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo check

  fmt:
    name: Format
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - run: cargo fmt --check

  clippy:
    name: Clippy
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - run: cargo clippy -- -D warnings

  test:
    name: Test
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test

  security:
    name: Security Audit
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo install cargo-audit
      - run: cargo audit

  release:
    name: Release Build
    needs: [check, fmt, clippy, test]
    runs-on: ${{ matrix.os }}
    if: github.ref == 'refs/heads/main'
    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
            artifact: CompareIt
          - os: windows-latest
            target: x86_64-pc-windows-msvc
            artifact: CompareIt.exe
          - os: macos-latest
            target: x86_64-apple-darwin
            artifact: CompareIt
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo build --release
      - uses: actions/upload-artifact@v4
        with:
          name: CompareIt-${{ matrix.target }}
          path: target/release/${{ matrix.artifact }}
```

---

## Quick Reference

| Task | Command |
|------|---------|
| Build (debug) | `cargo build` |
| Build (release) | `cargo build --release` |
| Run | `cargo run -- compare file1 file2` |
| Test | `cargo test` |
| Format | `cargo fmt` |
| Lint | `cargo clippy` |
| Docs | `cargo doc --open` |
| Benchmark | `cargo bench` |
| Watch | `cargo watch -x check` |
| Audit | `cargo audit` |

---

*Last updated: January 2026*
