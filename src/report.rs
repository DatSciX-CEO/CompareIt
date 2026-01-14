//! HTML report generation
//!
//! This module generates self-contained HTML reports from comparison results,
//! with sortable tables and links to artifacts.

use crate::types::{ComparisonResult, ComparisonSummary};
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Generate an HTML report from comparison results
pub fn generate_html_report(
    results: &[ComparisonResult],
    summary: &ComparisonSummary,
    output_path: &Path,
    artifacts_dir: Option<&Path>,
) -> Result<()> {
    let html = build_html_report(results, summary, artifacts_dir);

    fs::write(output_path, html)
        .with_context(|| format!("Failed to write HTML report to {}", output_path.display()))?;

    Ok(())
}

/// Build the HTML report content
fn build_html_report(
    results: &[ComparisonResult],
    summary: &ComparisonSummary,
    artifacts_dir: Option<&Path>,
) -> String {
    let mut html = String::new();

    // HTML header with embedded CSS and JS
    html.push_str(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>CompareIt Report</title>
    <style>
        :root {
            --bg-primary: #0d1117;
            --bg-secondary: #161b22;
            --bg-tertiary: #21262d;
            --text-primary: #c9d1d9;
            --text-secondary: #8b949e;
            --accent: #58a6ff;
            --success: #3fb950;
            --warning: #d29922;
            --danger: #f85149;
            --border: #30363d;
        }
        
        * {
            box-sizing: border-box;
            margin: 0;
            padding: 0;
        }
        
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', 'Noto Sans', Helvetica, Arial, sans-serif;
            background: var(--bg-primary);
            color: var(--text-primary);
            line-height: 1.6;
            padding: 2rem;
        }
        
        .container {
            max-width: 1400px;
            margin: 0 auto;
        }
        
        h1 {
            font-size: 2rem;
            font-weight: 600;
            margin-bottom: 0.5rem;
            color: var(--text-primary);
        }
        
        .subtitle {
            color: var(--text-secondary);
            margin-bottom: 2rem;
        }
        
        .summary-grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
            gap: 1rem;
            margin-bottom: 2rem;
        }
        
        .summary-card {
            background: var(--bg-secondary);
            border: 1px solid var(--border);
            border-radius: 6px;
            padding: 1rem;
        }
        
        .summary-card .label {
            font-size: 0.75rem;
            color: var(--text-secondary);
            text-transform: uppercase;
            letter-spacing: 0.05em;
        }
        
        .summary-card .value {
            font-size: 1.5rem;
            font-weight: 600;
            margin-top: 0.25rem;
        }
        
        .summary-card .value.success { color: var(--success); }
        .summary-card .value.warning { color: var(--warning); }
        .summary-card .value.danger { color: var(--danger); }
        
        .table-container {
            background: var(--bg-secondary);
            border: 1px solid var(--border);
            border-radius: 6px;
            overflow: hidden;
        }
        
        table {
            width: 100%;
            border-collapse: collapse;
            font-size: 0.875rem;
        }
        
        th, td {
            padding: 0.75rem 1rem;
            text-align: left;
            border-bottom: 1px solid var(--border);
        }
        
        th {
            background: var(--bg-tertiary);
            font-weight: 600;
            color: var(--text-secondary);
            cursor: pointer;
            user-select: none;
        }
        
        th:hover {
            color: var(--text-primary);
        }
        
        th.sorted-asc::after { content: ' ▲'; }
        th.sorted-desc::after { content: ' ▼'; }
        
        tr:hover {
            background: var(--bg-tertiary);
        }
        
        .similarity-bar {
            width: 80px;
            height: 8px;
            background: var(--bg-tertiary);
            border-radius: 4px;
            overflow: hidden;
            display: inline-block;
            vertical-align: middle;
            margin-right: 0.5rem;
        }
        
        .similarity-bar .fill {
            height: 100%;
            border-radius: 4px;
        }
        
        .similarity-bar .fill.high { background: var(--success); }
        .similarity-bar .fill.medium { background: var(--warning); }
        .similarity-bar .fill.low { background: var(--danger); }
        
        .badge {
            display: inline-block;
            padding: 0.125rem 0.5rem;
            border-radius: 9999px;
            font-size: 0.75rem;
            font-weight: 500;
        }
        
        .badge.identical { background: rgba(63, 185, 80, 0.2); color: var(--success); }
        .badge.different { background: rgba(210, 153, 34, 0.2); color: var(--warning); }
        .badge.error { background: rgba(248, 81, 73, 0.2); color: var(--danger); }
        
        .path {
            max-width: 300px;
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
            font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, monospace;
            font-size: 0.8125rem;
        }
        
        a {
            color: var(--accent);
            text-decoration: none;
        }
        
        a:hover {
            text-decoration: underline;
        }
        
        .no-results {
            text-align: center;
            padding: 3rem;
            color: var(--text-secondary);
        }
    </style>
