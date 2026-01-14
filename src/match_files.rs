//! File matching and candidate generation
//!
//! This module implements all-vs-all candidate generation with:
//! - Blocking rules (extension, size ratio, schema) to prune unlikely matches
//! - Top-K candidate selection based on fingerprint similarity
//! - Caps to prevent combinatorial explosion
//!
//! ## Blocking Rules
//!
//! Blocking rules are applied to quickly reject unlikely file pairs before
//! computing expensive similarity metrics. The rules check:
//!
//! 1. **Extension Compatibility**: Files must have compatible extensions
//!    (e.g., `.csv` can match `.tsv`, `.rs` can match `.py`)
//! 2. **Size Ratio**: File sizes must be within 0.1x to 10x of each other
//! 3. **File Type Compatibility**: Binary files can only match other binary files

use crate::fingerprint::simhash_similarity;
use crate::types::{CandidatePair, CompareConfig, FileEntry, FileType, PairingStrategy};
use std::collections::{HashMap, HashSet};

/// Generate candidate pairs for comparison
pub fn generate_candidates(
    files1: &[FileEntry],
    files2: &[FileEntry],
    config: &CompareConfig,
) -> Vec<CandidatePair> {
    match config.pairing {
        PairingStrategy::SamePath => match_by_path(files1, files2),
        PairingStrategy::SameName => match_by_name(files1, files2),
        PairingStrategy::AllVsAll => {
            all_vs_all_match(files1, files2, config.top_k, config.max_pairs)
        }
    }
}

/// Match files by same relative path
fn match_by_path(files1: &[FileEntry], files2: &[FileEntry]) -> Vec<CandidatePair> {
    // Build lookup by path (relative to root)
    let map2: HashMap<&std::path::Path, &FileEntry> = files2
        .iter()
        .map(|f| (f.path.as_path(), f))
        .collect();

    files1
        .iter()
        .filter_map(|f1| {
            map2.get(f1.path.as_path()).map(|f2| CandidatePair {
                file1: f1.clone(),
                file2: (*f2).clone(),
                estimated_similarity: estimate_similarity(f1, f2),
                exact_hash_match: !f1.content_hash.is_empty()
                    && f1.content_hash == f2.content_hash,
            })
        })
        .collect()
}

/// Match files by same filename (ignoring directory structure)
fn match_by_name(files1: &[FileEntry], files2: &[FileEntry]) -> Vec<CandidatePair> {
    // Build lookup by filename
    let map2: HashMap<&str, Vec<&FileEntry>> = {
        let mut m: HashMap<&str, Vec<&FileEntry>> = HashMap::new();
        for f in files2 {
            if let Some(name) = f.path.file_name().and_then(|n| n.to_str()) {
                m.entry(name).or_default().push(f);
            }
        }
        m
    };

    let mut pairs = Vec::new();
    for f1 in files1 {
        if let Some(name) = f1.path.file_name().and_then(|n| n.to_str()) {
            if let Some(candidates) = map2.get(name) {
                // If multiple matches, pick the best one
                if let Some(f2) = candidates.iter().max_by(|a, b| {
                    estimate_similarity(f1, a)
                        .partial_cmp(&estimate_similarity(f1, b))
                        .unwrap_or(std::cmp::Ordering::Equal)
                }) {
                    pairs.push(CandidatePair {
                        file1: f1.clone(),
                        file2: (*f2).clone(),
                        estimated_similarity: estimate_similarity(f1, f2),
                        exact_hash_match: !f1.content_hash.is_empty()
                            && f1.content_hash == f2.content_hash,
                    });
                }
            }
        }
    }

    pairs
}

