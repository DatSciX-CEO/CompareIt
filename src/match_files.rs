//! File matching and candidate generation
//!
//! This module implements all-vs-all candidate generation with:
//! - Blocking rules (extension, size ratio, schema)
//! - Top-K candidate selection based on fingerprint similarity
//! - Caps to prevent combinatorial explosion

use crate::fingerprint::simhash_similarity;
use crate::types::{CandidatePair, CompareConfig, FileEntry, FileType, PairingStrategy};
use std::collections::HashMap;

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
fn all_vs_all_match(
    files1: &[FileEntry],
    files2: &[FileEntry],
    top_k: usize,
    max_pairs: Option<usize>,
) -> Vec<CandidatePair> {
    let mut all_pairs = Vec::new();

    // First pass: find exact hash matches
    let hash_map2: HashMap<&str, Vec<&FileEntry>> = {
        let mut m: HashMap<&str, Vec<&FileEntry>> = HashMap::new();
        for f in files2 {
            if !f.content_hash.is_empty() {
                m.entry(f.content_hash.as_str()).or_default().push(f);
            }
        }
        m
    };

    let mut matched_in_set1: std::collections::HashSet<&std::path::Path> =
        std::collections::HashSet::new();
    let mut matched_in_set2: std::collections::HashSet<&std::path::Path> =
        std::collections::HashSet::new();

    // Exact hash matches
    for f1 in files1 {
        if !f1.content_hash.is_empty() {
            if let Some(matches) = hash_map2.get(f1.content_hash.as_str()) {
                // Find first unmatched
                for f2 in matches {
                    if !matched_in_set2.contains(f2.path.as_path()) {
                        all_pairs.push(CandidatePair {
                            file1: f1.clone(),
                            file2: (*f2).clone(),
                            estimated_similarity: 1.0,
                            exact_hash_match: true,
                        });
                        matched_in_set1.insert(f1.path.as_path());
                        matched_in_set2.insert(f2.path.as_path());
                        break;
                    }
                }
            }
        }
    }

    // Second pass: similarity-based matching for unmatched files
    let unmatched1: Vec<&FileEntry> = files1
        .iter()
        .filter(|f| !matched_in_set1.contains(f.path.as_path()))
        .collect();

    let unmatched2: Vec<&FileEntry> = files2
        .iter()
        .filter(|f| !matched_in_set2.contains(f.path.as_path()))
        .collect();

    // For each file in set1, find top-k candidates in set2
    for f1 in &unmatched1 {
        let mut candidates: Vec<(&FileEntry, f64)> = unmatched2
            .iter()
            .filter(|f2| passes_blocking_rules(f1, f2))
            .map(|f2| (*f2, estimate_similarity(f1, f2)))
            .collect();

        // Sort by similarity (descending)
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top-k
        for (f2, sim) in candidates.into_iter().take(top_k) {
            all_pairs.push(CandidatePair {
                file1: (*f1).clone(),
                file2: f2.clone(),
                estimated_similarity: sim,
                exact_hash_match: false,
            });
        }
    }

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

    #[test]
    fn test_extensions_compatible() {
        assert!(extensions_compatible("csv", "csv"));
        assert!(extensions_compatible("csv", "tsv"));
        assert!(!extensions_compatible("csv", "py"));
        assert!(extensions_compatible("rs", "py"));
    }
}
