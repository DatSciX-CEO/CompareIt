//! Text-based file comparison
//!
//! This module implements line-based diff comparison using the `similar` crate,
//! with support for various normalization options, regex filtering, and similarity scoring.
//!
//! **Performance Note (Phase 1 Optimization):**
//! Uses `TextDiff::diff_slices` to compare lines directly without joining them
//! into a single massive string. This eliminates OOM crashes on files >500MB.

use crate::fingerprint::read_normalized_lines;
use crate::types::{CompareConfig, FileEntry, SimilarityAlgorithm, TextComparisonResult};
use anyhow::Result;
use log::warn;
use regex::Regex;
use similar::{Algorithm, ChangeTag, TextDiff};
use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use strsim::jaro_winkler;

/// Compare two text files and produce a detailed result
///
/// This function uses vector/slice-based comparison to avoid memory issues
/// with large files. Lines are diffed directly without joining into a single string.
pub fn compare_text_files(
    file1: &FileEntry,
    file2: &FileEntry,
    config: &CompareConfig,
) -> Result<TextComparisonResult> {
    // Read and normalize content
    let mut lines1 = read_normalized_lines(&file1.path, &config.normalization)?;
    let mut lines2 = read_normalized_lines(&file2.path, &config.normalization)?;

    // Apply regex filtering if specified
    if let Some(ref pattern) = config.ignore_regex {
        let re = compile_ignore_regex(pattern);
        if let Some(ref regex) = re {
            lines1 = apply_regex_filter(&lines1, regex);
            lines2 = apply_regex_filter(&lines2, regex);
        }
    }

    // Perform diff using slice comparison (no massive string allocation!)
    // This is the key optimization: diff_slices operates on Vec<String> directly
    let diff = TextDiff::configure()
        .algorithm(Algorithm::Myers)
        .diff_slices(&lines1, &lines2);

    // Collect statistics
    let mut common_lines = 0;
    let mut only_in_file1 = 0;
    let mut only_in_file2 = 0;
    let mut different_positions = Vec::new();
    let mut detailed_diff = String::new();
    let mut diff_bytes = 0;
    let mut diff_truncated = false;

    for (idx, change) in diff.iter_all_changes().enumerate() {
        match change.tag() {
            ChangeTag::Equal => {
                common_lines += 1;
            }
            ChangeTag::Delete => {
                only_in_file1 += 1;
                different_positions.push(idx);

                // Add to detailed diff if under limit
                if diff_bytes < config.max_diff_bytes {
                    let line = format!("-{}", change.value());
                    diff_bytes += line.len();
                    let _ = write!(detailed_diff, "{}", line);
                    if !line.ends_with('\n') {
                        detailed_diff.push('\n');
                    }
                } else {
                    diff_truncated = true;
                }
            }
            ChangeTag::Insert => {
                only_in_file2 += 1;
                different_positions.push(idx);

                // Add to detailed diff if under limit
                if diff_bytes < config.max_diff_bytes {
                    let line = format!("+{}", change.value());
                    diff_bytes += line.len();
                    let _ = write!(detailed_diff, "{}", line);
                    if !line.ends_with('\n') {
                        detailed_diff.push('\n');
                    }
                } else {
                    diff_truncated = true;
                }
            }
        }
    }

    // Calculate similarity score
    let similarity_score = match config.similarity_algorithm {
        SimilarityAlgorithm::Diff => {
            let total = common_lines + only_in_file1 + only_in_file2;
            if total > 0 {
                common_lines as f64 / total as f64
            } else {
                1.0 // Both empty = identical
            }
        }
        // For character-based algorithms, we need full text - but only construct lazily
        // This is acceptable because these algorithms are rarely used on huge files
        SimilarityAlgorithm::CharJaro => {
            let text1 = lines1.join("\n");
            let text2 = lines2.join("\n");
            jaro_winkler(&text1, &text2)
        }
        SimilarityAlgorithm::Levenshtein => {
            let text1 = lines1.join("\n");
            let text2 = lines2.join("\n");
            strsim::normalized_levenshtein(&text1, &text2)
        }
        SimilarityAlgorithm::DamerauLevenshtein => {
            let text1 = lines1.join("\n");
            let text2 = lines2.join("\n");
            strsim::normalized_damerau_levenshtein(&text1, &text2)
        }
        SimilarityAlgorithm::SorensenDice => {
            let text1 = lines1.join("\n");
            let text2 = lines2.join("\n");
            strsim::sorensen_dice(&text1, &text2)
        }
        SimilarityAlgorithm::Jaccard => calculate_jaccard_similarity(&lines1, &lines2),
        SimilarityAlgorithm::Cosine => calculate_cosine_similarity(&lines1, &lines2),
        SimilarityAlgorithm::RatcliffObershelp => {
            // Using similar's ratio() which roughly approximates Ratcliff/Obershelp 2.0*M/T
            // but is highly optimized (unlike a naive manual implementation)
            TextDiff::configure()
                .algorithm(Algorithm::Myers)
                .diff_slices(&lines1, &lines2)
                .ratio()
        }
        SimilarityAlgorithm::SmithWaterman => {
            // Full Smith-Waterman is O(N*M) and will hang on large files.
            // We use a token-based local alignment approximation here for safety.
            calculate_token_smith_waterman(&lines1, &lines2)
        }
        SimilarityAlgorithm::Lcs => calculate_lcs_similarity(&lines1, &lines2),
        SimilarityAlgorithm::Hamming => calculate_hamming_similarity(&lines1, &lines2),
        SimilarityAlgorithm::NGram => calculate_ngram_similarity(&lines1, &lines2),
        SimilarityAlgorithm::TfIdf => calculate_tfidf_cosine_similarity(&lines1, &lines2),
    };

    // Generate unified diff format (also uses slice-based diff)
    let unified_diff = generate_unified_diff_from_slices(
        &file1.path.display().to_string(),
        &file2.path.display().to_string(),
        &lines1,
        &lines2,
        config.max_diff_bytes,
    );

    // Create linked ID
    let linked_id = format!(
        "{}:{}",
        &file1.content_hash[..16.min(file1.content_hash.len())],
        &file2.content_hash[..16.min(file2.content_hash.len())]
    );

    // Encode different positions as ranges
    let positions_str = encode_ranges(&different_positions);

    let identical = only_in_file1 == 0 && only_in_file2 == 0;

    Ok(TextComparisonResult {
        linked_id,
        file1_path: file1.path.display().to_string(),
        file2_path: file2.path.display().to_string(),
        file1_line_count: lines1.len(),
        file2_line_count: lines2.len(),
        common_lines,
        only_in_file1,
        only_in_file2,
        similarity_score,
        different_positions: positions_str,
        detailed_diff: if unified_diff.0.is_empty() {
            detailed_diff
        } else {
            unified_diff.0
        },
        diff_truncated: diff_truncated || unified_diff.1,
        identical,
    })
}

