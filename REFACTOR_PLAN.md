# Refactoring & Optimization Plan for CompareIt

## 1. Executive Summary

This document outlines a comprehensive plan to refactor, debug, and enhance the `CompareIt` application. The review of the codebase identified three critical areas for improvement:
1.  **Frontend Architecture**: The React application is currently monolithic (`App.tsx`), making it hard to maintain and extend.
2.  **Core Engine Scalability**: The current implementation loads entire files into memory, which contradicts the documentation's claim of "streaming" and "memory-efficient" processing for multi-gigabyte files. This poses a significant crash risk (OOM).
3.  **Testing & Validation**: The project lacks an integration testing suite (`tests/` directory is missing), and there is no automated type synchronization between Rust and TypeScript.

## 2. Phase I: Core Engine Refactoring (Rust)

**Goal**: Align implementation with documentation claims (streaming/low-memory) and ensure robustness.

### 2.1. Implement True Streaming for Structured Comparison
*   **Current State**: `compare_structured.rs` loads all CSV records into a `HashMap<String, HashMap<String, String>>`. This is O(N) space complexity.
*   **Refactor Plan**:
    1.  **External Sort**: Implement or utilize an external sorting algorithm (e.g., merge sort on disk) to sort both input files by their Key Columns before comparison.
    2.  **Streaming Merge Join**: Once sorted, use a streaming iterator to read both files line-by-line and compare records.
        *   *Benefit*: Reduces memory usage from O(File Size) to O(Buffer Size).
    3.  **Update `compare_structured.rs`**: Replace `parse_structured_file` (which returns a HashMap) with a `StructuredReader` iterator.

### 2.2. Optimize Text Comparison
*   **Current State**: `compare_text.rs` uses `std::fs::read_to_string`, loading the full file into RAM.
*   **Refactor Plan**:
    1.  **Chunked Reading**: For files larger than a certain threshold (e.g., 100MB), switch to a chunked comparison or a hash-block comparison to identify changed regions before loading detailed diffs.
    2.  **Myers Diff Optimization**: The `similar` crate is fast, but diffing huge files is expensive. Limit the "detailed diff" generation to a specific window size or strictly enforce `max_diff_bytes` *before* loading the text.

### 2.3. Add Integration Tests
*   **Current State**: Missing `tests/` directory.
*   **Refactor Plan**:
    1.  Create `tests/integration_tests.rs`.
    2.  Add test cases for:
        *   **CLI End-to-End**: Run the binary against sample data artifacts.
        *   **Large File Simulation**: Test with generated large CSVs (e.g., 1GB+) to verify memory usage.
        *   **Edge Cases**: Empty files, files with only headers, binary files renamed as CSV.

## 3. Phase II: Frontend Architecture (React/Tauri)

**Goal**: Modularize the UI for maintainability and separation of concerns.

### 3.1. Component Decomposition
*   **Current State**: `App.tsx` contains all layout, state, and logic (~600 lines).
*   **Refactor Plan**:
    *   Create `ui/src/components/`:
        *   `layout/Sidebar.tsx`: Navigation and File Selection (Dropzones).
        *   `layout/Header.tsx`: Branding and status.
        *   `settings/SettingsPanel.tsx`: The accordion for advanced settings.
        *   `dashboard/SummaryCards.tsx`: The top-level KPI metrics.
        *   `dashboard/ResultsTable.tsx`: The main list of comparison results.
        *   `details/FileDetailView.tsx`: The modal/panel for viewing specific file diffs.
        *   `details/DiffViewer.tsx`: Specialized component for rendering text diffs (syntax highlighting).
        *   `details/StructuredDiff.tsx`: Specialized component for rendering CSV mismatches.

### 3.2. Custom Hooks & Logic Extraction
*   **Refactor Plan**:
    *   Create `ui/src/hooks/useComparison.ts`: Encapsulate the `run_comparison` Tauri invoke, loading states, and error handling.
    *   Create `ui/src/hooks/useProgress.ts`: Encapsulate the event listener for `compare-progress`.

### 3.3. Type Safety Enhancements
*   **Current State**: Types are manually defined in `App.tsx`.
*   **Refactor Plan**:
    *   Create `ui/src/types/index.ts`.
    *   **Action**: Move all interfaces (`ComparisonResult`, `CompareConfig`, etc.) to this file.
    *   **Future Proofing**: Investigate `ts-rs` to automatically generate `ui/src/types/generated.ts` from Rust structs during build.

## 4. Phase III: Project Structure & Quality Assurance

### 4.1. Code Organization
*   **Refactor Plan**:
    *   **Shared Logic**: Ensure `src/lib.rs` strictly separates the "Library" logic from "CLI" logic. `main.rs` (CLI) should just be a consumer of `lib.rs`, similar to how `src-tauri` is a consumer.
    *   **Error Handling**: Create a dedicated `errors.rs` in `src/` using `thiserror`. Define strict error types (e.g., `IoError`, `ParseError`, `MemoryLimitExceeded`) instead of generic `anyhow::Result` for library APIs. This helps the UI handle specific errors gracefully.

### 4.2. Developer Experience
*   **Refactor Plan**:
    *   Add `scripts/generate_test_data.sh`: A script to generate dummy CSV/Text files for testing.
    *   Update `AGENTS.md` or `DEVELOPMENT.md`: Document the new architecture and testing requirements.

## 5. Detailed Task List

1.  [ ] **Rust**: Create `tests/` and add initial integration tests.
2.  [ ] **Rust**: Refactor `src/types.rs` to prepare for streaming (add Iterator traits if needed).
3.  [ ] **Rust**: Rewrite `compare_structured.rs` to use streaming iterators + external sort (or warn on large files if full streaming is out of scope for v1).
4.  [ ] **UI**: Extract `ui/src/types/index.ts`.
5.  [ ] **UI**: Split `App.tsx` into components in `ui/src/components/`.
6.  [ ] **UI**: Implement `useComparison` hook.
7.  [ ] **Docs**: Update `DETAILS.md` to accurately reflect the memory model (or update the model to match docs).

## 6. Verification Plan

After refactoring, the following checks must be performed:
1.  **Regression Test**: Run `cargo test` and ensure all existing unit tests pass.
2.  **Integration Test**: Run the new integration suite with `cargo test --test integration_tests`.
3.  **UI Smoke Test**: Launch `npm run tauri dev` and verify the file picker, settings, and result display work identical to before.
4.  **Performance Test**: Compare two 500MB CSV files. Monitor RAM usage. It should stay stable (if streaming is implemented) or at least not crash.