</head>
<body>
    <div class="container">
        <h1>CompareIt Report</h1>
        <p class="subtitle">File comparison analysis</p>
"#);

    // Summary cards
    html.push_str(&format!(r#"
        <div class="summary-grid">
            <div class="summary-card">
                <div class="label">Pairs Compared</div>
                <div class="value">{}</div>
            </div>
            <div class="summary-card">
                <div class="label">Identical</div>
                <div class="value success">{}</div>
            </div>
            <div class="summary-card">
                <div class="label">Different</div>
                <div class="value warning">{}</div>
            </div>
            <div class="summary-card">
                <div class="label">Errors</div>
                <div class="value{}">{}</div>
            </div>
            <div class="summary-card">
                <div class="label">Avg Similarity</div>
                <div class="value">{:.1}%</div>
            </div>
            <div class="summary-card">
                <div class="label">Min Similarity</div>
                <div class="value">{:.1}%</div>
            </div>
        </div>
"#,
        summary.pairs_compared,
        summary.identical_pairs,
        summary.different_pairs,
        if summary.error_pairs > 0 { " danger" } else { "" },
        summary.error_pairs,
        summary.average_similarity * 100.0,
        summary.min_similarity * 100.0
    ));

    // Results table
    html.push_str(r#"
        <div class="table-container">
            <table id="results-table">
                <thead>
                    <tr>
                        <th data-sort="status">Status</th>
                        <th data-sort="file1">File 1</th>
                        <th data-sort="file2">File 2</th>
                        <th data-sort="similarity">Similarity</th>
                        <th data-sort="type">Type</th>
                        <th data-sort="common">Common</th>
                        <th data-sort="only1">Only in F1</th>
                        <th data-sort="only2">Only in F2</th>
                        <th>Artifacts</th>
                    </tr>
                </thead>
                <tbody>
"#);

    // Table rows
    for result in results {
        let (file1, file2) = result.file_paths();
        let similarity = result.similarity_score();
        let identical = result.is_identical();

        let (status_badge, status_text) = if identical {
            ("identical", "Identical")
        } else {
            match result {
                ComparisonResult::Error { .. } => ("error", "Error"),
                _ => ("different", "Different"),
            }
        };

        let sim_class = if similarity >= 0.9 {
            "high"
        } else if similarity >= 0.5 {
            "medium"
        } else {
            "low"
        };

        let (type_str, common, only1, only2) = match result {
            ComparisonResult::Text(r) => (
                "text",
                r.common_lines.to_string(),
                r.only_in_file1.to_string(),
                r.only_in_file2.to_string(),
            ),
            ComparisonResult::Structured(r) => (
                "csv",
                r.common_records.to_string(),
                r.only_in_file1.to_string(),
                r.only_in_file2.to_string(),
            ),
            ComparisonResult::HashOnly { identical, .. } => (
                "binary",
                if *identical { "1" } else { "0" }.to_string(),
                "0".to_string(),
                "0".to_string(),
            ),
            ComparisonResult::Error { error, .. } => ("error", "-".to_string(), "-".to_string(), error.clone()),
        };

        // Generate artifact links
        let artifact_links = if let Some(dir) = artifacts_dir {
            let linked_id = result.linked_id();
            let sanitized = sanitize_for_filename(linked_id);
            let mut links = Vec::new();

            if matches!(result, ComparisonResult::Text(_)) && !identical {
                links.push(format!(
                    "<a href=\"{}/patches/{}.diff\">patch</a>",
                    dir.display(),
                    sanitized
                ));
            }
            if matches!(result, ComparisonResult::Structured(_)) && !identical {
                links.push(format!(
                    "<a href=\"{}/mismatches/{}.json\">details</a>",
                    dir.display(),
                    sanitized
                ));
            }
            links.join(" | ")
        } else {
            "-".to_string()
        };

        html.push_str(&format!(
            r#"                    <tr>
                        <td><span class="badge {}">{}</span></td>
                        <td class="path" title="{}">{}</td>
                        <td class="path" title="{}">{}</td>
                        <td>
                            <span class="similarity-bar"><span class="fill {}" style="width: {}%"></span></span>
                            {:.1}%
                        </td>
                        <td>{}</td>
                        <td>{}</td>
                        <td>{}</td>
                        <td>{}</td>
                        <td>{}</td>
                    </tr>
"#,
            status_badge,
            status_text,
            file1,
            truncate_path(file1, 40),
            file2,
            truncate_path(file2, 40),
            sim_class,
            (similarity * 100.0).round(),
            similarity * 100.0,
            type_str,
            common,
            only1,
            only2,
            artifact_links
        ));
    }

    html.push_str(r#"                </tbody>
            </table>
        </div>
"#);

    // JavaScript for sorting
    html.push_str(r#"
    <script>
        document.querySelectorAll('th[data-sort]').forEach(th => {
            th.addEventListener('click', () => {
                const table = th.closest('table');
                const tbody = table.querySelector('tbody');
                const rows = Array.from(tbody.querySelectorAll('tr'));
                const col = th.cellIndex;
                const isAsc = th.classList.contains('sorted-asc');
                
                // Clear other sort indicators
                table.querySelectorAll('th').forEach(h => {
                    h.classList.remove('sorted-asc', 'sorted-desc');
                });
                
                th.classList.add(isAsc ? 'sorted-desc' : 'sorted-asc');
                
                rows.sort((a, b) => {
                    let aVal = a.cells[col].textContent.trim();
                    let bVal = b.cells[col].textContent.trim();
                    
                    // Try numeric sort
                    const aNum = parseFloat(aVal.replace('%', ''));
                    const bNum = parseFloat(bVal.replace('%', ''));
                    
                    if (!isNaN(aNum) && !isNaN(bNum)) {
                        return isAsc ? bNum - aNum : aNum - bNum;
                    }
                    
                    return isAsc ? bVal.localeCompare(aVal) : aVal.localeCompare(bVal);
                });
                
                rows.forEach(row => tbody.appendChild(row));
            });
        });
    </script>
</body>
</html>
"#);

    html
}

/// Truncate a path string for display
fn truncate_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        path.to_string()
    } else {
        format!("...{}", &path[path.len() - max_len + 3..])
    }
}

/// Sanitize a string for use as a filename
fn sanitize_for_filename(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

/// Load results from a JSONL file
pub fn load_results_from_jsonl(path: &Path) -> Result<Vec<ComparisonResult>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    let mut results = Vec::new();
    for line in content.lines() {
        if !line.trim().is_empty() {
            let result: ComparisonResult = serde_json::from_str(line)
                .with_context(|| format!("Failed to parse JSON line: {}", line))?;
            results.push(result);
        }
    }

    Ok(results)
}