/// Generate unified diff format output from line slices
///
/// Uses `diff_slices` to avoid constructing massive strings for large files.
fn generate_unified_diff_from_slices(
    file1_name: &str,
    file2_name: &str,
    lines1: &[String],
    lines2: &[String],
    max_bytes: usize,
) -> (String, bool) {
    let diff = TextDiff::configure()
        .algorithm(Algorithm::Myers)
        .diff_slices(lines1, lines2);

    let mut output = String::new();
    let mut truncated = false;

    // Header
    let _ = writeln!(output, "--- {}", file1_name);
    let _ = writeln!(output, "+++ {}", file2_name);

    // Generate hunks
    for hunk in diff.unified_diff().context_radius(3).iter_hunks() {
        if output.len() >= max_bytes {
            truncated = true;
            break;
        }

        let _ = writeln!(output, "{}", hunk.header());
        for change in hunk.iter_changes() {
            if output.len() >= max_bytes {
                truncated = true;
                break;
            }

            let prefix = match change.tag() {
                ChangeTag::Delete => "-",
                ChangeTag::Insert => "+",
                ChangeTag::Equal => " ",
            };
            let _ = write!(output, "{}{}", prefix, change.value());
            // Lines from diff_slices don't have trailing newlines, so always add one
            output.push('\n');
        }
    }

    if truncated {
        output.push_str("\n... [diff truncated] ...\n");
    }

    (output, truncated)
}