/// All-vs-all matching with candidate pruning and Top-K selection
///
/// This function performs two-pass matching:
/// 1. **Exact matches**: Files with identical content hashes are paired immediately
/// 2. **Similarity matches**: Remaining files are matched using fingerprint similarity
///
/// The function applies blocking rules to prune unlikely pairs and uses Top-K
/// selection to limit the number of candidates per file.
fn all_vs_all_match(
    files1: &[FileEntry],
    files2: &[FileEntry],
    top_k: usize,
    max_pairs: Option<usize>,
) -> Vec<CandidatePair> {
    let mut all_pairs = Vec::new();
    let mut matched_in_set1: HashSet<std::path::PathBuf> = HashSet::new();
    let mut matched_in_set2: HashSet<std::path::PathBuf> = HashSet::new();

    // First pass: find exact hash matches
    let exact_matches = find_exact_hash_matches(files1, files2);
    for pair in exact_matches {
        matched_in_set1.insert(pair.file1.path.clone());
        matched_in_set2.insert(pair.file2.path.clone());
        all_pairs.push(pair);
    }

    // Second pass: similarity-based matching for unmatched files
    let unmatched1: Vec<&FileEntry> = files1
        .iter()
        .filter(|f| !matched_in_set1.contains(&f.path))
        .collect();

    let unmatched2: Vec<&FileEntry> = files2
        .iter()
        .filter(|f| !matched_in_set2.contains(&f.path))
        .collect();

    let similarity_matches = find_similarity_matches(&unmatched1, &unmatched2, top_k);
    all_pairs.extend(similarity_matches);

    // Sort all pairs by estimated similarity (descending) for deterministic ordering
    all_pairs.sort_by(|a, b| {
        b.estimated_similarity
            .partial_cmp(&a.estimated_similarity)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Apply max_pairs cap
    if let Some(max) = max_pairs {
        all_pairs.truncate(max);
    }

    all_pairs
}

/// Find exact hash matches between two file sets
///
/// Files with identical content hashes are paired greedily (first match wins).
/// This is O(n + m) where n and m are the sizes of the two sets.
fn find_exact_hash_matches(files1: &[FileEntry], files2: &[FileEntry]) -> Vec<CandidatePair> {
    let mut pairs = Vec::new();

    // Build hash lookup for set2
    let hash_map2: HashMap<&str, Vec<&FileEntry>> = {
        let mut m: HashMap<&str, Vec<&FileEntry>> = HashMap::new();
        for f in files2 {
            if !f.content_hash.is_empty() {
                m.entry(f.content_hash.as_str()).or_default().push(f);
            }
        }
        m
    };

    let mut matched_in_set2: HashSet<&std::path::Path> = HashSet::new();

    // Find matches
    for f1 in files1 {
        if !f1.content_hash.is_empty() {
            if let Some(matches) = hash_map2.get(f1.content_hash.as_str()) {
                // Find first unmatched file with same hash
                for f2 in matches {
                    if !matched_in_set2.contains(f2.path.as_path()) {
                        pairs.push(CandidatePair {
                            file1: f1.clone(),
                            file2: (*f2).clone(),
                            estimated_similarity: 1.0,
                            exact_hash_match: true,
                        });
                        matched_in_set2.insert(f2.path.as_path());
                        break;
                    }
                }
            }
        }
    }

    pairs
}

/// Find similarity-based matches for files that didn't have exact hash matches
///
/// For each file in set1, finds the top-k most similar files in set2 based on
/// fingerprint similarity. Blocking rules are applied to prune unlikely pairs.
fn find_similarity_matches(
    files1: &[&FileEntry],
    files2: &[&FileEntry],
    top_k: usize,
) -> Vec<CandidatePair> {
    let mut pairs = Vec::new();

    for f1 in files1 {
        let mut candidates: Vec<(&FileEntry, f64)> = files2
            .iter()
            .filter(|f2| passes_blocking_rules(f1, f2))
            .map(|f2| (*f2, estimate_similarity(f1, f2)))
            .collect();

        // Sort by similarity (descending)
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top-k
        for (f2, sim) in candidates.into_iter().take(top_k) {
            pairs.push(CandidatePair {
                file1: (*f1).clone(),
                file2: f2.clone(),
                estimated_similarity: sim,
                exact_hash_match: false,
            });
        }
    }

    pairs
}

/// Check if two files pass blocking rules for candidate consideration
fn passes_blocking_rules(f1: &FileEntry, f2: &FileEntry) -> bool {
    // Rule 1: Same or compatible extension
    if !extensions_compatible(&f1.extension, &f2.extension) {
        return false;
    }

    // Rule 2: Size ratio within threshold (0.1x to 10x)
    if f1.size > 0 && f2.size > 0 {
        let ratio = f1.size as f64 / f2.size as f64;
        if ratio < 0.1 || ratio > 10.0 {
            return false;
        }
    }

    // Rule 3: For structured files, require compatible schema
    if f1.file_type.is_structured() && f2.file_type.is_structured() {
        // If both have schema signatures, they should match
        if let (Some(s1), Some(s2)) = (&f1.schema_signature, &f2.schema_signature) {
            if s1 != s2 {
                // Allow partial match if schemas are different but we're doing all-vs-all
                // Return true but with lower priority (handled by similarity score)
            }
        }
    }

    // Rule 4: Compatible file types
    match (&f1.file_type, &f2.file_type) {
        (FileType::Binary, FileType::Binary) => true,
        (FileType::Binary, _) | (_, FileType::Binary) => false,
        _ => true,
    }
}

/// Check if two extensions are compatible for comparison
fn extensions_compatible(ext1: &str, ext2: &str) -> bool {
    if ext1 == ext2 {
        return true;
    }

    // Define extension groups that are compatible
    let text_exts = ["txt", "log", "md", "rst", ""];
    let csv_exts = ["csv", "tsv", "tab"];
    let code_exts = ["rs", "py", "js", "ts", "java", "c", "cpp", "h", "hpp", "go"];
    let config_exts = ["json", "yaml", "yml", "toml", "ini", "cfg"];

    let in_same_group = |e1: &str, e2: &str, group: &[&str]| {
        group.contains(&e1) && group.contains(&e2)
    };

    in_same_group(ext1, ext2, &text_exts)
        || in_same_group(ext1, ext2, &csv_exts)
        || in_same_group(ext1, ext2, &code_exts)
        || in_same_group(ext1, ext2, &config_exts)
}

/// Estimate similarity between two files based on fingerprints
fn estimate_similarity(f1: &FileEntry, f2: &FileEntry) -> f64 {
    // Exact hash match
    if !f1.content_hash.is_empty() && f1.content_hash == f2.content_hash {
        return 1.0;
    }

    // Simhash similarity
    if let (Some(h1), Some(h2)) = (f1.simhash, f2.simhash) {
        return simhash_similarity(h1, h2);
    }

    // Schema signature match for structured files
    if let (Some(s1), Some(s2)) = (&f1.schema_signature, &f2.schema_signature) {
        if s1 == s2 {
            return 0.5; // Base similarity for same schema
        }
    }

    // Size-based fallback
    if f1.size > 0 && f2.size > 0 {
        let ratio = f1.size.min(f2.size) as f64 / f1.size.max(f2.size) as f64;
        return ratio * 0.3; // Low confidence size-based estimate
    }

    0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_file_entry(path: &str, hash: &str, size: u64, ext: &str, file_type: FileType) -> FileEntry {
        FileEntry {
            path: PathBuf::from(path),
            size,
            file_type,
            extension: ext.to_string(),
            content_hash: hash.to_string(),
            simhash: Some(0),
            schema_signature: None,
            line_count: 10,
            columns: None,
        }
    }

    #[test]
    fn test_extensions_compatible() {
        assert!(extensions_compatible("csv", "csv"));
        assert!(extensions_compatible("csv", "tsv"));
        assert!(!extensions_compatible("csv", "py"));
        assert!(extensions_compatible("rs", "py"));
    }

    #[test]
    fn test_find_exact_hash_matches_single() {
        let files1 = vec![make_file_entry("a.txt", "hash1", 100, "txt", FileType::Text)];
        let files2 = vec![make_file_entry("b.txt", "hash1", 100, "txt", FileType::Text)];

        let matches = find_exact_hash_matches(&files1, &files2);

        assert_eq!(matches.len(), 1);
        assert!(matches[0].exact_hash_match);
        assert_eq!(matches[0].estimated_similarity, 1.0);
    }

    #[test]
    fn test_find_exact_hash_matches_no_match() {
        let files1 = vec![make_file_entry("a.txt", "hash1", 100, "txt", FileType::Text)];
        let files2 = vec![make_file_entry("b.txt", "hash2", 100, "txt", FileType::Text)];

        let matches = find_exact_hash_matches(&files1, &files2);

        assert!(matches.is_empty());
    }

    #[test]
    fn test_find_exact_hash_matches_multiple() {
        let files1 = vec![
            make_file_entry("a.txt", "hash1", 100, "txt", FileType::Text),
            make_file_entry("b.txt", "hash2", 200, "txt", FileType::Text),
        ];
        let files2 = vec![
            make_file_entry("c.txt", "hash2", 200, "txt", FileType::Text),
            make_file_entry("d.txt", "hash1", 100, "txt", FileType::Text),
        ];

        let matches = find_exact_hash_matches(&files1, &files2);

        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_passes_blocking_rules_same_ext() {
        let f1 = make_file_entry("a.txt", "h1", 100, "txt", FileType::Text);
        let f2 = make_file_entry("b.txt", "h2", 100, "txt", FileType::Text);

        assert!(passes_blocking_rules(&f1, &f2));
    }

    #[test]
    fn test_passes_blocking_rules_size_mismatch() {
        let f1 = make_file_entry("a.txt", "h1", 100, "txt", FileType::Text);
        let f2 = make_file_entry("b.txt", "h2", 10000, "txt", FileType::Text); // 100x larger

        assert!(!passes_blocking_rules(&f1, &f2));
    }

    #[test]
    fn test_passes_blocking_rules_binary_vs_text() {
        let f1 = make_file_entry("a.bin", "h1", 100, "bin", FileType::Binary);
        let f2 = make_file_entry("b.txt", "h2", 100, "txt", FileType::Text);

        assert!(!passes_blocking_rules(&f1, &f2));
    }

    #[test]
    fn test_find_similarity_matches_top_k() {
        let f1 = make_file_entry("a.txt", "h1", 100, "txt", FileType::Text);
        let f2 = make_file_entry("b.txt", "h2", 100, "txt", FileType::Text);
        let f3 = make_file_entry("c.txt", "h3", 100, "txt", FileType::Text);
        let f4 = make_file_entry("d.txt", "h4", 100, "txt", FileType::Text);

        let files1: Vec<&FileEntry> = vec![&f1];
        let files2: Vec<&FileEntry> = vec![&f2, &f3, &f4];

        let matches = find_similarity_matches(&files1, &files2, 2);

        // Should return at most top_k matches per file in files1
        assert_eq!(matches.len(), 2);
    }
}
