# CompareIt Technical Architecture & File Details

This document provides a technical breakdown of the `CompareIt` codebase, explaining the responsibility and purpose of each module, the folder structure, and the logic behind the system's design.

---

## 📂 Project Structure

```text
CompareIt/
├── src/                # Core Engine (Rust Library & CLI)
├── src-tauri/          # Desktop Backend (Rust/Tauri)
├── ui/                 # Desktop Frontend (React/TypeScript)
├── results/            # Auto-generated reports and artifacts
├── tests/              # Integration tests
├── Cargo.toml          # Workspace configuration
└── DETAILS.md          # Technical documentation (this file)
```

---

## ⚙️ Core Engine Structure (`src/`)

The core engine is built as a highly optimized Rust library designed for massive parallelism.

### 1. `lib.rs`
*   **What it does:** Acts as the public API for the `compare_it` crate. It defines the `ComparisonEngine` orchestrator and the `ProgressReporter` trait.
*   **Why it exists:** To allow different interfaces (CLI, UI, or 3rd-party scripts) to trigger comparisons with consistent logic while providing their own progress visualization.

### 2. `main.rs`
*   **What it does:** The entry point for the Command Line Interface (CLI). It uses `clap` for argument parsing and `indicatif` for terminal progress bars.
*   **Why it exists:** To provide a headless, scriptable version of the engine that can be used in terminals or automated pipelines.

### 3. `types.rs`
*   **What it does:** Defines all shared data structures (structs and enums) used throughout the pipeline.
*   **Why it exists:** To provide a "Single Source of Truth." By centralizing types like `FileEntry` and `ComparisonResult`, we ensure that hashing, diffing, and reporting stay in sync.

### 4. `index.rs`
*   **What it does:** Recursively crawls directories, gathers file metadata (size, extension), and filters files based on glob patterns.
*   **Why it exists:** To build an initial "map" of the file system before any expensive processing happens, allowing for early exclusion of irrelevant data.

### 5. `fingerprint.rs`
*   **What it does:** Implements multi-threaded hashing. It calculates `Blake3` for exact matches and `Simhash` for fuzzy similarity.
*   **Why it exists:** To enable "Smart Matching." `Simhash` allows the engine to find renamed or slightly modified files without doing a full line-by-line comparison first.

### 6. `match_files.rs`
*   **What it does:** Takes the indexed files and creates "Candidate Pairs" based on the chosen strategy (SamePath, SameName, or All-vs-All).
*   **Why it exists:** To prune the comparison tree. It uses fingerprints to decide which files are worth comparing, preventing unnecessary work on unrelated files.

### 7. `compare_text.rs`
*   **What it does:** Performs line-by-line comparison using a Myers diff algorithm. It applies normalization (whitespace, case) and regex filtering.
*   **Why it exists:** To provide standard text diffing functionality for code, config files, and logs.

### 8. `compare_structured.rs`
*   **What it does:** Parses CSV/TSV data into rows, aligns them by primary keys, and performs cell-level auditing.
*   **Why it exists:** Because data files shouldn't be compared like code. This module handles row reordering and numeric precision drift (`0.999` vs `1.000`).

### 9. `export.rs`
*   **What it does:** Handles the serialization of results into machine-readable formats like `JSONL` and `CSV`.
*   **Why it exists:** To allow users to pipe CompareIt results into other tools (like Excel, Splunk, or ELK) for further analysis.

### 10. `report.rs`
*   **What it does:** Generates a standalone, interactive HTML dashboard using an embedded template.
*   **Why it exists:** To provide a human-friendly way to browse thousands of comparison results, complete with searchable tables and visual diffs.

---

## 🖥️ Desktop Backend (`src-tauri/`)

Tauri handles the native OS integration and bridges the Rust engine to the Webview.

### 1. `src/main.rs`
*   **What it does:** Bootstraps the Tauri application and defines the `run_comparison` async command.
*   **Why it exists:** To act as the bridge. It receives JSON-serialized configs from React, calls the `src/` engine, and streams progress events back to the UI.

### 2. `build.rs`
*   **What it does:** A standard Rust build script that invokes `tauri-build`.
*   **Why it exists:** To compile the Tauri-specific assets and ensure the native windowing environment is correctly initialized during the build process.

### 3. `tauri.conf.json`
*   **What it does:** The primary configuration file for the Tauri app, defining window size, allowed file system paths, and security permissions.
*   **Why it exists:** To define the application's "Capabilities" and ensure it has the necessary permissions to read the user's files safely.

---

## 🎨 Desktop Frontend (`ui/`)

The frontend is a modern React application optimized for performance and large data displays.

### 1. `src/main.tsx`
*   **What it does:** The entry point for the React application, mounting the `App` component to the DOM.
*   **Why it exists:** Standard boilerplate for modern web apps to initialize the React runtime.

### 2. `src/App.tsx`
*   **What it does:** Contains the core UI logic, including state management for paths, settings, and the results dashboard.
*   **Why it exists:** It is the "brain" of the UI, coordinating user interactions and calling the Tauri commands.

### 3. `src/index.css`
*   **What it does:** Defines global Tailwind CSS styles and custom theme variables (like the "Engineering Dark" palette).
*   **Why it exists:** To provide a consistent, high-contrast visual experience across the entire application.

### 4. `vite.config.ts`
*   **What it does:** Configures the Vite build tool, tailored for Tauri (fixed ports, HMR settings).
*   **Why it exists:** Ensures a smooth development experience with fast hot-module-reloading and correct asset bundling.

### 5. `package.json`
*   **What it does:** Lists all frontend dependencies (React, Tailwind, Tauri API) and defines build scripts.
*   **Why it exists:** Standard dependency management for the Node.js ecosystem.

---

## 🛠️ Workspace & Configs

### 1. `Cargo.toml` (Root)
*   **What it does:** Defines the Rust workspace, linking the core engine and the Tauri project.
*   **Why it exists:** To manage dependencies across both Rust projects in a single place, ensuring version consistency.

### 2. `DEVELOPMENT.md`
*   **What it does:** Provides detailed instructions on setting up the environment, running tests, and contributing.
*   **Why it exists:** To onboard new developers and maintain high code quality through standardized workflows.

### 3. `UI_IMPLEMENTATION_PLAN.md`
*   **What it does:** Outlines the design philosophy and roadmap for the desktop interface.
*   **Why it exists:** Acts as a reference for the design system and future feature implementations.

---

## 🚀 The Pipeline: How It Works

1.  **Configuration:** The user specifies paths and settings (CLI flags or UI toggles).
2.  **Indexing:** The system scans the folders and builds a file list (`index.rs`).
3.  **Fingerprinting:** Every file is hashed in parallel using all available CPU cores (`fingerprint.rs`).
4.  **Matching:** The system pairs files based on name, path, or content similarity (`match_files.rs`).
5.  **Comparison:** 
    *   **Text:** Myers diff for source code (`compare_text.rs`).
    *   **Structured:** Key-based record matching for CSVs (`compare_structured.rs`).
6.  **Reporting:** Results are saved to disk (`export.rs`) and displayed via the dashboard (`report.rs` or `App.tsx`).
