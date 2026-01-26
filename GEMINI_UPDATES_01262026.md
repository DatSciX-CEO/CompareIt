# Gemini Updates - Detailed Implementation Reference
**Date:** January 26, 2026
**Previous Version:** January 22, 2026

This document serves as the **Master Plan** for optimizing `CompareIt` to handle massive files (700MB+) while maintaining the strict exactness required for legal cases.

## 1. The Core Constraints
### A. Performance Constraint (The "700MB Problem")
*   **Issue:** The application crashes or freezes on files >500MB.
*   **Root Cause (Text):** Loading `lines.join("\n")` creates a massive contiguous string in memory, causing OOM and cache thrashing.
*   **Root Cause (CSV):** `HashMap`-based comparison has ~10x memory overhead and random access patterns that stall the CPU.

### B. Legal Constraint (The "Exactness Problem")
*   **Requirement:** Comparisons must be **forensically precise**.
*   **Strict Order:** "Alice sued Bob" must NOT match "Bob sued Alice".
*   **Conclusion:** We cannot use "Bag of Words" or approximate vectors. We must use **Strict Line-by-Line (Vector) Comparison**.

---

## 2. The Solution: Vector & Streaming Architecture

We are shifting the architecture from "Load & Compare" to "Stream/Slice & Compare".

### A. Text Files: Vector/Slice Comparison
Instead of treating the file as one giant text blob, we treat it as a **Vector of Lines**.

*   **Old Way (Bad):** `String` (700MB) -> `TextDiff` (Diffs characters).
*   **New Way (Vector):** `Vec<String>` (Lines) -> `TextDiff::diff_slices` (Diffs lines).
*   **Benefit:** 
    *   Eliminates the massive string allocation.
    *   Operates purely on the existing line vectors.
    *   Guarantees 100% ordered exactness.
*   **Large File Strategy (>1GB):** For files that don't fit in RAM even as vectors, we will implement a **Fast Scan** using a streaming iterator that compares hashes of 64KB blocks first, only diffing changed blocks.

### B. Structured Files (CSV): Sorted Merge Join
We move from Random Access (HashMap) to Sequential Access (Streaming).

*   **Old Way (Bad):** Load everything into `HashMap`. Random lookups are slow and heavy.
*   **New Way (Streaming):**
    1.  Read records into a `Vec` of `(Key, ByteRecord)`.
    2.  Sort the vectors by Key (Parallelized).
    3.  Iterate both vectors simultaneously ("Merge Join").
*   **Benefit:** Memory overhead drops from ~10x to ~1.2x. Speed increases dramatically due to CPU cache locality.

### C. Excel Support (`calamine`)
We will integrate the `calamine` crate to standardise data ingestion.
*   **Goal:** Treat `.xlsx` exactly like `.csv`.
*   **Method:** Calamine reads Excel rows -> Convert to standard Vector format -> Feed into the **same** Streaming/Sorted engine used for CSVs.
*   **Result:** You can compare a 500MB CSV against a 500MB Excel file with full legal exactness.

---

## 3. Implementation Checklist (Technical)

### Phase 1: Clean High-Performance Text Compare
- [ ] Refactor `compare_text.rs` to remove `lines.join("\n")`.
- [ ] Implement `TextDiff::configure().diff_slices(&lines1, &lines2)`.
- [ ] Verification: Ensure identical output format to current version.

### Phase 2: Streaming Structured Compare
- [ ] Create `SortedRecordProvider` struct.
- [ ] Replace `HashMap` parsing in `compare_structured.rs` with `ByteRecord` vectors.
- [ ] Implement the Merge Join logic (Iterate `idx1` and `idx2` based on Key comparison).

### Phase 3: Calamine Integration
- [ ] Add `calamine = "0.24"` to `Cargo.toml`.
- [ ] Update `index.rs` to detect `.xlsx` files.
- [ ] Implement `read_excel_rows` adapter to feed generic rows to the comparison engine.

---

## 4. Glossary for Reviewers
*   **Vector:** A strictly ordered list (e.g., Line 1, Line 2, Line 3).
*   **Bag of Words:** A pile of words with no order (unsafe for legal).
*   **Merge Join:** A fast way to find matches between two sorted lists by reading them top-to-bottom.
*   **Rayon:** Rust library used to parallelize the *processing* of files (e.g., sorting), but not the core diff algorithm itself.