/// Encode a list of positions as ranges (e.g., "1-5,8,10-15")
fn encode_ranges(positions: &[usize]) -> String {
    if positions.is_empty() {
        return String::new();
    }

    let mut ranges = Vec::new();
    let mut start = positions[0];
    let mut end = positions[0];

    for &pos in &positions[1..] {
        if pos == end + 1 {
            end = pos;
        } else {
            if start == end {
                ranges.push(start.to_string());
            } else {
                ranges.push(format!("{}-{}", start, end));
            }
            start = pos;
            end = pos;
        }
    }

    // Add last range
    if start == end {
        ranges.push(start.to_string());
    } else {
        ranges.push(format!("{}-{}", start, end));
    }

    ranges.join(",")
}

/// Compile a regex pattern for line filtering, logging a warning if invalid
///
/// Uses RegexBuilder with size limits to prevent ReDoS attacks.
fn compile_ignore_regex(pattern: &str) -> Option<Regex> {
    use regex::RegexBuilder;
    
    match RegexBuilder::new(pattern)
        .size_limit(1_000_000)      // 1MB compiled size limit
        .dfa_size_limit(1_000_000)  // 1MB DFA size limit to prevent explosion
        .build()
    {
        Ok(re) => Some(re),
        Err(e) => {
            warn!("Invalid ignore_regex pattern '{}': {}", pattern, e);
            None
        }
    }
}

/// Apply regex filter to lines, replacing matches with `<IGNORED>`
///
/// This allows comparing files while ignoring specific content like timestamps,
/// generated IDs, or other dynamic values.
fn apply_regex_filter(lines: &[String], regex: &Regex) -> Vec<String> {
    lines
        .iter()
        .map(|line| regex.replace_all(line, "<IGNORED>").into_owned())
        .collect()
}


/// Calculate Jaccard similarity (Intersection over Union) of tokens
fn calculate_jaccard_similarity(lines1: &[String], lines2: &[String]) -> f64 {
    let tokens1: HashSet<String> = lines1
        .iter()
        .flat_map(|l| l.split_whitespace())
        .map(|s| s.to_lowercase())
        .collect();
    let tokens2: HashSet<String> = lines2
        .iter()
        .flat_map(|l| l.split_whitespace())
        .map(|s| s.to_lowercase())
        .collect();

    if tokens1.is_empty() && tokens2.is_empty() {
        return 1.0;
    }

    let intersection = tokens1.intersection(&tokens2).count();
    let union = tokens1.union(&tokens2).count();

    if union == 0 {
        1.0
    } else {
        intersection as f64 / union as f64
    }
}

/// Calculate Cosine similarity of token frequency vectors
fn calculate_cosine_similarity(lines1: &[String], lines2: &[String]) -> f64 {
    let mut freq1: HashMap<String, usize> = HashMap::new();
    let mut freq2: HashMap<String, usize> = HashMap::new();

    for line in lines1 {
        for token in line.split_whitespace() {
            *freq1.entry(token.to_lowercase()).or_default() += 1;
        }
    }

    for line in lines2 {
        for token in line.split_whitespace() {
            *freq2.entry(token.to_lowercase()).or_default() += 1;
        }
    }

    let mut dot_product = 0.0;
    let mut mag1_sq = 0.0;
    let mut mag2_sq = 0.0;

    // Calculate dot product and magnitude of vec1
    for (token, count1) in &freq1 {
        let c1 = *count1 as f64;
        mag1_sq += c1 * c1;
        if let Some(&count2) = freq2.get(token) {
            dot_product += c1 * count2 as f64;
        }
    }

    // Calculate magnitude of vec2
    for count2 in freq2.values() {
        let c2 = *count2 as f64;
        mag2_sq += c2 * c2;
    }

    let mag1 = mag1_sq.sqrt();
    let mag2 = mag2_sq.sqrt();

    if mag1 == 0.0 && mag2 == 0.0 {
        1.0
    } else if mag1 == 0.0 || mag2 == 0.0 {
        0.0
    } else {
        dot_product / (mag1 * mag2)
    }
}

