//! Text-based file comparison
//!
//! This module implements line-based diff comparison using the `similar` crate,
//! with support for various normalization options and similarity scoring.

use crate::fingerprint::read_normalized_lines;
use crate::types::{CompareConfig, FileEntry, SimilarityAlgorithm, TextComparisonResult};
use anyhow::Result;
use similar::{ChangeTag, TextDiff};
use std::fmt::Write;
use strsim::jaro_winkler;

/// Compare two text files and produce a detailed result
pub fn compare_text_files(
    file1: &FileEntry,
    file2: &FileEntry,
    config: &CompareConfig,
) -> Result<TextComparisonResult> {
    // Read and normalize content
    let lines1 = read_normalized_lines(&file1.path, &config.normalization)?;
    let lines2 = read_normalized_lines(&file2.path, &config.normalization)?;

    // Perform diff
    let text1 = lines1.join("\n");
    let text2 = lines2.join("\n");

    let diff = TextDiff::from_lines(&text1, &text2);

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
        SimilarityAlgorithm::CharJaro => jaro_winkler(&text1, &text2),
    };

    // Generate unified diff format
    let unified_diff = generate_unified_diff(
        &file1.path.display().to_string(),
        &file2.path.display().to_string(),
        &text1,
        &text2,
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

/// Generate unified diff format output
fn generate_unified_diff(
    file1_name: &str,
    file2_name: &str,
    text1: &str,
    text2: &str,
    max_bytes: usize,
) -> (String, bool) {
    let diff = TextDiff::from_lines(text1, text2);

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
            if !change.value().ends_with('\n') {
                output.push('\n');
            }
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

/// Quick check if two files are identical without full diff
pub fn files_identical(file1: &FileEntry, file2: &FileEntry) -> bool {
    !file1.content_hash.is_empty() && file1.content_hash == file2.content_hash
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
}
