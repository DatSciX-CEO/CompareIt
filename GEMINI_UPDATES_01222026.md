# Gemini Updates - Reference Document
**Date:** January 22, 2026

This document summarizes our discussion regarding the performance performance optimization of `CompareIt`, specifically focusing on handling large (700MB+) files.

## 1. Performance Analysis (The Problem)
**Issue:** Comparing large files (~700MB) is extremely slow and memory-intensive.
**Cause:**
- **Memory Loading:** The current implementation reads entire files into memory strings or vectors.
- **Data Structures:** For structured data (CSV), using `HashMap<String, HashMap<String, String>>` causes massive memory overhead (10x+ file size).
- **Algorithmic Complexity:** Standard text comparison algorithms like `Diff` (Myers) or `Levenshtein` are `O(N*M)` or `O(N*D)`, which scales poorly for massive single files.

## 2. Similarity Algorithms (`types.rs`)
We discussed how `SimilarityAlgorithm` determines the "Similarity Score" for text files.

### Existing Algorithms
| Algorithm | Logic | Best For | Performance Note |
| :--- | :--- | :--- | :--- |
| **`Diff`** (Default) | Calculates % of shared lines vs unique lines. | Source code, Logs | Requires storing all lines in memory. |
| **`CharJaro`** | Calculates Jaro-Winkler distance on characters. | Short strings (names) | **Fatal** for large files (loads full string, O(N*M)). |

### New Algorithms Added
We added the following to `types.rs` and `compare_text.rs` using the `strsim` crate:
- **`Levenshtein`**: Standard edit distance (inserts/deletes/subs).
- **`DamerauLevenshtein`**: Like Levenshtein but handles transpositions (swaps).
- **`SorensenDice`**: Bigram-based similarity.

> **Warning:** While available, these algorithms (especially Levenshtein) are **not recommended** for 700MB files as they are single-threaded and computationally prohibitive for that size.

## 3. Parallelism & Rayon
**Question:** Does Rayon parallelize the comparison?
**Answer:**
- **Yes (File Level):** Rayon parallelizes **across file pairs**. If you have 100 pairs, it uses all cores.
- **No (Algorithm Level):** Rayon does **not** parallelize the internal math of `levenshtein(string1, string2)`. It runs on a single core.

### The "Vector" Approach (Optimization)
We discussed checking "Vector" or "Bag of Words" similarity for large files to leverage parallelism:
- **Token Jaccard**: Split file into words -> parallelize word counting -> compare sets.
- This is `O(N)` and fully parallelizable, unlike edit distance.

## 4. The Comparison Engine (`lib.rs`)
The `ComparisonEngine` is the central controller that orchestrates the workflow:
1.  **Indexing:** Scans folders, filters ignores, detects file types.
2.  **Fingerprinting:** Computes Hashes (exact match) and SimHashes (fuzzy match).
3.  **Matching:** Pairs files between folders (by Path, Name, or Similarity).
4.  **Comparison:** Runs the actual compare logic (Diff/CSV) on pairs in parallel.
5.  **Reporting:** Generates the HTML report.

## 5. Recommended Optimization Plan
To fix the 700MB file slowness, we agreed on a **Streaming/Sorted** strategy (detailed in `implementation_plan.md`):

1.  **Structured (CSV):** Stop using HashMaps. Sort records on disk/stream and use a "Merge Join" (like database engines) to find matches with very low memory usage.
2.  **Text:** Use streaming comparison or block matching to avoid loading the full file.
3.  **Optional:** Implement Parallel Token Jaccard for faster similarity estimation on large text files.