/// Calculate Smith-Waterman similarity roughly approximated on LINES
///
/// This does a local alignment on the sequence of lines.
/// Normalizes the score to 0.0 - 1.0 range based on the theoretical max score.
fn calculate_token_smith_waterman(lines1: &[String], lines2: &[String]) -> f64 {
    // If files are huge, simple Myers diff ratio is better than O(N*M) Smith-Waterman
    // Fallback if too large (>2000 lines) to prevent hangs
    if lines1.len() > 2000 || lines2.len() > 2000 {
        return TextDiff::configure()
            .algorithm(Algorithm::Myers)
            .diff_slices(lines1, lines2)
            .ratio();
    }

    let n = lines1.len();
    let m = lines2.len();
    if n == 0 && m == 0 { return 1.0; }
    if n == 0 || m == 0 { return 0.0; }

    let match_score = 2.0;
    let mismatch_score = -1.0;
    let gap_score = -1.0;

    // Standard DP matrix: (n+1) x (m+1) - flattened
    // Note: This optimization is crucial for memory, but for SW we really need the whole matrix
    // or at least 2 rows. Here we use 1 row + prev variable to simulate
    let mut matrix = vec![0.0; m + 1];
    let mut max_score = 0.0;

    for i in 1..=n {
        let mut prev_diag = 0.0; // Matrix[i-1][j-1]
        for j in 1..=m {
            let current_prev_diag = matrix[j]; // Save for next iteration (this will be i-1, j-1)
            
            // Score calculations
            let s_match = if lines1[i-1] == lines2[j-1] { match_score } else { mismatch_score };
            let val_match = prev_diag + s_match;
            let val_del = matrix[j] + gap_score;   // Up
            let val_ins = matrix[j-1] + gap_score; // Left
            
            let val = val_match.max(val_del).max(val_ins).max(0.0);
            
            matrix[j] = val; // Update current row
            if val > max_score {
                max_score = val;
            }
            
            prev_diag = current_prev_diag; // Move diagonal for next step
        }
    }

    // Normalize: Perfect match is matching min_len lines * match_score? 
    // Actually SW finds *local* regions. 
    // A perfect match of the whole file would be lines.len() * match_score.
    // We normalize by the shorter length * match_score to indicate how "good" the found region is.
    let min_len = n.min(m) as f64;
    if min_len == 0.0 { 0.0 } else { (max_score / (min_len * match_score)).min(1.0) }
}

/// Calculate LCS (Longest Common Subsequence) similarity
///
/// Uses dynamic programming to find the longest subsequence present in both.
/// Normalized by the max length of the two inputs.
fn calculate_lcs_similarity(lines1: &[String], lines2: &[String]) -> f64 {
    let n = lines1.len();
    let m = lines2.len();
    
    if n == 0 && m == 0 { return 1.0; }
    if n == 0 || m == 0 { return 0.0; }
    
    // Fallback for very large files
    if n > 5000 || m > 5000 {
        return TextDiff::configure()
            .algorithm(Algorithm::Myers)
            .diff_slices(lines1, lines2)
            .ratio();
    }
    
    // Standard LCS DP with space optimization (only need previous row)
    let mut prev = vec![0usize; m + 1];
    let mut curr = vec![0usize; m + 1];
    
    for i in 1..=n {
        for j in 1..=m {
            if lines1[i-1] == lines2[j-1] {
                curr[j] = prev[j-1] + 1;
            } else {
                curr[j] = prev[j].max(curr[j-1]);
            }
        }
        std::mem::swap(&mut prev, &mut curr);
        curr.fill(0);
    }
    
    let lcs_len = prev[m];
    let max_len = n.max(m) as f64;
    lcs_len as f64 / max_len
}

/// Calculate Hamming similarity (line-level)
///
/// Compares lines at matching positions. Fast but assumes similar structure.
fn calculate_hamming_similarity(lines1: &[String], lines2: &[String]) -> f64 {
    let n = lines1.len();
    let m = lines2.len();
    
    if n == 0 && m == 0 { return 1.0; }
    
    let max_len = n.max(m);
    let min_len = n.min(m);
    
    let mut matches = 0;
    for i in 0..min_len {
        if lines1[i] == lines2[i] {
            matches += 1;
        }
    }
    
    // Unmatched positions count as differences
    matches as f64 / max_len as f64
}

