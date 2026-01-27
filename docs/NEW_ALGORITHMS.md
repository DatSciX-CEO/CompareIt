# CompareIt - Similarity Algorithm Reference

This document describes all available similarity algorithms and when to use each one.

---

## Algorithm Categories

| Category | Algorithms | Best For |
|----------|------------|----------|
| **Positional** | Diff, Hamming, LCS | Line-by-line comparison where position matters |
| **Edit-Based** | Levenshtein, Damerau-Levenshtein | Typo detection, small changes |
| **Token/Set** | Jaccard, Sorensen-Dice | Bag-of-words, topic matching |
| **Vector Space** | Cosine, TF-IDF | Document similarity, keyword overlap |
| **Pattern Match** | Ratcliff/Obershelp, N-Gram | Moved blocks, partial matches |
| **Alignment** | Smith-Waterman | Finding hidden similar regions |
| **Phonetic** | Jaro-Winkler | Short strings, names, typos |

---

## Detailed Algorithm Descriptions

### 1. Diff (Default)
**Formula:** `common_lines / total_lines`

The standard diff-based scoring. Counts lines that appear in both files at matching positions.

**Use When:** You want traditional diff behavior where position matters.

---

### 2. Jaro-Winkler (CharJaro)
**Formula:** Character-level Jaro-Winkler distance

Optimized for short strings. Gives extra weight to matching prefixes.

**Use When:** Comparing filenames, short identifiers, or when typo tolerance is needed.

---

### 3. Levenshtein
**Formula:** `1 - (edit_distance / max_length)`

Counts minimum insertions, deletions, and substitutions to transform one into the other.

**Use When:** You want to quantify "how many changes" were made.

---

### 4. Damerau-Levenshtein
**Formula:** Like Levenshtein but allows transpositions (ab → ba counts as 1 edit)

**Use When:** Detecting typos where characters were swapped.

---

### 5. Sorensen-Dice
**Formula:** `2 * |intersection| / (|A| + |B|)` using bigrams

Compares overlapping character pairs. More sensitive than word-based.

**Use When:** You need character-level granularity but faster than Levenshtein.

---

### 6. Jaccard Index
**Formula:** `|A ∩ B| / |A ∪ B|` using word tokens

Ignores word order completely. Treats files as "bags of words".

**Use When:** Checking if documents cover the same topics regardless of structure.

---

### 7. Cosine Similarity
**Formula:** `(A · B) / (|A| × |B|)` using term-frequency vectors

Classic vector space model. Measures angular distance between documents.

**Use When:** Detecting conceptually similar documents.

---

### 8. Ratcliff/Obershelp
**Formula:** `2M / T` where M = matching characters

The algorithm behind Python's `difflib.SequenceMatcher`. Recursively finds longest common substrings.

**Use When:** Finding moved/rearranged blocks of text.

---

### 9. Smith-Waterman
**Formula:** Local alignment via dynamic programming

Originated in bioinformatics. Finds the best *local* region of similarity.

**Use When:** Finding similar fragments buried in otherwise different files.

**⚠️ Performance:** Falls back to Myers diff for files >2000 lines.

---

### 10. LCS (Longest Common Subsequence)
**Formula:** `LCS_length / max(len1, len2)`

Finds the longest sequence of lines that appear in both (not necessarily contiguous).

**Use When:** Detecting reordering while preserving content.

**⚠️ Performance:** Falls back to Myers diff for files >5000 lines.

---

### 11. Hamming Distance
**Formula:** `matching_positions / max_length`

Compares lines at identical positions only. Very fast but assumes similar structure.

**Use When:** Files have the same structure and you want a quick sanity check.

---

### 12. N-Gram (Shingling)
**Formula:** Jaccard of character trigrams

Breaks text into overlapping 3-character sequences. Catches partial word matches.

**Use When:** Detecting plagiarism or copy-paste with minor modifications.

---

### 13. TF-IDF Weighted Cosine
**Formula:** Cosine similarity with TF-IDF weighted terms

Weights rare words higher than common words. More meaningful than raw Cosine.

**Use When:** You want keyword-focused comparison ignoring filler words.

---

## CLI Usage Examples

```bash
# Use Python-style difflib matching
CompareIt folder1 folder2 --similarity-algorithm ratcliff-obershelp

# Use topic-based matching
CompareIt folder1 folder2 --similarity-algorithm jaccard

# Use TF-IDF for keyword focus
CompareIt folder1 folder2 --similarity-algorithm tf-idf

# Use local alignment for forensics
CompareIt folder1 folder2 --similarity-algorithm smith-waterman
```

---

## Choosing the Right Algorithm

```
                         ┌─────────────────────────────────────┐
                         │   What matters most?                │
                         └─────────────────────────────────────┘
                                         │
            ┌────────────────────────────┼─────────────────────────────┐
            │                            │                             │
       Position                       Content                      Fragments
            │                            │                             │
    ┌───────┴───────┐          ┌─────────┴─────────┐          ┌────────┴────────┐
    │               │          │                   │          │                 │
   Diff          Hamming   Word-based         Character    Local Align      N-Gram
    │               │          │                   │          │                 │
 (default)     (fastest)   Jaccard/Cosine    Sorensen-Dice  Smith-Waterman   (partial)
                              TF-IDF          Levenshtein                     matches
```