/// Calculate N-Gram (Shingling) similarity
///
/// Uses character-level n-grams (default n=3) for fine-grained comparison.
fn calculate_ngram_similarity(lines1: &[String], lines2: &[String]) -> f64 {
    let text1 = lines1.join("\n");
    let text2 = lines2.join("\n");
    
    if text1.is_empty() && text2.is_empty() { return 1.0; }
    if text1.is_empty() || text2.is_empty() { return 0.0; }
    
    let n = 3; // trigrams
    
    let ngrams1: HashSet<&str> = text1
        .as_bytes()
        .windows(n)
        .filter_map(|w| std::str::from_utf8(w).ok())
        .collect();
    
    let ngrams2: HashSet<&str> = text2
        .as_bytes()
        .windows(n)
        .filter_map(|w| std::str::from_utf8(w).ok())
        .collect();
    
    if ngrams1.is_empty() && ngrams2.is_empty() { return 1.0; }
    
    let intersection = ngrams1.intersection(&ngrams2).count();
    let union = ngrams1.union(&ngrams2).count();
    
    if union == 0 { 1.0 } else { intersection as f64 / union as f64 }
}

/// Calculate TF-IDF weighted Cosine similarity
///
/// Like regular cosine but weights tokens by their inverse document frequency.
/// In a 2-document context, rare tokens (appearing in only one) get higher weight.
fn calculate_tfidf_cosine_similarity(lines1: &[String], lines2: &[String]) -> f64 {
    let mut freq1: HashMap<String, usize> = HashMap::new();
    let mut freq2: HashMap<String, usize> = HashMap::new();
    
    for line in lines1 {
        for token in line.split_whitespace() {
            *freq1.entry(token.to_lowercase()).or_default() += 1;
        }
    }
    
    for line in lines2 {
        for token in line.split_whitespace() {
            *freq2.entry(token.to_lowercase()).or_default() += 1;
        }
    }
    
    if freq1.is_empty() && freq2.is_empty() { return 1.0; }
    if freq1.is_empty() || freq2.is_empty() { return 0.0; }
    
    // Calculate IDF: log(2 / df) where df = number of docs containing term (1 or 2)
    let mut idf: HashMap<String, f64> = HashMap::new();
    let all_tokens: HashSet<String> = freq1.keys().chain(freq2.keys()).cloned().collect();
    
    for token in &all_tokens {
        let df = (freq1.contains_key(token) as usize) + (freq2.contains_key(token) as usize);
        // IDF = log(N/df) where N=2 (we have 2 documents)
        idf.insert(token.clone(), (2.0 / df as f64).ln().max(0.0) + 1.0);
    }
    
    // TF-IDF vectors and cosine similarity
    let mut dot_product = 0.0;
    let mut mag1_sq = 0.0;
    let mut mag2_sq = 0.0;
    
    for token in &all_tokens {
        let tf1 = *freq1.get(token).unwrap_or(&0) as f64;
        let tf2 = *freq2.get(token).unwrap_or(&0) as f64;
        let idf_val = *idf.get(token).unwrap_or(&1.0);
        
        let tfidf1 = tf1 * idf_val;
        let tfidf2 = tf2 * idf_val;
        
        dot_product += tfidf1 * tfidf2;
        mag1_sq += tfidf1 * tfidf1;
        mag2_sq += tfidf2 * tfidf2;
    }
    
    let mag1 = mag1_sq.sqrt();
    let mag2 = mag2_sq.sqrt();
    
    if mag1 == 0.0 || mag2 == 0.0 { 0.0 } else { dot_product / (mag1 * mag2) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_ranges() {
        assert_eq!(encode_ranges(&[1, 2, 3, 5, 7, 8, 9]), "1-3,5,7-9");
        assert_eq!(encode_ranges(&[1]), "1");
        assert_eq!(encode_ranges(&[]), "");
    }

    #[test]
    fn test_diff_slices_basic() {
        let lines1 = vec!["line1".to_string(), "line2".to_string(), "line3".to_string()];
        let lines2 = vec!["line1".to_string(), "modified".to_string(), "line3".to_string()];
        
        let diff = TextDiff::configure()
            .algorithm(Algorithm::Myers)
            .diff_slices(&lines1, &lines2);
        
        let mut changes = 0;
        for change in diff.iter_all_changes() {
            if change.tag() != ChangeTag::Equal {
                changes += 1;
            }
        }
        assert_eq!(changes, 2); // One delete, one insert
    }
}
